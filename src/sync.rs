//! HTTP side of reading progress: OPDShelf's own API (`/api/progress`) and a
//! KOReader sync server (`/kosync`), both over the same saved positions.

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use serde_json::json;

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
    match state.progress.update(&document, update, now()) {
        Ok(current) => Json(json!({ "document": document, "current": current })).into_response(),
        Err(err) => save_failed(err),
    }
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
