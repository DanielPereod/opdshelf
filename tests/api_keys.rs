//! Read-only API keys: made and revoked from the panel, used with `Authorization: Bearer`.

use axum::body::Body;
use axum::http::{header, HeaderMap, Request, StatusCode};
use axum::Router;
use base64::Engine;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use std::collections::HashMap;
use tower::ServiceExt;

use opdshelf::{router, Config};

const USER: &str = "admin";
const PASS: &str = "secret";
/// KOReader's id of a file containing "book one"
const DOC: &str = "502000418ae95849b554660b1ee1d388";

struct TestApp {
    app: Router,
    dir: tempfile::TempDir,
}

fn app_with(auth: bool) -> TestApp {
    let dir = tempfile::tempdir().unwrap();
    let books = dir.path().join("books");
    std::fs::create_dir_all(&books).unwrap();
    std::fs::write(books.join("One.epub"), b"book one").unwrap();
    let mut env = HashMap::new();
    env.insert("BOOKS_DIR".to_string(), books.to_string_lossy().into_owned());
    env.insert("WATCH".into(), "false".into());
    if auth {
        env.insert("ADMIN_USERNAME".into(), USER.into());
        env.insert("ADMIN_PASSWORD".into(), PASS.into());
    }
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
    async fn send(&self, request: Request<Body>) -> (StatusCode, HeaderMap, Value) {
        let response = self.app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        (status, headers, serde_json::from_slice(&body).unwrap_or(Value::Null))
    }

    /// A request with `authorization` (Basic, Bearer or none) and a JSON body.
    async fn call(
        &self,
        method: &str,
        uri: &str,
        authorization: Option<&str>,
        body: Option<Value>,
    ) -> (StatusCode, HeaderMap, Value) {
        let mut request = Request::builder().method(method).uri(uri);
        if let Some(value) = authorization {
            request = request.header(header::AUTHORIZATION, value);
        }
        if body.is_some() {
            request = request.header(header::CONTENT_TYPE, "application/json");
        }
        let body = body.map_or_else(Body::empty, |b| Body::from(b.to_string()));
        self.send(request.body(body).unwrap()).await
    }

    async fn csrf(&self) -> String {
        let request = Request::get("/admin")
            .header(header::AUTHORIZATION, basic())
            .body(Body::empty())
            .unwrap();
        let response = self.app.clone().oneshot(request).await.unwrap();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let html = String::from_utf8(body.to_vec()).unwrap();
        let start = html.find(r#"data-csrf=""#).unwrap() + 11;
        html[start..start + html[start..].find('"').unwrap()].to_string()
    }

    /// Makes a key from the panel and answers it with its id.
    async fn create_key(&self, name: &str) -> (String, String) {
        let csrf = self.csrf().await;
        let request = Request::post("/api/keys")
            .header(header::AUTHORIZATION, basic())
            .header(header::CONTENT_TYPE, "application/json")
            .header("x-csrf-token", csrf)
            .body(Body::from(json!({ "name": name }).to_string()))
            .unwrap();
        let (status, _, body) = self.send(request).await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        (
            body["secret"].as_str().unwrap().to_string(),
            body["key"]["id"].as_str().unwrap().to_string(),
        )
    }

    async fn add_highlight(&self) {
        let highlight =
            json!({ "id": "kobo-1", "text": "It was a dark and stormy night", "note": "Classic", "updated": 0 });
        let (status, _, _) = self
            .call(
                "PUT",
                &format!("/api/annotations/{DOC}"),
                Some(&basic()),
                Some(json!({ "device": "Kobo", "device_id": "kobo", "annotations": [highlight] })),
            )
            .await;
        assert_eq!(status, StatusCode::OK);
    }
}

#[tokio::test]
async fn a_key_reads_the_highlights_but_changes_nothing() {
    let app = app_with(true);
    app.add_highlight().await;
    let (key, _) = app.create_key("Notes app").await;
    assert!(key.starts_with("opds_"));
    let bearer = format!("Bearer {key}");

    let (status, headers, list) = app.call("GET", "/api/annotations", Some(&bearer), None).await;
    assert_eq!(status, StatusCode::OK, "{list}");
    // A key makes no session
    assert!(!headers.contains_key(header::SET_COOKIE));
    assert_eq!(list["books"][0]["document"], DOC);
    let (status, _, book) = app
        .call("GET", &format!("/api/annotations/{DOC}"), Some(&bearer), None)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(book["annotations"][0]["id"], "kobo-1");
    for uri in ["/api/highlights", "/api/progress", &format!("/api/progress/{DOC}")] {
        assert_eq!(
            app.call("GET", uri, Some(&bearer), None).await.0,
            StatusCode::OK,
            "{uri}"
        );
    }
    assert_eq!(
        app.call("HEAD", "/api/annotations", Some(&bearer), None).await.0,
        StatusCode::OK
    );

    // Writing, or anything outside what it reads, is refused in JSON
    let (status, _, body) = app
        .call(
            "PUT",
            &format!("/api/annotations/{DOC}"),
            Some(&bearer),
            Some(json!({ "annotations": [{ "id": "kobo-1", "deleted": true, "updated": i64::MAX / 2 }] })),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], "scope");
    for (method, uri) in [
        ("GET", "/admin"),
        ("GET", "/api/keys"),
        ("GET", "/download/One.epub"),
        ("GET", "/opds/books"),
        ("DELETE", "/api/devices/kobo"),
    ] {
        let (status, _, body) = app.call(method, uri, Some(&bearer), None).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {uri}");
        assert_eq!(body["code"], "scope", "{method} {uri}");
    }
    let (_, _, book) = app
        .call("GET", &format!("/api/annotations/{DOC}"), Some(&bearer), None)
        .await;
    assert_eq!(book["annotations"][0]["text"], "It was a dark and stormy night");

    // Listed with when it was last used, and only its hash on disk
    let (_, _, keys) = app.call("GET", "/api/keys", Some(&basic()), None).await;
    assert_eq!(keys["keys"][0]["name"], "Notes app");
    assert!(keys["keys"][0]["last_used"].is_i64());
    assert!(keys["keys"][0].get("hash").is_none());
    let saved = std::fs::read_to_string(app.dir.path().join("books/.opdshelf/api-keys.json")).unwrap();
    assert!(!saved.contains(&key[5..]));

    // Its live feed shows up on the devices page with the key's name
    let request = Request::get("/api/progress/events")
        .header(header::AUTHORIZATION, &bearer)
        .body(Body::empty())
        .unwrap();
    let events = app.app.clone().oneshot(request).await.unwrap();
    assert_eq!(events.status(), StatusCode::OK);
    let (_, _, devices) = app.call("GET", "/api/devices", Some(&basic()), None).await;
    let names: Vec<_> = devices["devices"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["device"].clone())
        .collect();
    assert!(names.contains(&json!("Notes app")), "{devices}");
    drop(events);
}

