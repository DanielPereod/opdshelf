//! Web reader: EPUB (epub.js), PDF (pdf.js), comics (one image per page) and plain text.

use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use crate::library::Book;
use crate::web::{book_json, page_with_layout, AppState};
use crate::{archive, metadata, paths};

const MAX_TEXT_BYTES: u64 = 10 * 1024 * 1024;

pub async fn read(State(state): State<AppState>, headers: HeaderMap, Path(path): Path<String>) -> Response {
    let Some(book) = state.library.book(&path).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !book.readable {
        return (StatusCode::NOT_FOUND, "This format cannot be read in the browser").into_response();
    }
    let mut data = json!({ "book": book_json(&book) });
    let view = match paths::extension(&book.path).as_str() {
        "epub" | "kepub" => "reader_epub",
        "pdf" => "reader_pdf",
        "txt" => {
            let Some((_, file)) = state.library.book_path(&book.path) else {
                return StatusCode::NOT_FOUND.into_response();
            };
            match tokio::task::spawn_blocking(move || read_text(&file)).await {
                Ok(Ok(text)) => data["text"] = json!(text),
                _ => return (StatusCode::INTERNAL_SERVER_ERROR, "Cannot read the file").into_response(),
            }
            "reader_text"
        }
        _ => "reader_comic",
    };
    page_with_layout(&state, &headers, view, "layouts/reader", &book.title, data)
}

/// Reads a text file as UTF-8, falling back to Windows-1252 for older files.
fn read_text(file: &std::path::Path) -> std::io::Result<String> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(file)?
        .take(MAX_TEXT_BYTES)
        .read_to_end(&mut bytes)?;
    Ok(match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(err) => encoding_rs::WINDOWS_1252.decode(err.as_bytes()).0.into_owned(),
    })
}

type PageCache = Mutex<HashMap<String, (u64, i64, Arc<Vec<String>>)>>;

/// Page lists are cached because listing a RAR or 7z archive spawns bsdtar.
async fn page_list(state: &AppState, book: &Book) -> Option<Arc<Vec<String>>> {
    static CACHE: OnceLock<PageCache> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let stamp = (book.size, book.modified.timestamp());
    if let Some((size, mtime, pages)) = cache.lock().unwrap().get(&book.path) {
        if (*size, *mtime) == stamp {
            return Some(pages.clone());
        }
    }
    let (_, file) = state.library.book_path(&book.path)?;
    let pages = tokio::task::spawn_blocking(move || archive::comic_pages(&file))
        .await
        .ok()?
        .ok()?;
    let pages = Arc::new(pages);
    let mut cache = cache.lock().unwrap();
    if cache.len() > 256 {
        cache.clear();
    }
    cache.insert(book.path.clone(), (stamp.0, stamp.1, pages.clone()));
    Some(pages)
}

async fn comic(state: &AppState, path: &str) -> Option<Book> {
    let book = state.library.book(path).await?;
    let ext = paths::extension(&book.path);
    (book.readable && (ext == "cbz" || ext == "zip" || archive::is_archive_comic(&book.path))).then_some(book)
}

pub async fn pages(State(state): State<AppState>, Path(path): Path<String>) -> Response {
    let Some(book) = comic(&state, &path).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match page_list(&state, &book).await {
        Some(pages) => Json(json!({ "count": pages.len() })).into_response(),
        None => (StatusCode::INTERNAL_SERVER_ERROR, "Cannot read the archive").into_response(),
    }
}

#[derive(Deserialize)]
pub struct PageQuery {
    n: usize,
}

pub async fn page(State(state): State<AppState>, Path(path): Path<String>, Query(query): Query<PageQuery>) -> Response {
    let Some(book) = comic(&state, &path).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(name) = page_list(&state, &book)
        .await
        .and_then(|pages| pages.get(query.n).cloned())
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some((_, file)) = state.library.book_path(&book.path) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let mime = metadata::image_mime(&name).unwrap_or("application/octet-stream");
    match tokio::task::spawn_blocking(move || archive::comic_page(&file, &name)).await {
        Ok(Ok(image)) => (
            [
                (header::CONTENT_TYPE, mime),
                (header::CACHE_CONTROL, "private, max-age=86400"),
            ],
            image,
        )
            .into_response(),
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}
