//! Renaming, moving and deleting folders, for the panel and the apps: the
//! books inside take their edits, covers and read marks along.

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use tower::ServiceExt;

use opdshelf::{router, Config};

struct TestApp {
    app: Router,
    dir: tempfile::TempDir,
}

fn app_with(watch: bool) -> TestApp {
    let dir = tempfile::tempdir().unwrap();
    let books = dir.path().join("books");
    std::fs::create_dir_all(&books).unwrap();
    let env: HashMap<String, String> = [
        ("BOOKS_DIR".to_string(), books.to_string_lossy().into_owned()),
        ("WATCH".to_string(), watch.to_string()),
    ]
    .into();
    TestApp {
        app: router(Config::from_lookup(|k| env.get(k).cloned())),
        dir,
    }
}

impl TestApp {
    fn books(&self) -> PathBuf {
        self.dir.path().join("books")
    }

    fn write(&self, rel: &str, data: &[u8]) {
        let path = self.books().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, data).unwrap();
    }

    async fn send(&self, request: Request<Body>) -> (StatusCode, HashMap<String, String>, Vec<u8>) {
        let response = self.app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let headers = response
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();
        let body = response.into_body().collect().await.unwrap().to_bytes().to_vec();
        (status, headers, body)
    }

    async fn get_json(&self, uri: &str) -> Value {
        let (status, _, body) = self.send(Request::get(uri).body(Body::empty()).unwrap()).await;
        assert_eq!(status, StatusCode::OK, "GET {uri}");
        serde_json::from_slice(&body).unwrap()
    }

    async fn text(&self, uri: &str) -> String {
        let (status, _, body) = self.send(Request::get(uri).body(Body::empty()).unwrap()).await;
        assert_eq!(status, StatusCode::OK, "GET {uri}");
        String::from_utf8(body).unwrap()
    }

    async fn json(&self, method: &str, uri: &str, body: Value) -> (StatusCode, Value) {
        let (status, _, body) = self
            .send(
                Request::builder()
                    .method(method)
                    .uri(uri)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await;
        (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
    }

    async fn post(&self, uri: &str, body: Value) -> (StatusCode, Value) {
        self.json("POST", uri, body).await
    }

    async fn csrf(&self) -> String {
        let html = self.text("/admin").await;
        let start = html.find(r#"name="csrf" value=""#).unwrap() + 19;
        html[start..start + 64].to_string()
    }

    async fn post_form(&self, uri: &str, form: &str) -> HashMap<String, String> {
        let (_, headers, _) = self
            .send(
                Request::post(uri)
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(form.to_string()))
                    .unwrap(),
            )
            .await;
        headers
    }

    async fn folders(&self) -> Vec<String> {
        self.get_json("/api/folders").await["folders"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["path"].as_str().unwrap().to_string())
            .collect()
    }

    async fn book_paths(&self) -> Vec<String> {
        let feed = self.text("/admin").await;
        let mut found: Vec<String> = feed
            .match_indices(r#""path":""#)
            .map(|(at, _)| {
                let rest = &feed[at + 8..];
                rest[..rest.find('"').unwrap()].to_string()
            })
            .collect();
        found.sort();
        found
    }
}

fn png(width: u32, height: u32) -> Vec<u8> {
    let image = image::RgbImage::from_pixel(width, height, image::Rgb([10, 120, 200]));
    let mut out = Vec::new();
    image::DynamicImage::ImageRgb8(image)
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .unwrap();
    out
}

fn write_epub(path: &Path, title: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let opf = format!(
        r#"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" xmlns:dc="http://purl.org/dc/elements/1.1/"><metadata><dc:title>{title}</dc:title><dc:creator>Autor</dc:creator></metadata></package>"#
    );
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    for (name, data) in [
        ("mimetype", b"application/epub+zip".to_vec()),
        (
            "META-INF/container.xml",
            br#"<container><rootfiles><rootfile full-path="content.opf"/></rootfiles></container>"#.to_vec(),
        ),
        ("content.opf", opf.into_bytes()),
    ] {
        zip.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(&data).unwrap();
    }
    zip.finish().unwrap();
}

/// Edits a book's title and gives it a custom cover, as the panel does.
async fn edit(t: &TestApp, path: &str, title: &str) {
    let csrf = t.csrf().await;
    let boundary = "XBOUNDARYX";
    let mut body = Vec::new();
    for (name, value) in [("csrf", csrf.as_str()), ("path", path), ("title", title)] {
        write!(
            body,
            "--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"
        )
        .unwrap();
    }
    write!(body, "--{boundary}\r\nContent-Disposition: form-data; name=\"cover\"; filename=\"c.png\"\r\nContent-Type: image/png\r\n\r\n").unwrap();
    body.extend_from_slice(&png(40, 60));
    write!(body, "\r\n--{boundary}--\r\n").unwrap();
    let (status, _, _) = t
        .send(
            Request::post("/admin/edit")
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
}

/// The read marks of the library, by book path.
async fn marks(t: &TestApp) -> HashMap<String, String> {
    t.get_json("/api/progress").await["marks"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|m| Some((m["path"].as_str()?.to_string(), m["state"].as_str()?.to_string())))
        .collect()
}

#[tokio::test]
async fn renaming_a_folder_keeps_the_data_of_every_book_inside() {
    // With the watcher on, the library is cached between requests: a move must show at once
    let t = app_with(true);
    write_epub(&t.books().join("Saga/Vol 1.epub"), "Uno");
    write_epub(&t.books().join("Saga/Extra/Vol 2.epub"), "Dos");
    write_epub(&t.books().join("Sagas/Other.epub"), "Otro");
    edit(&t, "Saga/Extra/Vol 2.epub", "Segundo tomo").await;
    let (status, _) = t
        .json(
            "PUT",
            "/api/marks",
            json!({ "paths": ["Saga/Vol 1.epub"], "state": "read" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (_, _, cover) = t
        .send(
            Request::get("/cover/Saga/Extra/Vol%202.epub")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(cover, png(40, 60));

    let (status, body) = t
        .post(
            "/api/folders/rename",
            json!({ "path": "Saga", "name": "Saga completa" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body, json!({ "path": "Saga completa", "from": "Saga" }));
    assert!(t.books().join("Saga completa/Extra/Vol 2.epub").is_file());
    assert!(!t.books().join("Saga").exists());
    assert_eq!(t.folders().await, ["Saga completa", "Saga completa/Extra", "Sagas"]);
    assert_eq!(
        t.book_paths().await,
        [
            "Saga completa/Extra/Vol 2.epub",
            "Saga completa/Vol 1.epub",
            "Sagas/Other.epub"
        ]
    );

    // The edits and the custom cover of a book in a subfolder came along
    let info = t.text("/info/Saga%20completa/Extra/Vol%202.epub").await;
    assert!(info.contains("Segundo tomo"));
    let (status, _, cover) = t
        .send(
            Request::get("/cover/Saga%20completa/Extra/Vol%202.epub")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cover, png(40, 60));
    let saved: Value =
        serde_json::from_slice(&std::fs::read(t.books().join(".opdshelf/metadata.json")).unwrap()).unwrap();
    assert!(saved.get("Saga/Extra/Vol 2.epub").is_none());
    assert_eq!(saved["Saga completa/Extra/Vol 2.epub"]["title"], "Segundo tomo");
    // A folder whose name only starts the same is left alone
    assert!(t.books().join("Sagas/Other.epub").is_file());

    // The read mark is kept by the book's content, so it follows it
    assert_eq!(
        marks(&t).await,
        HashMap::from([("Saga completa/Vol 1.epub".to_string(), "read".to_string())])
    );
    // The ids of the books moved with them instead of being read again
    let ids = std::fs::read_to_string(t.books().join(".opdshelf/document-ids.json")).unwrap();
    assert!(ids.contains("Saga completa/Vol 1.epub") && !ids.contains("\"Saga/Vol 1.epub\""));
}

#[tokio::test]
async fn moving_folders_into_others_and_back_to_the_root() {
    let t = app_with(false);
    write_epub(&t.books().join("Comics/Tintin/T1.epub"), "T1");
    write_epub(&t.books().join("Archive/old.epub"), "Old");
    edit(&t, "Comics/Tintin/T1.epub", "Tintín en el Tíbet").await;
    let (status, _) = t
        .json(
            "PUT",
            "/api/marks",
            json!({ "paths": ["Comics/Tintin/T1.epub"], "state": "unread" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // Into another folder, keeping its name
    let (status, body) = t
        .post(
            "/api/folders/move",
            json!({ "path": "Comics/Tintin", "parent": "Archive" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["path"], "Archive/Tintin");
    assert!(t.books().join("Archive/Tintin/T1.epub").is_file());
    assert!(t
        .text("/info/Archive/Tintin/T1.epub")
        .await
        .contains("Tintín en el Tíbet"));
    assert_eq!(marks(&t).await["Archive/Tintin/T1.epub"], "unread");

    // To the root, under another name
    let (status, body) = t
        .post(
            "/api/folders/move",
            json!({ "path": "Archive/Tintin", "parent": "", "name": "Tintín" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["path"], "Tintín");
    assert!(t.books().join("Tintín/T1.epub").is_file());
    assert!(t.text("/info/Tint%C3%ADn/T1.epub").await.contains("Tintín en el Tíbet"));
    assert_eq!(t.folders().await, ["Archive", "Comics", "Tintín"]);

    // A whole tree into a folder that was its sibling
    let (status, _) = t
        .post("/api/folders/move", json!({ "path": "Archive", "parent": "Comics" }))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(t.books().join("Comics/Archive/old.epub").is_file());

    // The same place answers the same path and changes nothing
    let (status, body) = t
        .post(
            "/api/folders/move",
            json!({ "path": "Comics/Archive", "parent": "Comics" }),
        )
        .await;
    assert_eq!(
        (status, body["path"].as_str()),
        (StatusCode::OK, Some("Comics/Archive"))
    );
}

#[tokio::test]
async fn folder_moves_that_cannot_be_done_are_refused() {
    let t = app_with(false);
    write_epub(&t.books().join("A/B/b.epub"), "B");
    write_epub(&t.books().join("C/A/c.epub"), "C");
    t.write("outside.epub", b"x");
    std::fs::create_dir_all(t.books().join("D")).unwrap();
    let before = t.book_paths().await;

    let refused = [
        // A folder of that name is already there
        (json!({ "path": "A", "parent": "C" }), StatusCode::CONFLICT, "exists"),
        (
            json!({ "path": "D", "parent": "", "name": "C" }),
            StatusCode::CONFLICT,
            "exists",
        ),
        // Into itself or one of its subfolders
        (
            json!({ "path": "A", "parent": "A" }),
            StatusCode::BAD_REQUEST,
            "into_itself",
        ),
        (
            json!({ "path": "A", "parent": "A/B" }),
            StatusCode::BAD_REQUEST,
            "into_itself",
        ),
        // Out of the library, or from outside it
        (
            json!({ "path": "A", "parent": ".." }),
            StatusCode::NOT_FOUND,
            "not_found",
        ),
        (
            json!({ "path": "A", "parent": "../../tmp" }),
            StatusCode::NOT_FOUND,
            "not_found",
        ),
        (
            json!({ "path": "../books/A", "parent": "C" }),
            StatusCode::NOT_FOUND,
            "not_found",
        ),
        (
            json!({ "path": "A", "parent": "", "name": "../escaped" }),
            StatusCode::BAD_REQUEST,
            "invalid_name",
        ),
        (
            json!({ "path": "A", "parent": "", "name": "x/y" }),
            StatusCode::BAD_REQUEST,
            "invalid_name",
        ),
        (
            json!({ "path": "A", "parent": "", "name": ".hidden" }),
            StatusCode::BAD_REQUEST,
            "invalid_name",
        ),
        (
            json!({ "path": "A", "parent": "", "name": " " }),
            StatusCode::BAD_REQUEST,
            "invalid_name",
        ),
        // The root itself, and folders that do not exist
        (json!({ "path": "", "parent": "C" }), StatusCode::NOT_FOUND, "not_found"),
        (
            json!({ "path": "Missing", "parent": "C" }),
            StatusCode::NOT_FOUND,
            "not_found",
        ),
        (
            json!({ "path": "A", "parent": "Missing" }),
            StatusCode::NOT_FOUND,
            "not_found",
        ),
        // A book is not a folder
        (
            json!({ "path": "outside.epub", "parent": "C" }),
            StatusCode::NOT_FOUND,
            "not_found",
        ),
    ];
    for (request, status, code) in refused {
        let (got, body) = t.post("/api/folders/move", request.clone()).await;
        assert_eq!((got, body["code"].as_str()), (status, Some(code)), "{request}: {body}");
        assert!(body["error"].is_string());
    }
    let (status, body) = t
        .post("/api/folders/rename", json!({ "path": "A", "name": "../C" }))
        .await;
    assert_eq!(
        (status, body["code"].as_str()),
        (StatusCode::BAD_REQUEST, Some("invalid_name"))
    );
    let (status, body) = t.post("/api/folders/rename", json!({ "path": "A", "name": "C" })).await;
    assert_eq!((status, body["code"].as_str()), (StatusCode::CONFLICT, Some("exists")));

    // Nothing moved, inside the library or out of it
    assert_eq!(t.book_paths().await, before);
    assert!(t.books().join("A/B/b.epub").is_file() && t.books().join("D").is_dir());
    assert!(!t.dir.path().join("escaped").exists());
    assert!(!t.dir.path().join("A").exists());
}

#[tokio::test]
async fn only_folders_without_files_are_deleted() {
    let t = app_with(false);
    write_epub(&t.books().join("Full/Sub/a.epub"), "A");
    std::fs::create_dir_all(t.books().join("Empty/Nested/Deeper")).unwrap();
    t.write("Other/notes.opf", b"not a book, still a file");

    let (status, body) = t.post("/api/folders/delete", json!({ "path": "Full" })).await;
    assert_eq!(
        (status, body["code"].as_str()),
        (StatusCode::CONFLICT, Some("not_empty"))
    );
    assert!(t.books().join("Full/Sub/a.epub").is_file());
    let (status, _) = t.post("/api/folders/delete", json!({ "path": "Other" })).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(t.books().join("Other/notes.opf").is_file());
    for path in ["", "..", "../books", "Missing"] {
        let (status, _) = t.post("/api/folders/delete", json!({ "path": path })).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
    }
    assert!(t.books().is_dir());

    // A folder with only empty folders in it goes, subfolders and all
    let (status, body) = t.post("/api/folders/delete", json!({ "path": "Empty" })).await;
    assert_eq!((status, &body), (StatusCode::OK, &json!({ "deleted": "Empty" })));
    assert!(!t.books().join("Empty").exists());
    assert_eq!(t.folders().await, ["Full", "Full/Sub", "Other"]);
}

#[tokio::test]
async fn the_panel_forms_rename_and_move_folders() {
    let t = app_with(false);
    write_epub(&t.books().join("Old/a.epub"), "A");
    std::fs::create_dir_all(t.books().join("Target")).unwrap();
    let csrf = t.csrf().await;

    let headers = t
        .post_form("/admin/folder", "csrf=bad&action=rename&path=Old&name=New")
        .await;
    assert!(!headers.contains_key("location"));
    assert!(t.books().join("Old").is_dir());

    let headers = t
        .post_form("/admin/folder", &format!("csrf={csrf}&action=rename&path=Old&name=New"))
        .await;
    assert!(
        headers["location"].starts_with("/admin?folder=New&notice="),
        "{headers:?}"
    );
    let headers = t
        .post_form(
            "/admin/folder",
            &format!("csrf={csrf}&action=move&path=New&parent=Target"),
        )
        .await;
    assert!(
        headers["location"].starts_with("/admin?folder=Target%2FNew&notice=")
            || headers["location"].starts_with("/admin?folder=Target/New&notice="),
        "{headers:?}"
    );
    assert!(t.books().join("Target/New/a.epub").is_file());
    let headers = t
        .post_form(
            "/admin/folder",
            &format!("csrf={csrf}&action=move&path=Target&parent=Target%2FNew"),
        )
        .await;
    assert!(headers["location"].contains("error="), "{headers:?}");
    assert!(t.books().join("Target/New/a.epub").is_file());
}

#[tokio::test]
async fn the_endpoints_of_the_android_app_still_work() {
    let t = app_with(false);
    write_epub(&t.books().join("a.epub"), "A");
    write_epub(&t.books().join("b.epub"), "B");

    let (status, body) = t.post("/api/folders", json!({ "name": "Sport" })).await;
    assert_eq!((status, &body), (StatusCode::CREATED, &json!({ "path": "Sport" })));
    let (status, body) = t
        .post(
            "/api/books/move",
            json!({ "paths": ["a.epub", "b.epub", "missing.epub"], "folder": "Sport" }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["moved"], 2);
    assert_eq!(body["folder"], "Sport");
    assert_eq!(body["failed"].as_array().unwrap().len(), 1);
    // Where each book went, for the panel's undo
    assert_eq!(
        body["paths"],
        json!([{ "from": "a.epub", "to": "Sport/a.epub" }, { "from": "b.epub", "to": "Sport/b.epub" }])
    );
    let list = t.get_json("/api/folders").await;
    assert_eq!(
        list,
        json!({ "folders": [{ "path": "Sport", "name": "Sport", "count": 2 }] })
    );

    // A folder renamed through the API shows up with its books in the list
    t.post("/api/folders/rename", json!({ "path": "Sport", "name": "Deporte" }))
        .await;
    let list = t.get_json("/api/folders").await;
    assert_eq!(
        list,
        json!({ "folders": [{ "path": "Deporte", "name": "Deporte", "count": 2 }] })
    );
    let (status, body) = t
        .post("/api/books/move", json!({ "paths": ["Deporte/a.epub"], "folder": "" }))
        .await;
    assert_eq!(
        (status, &body["paths"]),
        (StatusCode::OK, &json!([{ "from": "Deporte/a.epub", "to": "a.epub" }]))
    );
}

#[tokio::test]
async fn renaming_a_book_through_the_api() {
    let t = app_with(false);
    write_epub(&t.books().join("a.epub"), "A");
    write_epub(&t.books().join("Sub/taken.epub"), "B");
    edit(&t, "a.epub", "Editado").await;

    let (status, body) = t
        .post(
            "/api/books/rename",
            json!({ "path": "a.epub", "name": "nuevo", "folder": "Sub" }),
        )
        .await;
    assert_eq!(
        (status, &body),
        (StatusCode::OK, &json!({ "path": "Sub/nuevo.epub", "from": "a.epub" }))
    );
    assert!(t.text("/info/Sub/nuevo.epub").await.contains("Editado"));
    let (status, body) = t
        .post(
            "/api/books/rename",
            json!({ "path": "Sub/nuevo.epub", "name": "taken" }),
        )
        .await;
    assert_eq!((status, body["code"].as_str()), (StatusCode::CONFLICT, Some("exists")));
    let (status, _) = t
        .post(
            "/api/books/rename",
            json!({ "path": "Sub/nuevo.epub", "name": "x", "folder": "../out" }),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = t
        .post("/api/books/rename", json!({ "path": "../a.epub", "name": "x" }))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(t.books().join("Sub/nuevo.epub").is_file());
}