#[tokio::test]
async fn a_revoked_or_wrong_key_is_turned_away_without_the_sign_in_window() {
    let app = app_with(true);
    let (key, id) = app.create_key("Script").await;
    let bearer = format!("Bearer {key}");
    assert_eq!(
        app.call("GET", "/api/annotations", Some(&bearer), None).await.0,
        StatusCode::OK
    );

    // Revoking needs the CSRF token
    let (status, _, _) = app
        .call("DELETE", &format!("/api/keys/{id}"), Some(&basic()), None)
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let request = Request::delete(format!("/api/keys/{id}"))
        .header(header::AUTHORIZATION, basic())
        .header("x-csrf-token", app.csrf().await)
        .body(Body::empty())
        .unwrap();
    assert_eq!(app.send(request).await.0, StatusCode::OK);

    for authorization in [bearer.as_str(), "Bearer opds_made_up"] {
        let (status, headers, body) = app.call("GET", "/api/annotations", Some(authorization), None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert!(!headers.contains_key(header::WWW_AUTHENTICATE));
        assert_eq!(body["code"], "api_key");
    }
    // Without the CSRF token no key is made
    let (status, _, _) = app
        .call("POST", "/api/keys", Some(&basic()), Some(json!({ "name": "x" })))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn the_list_of_highlights_has_the_books_details() {
    let app = app_with(true);
    app.add_highlight().await;
    let position = json!({ "progress": "5", "percentage": 0.4, "device": "Kobo", "device_id": "kobo" });
    app.call("PUT", &format!("/api/progress/{DOC}"), Some(&basic()), Some(position))
        .await;
    let (_, _, list) = app.call("GET", "/api/annotations", Some(&basic()), None).await;
    let book = &list["books"][0];
    assert_eq!(book["path"], "One.epub");
    assert_eq!(book["title"], "One");
    assert_eq!(book["authors"], "");
    assert_eq!(book["cover"], Value::Null);
    assert_eq!(book["status"], "reading");
    assert_eq!(book["percentage"], 0.4);
    assert_eq!(
        (book["highlights"].as_i64(), book["notes"].as_i64()),
        (Some(1), Some(1))
    );
}

#[tokio::test]
async fn without_sign_in_everything_stays_open() {
    let app = app_with(false);
    app.add_highlight().await;
    for authorization in [None, Some("Bearer opds_made_up")] {
        let (status, _, _) = app.call("GET", "/api/annotations", authorization, None).await;
        assert_eq!(status, StatusCode::OK);
        let (status, _, _) = app
            .call(
                "PUT",
                &format!("/api/progress/{DOC}"),
                authorization,
                Some(json!({ "progress": "1", "percentage": 0.1, "device_id": "x" })),
            )
            .await;
        assert_eq!(status, StatusCode::OK);
    }
}
