//! Reading progress: OPDShelf's API and the KOReader sync server.

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
    dir: tempfile::TempDir,
}

const USER: &str = "admin";
const PASS: &str = "secret";
/// md5("secret"), what KOReader sends as the key
const KEY: &str = "5ebe2294ecd0e0f08eab7690d2a6ee69";
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
        dir,
    }
}

fn basic() -> String {
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{USER}:{PASS}"))
    )
}

impl TestApp {
    async fn send(&self, request: Request<Body>) -> (StatusCode, Value) {
        let response = self.app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
    }

    async fn api(&self, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
        let mut request = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::AUTHORIZATION, basic());
        if body.is_some() {
            request = request.header(header::CONTENT_TYPE, "application/json");
        }
        let body = body.map_or_else(Body::empty, |b| Body::from(b.to_string()));
        self.send(request.body(body).unwrap()).await
    }

    async fn kosync(&self, method: &str, uri: &str, key: &str, body: Option<Value>) -> (StatusCode, Value) {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::ACCEPT, "application/vnd.koreader.v1+json")
            .header(header::CONTENT_TYPE, "application/json")
            .header("x-auth-user", USER)
            .header("x-auth-key", key);
        let body = body.map_or_else(Body::empty, |b| Body::from(b.to_string()));
        self.send(request.body(body).unwrap()).await
    }
}

