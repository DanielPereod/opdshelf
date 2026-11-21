//! The finished state of a book: statistics, marks and what OPDS shows.

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use base64::Engine;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use std::collections::HashMap;
use tower::ServiceExt;

use opdshelf::{router, Config};

struct TestApp {
    app: Router,
    _dir: tempfile::TempDir,
}

const USER: &str = "admin";
const PASS: &str = "secret";
/// KOReader's id of a file containing "book one"
const DOC: &str = "502000418ae95849b554660b1ee1d388";

fn app() -> TestApp {
    let dir = tempfile::tempdir().unwrap();
    let books = dir.path().join("books");
    std::fs::create_dir_all(&books).unwrap();
    std::fs::write(books.join("One.epub"), b"book one").unwrap();
    let mut env = HashMap::new();
    env.insert("BOOKS_DIR".to_string(), books.to_string_lossy().into_owned());
    env.insert("WATCH".into(), "false".into());
    env.insert("ADMIN_USERNAME".into(), USER.into());
    env.insert("ADMIN_PASSWORD".into(), PASS.into());
    TestApp {
        app: router(Config::from_lookup(|k| env.get(k).cloned())),
        _dir: dir,
    }
}

fn basic() -> String {
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{USER}:{PASS}"))
    )
}

impl TestApp {
    async fn raw(&self, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, String) {
        let mut request = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::AUTHORIZATION, basic());
        if body.is_some() {
            request = request.header(header::CONTENT_TYPE, "application/json");
        }
        let body = body.map_or_else(Body::empty, |b| Body::from(b.to_string()));
        let response = self.app.clone().oneshot(request.body(body).unwrap()).await.unwrap();
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (status, String::from_utf8_lossy(&bytes).into_owned())
    }

    async fn api(&self, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
        let (status, text) = self.raw(method, uri, body).await;
        (status, serde_json::from_str(&text).unwrap_or(Value::Null))
    }

    async fn finished_in_stats(&self) -> i64 {
        let (_, stats) = self.api("GET", "/api/stats", None).await;
        stats["finished"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["count"].as_i64().unwrap())
            .sum()
    }

    async fn in_opds(&self, status: &str) -> bool {
        let (_, feed) = self.raw("GET", &format!("/opds/books?status={status}"), None).await;
        feed.contains("urn:opdshelf:book:One.epub")
    }
}

