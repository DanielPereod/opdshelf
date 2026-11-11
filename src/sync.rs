//! HTTP side of reading progress: OPDShelf's own API (`/api/progress`), its
//! live feed of changes (`/api/progress/events`) and a KOReader sync server
//! (`/kosync`), all over the same saved positions.

use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Json, Response};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashSet;
use std::convert::Infallible;
use std::time::Duration;
use tokio::sync::broadcast::error::RecvError;

use crate::progress::{self, Update};
use crate::web::AppState;

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

fn bad_document() -> Response {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": "Invalid document id" }))).into_response()
}

fn save_failed(err: std::io::Error) -> Response {
    tracing::error!("cannot save reading progress: {err}");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": "Cannot save the position" })),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// OPDShelf API

pub async fn get_progress(State(state): State<AppState>, Path(document): Path<String>) -> Response {
    if !progress::valid_document(&document) {
        return bad_document();
    }
    Json(json!({
        "document": document,
        "current": state.progress.current(&document),
        "history": state.progress.history(&document),
        "server_time": now(),
    }))
    .into_response()
}

pub async fn put_progress(
    State(state): State<AppState>,
    Path(document): Path<String>,
    Json(update): Json<Update>,
) -> Response {
    if !progress::valid_document(&document) {
        return bad_document();
    }
    if update.device_id.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "device_id is required" })),
        )
            .into_response();
    }
    match state.progress.apply(&document, update, now()) {
        Ok(saved) => Json(json!({
            "document": document,
            "current": saved.current,
            "accepted": saved.accepted,
            "server_time": now(),
        }))
        .into_response(),
        Err(err) => save_failed(err),
    }
}

#[derive(Deserialize)]
pub struct EventsQuery {
    /// Comma separated document ids to hear about. Every book when missing.
    #[serde(default)]
    documents: Option<String>,
}

/// Server-sent events with every change of a book's current position, so a
/// device that has the book open can follow reading done elsewhere at once.
///
/// Sends `hello` with the server's time when connecting, then a `progress`
/// event (`{document, current}`) per change, and `resync` when changes were
/// missed and the client should fetch its books' positions again.
pub async fn events(State(state): State<AppState>, Query(query): Query<EventsQuery>) -> Response {
    let wanted: Option<HashSet<String>> = query.documents.map(|list| {
        list.split(',')
            .map(str::trim)
            .filter(|d| progress::valid_document(d))
            .map(str::to_string)
            .collect()
    });
    let hello = Event::default()
        .event("hello")
        .data(json!({ "server_time": now() }).to_string());
    let changes = state.progress.subscribe();
    let stream = futures_util::stream::unfold(
        (Some(hello), changes, wanted),
        |(first, mut changes, wanted)| async move {
            if let Some(event) = first {
                return Some((Ok::<_, Infallible>(event), (None, changes, wanted)));
            }
            loop {
                let event = match changes.recv().await {
                    Ok(change) => {
                        if wanted.as_ref().is_some_and(|w| !w.contains(&change.document)) {
                            continue;
                        }
                        Event::default()
                            .event("progress")
                            .data(serde_json::to_string(&change).unwrap_or_default())
                    }
                    Err(RecvError::Lagged(_)) => Event::default().event("resync").data("{}"),
                    Err(RecvError::Closed) => return None,
                };
                return Some((Ok(event), (None, changes, wanted)));
            }
        },
    );
    let sse = Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(20)));
    // Proxies such as nginx would otherwise hold events back
    (
        [
            (header::CACHE_CONTROL, "no-cache"),
            (header::HeaderName::from_static("x-accel-buffering"), "no"),
        ],
        sse,
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// KOReader sync server (the protocol of KOReader's "Progress sync" plugin)

fn kosync_error(status: StatusCode, code: u32, message: &str) -> Response {
    (status, Json(json!({ "code": code, "message": message }))).into_response()
}

fn unauthorized() -> Response {
    kosync_error(StatusCode::UNAUTHORIZED, 2001, "Unauthorized")
}

fn header<'a>(headers: &'a HeaderMap, name: &str) -> &'a str {
    headers.get(name).and_then(|v| v.to_str().ok()).unwrap_or("")
}

