//! HTTP side of reading progress: OPDShelf's own API (`/api/progress`), its
//! live feed of changes (`/api/progress/events`) and a KOReader sync server
//! (`/kosync`), all over the same saved positions.

use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Json, Response};
use serde::Deserialize;
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::broadcast::error::RecvError;

use crate::annotations;
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
        "mark": state.marks.get(&document),
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
    /// The listening device, so the web panel can show it as connected.
    #[serde(default)]
    device_id: Option<String>,
    #[serde(default)]
    device: Option<String>,
}

/// Devices listening to `/api/progress/events` right now.
#[derive(Clone, Default)]
pub struct Presence {
    devices: Arc<Mutex<HashMap<String, Listener>>>,
}

#[derive(Clone)]
struct Listener {
    device: String,
    connections: usize,
    since: i64,
}

impl Presence {
    fn join(&self, device_id: &str, device: &str) -> PresenceGuard {
        let mut devices = self.devices.lock().unwrap();
        let entry = devices.entry(device_id.to_string()).or_insert_with(|| Listener {
            device: String::new(),
            connections: 0,
            since: now(),
        });
        entry.connections += 1;
        if !device.is_empty() {
            entry.device = device.to_string();
        }
        PresenceGuard {
            presence: self.clone(),
            device_id: device_id.to_string(),
        }
    }

    /// Connected device ids with their name, number of connections and since when.
    fn snapshot(&self) -> HashMap<String, Listener> {
        self.devices.lock().unwrap().clone()
    }
}

/// Counts a device as connected while its event stream is open.
struct PresenceGuard {
    presence: Presence,
    device_id: String,
}

impl Drop for PresenceGuard {
    fn drop(&mut self) {
        let mut devices = self.presence.devices.lock().unwrap();
        if let Some(entry) = devices.get_mut(&self.device_id) {
            entry.connections = entry.connections.saturating_sub(1);
            if entry.connections == 0 {
                devices.remove(&self.device_id);
            }
        }
    }
}

fn short(value: Option<String>) -> String {
    value.unwrap_or_default().trim().chars().take(120).collect()
}