/// A book read to the end, marked as unread, and read to the end again is
/// finished for OPDS, the panel and the statistics.
#[tokio::test]
async fn a_book_finished_again_after_unread_counts_in_stats() {
    let app = app();
    let now = chrono::Utc::now().timestamp();
    let uri = format!("/api/progress/{DOC}");
    let pos = |progress: &str, ts: i64| json!({ "progress": progress, "percentage": 1.0, "device": "Kobo", "device_id": "kobo", "timestamp": ts });
    assert_eq!(
        app.api("PUT", &uri, Some(pos("p1", now - 7200))).await.0,
        StatusCode::OK
    );
    assert_eq!(app.finished_in_stats().await, 1);
    let (status, body) = app
        .api(
            "PUT",
            "/api/marks",
            Some(json!({ "documents": [DOC], "state": "unread", "at": now - 3600, "sent_at": now })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(app.finished_in_stats().await, 0);
    // Read to the end again, after the mark
    assert_eq!(
        app.api("PUT", &uri, Some(pos("p2", now - 1800))).await.0,
        StatusCode::OK
    );
    assert!(app.in_opds("finished").await);
    assert_eq!(app.finished_in_stats().await, 1);
}

/// A mark cleared (state null) is remembered: an older mark sent late by a
/// device that was offline does not bring it back.
#[tokio::test]
async fn a_cleared_mark_does_not_come_back_with_an_older_one() {
    let app = app();
    let now = chrono::Utc::now().timestamp();
    let (_, body) = app
        .api(
            "PUT",
            "/api/marks",
            Some(json!({ "documents": [DOC], "state": "read" })),
        )
        .await;
    assert_eq!(body["marks"][DOC]["state"], "read");
    let (_, body) = app
        .api("PUT", "/api/marks", Some(json!({ "documents": [DOC], "state": null })))
        .await;
    assert_eq!(body["marks"][DOC], Value::Null, "cleared");
    // Sent now by a phone that marked it finished an hour ago, offline
    let (_, body) = app
        .api(
            "PUT",
            "/api/marks",
            Some(json!({ "documents": [DOC], "state": "read", "at": now - 3600, "sent_at": now })),
        )
        .await;
    assert_eq!(body["marks"][DOC], Value::Null, "the clear is newer");
    // The clients can see when it was cleared
    let (_, body) = app.api("GET", "/api/progress", None).await;
    assert_eq!(body["cleared"][0]["document"], DOC);
    // A mark made now, after the clear, is kept
    let (_, body) = app
        .api(
            "PUT",
            "/api/marks",
            Some(json!({ "documents": [DOC], "state": "read" })),
        )
        .await;
    assert_eq!(body["marks"][DOC]["state"], "read");
    let (_, body) = app.api("GET", "/api/progress", None).await;
    assert!(body["cleared"].as_array().unwrap().is_empty());
}

/// One threshold everywhere: 98.5 % is not finished for OPDS, the statistics
/// or the API, and 99 % is.
#[tokio::test]
async fn the_finished_threshold_is_the_same_everywhere() {
    let app = app();
    let uri = format!("/api/progress/{DOC}");
    let position =
        |percentage: f64| json!({ "progress": "p", "percentage": percentage, "device": "Kobo", "device_id": "kobo" });
    assert_eq!(app.api("PUT", &uri, Some(position(0.985))).await.0, StatusCode::OK);
    assert!(app.in_opds("reading").await);
    assert!(!app.in_opds("finished").await);
    assert_eq!(app.finished_in_stats().await, 0);
    let (_, body) = app.api("GET", &uri, None).await;
    assert_eq!(body["status"], "reading");
    assert_eq!(body["finished_at"], 0.99);
    assert_eq!(app.api("PUT", &uri, Some(position(0.99))).await.0, StatusCode::OK);
    assert!(app.in_opds("finished").await);
    assert_eq!(app.finished_in_stats().await, 1);
    let (_, body) = app.api("GET", "/api/progress", None).await;
    assert_eq!(body["books"][0]["status"], "finished");
    assert_eq!(body["finished_at"], 0.99);
}

/// "Not finished" below the threshold clears the mark (state null): the book
/// goes back to showing how far it was read, in the API and in OPDS.
#[tokio::test]
async fn clearing_the_mark_shows_the_progress_again() {
    let app = app();
    let now = chrono::Utc::now().timestamp();
    let uri = format!("/api/progress/{DOC}");
    let position =
        json!({ "progress": "p", "percentage": 0.4, "device": "Kobo", "device_id": "kobo", "timestamp": now - 600 });
    assert_eq!(app.api("PUT", &uri, Some(position)).await.0, StatusCode::OK);
    for (state, at) in [(json!("read"), now - 120), (Value::Null, now - 60)] {
        let (status, _) = app
            .api(
                "PUT",
                "/api/marks",
                Some(json!({ "documents": [DOC], "state": state, "at": at, "sent_at": now })),
            )
            .await;
        assert_eq!(status, StatusCode::OK);
    }
    assert!(app.in_opds("reading").await);
    assert!(!app.in_opds("unread").await);
    let (_, body) = app.api("GET", &uri, None).await;
    assert_eq!(body["status"], "reading");
    assert_eq!(body["current"]["percentage"], 0.4);
}

/// Each book of an OPDS feed says its document id, so the apps can match a
/// downloaded file to its book even after it was moved or renamed.
#[tokio::test]
async fn opds_entries_carry_the_document_id() {
    let app = app();
    // Looking at the statuses reads the id of every book once
    app.in_opds("reading").await;
    let (_, feed) = app.raw("GET", "/opds/books", None).await;
    assert!(
        feed.contains(&format!("<opdshelf:document>{DOC}</opdshelf:document>")),
        "{feed}"
    );
}