/// KOReader sends the username and the MD5 of the password in its own headers.
fn kosync_authorized(state: &AppState, headers: &HeaderMap) -> bool {
    state.auth.is_authorized(headers)
        || state
            .auth
            .check_hashed_password(header(headers, "x-auth-user"), header(headers, "x-auth-key"))
}

/// KOReader can identify books by the MD5 of their file name instead of their
/// content. Those ids are turned into the content one so every client agrees.
async fn resolve_document(state: &AppState, document: &str) -> String {
    use md5::{Digest, Md5};
    let snapshot = state.library.snapshot().await;
    let book = snapshot.books.iter().find(|b| {
        let digest: String = Md5::digest(b.file_name.as_bytes())
            .iter()
            .map(|x| format!("{x:02x}"))
            .collect();
        digest == document
    });
    let Some(book) = book else {
        return document.to_string();
    };
    let Some((_, file)) = state.library.book_path(&book.path) else {
        return document.to_string();
    };
    let (size, modified) = (book.size, book.modified.timestamp());
    tokio::task::spawn_blocking(move || progress::document_id(&file, size, modified))
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| document.to_string())
}

pub async fn kosync_register(State(state): State<AppState>, body: axum::body::Bytes) -> Response {
    // There is a single account: "registering" with it just checks the password
    let form: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
    let username = form["username"].as_str().unwrap_or("");
    let key = form["password"].as_str().unwrap_or("");
    if state.auth.check_hashed_password(username, key) {
        return (StatusCode::CREATED, Json(json!({ "username": username }))).into_response();
    }
    kosync_error(
        StatusCode::PAYMENT_REQUIRED,
        2002,
        "OPDShelf has one account: log in with its username and password",
    )
}

pub async fn kosync_auth(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if !kosync_authorized(&state, &headers) {
        return unauthorized();
    }
    Json(json!({ "authorized": "OK" })).into_response()
}

pub async fn kosync_get(State(state): State<AppState>, headers: HeaderMap, Path(document): Path<String>) -> Response {
    if !kosync_authorized(&state, &headers) {
        return unauthorized();
    }
    if !progress::valid_document(&document) {
        return kosync_error(StatusCode::FORBIDDEN, 2004, "Field 'document' not provided.");
    }
    let id = resolve_document(&state, &document).await;
    match state.progress.current(&id) {
        Some(record) => {
            let mut body = serde_json::to_value(record).unwrap_or_default();
            body["document"] = json!(document);
            Json(body).into_response()
        }
        None => Json(json!({})).into_response(),
    }
}

pub async fn kosync_put(State(state): State<AppState>, headers: HeaderMap, body: axum::body::Bytes) -> Response {
    if !kosync_authorized(&state, &headers) {
        return unauthorized();
    }
    let Ok(update) = serde_json::from_slice::<Update>(&body) else {
        return kosync_error(StatusCode::FORBIDDEN, 2003, "Invalid request");
    };
    let document = serde_json::from_slice::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| v["document"].as_str().map(str::to_string))
        .unwrap_or_default();
    if !progress::valid_document(&document) {
        return kosync_error(StatusCode::FORBIDDEN, 2004, "Field 'document' not provided.");
    }
    if update.device_id.trim().is_empty() {
        return kosync_error(StatusCode::FORBIDDEN, 2003, "Invalid request");
    }
    let id = resolve_document(&state, &document).await;
    // KOReader's own plugin does not say when the page was read, so it is taken as now
    match state.progress.update(&id, update, now()) {
        Ok(_) => Json(json!({ "document": document, "timestamp": now() })).into_response(),
        Err(err) => save_failed(err),
    }
}

pub async fn kosync_health() -> Response {
    Json(json!({ "state": "OK" })).into_response()
}