/// Server-sent events with every change of a book's current position, so a
/// device that has the book open can follow reading done elsewhere at once.
///
/// Sends `hello` with the server's time when connecting, then a `progress`
/// event (`{document, current}`) per change of position, an `annotations`
/// event (`{document, annotations}`) with the highlights that changed, and
/// `resync` when changes were missed and the client should fetch its books'
/// positions and highlights again.
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
    let notes = state.annotations.subscribe();
    let device_id = short(query.device_id);
    let guard = (!device_id.is_empty()).then(|| state.presence.join(&device_id, &short(query.device)));
    let stream = futures_util::stream::unfold(
        (Some(hello), changes, notes, wanted, guard),
        |(first, mut changes, mut notes, wanted, guard)| async move {
            if let Some(event) = first {
                return Some((Ok::<_, Infallible>(event), (None, changes, notes, wanted, guard)));
            }
            loop {
                let event = tokio::select! {
                    change = changes.recv() => match change {
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
                    },
                    change = notes.recv() => match change {
                        Ok(change) => {
                            if wanted.as_ref().is_some_and(|w| !w.contains(&change.document)) {
                                continue;
                            }
                            Event::default()
                                .event("annotations")
                                .data(serde_json::to_string(&change).unwrap_or_default())
                        }
                        Err(RecvError::Lagged(_)) => Event::default().event("resync").data("{}"),
                        Err(RecvError::Closed) => return None,
                    },
                };
                return Some((Ok(event), (None, changes, notes, wanted, guard)));
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
// Summaries for the web panel

/// What the panel needs to know about a book from its document id.
async fn books_by_document(state: &AppState) -> HashMap<String, (String, String)> {
    let index = state.documents.index(&state.library).await;
    let snapshot = state.library.snapshot().await;
    let titles: HashMap<&str, &str> = snapshot
        .books
        .iter()
        .map(|b| (b.path.as_str(), b.title.as_str()))
        .collect();
    index
        .into_iter()
        .map(|(id, path)| {
            let title = titles.get(path.as_str()).copied().unwrap_or_default().to_string();
            (id, (path, title))
        })
        .collect()
}

fn book_fields(books: &HashMap<String, (String, String)>, document: &str) -> serde_json::Value {
    match books.get(document) {
        Some((path, title)) => json!({ "path": path, "title": title }),
        None => json!({ "path": null, "title": null }),
    }
}

/// The current position of every book that has one, newest first.
pub async fn list_progress(State(state): State<AppState>) -> Response {
    let books = books_by_document(&state).await;
    let mut current = state.progress.all_current();
    current.sort_by_key(|c| std::cmp::Reverse(c.1.timestamp));
    let list: Vec<_> = current
        .into_iter()
        .map(|(document, record)| {
            let mut entry = book_fields(&books, &document);
            entry["document"] = json!(document);
            entry["current"] = serde_json::to_value(record).unwrap_or_default();
            entry
        })
        .collect();
    let marks: Vec<_> = state
        .marks
        .all()
        .into_iter()
        .map(|(document, mark)| {
            let mut entry = book_fields(&books, &document);
            entry["document"] = json!(document);
            entry["state"] = json!(mark.state);
            entry["at"] = json!(mark.at);
            entry
        })
        .collect();
    Json(json!({ "books": list, "marks": marks, "server_time": now() })).into_response()
}

#[derive(Deserialize)]
pub struct MarkRequest {
    /// Books by their path in the library.
    paths: Vec<String>,
    /// `read`, `unread`, or null to clear the mark.
    state: Option<crate::marks::State>,
}

/// Marks books as read or unread in the web panel (positions are not changed).
pub async fn put_marks(State(state): State<AppState>, Json(request): Json<MarkRequest>) -> Response {
    let index = state.documents.index(&state.library).await;
    let by_path: HashMap<&str, &str> = index.iter().map(|(id, path)| (path.as_str(), id.as_str())).collect();
    let documents: Vec<String> = request
        .paths
        .iter()
        .filter_map(|p| by_path.get(p.as_str()).map(|id| id.to_string()))
        .collect();
    if documents.is_empty() {
        return (StatusCode::NOT_FOUND, Json(json!({ "error": "no such book" }))).into_response();
    }
    match state.marks.set(&documents, request.state, now()) {
        Ok(()) => Json(json!({ "marked": documents.len(), "state": request.state, "at": now() })).into_response(),
        Err(err) => save_failed(err),
    }
}

/// Every device that saved a position or is listening now, with what it read last.
pub async fn devices(State(state): State<AppState>) -> Response {
    const RECENT: usize = 5;
    let books = books_by_document(&state).await;
    let live = state.presence.snapshot();

    struct Device {
        name: String,
        last_seen: i64,
        positions: usize,
        // Latest position per book, newest first
        recent: Vec<(String, progress::Record)>,
    }
    let mut devices: HashMap<String, Device> = HashMap::new();
    for (document, history) in state.progress.all_history() {
        for record in history {
            let device = devices.entry(record.device_id.clone()).or_insert_with(|| Device {
                name: String::new(),
                last_seen: i64::MIN,
                positions: 0,
                recent: Vec::new(),
            });
            device.positions += 1;
            if record.timestamp >= device.last_seen {
                device.last_seen = record.timestamp;
                device.name = record.device.clone();
            }
            match device.recent.iter_mut().find(|(d, _)| *d == document) {
                Some((_, kept)) if kept.timestamp >= record.timestamp => {}
                Some((_, kept)) => *kept = record,
                None => device.recent.push((document.clone(), record)),
            }
        }
    }
    let mut list: Vec<_> = devices
        .into_iter()
        .map(|(id, mut device)| {
            device.recent.sort_by_key(|r| std::cmp::Reverse(r.1.timestamp));
            let book_count = device.recent.len();
            let listener = live.get(&id);
            let name = match listener {
                Some(l) if device.name.is_empty() => l.device.clone(),
                _ => device.name,
            };
            let recent: Vec<_> = device
                .recent
                .into_iter()
                .take(RECENT)
                .map(|(document, record)| {
                    let mut entry = book_fields(&books, &document);
                    entry["document"] = json!(document);
                    entry["percentage"] = json!(record.percentage);
                    entry["progress"] = json!(record.progress);
                    entry["timestamp"] = json!(record.timestamp);
                    entry["manual"] = json!(record.manual);
                    entry
                })
                .collect();
            json!({
                "device_id": id,
                "device": name,
                "last_seen": device.last_seen,
                "live": listener.is_some(),
                "connections": listener.map_or(0, |l| l.connections),
                "connected_since": listener.map(|l| l.since),
                "positions": device.positions,
                "books": book_count,
                "recent": recent,
            })
        })
        .collect();
    // Listening devices that have not saved anything yet
    for (id, listener) in &live {
        if list.iter().any(|d| d["device_id"] == json!(id)) {
            continue;
        }
        list.push(json!({
            "device_id": id,
            "device": listener.device,
            "last_seen": null,
            "live": true,
            "connections": listener.connections,
            "connected_since": listener.since,
            "positions": 0,
            "books": 0,
            "recent": [],
        }));
    }
    list.sort_by(|a, b| {
        let key = |d: &serde_json::Value| {
            (
                d["live"].as_bool() == Some(true),
                d["last_seen"].as_i64().unwrap_or(i64::MIN),
            )
        };
        key(b).cmp(&key(a))
    });
    Json(json!({
        "devices": list,
        "listening": live.values().map(|l| l.connections).sum::<usize>(),
        "server_time": now(),
    }))
    .into_response()
}

/// Forgets a device: removes every position it saved.
pub async fn forget_device(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(device_id): Path<String>,
) -> Response {
    // A form on another site cannot send this header, so the token is enough
    if !state.auth.check_csrf(header(&headers, "x-csrf-token")) {
        return (StatusCode::FORBIDDEN, Json(json!({ "error": "Invalid CSRF token" }))).into_response();
    }
    match state.progress.forget_device(&device_id) {
        Ok(removed) => Json(json!({ "removed": removed })).into_response(),
        Err(err) => save_failed(err),
    }
}

// ---------------------------------------------------------------------------
// Highlights and notes

fn missing_device() -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({ "error": "device_id is required" })),
    )
        .into_response()
}

