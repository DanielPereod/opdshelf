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

#[tokio::test]
async fn live_events_follow_position_changes() {
    let app = app();
    let response = app
        .app
        .clone()
        .oneshot(
            Request::get(format!("/api/progress/events?documents={DOC}"))
                .header(header::AUTHORIZATION, basic())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers()[header::CONTENT_TYPE]
        .to_str()
        .unwrap()
        .starts_with("text/event-stream"));
    let mut body = response.into_body();
    async fn next(body: &mut Body) -> String {
        let frame = tokio::time::timeout(std::time::Duration::from_secs(5), body.frame())
            .await
            .expect("an event")
            .unwrap()
            .unwrap();
        String::from_utf8(frame.into_data().unwrap().to_vec()).unwrap()
    }
    let hello = next(&mut body).await;
    assert!(hello.starts_with("event: hello\n"), "{hello}");

    // A change of another book is not sent
    let other = "0123456789abcdef0123456789abcdef";
    let position = |device: &str, progress: &str| json!({ "progress": progress, "percentage": 0.5, "device": device, "device_id": device });
    app.api("PUT", &format!("/api/progress/{other}"), Some(position("kobo", "/a")))
        .await;
    let (_, saved) = app
        .api("PUT", &format!("/api/progress/{DOC}"), Some(position("kobo", "/b")))
        .await;
    assert_eq!(saved["accepted"], true);
    assert!(saved["server_time"].is_i64());
    let event = next(&mut body).await;
    assert!(event.starts_with("event: progress\n"), "{event}");
    let data: Value = serde_json::from_str(event.lines().nth(1).unwrap().trim_start_matches("data: ")).unwrap();
    assert_eq!(data["document"], DOC);
    assert_eq!(data["current"]["progress"], "/b");
    assert_eq!(data["current"]["device_id"], "kobo");

    // KOReader's own sync is announced too
    app.kosync(
        "PUT",
        "/kosync/syncs/progress",
        KEY,
        Some(
            json!({ "document": DOC, "progress": "/c", "percentage": 0.6, "device": "Kindle", "device_id": "kindle" }),
        ),
    )
    .await;
    let event = next(&mut body).await;
    assert!(event.contains(r#""progress":"/c""#), "{event}");
}

#[tokio::test]
async fn a_position_that_lost_is_not_accepted() {
    let app = app();
    let uri = format!("/api/progress/{DOC}");
    app.api(
        "PUT",
        &uri,
        Some(json!({ "progress": "/new", "percentage": 0.6, "device": "Phone", "device_id": "phone" })),
    )
    .await;
    // Read offline an hour ago on a Kobo whose clock runs 10 minutes behind
    let now = chrono::Utc::now().timestamp();
    let (_, body) = app
        .api(
            "PUT",
            &uri,
            Some(json!({
                "progress": "/old", "percentage": 0.4, "device": "Kobo", "device_id": "kobo",
                "timestamp": now - 600 - 3600, "sent_at": now - 600,
            })),
        )
        .await;
    assert_eq!(body["accepted"], false);
    assert_eq!(body["current"]["device_id"], "phone");
    let (_, body) = app.api("GET", &uri, None).await;
    let kobo = body["history"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["device_id"] == "kobo")
        .unwrap()
        .clone();
    // Its time was put right: an hour ago, not an hour and ten minutes
    assert!(
        (kobo["timestamp"].as_i64().unwrap() - (now - 3600)).abs() <= 2,
        "{kobo}"
    );
}

#[tokio::test]
async fn panel_lists_positions_and_devices() {
    let app = app();
    let other = "0123456789abcdef0123456789abcdef";
    let position = |device: &str, pct: f64, timestamp: i64| json!({ "progress": "/p", "percentage": pct, "device": device, "device_id": device, "timestamp": timestamp });
    app.api(
        "PUT",
        &format!("/api/progress/{DOC}"),
        Some(position("Kobo", 0.4, 1000)),
    )
    .await;
    app.api(
        "PUT",
        &format!("/api/progress/{DOC}"),
        Some(position("Phone", 0.5, 2000)),
    )
    .await;
    app.api(
        "PUT",
        &format!("/api/progress/{other}"),
        Some(position("Kobo", 0.9, 1500)),
    )
    .await;

    // Every book's current position, newest first, with the book it belongs to
    let (status, body) = app.api("GET", "/api/progress", None).await;
    assert_eq!(status, StatusCode::OK);
    let books = body["books"].as_array().unwrap();
    assert_eq!(books.len(), 2);
    assert_eq!(books[0]["document"], DOC);
    assert_eq!(books[0]["path"], "One.epub");
    assert_eq!(books[0]["current"]["device"], "Phone");
    assert_eq!(books[1]["path"], Value::Null, "a book that is not in the library");

    // A device listening for changes shows up as connected
    let events = app
        .app
        .clone()
        .oneshot(
            Request::get("/api/progress/events?device_id=Kobo&device=Kobo%20Libra")
                .header(header::AUTHORIZATION, basic())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, body) = app.api("GET", "/api/devices", None).await;
    assert_eq!(status, StatusCode::OK);
    let devices = body["devices"].as_array().unwrap();
    assert_eq!(devices.len(), 2);
    let kobo = &devices[0];
    assert_eq!(
        (kobo["device_id"].as_str(), kobo["live"].as_bool()),
        (Some("Kobo"), Some(true))
    );
    assert_eq!(kobo["books"], 2);
    assert_eq!(kobo["last_seen"], 1500);
    assert_eq!(kobo["recent"][0]["percentage"], 0.9);
    assert_eq!(kobo["recent"][1]["title"], "One");
    assert_eq!(devices[1]["live"], false);
    assert_eq!(body["listening"], 1);
    drop(events);
    let (_, body) = app.api("GET", "/api/devices", None).await;
    assert_eq!(body["listening"], 0, "closing the stream disconnects the device");

    // Forgetting a device needs the panel's token
    let (status, _) = app.api("DELETE", "/api/devices/Phone", None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let html = app
        .app
        .clone()
        .oneshot(
            Request::get("/admin")
                .header(header::AUTHORIZATION, basic())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    let html = String::from_utf8_lossy(&html);
    let start = html.find(r#"data-csrf=""#).unwrap() + 11;
    let csrf = &html[start..start + 64];
    let (status, body) = app
        .send(
            Request::delete("/api/devices/Phone")
                .header(header::AUTHORIZATION, basic())
                .header("x-csrf-token", csrf)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["removed"], 1);
    let (_, body) = app.api("GET", &format!("/api/progress/{DOC}"), None).await;
    assert_eq!(
        body["current"]["device"], "Kobo",
        "the other device's position is current again"
    );
}

#[tokio::test]
async fn the_panel_speaks_the_browsers_language() {
    let app = app();
    let get = |lang: &'static str| {
        let app = app.app.clone();
        async move {
            let response = app
                .oneshot(
                    Request::get("/admin/sync")
                        .header(header::AUTHORIZATION, basic())
                        .header(header::ACCEPT_LANGUAGE, lang)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            let body = response.into_body().collect().await.unwrap().to_bytes();
            String::from_utf8_lossy(&body).into_owned()
        }
    };
    let spanish = get("es-ES,es;q=0.9").await;
    assert!(spanish.contains(r#"<html lang="es">"#));
    assert!(spanish.contains("Todos los libros") && spanish.contains("Conecta tus apps"));
    let english = get("en-US").await;
    assert!(english.contains(r#"<html lang="en">"#) && english.contains("All books"));
    assert!(
        english.contains("Download the plugin"),
        "the KOReader plugin can be downloaded"
    );
}

#[tokio::test]
async fn reading_apps_download_from_the_panel() {
    let app = app();
    let get = |uri: &'static str| {
        let app = app.app.clone();
        async move {
            let response = app
                .oneshot(
                    Request::get(uri)
                        .header(header::AUTHORIZATION, basic())
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            let status = response.status();
            let headers = response.headers().clone();
            let body = response.into_body().collect().await.unwrap().to_bytes();
            (status, headers, body)
        }
    };
    let (status, headers, body) = get("/apps/opdshelf.koplugin.zip").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CONTENT_TYPE], "application/zip");
    assert!(body.starts_with(b"PK"));

    // The APK is only there once it is copied to DATA_DIR/apps
    assert_eq!(get("/apps/opdshelf.apk").await.0, StatusCode::NOT_FOUND);
    let sync = String::from_utf8(get("/admin/sync").await.2.to_vec()).unwrap();
    assert!(sync.contains("/apps/opdshelf.koplugin.zip") && !sync.contains(r#"href="/apps/opdshelf.apk""#));
    let apps = app.dir.path().join("books/.opdshelf/apps");
    std::fs::create_dir_all(&apps).unwrap();
    std::fs::write(apps.join("opdshelf.apk"), b"PK apk").unwrap();
    let (status, headers, body) = get("/apps/opdshelf.apk").await;
    assert_eq!(status, StatusCode::OK);
    assert!(headers[header::CONTENT_DISPOSITION]
        .to_str()
        .unwrap()
        .contains("opdshelf.apk"));
    assert_eq!(&body[..], b"PK apk");
    let sync = String::from_utf8(get("/admin/sync").await.2.to_vec()).unwrap();
    assert!(sync.contains(r#"href="/apps/opdshelf.apk""#));
}
