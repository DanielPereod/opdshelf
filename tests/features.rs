//! Metadata editing, ISBN lookup, covers, thumbnails and the web reader.

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::routing::get;
use axum::{Json, Router};
use http_body_util::BodyExt;
use serde_json::json;
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use tower::ServiceExt;

use opdshelf::{router, Config};

struct TestApp {
    app: Router,
    dir: tempfile::TempDir,
}

type Res = (StatusCode, HashMap<String, String>, Vec<u8>);

fn app_with(vars: &[(&str, &str)]) -> TestApp {
    let dir = tempfile::tempdir().unwrap();
    let mut env: HashMap<String, String> = vars.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    env.insert(
        "BOOKS_DIR".into(),
        dir.path().join("books").to_string_lossy().into_owned(),
    );
    env.insert("WATCH".into(), "false".into());
    std::fs::create_dir_all(dir.path().join("books")).unwrap();
    TestApp {
        app: router(Config::from_lookup(|k| env.get(k).cloned())),
        dir,
    }
}

impl TestApp {
    fn books(&self) -> PathBuf {
        self.dir.path().join("books")
    }

    async fn send(&self, request: Request<Body>) -> Res {
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

    async fn get(&self, uri: &str) -> Res {
        self.send(Request::get(uri).body(Body::empty()).unwrap()).await
    }

    async fn text(&self, uri: &str) -> String {
        let (status, _, body) = self.get(uri).await;
        assert_eq!(status, StatusCode::OK, "GET {uri}");
        String::from_utf8(body).unwrap()
    }

    async fn csrf(&self) -> String {
        let html = self.text("/admin").await;
        let start = html.find(r#"name="csrf" value=""#).unwrap() + 19;
        html[start..start + 64].to_string()
    }

    async fn multipart(&self, uri: &str, fields: &[(&str, &str)], file: Option<(&str, &[u8])>) -> Res {
        let boundary = "XBOUNDARYX";
        let mut body = Vec::new();
        for (name, value) in fields {
            write!(
                body,
                "--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"
            )
            .unwrap();
        }
        if let Some((name, data)) = file {
            write!(body, "--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"; filename=\"c.png\"\r\nContent-Type: image/png\r\n\r\n").unwrap();
            body.extend_from_slice(data);
            body.extend_from_slice(b"\r\n");
        }
        write!(body, "--{boundary}--\r\n").unwrap();
        self.send(
            Request::post(uri)
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
    }

    async fn post_form(&self, uri: &str, form: &str) -> Res {
        self.send(
            Request::post(uri)
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(form.to_string()))
                .unwrap(),
        )
        .await
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

fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    for (name, data) in entries {
        zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(data).unwrap();
    }
    zip.finish().unwrap();
}

fn write_epub(path: &Path, cover: &[u8]) {
    let opf = r#"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" xmlns:dc="http://purl.org/dc/elements/1.1/"><metadata><dc:title>Original</dc:title><dc:creator>Autor Original</dc:creator><dc:identifier>978-84-376-0494-7</dc:identifier><meta name="cover" content="c"/></metadata><manifest><item id="c" href="cover.png" media-type="image/png"/></manifest></package>"#;
    write_zip(
        path,
        &[
            ("mimetype", b"application/epub+zip"),
            (
                "META-INF/container.xml",
                br#"<container><rootfiles><rootfile full-path="content.opf"/></rootfiles></container>"#,
            ),
            ("content.opf", opf.as_bytes()),
            ("cover.png", cover),
        ],
    );
}

#[tokio::test]
async fn thumbnails_are_small_jpegs() {
    let t = app_with(&[]);
    write_epub(&t.books().join("b.epub"), &png(900, 1350));
    let (status, headers, body) = t.get("/thumb/b.epub").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["content-type"], "image/jpeg");
    let thumb = image::load_from_memory(&body).unwrap();
    assert_eq!((thumb.width(), thumb.height()), (320, 480));
    // Cached on disk under DATA_DIR
    let cached = std::fs::read_dir(t.books().join(".opdshelf/cache/thumbs"))
        .unwrap()
        .count();
    assert_eq!(cached, 1);
    // The cache folder is hidden, so it is not listed as books
    assert!(!t.text("/opds/books").await.contains(".opdshelf"));
}

#[tokio::test]
async fn pdf_covers_when_poppler_is_installed() {
    if std::process::Command::new("pdftoppm").arg("-v").output().is_err() {
        eprintln!("poppler not installed, skipping");
        return;
    }
    let t = app_with(&[]);
    std::fs::write(
        t.books().join("doc.pdf"),
        "%PDF-1.1\n1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj 2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj 3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 200 300]>>endobj 4 0 obj<</Title(Informe)>>endobj\ntrailer<</Root 1 0 R/Info 4 0 R>>\n",
    )
    .unwrap();
    let feed = t.text("/opds/books").await;
    assert!(feed.contains("<title>Informe</title>"));
    assert!(feed.contains("/cover/doc.pdf?v="));
    let (status, headers, body) = t.get("/cover/doc.pdf").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["content-type"], "image/jpeg");
    assert!(body.starts_with(&[0xff, 0xd8]));
}

#[tokio::test]
async fn edit_metadata_and_cover() {
    let t = app_with(&[]);
    write_epub(&t.books().join("b.epub"), &png(10, 15));
    let csrf = t.csrf().await;

    let page = t.text("/admin/edit/b.epub").await;
    assert!(page.contains(r#"value="Original""#));
    assert!(
        page.contains(r#"value="9788437604947""#),
        "ISBN prefilled from the identifier"
    );

    let fields = [
        ("csrf", csrf.as_str()),
        ("path", "b.epub"),
        ("title", "Título nuevo"),
        ("authors", "Ana; Luis"),
        ("description", "<b>Hola</b>"),
        ("language", "es"),
        ("publisher", ""),
        ("subjects", "Novela, Clásicos"),
        ("date", ""),
        ("identifier", "978-84-376-0494-7"),
    ];
    let (status, _, _) = t
        .multipart("/admin/edit", &[("csrf", "bad"), ("path", "b.epub")], None)
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let new_cover = png(40, 60);
    let (status, headers, _) = t.multipart("/admin/edit", &fields, Some(("cover", &new_cover))).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert!(headers["location"].contains("notice=Saved"));

    // The book file is untouched; the edits live in DATA_DIR
    let saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(t.books().join(".opdshelf/metadata.json")).unwrap()).unwrap();
    assert_eq!(saved["b.epub"]["title"], "Título nuevo");
    assert!(
        saved["b.epub"].get("identifier").is_none(),
        "unchanged fields are not stored"
    );

    let feed = t.text("/opds/books").await;
    assert!(feed.contains("<title>Título nuevo</title>"));
    assert!(feed.contains("<name>Ana</name><") && feed.contains("<name>Luis</name>"));
    assert!(feed.contains(r#"term="Novela""#));
    let info = t.text("/info/b.epub").await;
    assert!(info.contains("&lt;b&gt;Hola&lt;/b&gt;"));
    let (_, headers, body) = t.get("/cover/b.epub").await;
    assert_eq!(headers["content-type"], "image/png");
    assert_eq!(body, new_cover);

    // Renaming keeps the edits
    t.post_form(
        "/admin/rename",
        &format!("csrf={csrf}&path=b.epub&name=renamed&folder=Sub"),
    )
    .await;
    assert!(t.text("/opds/books").await.contains("<title>Título nuevo</title>"));

    // Reset goes back to the file's metadata and removes the uploaded cover
    t.post_form("/admin/reset", &format!("csrf={csrf}&path=Sub%2Frenamed.epub"))
        .await;
    let feed = t.text("/opds/books").await;
    assert!(feed.contains("<title>Original</title>"));
    assert_eq!(
        std::fs::read_dir(t.books().join(".opdshelf/covers")).unwrap().count(),
        0
    );
}

async fn mock_catalog(open_library_has_book: bool) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let cover_url = format!("{base}/b/cover.png");
    let app = Router::new()
        .route(
            "/api/books",
            get(move || async move {
                if open_library_has_book {
                    Json(json!({ "ISBN:9788437604947": {
                        "title": "Cien años de soledad",
                        "authors": [{ "name": "Gabriel García Márquez" }],
                        "publishers": [{ "name": "Cátedra" }],
                        "publish_date": "2007",
                        "subjects": [{ "name": "Novela" }],
                        "cover": { "large": cover_url }
                    }}))
                } else {
                    Json(json!({}))
                }
            }),
        )
        .route(
            "/isbn/9788437604947.json",
            get(|| async { Json(json!({ "description": "Macondo." })) }),
        )
        .route(
            "/books/v1/volumes",
            get(|| async {
                Json(json!({ "items": [{ "volumeInfo": {
                    "title": "Desde Google", "authors": ["Autora"], "language": "es",
                    "description": "<p>Texto</p>", "categories": ["Fiction"]
                }}]}))
            }),
        )
        .route("/b/cover.png", get(|| async { png(30, 45) }));
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    base
}

#[tokio::test]
async fn isbn_lookup_and_cover_download() {
    let base = mock_catalog(true).await;
    let t = app_with(&[("OPENLIBRARY_URL", &base), ("GOOGLE_BOOKS_URL", &base)]);
    write_epub(&t.books().join("b.epub"), &png(10, 15));

    let (status, _, body) = t.get("/admin/lookup?isbn=978-84-376-0494-7").await;
    assert_eq!(status, StatusCode::OK);
    let found: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(found["title"], "Cien años de soledad");
    assert_eq!(found["authors"][0], "Gabriel García Márquez");
    assert_eq!(found["description"], "Macondo.");
    assert_eq!(found["source"], "Open Library");

    let (status, _, _) = t.get("/admin/lookup?isbn=123").await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);

    let csrf = t.csrf().await;
    let cover_url = found["coverUrl"].as_str().unwrap();
    let (_, headers, _) = t
        .multipart(
            "/admin/edit",
            &[("csrf", &csrf), ("path", "b.epub"), ("cover_url", cover_url)],
            None,
        )
        .await;
    assert!(headers["location"].contains("notice=Saved"));
    let (_, _, body) = t.get("/cover/b.epub").await;
    assert_eq!(image::load_from_memory(&body).unwrap().width(), 30);

    // Covers can only come from the catalogs
    let (_, headers, _) = t
        .multipart(
            "/admin/edit",
            &[
                ("csrf", &csrf),
                ("path", "b.epub"),
                ("cover_url", "http://127.0.0.2:1/x.png"),
            ],
            None,
        )
        .await;
    assert!(headers["location"].contains("error="));
}

#[tokio::test]
async fn isbn_lookup_falls_back_to_google_books() {
    let base = mock_catalog(false).await;
    let t = app_with(&[("OPENLIBRARY_URL", &base), ("GOOGLE_BOOKS_URL", &base)]);
    let (status, _, body) = t.get("/admin/lookup?isbn=9788437604947").await;
    assert_eq!(status, StatusCode::OK);
    let found: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(found["source"], "Google Books");
    assert_eq!(found["title"], "Desde Google");
    assert_eq!(found["description"], "Texto");
}

#[tokio::test]
async fn web_reader() {
    let t = app_with(&[]);
    write_epub(&t.books().join("b.epub"), &png(10, 15));
    write_zip(
        &t.books().join("c.cbz"),
        &[("p10.png", &png(2, 2)), ("p2.png", &png(3, 3)), ("notes.txt", b"x")],
    );
    std::fs::write(t.books().join("t.txt"), "Hola <mundo>\nlínea dos").unwrap();
    std::fs::write(t.books().join("m.mobi"), "x").unwrap();

    assert!(t
        .text("/read/b.epub")
        .await
        .contains("/static/vendor/epubjs/epub.min.js"));
    let text = t.text("/read/t.txt").await;
    assert!(text.contains("Hola &lt;mundo&gt;") && text.contains("línea dos"));
    assert!(t.text("/read/c.cbz").await.contains(r#"data-path="c.cbz""#));
    let (status, _, _) = t.get("/read/m.mobi").await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (_, _, body) = t.get("/pages/c.cbz").await;
    assert_eq!(serde_json::from_slice::<serde_json::Value>(&body).unwrap()["count"], 2);
    let (status, headers, body) = t.get("/page/c.cbz?n=0").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["content-type"], "image/png");
    assert_eq!(
        image::load_from_memory(&body).unwrap().width(),
        3,
        "pages are in natural order"
    );
    let (status, _, _) = t.get("/page/c.cbz?n=2").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = t.get("/pages/b.epub").await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // The reader loads the file inline
    let (_, headers, _) = t.get("/download/b.epub?inline=1").await;
    assert!(headers["content-disposition"].starts_with("inline"));
    let admin = t.text("/admin").await;
    assert!(admin.contains("/read/b.epub") && !admin.contains("/read/m.mobi"));
}

#[tokio::test]
async fn bulk_move_and_delete() {
    let t = app_with(&[]);
    for name in ["a.txt", "b.txt", "c.txt"] {
        std::fs::write(t.books().join(name), "x").unwrap();
    }
    let csrf = t.csrf().await;

    let (status, _, _) = t.post_form("/admin/bulk", "csrf=bad&action=delete&path=a.txt").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(t.books().join("a.txt").exists());

    // The form repeats `path` for every selected book
    let (_, headers, _) = t
        .post_form(
            "/admin/bulk",
            &format!("csrf={csrf}&action=move&folder=Pila&path=a.txt&path=b.txt"),
        )
        .await;
    assert!(headers["location"].contains("notice"));
    assert!(t.books().join("Pila/a.txt").exists() && t.books().join("Pila/b.txt").exists());

    // A path outside the library is skipped and reported
    let (_, headers, _) = t
        .post_form(
            "/admin/bulk",
            &format!("csrf={csrf}&action=delete&path=Pila%2Fa.txt&path=..%2Fsecret.txt"),
        )
        .await;
    assert!(headers["location"].contains("error"));
    assert!(!t.books().join("Pila/a.txt").exists());
    assert!(t.books().join("c.txt").exists());

    let (_, headers, _) = t
        .post_form(
            "/admin/bulk",
            &format!("csrf={csrf}&action=move&folder=..%2Fout&path=c.txt"),
        )
        .await;
    assert!(headers["location"].contains("error"));
    assert!(t.books().join("c.txt").exists());
}

#[tokio::test]
async fn inspector_panel_is_a_fragment() {
    let t = app_with(&[]);
    std::fs::write(t.books().join("a.txt"), "x").unwrap();
    let full = t.text("/info/a.txt").await;
    assert!(full.contains("<html") && full.contains(r#"href="/info/a.txt""#));
    let panel = t.text("/info/a.txt?panel=1").await;
    assert!(!panel.contains("<html") && panel.contains("/download/a.txt"));
    let edit = t.text("/admin/edit/a.txt?panel=1").await;
    assert!(!edit.contains("<html") && edit.contains(r#"name="csrf" value=""#));
}