pub async fn get_annotations(State(state): State<AppState>, Path(document): Path<String>) -> Response {
    if !progress::valid_document(&document) {
        return bad_document();
    }
    Json(json!({
        "document": document,
        "annotations": state.annotations.list(&document),
        "server_time": now(),
    }))
    .into_response()
}

/// Merges the highlights a device sends and answers every highlight of the book.
pub async fn put_annotations(
    State(state): State<AppState>,
    Path(document): Path<String>,
    Json(batch): Json<annotations::Batch>,
) -> Response {
    if !progress::valid_document(&document) {
        return bad_document();
    }
    if batch.device_id.trim().is_empty() && batch.annotations.iter().any(|a| a.device_id.trim().is_empty()) {
        return missing_device();
    }
    match state.annotations.merge(&document, batch, now()) {
        Ok(changed) => Json(json!({
            "document": document,
            "changed": changed.len(),
            "annotations": state.annotations.list(&document),
            "server_time": now(),
        }))
        .into_response(),
        Err(err) => {
            tracing::error!("cannot save highlights: {err}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Cannot save the highlights" })),
            )
                .into_response()
        }
    }
}

/// Every book with highlights: how many, how many with notes, and the last change.
pub async fn list_annotations(State(state): State<AppState>) -> Response {
    let books = books_by_document(&state).await;
    let mut list: Vec<_> = state
        .annotations
        .all()
        .into_iter()
        .map(|(document, notes)| {
            let mut entry = book_fields(&books, &document);
            entry["document"] = json!(document);
            entry["highlights"] = json!(notes.len());
            entry["notes"] = json!(notes.iter().filter(|a| !a.note.trim().is_empty()).count());
            entry["updated"] = json!(notes.iter().map(|a| a.updated).max().unwrap_or(0));
            entry
        })
        .collect();
    list.sort_by_key(|e| std::cmp::Reverse(e["updated"].as_i64().unwrap_or(0)));
    Json(json!({ "books": list, "server_time": now() })).into_response()
}

#[derive(Deserialize)]
pub struct ExportQuery {
    #[serde(default)]
    format: Option<String>,
}

/// The highlights of a book as a Markdown or JSON file.
pub async fn export_annotations(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(document): Path<String>,
    Query(query): Query<ExportQuery>,
) -> Response {
    if !progress::valid_document(&document) {
        return bad_document();
    }
    let index = state.documents.index(&state.library).await;
    let book = match index.get(&document) {
        Some(path) => state.library.book(path).await,
        None => None,
    };
    let info = annotations::BookInfo {
        title: book.as_ref().map_or_else(|| document.clone(), |b| b.title.clone()),
        authors: book.as_ref().map(|b| b.author_line()).unwrap_or_default(),
    };
    let list: Vec<_> = state
        .annotations
        .list(&document)
        .into_iter()
        .filter(|a| !a.deleted)
        .collect();
    let stem: String = info
        .title
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim()
        .chars()
        .take(80)
        .collect();
    let stem = if stem.is_empty() {
        "highlights".to_string()
    } else {
        stem
    };
    let (body, mime, ext) = match query.format.as_deref() {
        Some("json") => (
            serde_json::to_string_pretty(&json!({
                "title": info.title,
                "authors": info.authors,
                "document": document,
                "annotations": list,
            }))
            .unwrap_or_default(),
            "application/json; charset=utf-8",
            "json",
        ),
        _ => {
            let spanish = crate::i18n::Lang::from_headers(&headers) == crate::i18n::Lang::Es;
            (
                annotations::markdown(&info, &list, spanish),
                "text/markdown; charset=utf-8",
                "md",
            )
        }
    };
    let ascii: String = stem.chars().map(|c| if c.is_ascii() { c } else { '_' }).collect();
    let disposition = format!(
        "attachment; filename=\"{ascii}.{ext}\"; filename*=UTF-8''{}.{ext}",
        crate::paths::url_encode(&stem)
    );
    (
        [
            (header::CONTENT_TYPE, mime.to_string()),
            (header::CONTENT_DISPOSITION, disposition),
        ],
        body,
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