#[tokio::test]
async fn reader_pages_carry_koreader_document_id() {
    let app = app();
    let response = app
        .app
        .clone()
        .oneshot(
            Request::get("/read/One.epub")
                .header(header::AUTHORIZATION, basic())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let html = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8_lossy(&html);
    assert!(html.contains(&format!(r#"data-document="{DOC}""#)), "{html}");
    assert!(html.contains("/static/reader/sync.js"));
}

#[tokio::test]
async fn api_saves_and_returns_positions() {
    let app = app();
    let uri = format!("/api/progress/{DOC}");
    let (status, body) = app.api("GET", &uri, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["current"], Value::Null);

    let position = json!({
        "progress": "/body/DocFragment[2]/body/p[3]/text()[1].10",
        "percentage": 0.25,
        "device": "Firefox on Linux",
        "device_id": "web1",
        "cfi": "epubcfi(/6/4!/4/6/1:10)",
        "context": { "before": "It was", "after": " a dark night", "selected": "dark" },
        "manual": true,
    });
    let (status, body) = app.api("PUT", &uri, Some(position)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["current"]["device_id"], "web1");
    assert_eq!(body["current"]["manual"], true);

    let (_, body) = app.api("GET", &uri, None).await;
    assert_eq!(body["current"]["cfi"], "epubcfi(/6/4!/4/6/1:10)");
    assert_eq!(body["current"]["context"]["selected"], "dark");
    assert_eq!(body["history"].as_array().unwrap().len(), 1);

    // Read offline an hour ago on a phone and sent now: kept in the history but not current
    let earlier = body["current"]["timestamp"].as_i64().unwrap() - 3600;
    let offline =
        json!({ "progress": "5", "percentage": 0.1, "device": "Phone", "device_id": "phone", "timestamp": earlier });
    let (_, body) = app.api("PUT", &uri, Some(offline)).await;
    assert_eq!(body["current"]["device_id"], "web1");
    let (_, body) = app.api("GET", &uri, None).await;
    assert_eq!(body["history"].as_array().unwrap().len(), 2);

    // Saved to disk
    let saved = std::fs::read_to_string(app.dir.path().join("books/.opdshelf/progress.json")).unwrap();
    assert!(saved.contains("Firefox on Linux"));
}

#[tokio::test]
async fn api_validates_input_and_needs_login() {
    let app = app();
    let (status, _) = app.api("GET", "/api/progress/not-hex!", None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = app
        .api("PUT", &format!("/api/progress/{DOC}"), Some(json!({ "progress": "1" })))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = app
        .send(
            Request::get(format!("/api/progress/{DOC}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn koreader_sync_protocol() {
    let app = app();
    let (status, body) = app.kosync("GET", "/kosync/users/auth", KEY, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["authorized"], "OK");
    let (status, body) = app.kosync("GET", "/kosync/users/auth", "wrong", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], 2001);

    // "Register" works only with the server's own account
    let (status, _) = app
        .kosync(
            "POST",
            "/kosync/users/create",
            "",
            Some(json!({ "username": USER, "password": KEY })),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, _) = app
        .kosync(
            "POST",
            "/kosync/users/create",
            "",
            Some(json!({ "username": "eve", "password": KEY })),
        )
        .await;
    assert_eq!(status, StatusCode::PAYMENT_REQUIRED);

    let (status, body) = app
        .kosync("GET", &format!("/kosync/syncs/progress/{DOC}"), KEY, None)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({}));

    let push = json!({
        "document": DOC,
        "progress": "/body/DocFragment[3]/body/p[2]/text().0",
        "percentage": 0.42,
        "device": "Kobo",
        "device_id": "KOBO1",
        "metadata": { "title": "One" },
    });
    let (status, body) = app
        .kosync("PUT", "/kosync/syncs/progress", KEY, Some(push.clone()))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["document"], DOC);
    assert!(body["timestamp"].as_i64().unwrap() > 0);
    let (status, _) = app.kosync("PUT", "/kosync/syncs/progress", "wrong", Some(push)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (_, body) = app
        .kosync("GET", &format!("/kosync/syncs/progress/{DOC}"), KEY, None)
        .await;
    assert_eq!(body["progress"], "/body/DocFragment[3]/body/p[2]/text().0");
    assert_eq!(body["percentage"], 0.42);
    assert_eq!(body["device"], "Kobo");
    assert_eq!(body["device_id"], "KOBO1");

    // The web reader sees what KOReader saved
    let (_, body) = app.api("GET", &format!("/api/progress/{DOC}"), None).await;
    assert_eq!(body["current"]["device"], "Kobo");

    let (status, body) = app.kosync("GET", "/kosync/healthcheck", "", None).await;
    assert_eq!((status, body["state"].as_str()), (StatusCode::OK, Some("OK")));
}

#[tokio::test]
async fn koreader_file_name_ids_map_to_the_book() {
    let app = app();
    // KOReader's "file name" checksum: md5("One.epub")
    let by_name = "6959caa56e476c93efdc293335af7b8d";
    let push =
        json!({ "document": by_name, "progress": "7", "percentage": 0.7, "device": "Kobo", "device_id": "KOBO1" });
    let (status, _) = app.kosync("PUT", "/kosync/syncs/progress", KEY, Some(push)).await;
    assert_eq!(status, StatusCode::OK);
    let (_, body) = app.api("GET", &format!("/api/progress/{DOC}"), None).await;
    assert_eq!(body["current"]["progress"], "7");
    let (_, body) = app
        .kosync("GET", &format!("/kosync/syncs/progress/{by_name}"), KEY, None)
        .await;
    assert_eq!(body["progress"], "7");
}

#[tokio::test]
async fn koreader_plugin_marks_survive_progress_sync_resending_them() {
    let app = app();
    // What the OPDShelf KOReader plugin sends for "Set as last position"
    let mark = json!({
        "document": DOC,
        "progress": "/body/DocFragment[2]/body/p[7]/text().31",
        "percentage": 0.3,
        "device": "Kobo",
        "device_id": "KOBO1",
        "manual": true,
        "timestamp": 1_700_000_000,
        "context": { "before": "and then", "after": "she left", "selected": "she left" },
    });
    let (status, _) = app.kosync("PUT", "/kosync/syncs/progress", KEY, Some(mark)).await;
    assert_eq!(status, StatusCode::OK);
    // KOReader's own Progress sync then pushes the same point when the book is closed
    let resend = json!({
        "document": DOC,
        "progress": "/body/DocFragment[2]/body/p[7]/text().31",
        "percentage": 0.3,
        "device": "Kobo",
        "device_id": "KOBO1",
    });
    app.kosync("PUT", "/kosync/syncs/progress", KEY, Some(resend)).await;
    let (_, body) = app
        .kosync("GET", &format!("/kosync/syncs/progress/{DOC}"), KEY, None)
        .await;
    assert_eq!(body["manual"], true);
    assert_eq!(body["timestamp"], 1_700_000_000);
    assert_eq!(body["context"]["selected"], "she left");
}
