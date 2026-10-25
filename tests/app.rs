use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use base64::Engine;
use http_body_util::BodyExt;
use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use tower::ServiceExt;

use opdshelf::{router, Config};

struct TestApp {
    app: Router,
    dir: tempfile::TempDir,
}

const USER: &str = "admin";
const PASS: &str = "secret";

fn app_with(vars: &[(&str, &str)]) -> TestApp {
    let dir = tempfile::tempdir().unwrap();
    let books = dir.path().join("books");
    std::fs::create_dir_all(&books).unwrap();
    let mut env: HashMap<String, String> = vars.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    env.insert("BOOKS_DIR".into(), books.to_string_lossy().into_owned());
    // Tests write files and query right away, so rescan on every request
    env.entry("WATCH".into()).or_insert_with(|| "false".into());
    TestApp {
        app: router(Config::from_lookup(|k| env.get(k).cloned())),
        dir,
    }
}

fn app() -> TestApp {
    app_with(&[("ADMIN_USERNAME", USER), ("ADMIN_PASSWORD", PASS)])
}

fn basic() -> String {
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{USER}:{PASS}"))
    )
}

impl TestApp {
    fn books(&self) -> std::path::PathBuf {
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

    async fn get(&self, uri: &str) -> (StatusCode, HashMap<String, String>, Vec<u8>) {
        self.send(
            Request::get(uri)
                .header(header::AUTHORIZATION, basic())
                .body(Body::empty())
                .unwrap(),
        )
        .await
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

    async fn post_form(&self, uri: &str, form: &[(&str, &str)]) -> (StatusCode, HashMap<String, String>, Vec<u8>) {
        let body = form
            .iter()
            .map(|(k, v)| {
                format!(
                    "{k}={}",
                    percent_encoding::utf8_percent_encode(v, percent_encoding::NON_ALPHANUMERIC)
                )
            })
            .collect::<Vec<_>>()
            .join("&");
        self.send(
            Request::post(uri)
                .header(header::AUTHORIZATION, basic())
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
    }

    async fn upload(
        &self,
        csrf: &str,
        folder: &str,
        files: &[(&str, &[u8])],
    ) -> (StatusCode, HashMap<String, String>, Vec<u8>) {
        let boundary = "XBOUNDARYX";
        let mut body = Vec::new();
        let mut part = |headers: String, data: &[u8]| {
            write!(body, "--{boundary}\r\n{headers}\r\n\r\n").unwrap();
            body.extend_from_slice(data);
            body.extend_from_slice(b"\r\n");
        };
        part("Content-Disposition: form-data; name=\"csrf\"".into(), csrf.as_bytes());
        part(
            "Content-Disposition: form-data; name=\"folder\"".into(),
            folder.as_bytes(),
        );
        for (name, data) in files {
            part(format!("Content-Disposition: form-data; name=\"books\"; filename=\"{name}\"\r\nContent-Type: application/octet-stream"), data);
        }
        write!(body, "--{boundary}--\r\n").unwrap();
        self.send(
            Request::post("/admin/upload")
                .header(header::AUTHORIZATION, basic())
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
    }
}

fn write_epub(path: &Path, title: &str, author: &str) {
    let opf = format!(
        r#"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" xmlns:dc="http://purl.org/dc/elements/1.1/"><metadata><dc:title>{title}</dc:title><dc:creator>{author}</dc:creator><dc:description>&lt;img src=x onerror=alert(1)&gt;Hola</dc:description><meta name="cover" content="c"/></metadata><manifest><item id="c" href="cover.jpg" media-type="image/jpeg"/></manifest></package>"#
    );
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    for (name, data) in [
        ("mimetype", b"application/epub+zip".as_slice()),
        (
            "META-INF/container.xml",
            br#"<container><rootfiles><rootfile full-path="OPS/content.opf"/></rootfiles></container>"#,
        ),
        ("OPS/content.opf", opf.as_bytes()),
        ("OPS/cover.jpg", b"JPEGDATA"),
    ] {
        zip.start_file(name, options).unwrap();
        zip.write_all(data).unwrap();
    }
    zip.finish().unwrap();
}

#[tokio::test]
async fn requires_auth_and_rejects_forged_cookies() {
    let t = app();
    for cookie in ["auth_session=valid", "opdshelf_session=valid"] {
        let (status, headers, _) = t
            .send(
                Request::get("/")
                    .header(header::COOKIE, cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert!(headers["www-authenticate"].starts_with("Basic"));
    }
    let (status, headers, _) = t
        .send(
            Request::get("/admin")
                .header(header::ACCEPT, "text/html")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(headers["location"], "/user/login");

    // Public routes
    for uri in ["/user/login", "/healthz", "/static/icon.png"] {
        let (status, _, _) = t.send(Request::get(uri).body(Body::empty()).unwrap()).await;
        assert_eq!(status, StatusCode::OK, "{uri}");
    }
}

#[tokio::test]
async fn login_sets_a_working_session_and_logout_needs_csrf() {
    let t = app();
    let (status, headers, _) = t
        .send(
            Request::post("/user/login")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(format!("username={USER}&password=wrong")))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert!(headers["location"].contains("error"));
    assert!(!headers.contains_key("set-cookie"));

    let (_, headers, _) = t
        .send(
            Request::post("/user/login")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(format!("username={USER}&password={PASS}")))
                .unwrap(),
        )
        .await;
    let cookie = headers["set-cookie"].split(';').next().unwrap().to_string();
    assert!(headers["set-cookie"].contains("HttpOnly"));
    let (status, _, _) = t
        .send(
            Request::get("/admin")
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _, _) = t.post_form("/user/logout", &[("csrf", "bad")]).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let csrf = t.csrf().await;
    let (status, headers, _) = t.post_form("/user/logout", &[("csrf", &csrf)]).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert!(headers["set-cookie"].contains("Max-Age=0"));
}

#[tokio::test]
async fn path_traversal_is_rejected() {
    let t = app();
    std::fs::write(t.dir.path().join("secret.txt"), b"secret").unwrap();
    t.write("ok.txt", b"ok");
    for uri in [
        "/download/..%2Fsecret.txt",
        "/download/../secret.txt",
        "/download/%2E%2E/secret.txt",
        "/cover/..%2Fsecret.txt",
        "/info/..%2Fsecret.txt",
        "/download/.hidden.txt",
    ] {
        let (status, _, body) = t.get(uri).await;
        assert_ne!(body, b"secret", "{uri}");
        assert_ne!(status, StatusCode::OK, "{uri}");
    }
    let csrf = t.csrf().await;
    let (status, _, _) = t.upload(&csrf, "../outside", &[("x.epub", b"x")]).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert!(!t.dir.path().join("outside").exists());
    t.upload(&csrf, "", &[("../escaped.txt", b"x")]).await;
    assert!(!t.dir.path().join("escaped.txt").exists());
    t.post_form("/admin/delete", &[("csrf", &csrf), ("path", "../secret.txt")])
        .await;
    assert!(t.dir.path().join("secret.txt").exists());
    t.post_form(
        "/admin/rename",
        &[
            ("csrf", &csrf),
            ("path", "ok.txt"),
            ("name", "moved"),
            ("folder", "../../tmp"),
        ],
    )
    .await;
    assert!(t.books().join("ok.txt").exists());
}

#[tokio::test]
async fn upload_rename_delete_flow() {
    let t = app();
    let csrf = t.csrf().await;

    let (status, _, _) = t.upload("wrong", "", &[("a.epub", b"x")]).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(!t.books().join("a.epub").exists());

    let (status, headers, _) = t
        .upload(
            &csrf,
            "Saga/Tomo 1",
            &[("a.epub", b"one"), ("a.epub", b"two"), ("song.mp3", b"x")],
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert!(headers["location"].contains("notice"));
    assert!(headers["location"].contains("song.mp3"));
    assert_eq!(std::fs::read(t.books().join("Saga/Tomo 1/a.epub")).unwrap(), b"one");
    assert_eq!(std::fs::read(t.books().join("Saga/Tomo 1/a (1).epub")).unwrap(), b"two");
    assert!(!t.books().join("Saga/Tomo 1/song.mp3").exists());

    t.post_form(
        "/admin/rename",
        &[
            ("csrf", &csrf),
            ("path", "Saga/Tomo 1/a (1).epub"),
            ("name", "b"),
            ("folder", "Otra"),
        ],
    )
    .await;
    assert!(t.books().join("Otra/b.epub").exists());

    let (status, _, _) = t
        .post_form("/admin/delete", &[("csrf", "bad"), ("path", "Otra/b.epub")])
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    t.post_form("/admin/delete", &[("csrf", &csrf), ("path", "Otra/b.epub")])
        .await;
    assert!(!t.books().join("Otra/b.epub").exists());
}

#[tokio::test]
async fn opds_catalog() {
    let t = app();
    write_epub(&t.books().join("quijote.epub"), "Don Quijote", "Cervantes");
    t.write("Comics/Saga/01 & co.cbz", b"not really a zip");
    t.write("notes.mp3", b"x");

    let root = t.text("/").await;
    roxmltree::Document::parse(&root).expect("root feed is valid XML");
    assert!(root.contains("kind=navigation"));
    assert!(root.contains("/opds/folders"));
    assert!(root.contains("/opds/authors"));

    let books = t.text("/opds/books?sort=name-asc").await;
    let doc = roxmltree::Document::parse(&books).expect("books feed is valid XML");
    let entries: Vec<_> = doc.descendants().filter(|n| n.has_tag_name("entry")).collect();
    assert_eq!(entries.len(), 2, "mp3 must be excluded");
    assert!(books.contains("<title>Don Quijote</title>"));
    assert!(books.contains("<name>Cervantes</name>"));
    assert!(books.contains("/download/Comics/Saga/01%20%26%20co.cbz"));
    assert!(books.contains("/cover/quijote.epub?v="));
    assert!(books.contains("/thumb/quijote.epub?v="));
    assert!(books.contains(r#"type="application/epub+zip""#));
    for updated in doc.descendants().filter(|n| n.has_tag_name("updated")) {
        chrono::DateTime::parse_from_rfc3339(updated.text().unwrap()).expect("RFC 3339 dates");
    }

    let folder = t.text("/opds/folders/Comics").await;
    assert!(folder.contains("/opds/folders/Comics/Saga"));
    assert!(t.text("/opds/folders/Comics/Saga").await.contains("01 &amp; co"));
    assert!(t.text("/opds/authors/Cervantes").await.contains("Don Quijote"));
    let search = t.text("/opds/search?q=quij").await;
    assert!(search.contains("Don Quijote") && !search.contains("01 &amp; co"));
    assert!(t.text("/opds/opensearch.xml").await.contains("{searchTerms}"));

    let (status, headers, _) = t.get("/?sort=name-asc").await;
    assert_eq!(status, StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(headers["location"], "/opds/books?sort=name-asc");
}

#[tokio::test]
async fn pagination() {
    let t = app_with(&[("PAGE_SIZE", "2")]);
    for i in 0..5 {
        t.write(&format!("book{i}.txt"), b"x");
    }
    let page1 = t.text("/opds/books?sort=name-asc").await;
    assert!(page1.contains(r#"rel="next" href="http://localhost/opds/books?sort=name-asc&amp;page=2""#));
    assert!(page1.contains("<opensearch:totalResults>5</opensearch:totalResults>"));
    let page3 = t.text("/opds/books?sort=name-asc&page=3").await;
    assert!(page3.contains("book4") && !page3.contains("book0"));
    assert!(!page3.contains(r#"rel="next""#));
}

#[tokio::test]
async fn download_cover_and_info() {
    let t = app();
    write_epub(&t.books().join("q.epub"), "Q", "A");
    t.write("big.pdf", b"0123456789");

    let (status, headers, body) = t.get("/download/big.pdf").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, b"0123456789");
    assert_eq!(headers["content-type"], "application/pdf");
    assert!(headers["content-disposition"].contains("big.pdf"));

    let (status, headers, body) = t
        .send(
            Request::get("/download/big.pdf")
                .header(header::AUTHORIZATION, basic())
                .header(header::RANGE, "bytes=2-4")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(body, b"234");
    assert_eq!(headers["content-range"], "bytes 2-4/10");

    let (status, headers, body) = t.get("/cover/q.epub").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, b"JPEGDATA");
    assert_eq!(headers["content-type"], "image/jpeg");
    let (status, _, _) = t
        .send(
            Request::get("/cover/q.epub")
                .header(header::AUTHORIZATION, basic())
                .header(header::IF_NONE_MATCH, &headers["etag"])
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_MODIFIED);
    let (status, _, _) = t.get("/cover/big.pdf").await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let info = t.text("/info/q.epub").await;
    assert!(info.contains("Hola"));
    assert!(!info.contains("<img src=x"), "descriptions must be escaped");
}

#[tokio::test]
async fn admin_escapes_filenames() {
    let t = app_with(&[]);
    t.write("it's <b>#1.txt", b"x");
    let html = t.text("/admin").await;
    assert!(!html.contains("<b>#1"));
    assert!(html.contains("/info/it%27s%20%3Cb%3E%231.txt"));
    // Without credentials the admin is open but forms still need the CSRF token
    let (status, _, _) = t
        .post_form("/admin/delete", &[("csrf", "x"), ("path", "it's <b>#1.txt")])
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn base_url_from_proxy_headers_or_config() {
    let t = app_with(&[]);
    let (_, _, body) = t
        .send(
            Request::get("/")
                .header("x-forwarded-proto", "https")
                .header("x-forwarded-host", "books.example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert!(String::from_utf8(body)
        .unwrap()
        .contains("https://books.example.com/opds/books"));

    let t = app_with(&[("BASE_URL", "https://lib.example.org/")]);
    let (_, _, body) = t
        .send(Request::get("/").header("host", "evil").body(Body::empty()).unwrap())
        .await;
    assert!(String::from_utf8(body)
        .unwrap()
        .contains("https://lib.example.org/opds/books"));
}
