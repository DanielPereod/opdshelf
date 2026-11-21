//! The live events, and how the web pages are told their session ended.

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use base64::Engine;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use std::collections::HashMap;
use tower::ServiceExt;

use opdshelf::{router, Config};

const USER: &str = "admin";
const PASS: &str = "secret";
const DOC: &str = "502000418ae95849b554660b1ee1d388";

struct TestApp {
    app: Router,
    dir: tempfile::TempDir,
}

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
    async fn request(&self, request: Request<Body>) -> axum::response::Response {
        self.app.clone().oneshot(request).await.unwrap()
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
        let response = self.request(request.body(body).unwrap()).await;
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }

    async fn events(&self, query: &str) -> Body {
        let response = self
            .request(
                Request::get(format!("/api/progress/events{query}"))
                    .header(header::AUTHORIZATION, basic())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
        response.into_body()
    }
}

async fn next(body: &mut Body) -> String {
    loop {
        let frame = tokio::time::timeout(std::time::Duration::from_secs(5), body.frame())
            .await
            .expect("an event")
            .unwrap()
            .unwrap();
        let text = String::from_utf8(frame.into_data().unwrap().to_vec()).unwrap();
        if !text.starts_with(':') {
            return text;
        }
    }
}

#[tokio::test]
async fn bookmarks_are_pushed_to_the_readers_that_have_the_book_open() {
    let t = app();
    let mut events = t.events(&format!("?documents={DOC}")).await;
    assert!(next(&mut events).await.starts_with("event: hello\n"));
    let bookmark = json!({
        "id": "mark-1",
        "kind": "bookmark",
        "text": "Page 3",
        "cfi": "epubcfi(/6/4!/4/2)",
        "percentage": 0.1,
        "updated": 0,
    });
    let (status, _) = t
        .api(
            "PUT",
            &format!("/api/bookmarks/{DOC}"),
            Some(json!({ "device_id": "web1", "annotations": [bookmark] })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let event = next(&mut events).await;
    assert!(event.starts_with("event: bookmarks\n"), "{event}");
    assert!(event.contains("mark-1"), "{event}");
}

#[tokio::test]
async fn the_panel_hears_about_books_added_or_removed_elsewhere() {
    let t = app();
    // The first scan of the library
    t.api("GET", "/api/progress", None).await;

    let mut panel = t.events("").await;
    let mut reader = t.events(&format!("?documents={DOC}")).await;
    assert!(next(&mut panel).await.starts_with("event: hello\n"));
    assert!(next(&mut reader).await.starts_with("event: hello\n"));

    std::fs::write(t.dir.path().join("books").join("Two.epub"), b"book two").unwrap();
    t.api("GET", "/api/progress", None).await;
    let event = next(&mut panel).await;
    assert!(event.starts_with("event: library\n"), "{event}");

    // A reader follows one book, so it is not bothered about the shelf
    std::fs::remove_file(t.dir.path().join("books").join("Two.epub")).unwrap();
    t.api("GET", "/api/progress", None).await;
    assert!(next(&mut panel).await.starts_with("event: library\n"));
    let quiet = tokio::time::timeout(std::time::Duration::from_millis(200), reader.frame()).await;
    assert!(quiet.is_err(), "a reader got a library event");
}

#[tokio::test]
async fn a_page_is_told_to_sign_in_again_without_the_browsers_own_window() {
    let t = app();
    // What a page's fetch or EventSource sends
    let response = t
        .request(
            Request::get("/api/progress")
                .header("sec-fetch-dest", "empty")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(!response.headers().contains_key(header::WWW_AUTHENTICATE));
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["code"], "login");

    // Apps and scripts still get the challenge
    let response = t
        .request(Request::get("/api/progress").body(Body::empty()).unwrap())
        .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(response.headers()[header::WWW_AUTHENTICATE]
        .to_str()
        .unwrap()
        .starts_with("Basic"));
}

#[tokio::test]
async fn using_the_site_keeps_the_session_and_signing_out_ends_every_copy_of_it() {
    let t = app();
    let login = t
        .request(
            Request::post("/user/login")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(format!("username={USER}&password={PASS}")))
                .unwrap(),
        )
        .await;
    let cookie = login.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();

    let response = t
        .request(
            Request::get("/api/progress")
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let renewed = response.headers()[header::SET_COOKIE].to_str().unwrap();
    assert!(
        renewed.starts_with(&cookie) && renewed.contains("Max-Age=1296000"),
        "{renewed}"
    );

    let page = t
        .request(
            Request::get("/admin")
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    let html = String::from_utf8(page.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
    let csrf = html
        .split("name=\"csrf\" value=\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .expect("a csrf token in the page")
        .to_string();
    let out = t
        .request(
            Request::post("/user/logout")
                .header(header::COOKIE, &cookie)
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(format!("csrf={csrf}")))
                .unwrap(),
        )
        .await;
    assert_eq!(out.status(), StatusCode::SEE_OTHER);

    // The copy of the cookie no longer works
    let response = t
        .request(
            Request::get("/api/progress")
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
