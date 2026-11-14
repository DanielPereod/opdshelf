//! The maintenance page: duplicates and data of books that are gone.

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use std::collections::HashMap;
use tower::ServiceExt;

use opdshelf::{router, Config};

async fn send(app: &axum::Router, request: Request<Body>) -> (StatusCode, String) {
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&body).into_owned())
}

async fn page(app: &axum::Router) -> String {
    let (status, html) = send(app, Request::get("/admin/maintenance").body(Body::empty()).unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    html
}

async fn post(app: &axum::Router, uri: &str, form: String) -> StatusCode {
    let request = Request::post(uri)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(form))
        .unwrap();
    send(app, request).await.0
}

fn csrf(html: &str) -> String {
    let start = html.find(r#"name="csrf" value=""#).unwrap() + 19;
    html[start..start + 64].to_string()
}

#[tokio::test]
async fn finds_duplicates_and_cleans_data_of_gone_books() {
    let dir = tempfile::tempdir().unwrap();
    let books = dir.path().join("books");
    let data = dir.path().join("data");
    std::fs::create_dir_all(books.join("copy")).unwrap();
    std::fs::create_dir_all(data.join("covers")).unwrap();
    std::fs::write(books.join("story.txt"), b"Once upon a time").unwrap();
    std::fs::write(books.join("copy/story.txt"), b"Once upon a time").unwrap();
    std::fs::write(books.join("other.txt"), b"Something else").unwrap();
    // Saved data of a book that is no longer there
    std::fs::write(
        data.join("metadata.json"),
        r#"{"gone.epub": {"title": "Gone", "cover": "old.png"}, "other.txt": {"title": "Kept"}}"#,
    )
    .unwrap();
    std::fs::write(data.join("covers/old.png"), b"x").unwrap();
    let record = r#"{"progress":"1","percentage":0.5,"device":"d","device_id":"d","timestamp":1}"#;
    std::fs::write(
        data.join("progress.json"),
        format!(r#"{{"0123456789abcdef0123456789abcdef":[{record}]}}"#),
    )
    .unwrap();
    std::fs::write(
        data.join("marks.json"),
        r#"{"0123456789abcdef0123456789abcdef":{"state":"read","at":1}}"#,
    )
    .unwrap();

    let env: HashMap<String, String> = [
        ("BOOKS_DIR", books.to_string_lossy().into_owned()),
        ("DATA_DIR", data.to_string_lossy().into_owned()),
        ("WATCH", "false".into()),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect();
    let app = router(Config::from_lookup(|k| env.get(k).cloned()));

    let html = page(&app).await;
    assert!(html.contains("copy/story.txt"), "the copy is listed");
    assert!(html.contains("Same file"));
    assert!(!html.contains(">other.txt<"));
    let token = csrf(&html);

    // Not duplicates: the group is hidden
    let key_at = html.find(r#"name="key" value=""#).unwrap() + 18;
    let key = &html[key_at..key_at + html[key_at..].find('"').unwrap()];
    let key = key.replace("&#x0A;", "\n").replace("&#10;", "\n");
    let form = format!(
        "csrf={token}&key={}",
        form_urlencoded::byte_serialize(key.as_bytes()).collect::<String>()
    );
    assert_eq!(
        post(&app, "/admin/maintenance/dismiss", form).await,
        StatusCode::SEE_OTHER
    );
    assert!(page(&app).await.contains("No duplicates found"));
    let form = format!("csrf={token}");
    assert_eq!(
        post(&app, "/admin/maintenance/undismiss", form).await,
        StatusCode::SEE_OTHER
    );
    assert!(page(&app).await.contains("copy/story.txt"));

    // Deleting one copy
    let form = format!("csrf={token}&path=copy%2Fstory.txt");
    assert_eq!(
        post(&app, "/admin/maintenance/delete", form).await,
        StatusCode::SEE_OTHER
    );
    assert!(!books.join("copy/story.txt").exists());
    assert!(books.join("story.txt").exists());

    // Cleaning: a bad token or an unknown part changes nothing
    assert_eq!(
        post(&app, "/admin/maintenance/clean", "csrf=bad&what=all".into()).await,
        StatusCode::FORBIDDEN
    );
    let form = format!("csrf={token}&what=progress");
    assert_eq!(
        post(&app, "/admin/maintenance/clean", form).await,
        StatusCode::SEE_OTHER
    );
    let progress = std::fs::read_to_string(data.join("progress.json")).unwrap();
    assert!(!progress.contains("0123456789abcdef"));
    assert!(std::fs::read_to_string(data.join("marks.json"))
        .unwrap()
        .contains("0123456789abcdef"));

    let form = format!("csrf={token}&what=all");
    assert_eq!(
        post(&app, "/admin/maintenance/clean", form).await,
        StatusCode::SEE_OTHER
    );
    let metadata = std::fs::read_to_string(data.join("metadata.json")).unwrap();
    assert!(!metadata.contains("gone.epub"));
    assert!(metadata.contains("other.txt"), "edits of books still there are kept");
    assert!(!data.join("covers/old.png").exists());
    assert!(!std::fs::read_to_string(data.join("marks.json"))
        .unwrap()
        .contains("0123456789abcdef"));
}

#[tokio::test]
async fn an_empty_library_is_never_cleaned() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(data.join("metadata.json"), r#"{"gone.epub": {"title": "Gone"}}"#).unwrap();
    let env: HashMap<String, String> = [
        ("BOOKS_DIR", dir.path().join("books").to_string_lossy().into_owned()),
        ("DATA_DIR", data.to_string_lossy().into_owned()),
        ("WATCH", "false".into()),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect();
    let app = router(Config::from_lookup(|k| env.get(k).cloned()));
    let token = csrf(&page(&app).await);
    post(&app, "/admin/maintenance/clean", format!("csrf={token}&what=all")).await;
    assert!(std::fs::read_to_string(data.join("metadata.json"))
        .unwrap()
        .contains("gone.epub"));
}
