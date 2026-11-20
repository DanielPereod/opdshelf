//! How the server copes with a large library. Not run by default:
//!
//! ```sh
//! SCALE_BOOKS=20000 cargo test --release --test scale -- --ignored --nocapture
//! ```
//!
//! It prints how long the main operations take and how much memory the
//! process uses. It only fails when something is grossly slow.

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use base64::Engine;
use http_body_util::BodyExt;
use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};
use tower::ServiceExt;

use opdshelf::annotations::{Annotations, Batch};
use opdshelf::config::DEFAULT_EXTENSIONS;
use opdshelf::library::Library;
use opdshelf::progress::{Progress, Update};
use opdshelf::store::{MetaOverride, Store};
use opdshelf::{router, Config};

fn books() -> usize {
    std::env::var("SCALE_BOOKS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20_000)
}

/// Resident memory of this process, in MiB (Linux only).
fn rss_mib() -> f64 {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    status
        .lines()
        .find_map(|l| l.strip_prefix("VmRSS:"))
        .and_then(|v| v.trim().trim_end_matches("kB").trim().parse::<f64>().ok())
        .map_or(0.0, |kb| kb / 1024.0)
}

fn report(what: &str, took: Duration) {
    println!("{what:<48} {:>10.1} ms", took.as_secs_f64() * 1000.0);
}

fn file_kib(path: &Path) -> f64 {
    std::fs::metadata(path).map_or(0.0, |m| m.len() as f64 / 1024.0)
}

fn write_epub(path: &Path, i: usize) {
    let author = format!("Author {}", i % 2000);
    let series = format!("Series {}", i % 3000);
    let opf = format!(
        r#"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" xmlns:dc="http://purl.org/dc/elements/1.1/"><metadata><dc:title>Book number {i}</dc:title><dc:creator>{author}</dc:creator><dc:description>A description of book {i}, long enough to look like a real blurb on the back cover of a paperback.</dc:description><dc:language>es</dc:language><dc:subject>Fiction</dc:subject><meta name="calibre:series" content="{series}"/><meta name="calibre:series_index" content="{}"/><meta name="cover" content="c"/></metadata><manifest><item id="c" href="cover.jpg" media-type="image/jpeg"/></manifest></package>"#,
        i % 7 + 1
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

fn make_library(root: &Path, n: usize) {
    for i in 0..n {
        let folder = root
            .join(format!("Shelf {}", i % 100))
            .join(format!("Author {}", i % 2000));
        std::fs::create_dir_all(&folder).unwrap();
        write_epub(&folder.join(format!("book-{i}.epub")), i);
    }
}

fn update(i: usize, device: usize) -> Update {
    Update {
        progress: format!("/body/DocFragment[{}]/body/p[{}]/text().0", i % 40 + 2, i % 300),
        percentage: (i % 100) as f64 / 100.0,
        device: format!("Device {device}"),
        device_id: format!("device{device}"),
        cfi: Some(format!("epubcfi(/6/{}!/4/{}/1:0)", i % 40 * 2 + 2, i % 300 * 2)),
        ..Default::default()
    }
}

fn highlights(book: usize, count: usize) -> Batch {
    let list: Vec<serde_json::Value> = (0..count)
        .map(|j| {
            serde_json::json!({
                "id": format!("h{book}x{j}"),
                "text": "Some highlighted sentence that is about as long as a real one in a novel.",
                "note": if j % 3 == 0 { "A short note" } else { "" },
                "pos0": format!("/body/DocFragment[{}]/body/p[{j}]/text().0", j % 40 + 2),
                "pos1": format!("/body/DocFragment[{}]/body/p[{j}]/text().60", j % 40 + 2),
                "percentage": j as f64 / count as f64,
                "updated": 1_700_000_000 + j as i64,
                "created": 1_700_000_000 + j as i64,
            })
        })
        .collect();
    Batch {
        annotations: serde_json::from_value(serde_json::Value::Array(list)).unwrap(),
        device: "Phone".into(),
        device_id: "phone".into(),
        sent_at: None,
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "slow: run by hand with --ignored"]
async fn large_library() {
    let n = books();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("books");
    let data = dir.path().join("data");
    println!("\n== {n} books ==");
    let start = Instant::now();
    make_library(&root, n);
    report("create the fake books", start.elapsed());
    let base_rss = rss_mib();
    println!("{:<48} {base_rss:>10.1} MiB", "memory before scanning");

    // Scanning
    let extensions = DEFAULT_EXTENSIONS.iter().map(|e| e.to_string()).collect();
    let library = Library::new(root.clone(), extensions, Store::open(&data));
    let start = Instant::now();
    let snapshot = library.refresh().await;
    report("first scan (reads every file)", start.elapsed());
    assert_eq!(snapshot.books.len(), n);
    println!("{:<48} {:>10.1} MiB", "memory after scanning", rss_mib());
    library.invalidate();
    let start = Instant::now();
    library.refresh().await;
    report("rescan (nothing changed)", start.elapsed());
    let start = Instant::now();
    let copy = library.scan().await;
    report("Library::scan() copy of the snapshot", start.elapsed());
    drop(copy);
    let start = Instant::now();
    let found = snapshot.search("number 1999");
    report("search in memory", start.elapsed());
    assert!(!found.is_empty());

    // Document ids (KOReader's partial MD5), needed for progress and highlights
    let start = Instant::now();
    for book in &snapshot.books {
        opdshelf::progress::partial_md5(&root.join(&book.path)).unwrap();
    }
    report("partial MD5 of every book", start.elapsed());

    // Edited metadata: every edit rewrites metadata.json
    let store = library.store().clone();
    let edited = n / 10;
    for (i, book) in snapshot.books.iter().enumerate().take(edited - 1) {
        store
            .set(
                &book.path,
                MetaOverride {
                    title: Some(format!("Edited {i}")),
                    ..Default::default()
                },
            )
            .ok();
    }
    let start = Instant::now();
    store
        .set(
            &snapshot.books[edited].path,
            MetaOverride {
                title: Some("Edited".into()),
                ..Default::default()
            },
        )
        .unwrap();
    report(&format!("save one edit ({edited} edited books)"), start.elapsed());
    println!(
        "{:<48} {:>10.0} KiB",
        "metadata.json",
        file_kib(&data.join("metadata.json"))
    );

    // Reading positions: every save rewrites progress.json
    // (filled in memory and saved once: saving 60.000 times would take too long)
    let now = 1_800_000_000;
    let one = Progress::open(&dir.path().join("one"));
    one.update("x", update(1, 0), now).unwrap();
    let record: serde_json::Value =
        serde_json::from_slice::<serde_json::Value>(&std::fs::read(dir.path().join("one/progress.json")).unwrap())
            .unwrap()["x"][0]
            .clone();
    let all: serde_json::Map<String, serde_json::Value> = (0..n)
        .map(|i| (format!("{i:032x}"), serde_json::json!([record, record, record])))
        .collect();
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(data.join("progress.json"), serde_json::to_vec(&all).unwrap()).unwrap();
    let progress = Progress::open(&data);
    let start = Instant::now();
    progress.update(&format!("{:032x}", 7), update(8, 0), now).unwrap();
    report(&format!("save one position ({n} books × 3 devices)"), start.elapsed());
    println!(
        "{:<48} {:>10.0} KiB",
        "progress.json",
        file_kib(&data.join("progress.json"))
    );
    let start = Instant::now();
    drop(Progress::open(&data));
    report("load progress.json at start-up", start.elapsed());

    // Highlights: every change rewrites annotations.json
    let annotations = Annotations::open(&data);
    let with_highlights = (n / 10).max(1);
    for i in 0..with_highlights {
        annotations.merge(&format!("{i:032x}"), highlights(i, 50), now).unwrap();
    }
    let start = Instant::now();
    annotations
        .merge(&format!("{:032x}", 1), highlights(n + 1, 1), now)
        .unwrap();
    report(
        &format!("save one highlight ({with_highlights} books × 50)"),
        start.elapsed(),
    );
    println!(
        "{:<48} {:>10.0} KiB",
        "annotations.json",
        file_kib(&data.join("annotations.json"))
    );
    println!("{:<48} {:>10.1} MiB", "memory with everything loaded", rss_mib());

    // Requests through the whole server (the data folder above is reused)
    let env: HashMap<String, String> = [
        ("BOOKS_DIR", root.to_string_lossy().into_owned()),
        ("DATA_DIR", data.to_string_lossy().into_owned()),
        ("ADMIN_USERNAME", "admin".into()),
        ("ADMIN_PASSWORD", "secret".into()),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect();
    let app = router(Config::from_lookup(|k| env.get(k).cloned()));
    let auth = format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode("admin:secret")
    );
    let get = |uri: &'static str| {
        let app = app.clone();
        let auth = auth.clone();
        async move {
            let start = Instant::now();
            let response = app
                .oneshot(
                    Request::get(uri)
                        .header(header::AUTHORIZATION, auth)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK, "{uri}");
            let body = response.into_body().collect().await.unwrap().to_bytes();
            (start.elapsed(), body.len())
        }
    };
    // The first request waits for the server's own first scan
    let (took, _) = get("/opds/books").await;
    report("first request after start-up", took);
    // Wait for the document ids to be worked out in the background
    let start = Instant::now();
    while !data.join("document-ids.json").exists() && start.elapsed() < Duration::from_secs(600) {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    report("document ids ready after start-up", start.elapsed() + took);
    for uri in [
        "/opds/books",
        "/opds/authors",
        "/opds/series",
        "/opds/search?q=number%2019",
        "/admin",
        "/api/progress",
        "/api/annotations",
    ] {
        let (took, size) = get(uri).await;
        report(&format!("GET {uri} ({} KiB)", size / 1024), took);
    }
    println!("{:<48} {:>10.1} MiB", "memory at the end", rss_mib());
}
