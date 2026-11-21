use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Multipart, Path, Query, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Form, Json, Router};
use handlebars::Handlebars;
use serde::Deserialize;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tower::ServiceExt;
use tower_http::compression::CompressionLayer;
use tower_http::services::ServeFile;

use crate::auth::Auth;
use crate::config::Config;
use crate::documents::DocumentIds;
use crate::i18n::Lang;
use crate::library::{format_size, Book, Library, SortMode};
use crate::lookup::Lookup;
use crate::progress::Progress;
use crate::store::Store;
use crate::templates::{self, Static};
use crate::{apps, covers, edit, opds, paths, reader, sync};

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub library: Library,
    pub auth: Auth,
    pub templates: Arc<Handlebars<'static>>,
    pub lookup: Lookup,
    pub progress: Progress,
    pub marks: crate::marks::Marks,
    pub documents: DocumentIds,
    pub presence: sync::Presence,
    pub annotations: crate::annotations::Annotations,
    /// Bookmarks of the web readers, kept like highlights.
    pub bookmarks: crate::annotations::Annotations,
}

pub fn router(config: Config) -> Router {
    let state = AppState {
        library: Library::new(
            config.books_dir.clone(),
            config.allowed_extensions.clone(),
            Store::open(&config.data_dir),
        ),
        auth: Auth::open(&config, &config.data_dir),
        templates: Arc::new(templates::registry()),
        lookup: Lookup::new(&config.openlibrary_url, &config.google_books_url),
        progress: Progress::open(&config.data_dir),
        marks: crate::marks::Marks::open(&config.data_dir),
        documents: DocumentIds::open(&config.data_dir),
        presence: sync::Presence::default(),
        annotations: crate::annotations::Annotations::open(&config.data_dir),
        bookmarks: crate::annotations::Annotations::open_bookmarks(&config.data_dir),
        config: Arc::new(config),
    };
    let upload_limit = state.config.max_upload_bytes;

    // Read every book's metadata in the background so the first feed request is fast
    if let Ok(runtime) = tokio::runtime::Handle::try_current() {
        let library = state.library.clone();
        let documents = state.documents.clone();
        let watch = state.config.watch;
        let interval = std::time::Duration::from_secs(state.config.scan_interval_secs);
        runtime.spawn(async move {
            if watch {
                crate::watch::start(library.clone(), interval);
            }
            let books = library.snapshot().await.books.len();
            tracing::info!("library ready: {books} books");
            // Work out which book each synced position belongs to ahead of time
            let known = documents.index(&library).await.len();
            tracing::debug!("document ids of {known} books ready");
        });
    }

    Router::new()
        .route("/", get(opds::root))
        .route("/opds/books", get(opds::books))
        .route("/opds/reading", get(opds::reading))
        .route("/opds/random", get(opds::random))
        .route("/opds/folders", get(opds::folders_root))
        .route("/opds/folders/{*path}", get(opds::folder))
        .route("/opds/authors", get(opds::authors))
        .route("/opds/authors/{name}", get(opds::author))
        .route("/opds/series", get(opds::series_index))
        .route("/opds/series/{name}", get(opds::series))
        .route("/opds/search", get(opds::search))
        .route("/opds/opensearch.xml", get(opds::opensearch))
        .route("/download/{*path}", get(download))
        .route("/cover/{*path}", get(cover))
        .route("/thumb/{*path}", get(thumb))
        .route("/info/{*path}", get(info))
        .route("/read/{*path}", get(reader::read))
        .route("/pages/{*path}", get(reader::pages))
        .route("/page/{*path}", get(reader::page))
        .route("/admin", get(admin).layer(CompressionLayer::new()))
        .route("/admin/sync", get(admin_sync).layer(CompressionLayer::new()))
        .route(
            "/admin/highlights",
            get(admin_highlights).layer(CompressionLayer::new()),
        )
        .route("/apps/opdshelf.koplugin.zip", get(apps::koreader_plugin))
        .route("/apps/opdshelf.apk", get(apps::android_app))
        .route("/apps/versions.json", get(apps::versions))
        .route("/apps/connect.svg", get(apps::connect_qr))
        .route("/admin/upload", post(upload).layer(DefaultBodyLimit::max(upload_limit)))
        .route("/api/upload/{*path}", axum::routing::put(api_upload))
        .route("/admin/delete", post(delete))
        .route("/admin/rename", post(rename))
        .route("/admin/bulk", post(bulk))
        .route("/admin/folder", post(folder_action))
        .route("/api/folders", get(api_folders).post(api_create_folder))
        .route("/api/folders/rename", post(api_rename_folder))
        .route("/api/folders/move", post(api_move_folder))
        .route("/api/folders/delete", post(api_delete_folder))
        .route("/api/books/move", post(api_move_books))
        .route("/api/books/rename", post(api_rename_book))
        .route("/api/books/delete", post(api_delete_books))
        .route("/api/books/rows", post(api_book_rows))
        .route("/admin/edit/{*path}", get(edit::edit_page))
        .route(
            "/admin/edit",
            post(edit::save).layer(DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route("/admin/reset", post(edit::reset))
        .route("/admin/settings", get(admin_settings).layer(CompressionLayer::new()))
        .route(
            "/admin/settings/{tab}",
            get(admin_settings_tab).layer(CompressionLayer::new()),
        )
        .route("/admin/maintenance", get(crate::maintenance::old_page))
        .route("/admin/maintenance/delete", post(crate::maintenance::delete))
        .route("/admin/maintenance/dismiss", post(crate::maintenance::dismiss))
        .route("/admin/maintenance/undismiss", post(crate::maintenance::undismiss))
        .route("/admin/maintenance/clean", post(crate::maintenance::clean))
        .route("/admin/lookup", get(edit::lookup))
        .route("/user/login", get(login_page).post(login))
        .route("/user/logout", post(logout))
        .route("/api/progress", get(sync::list_progress))
        .route("/api/progress/events", get(sync::events))
        .route("/api/devices", get(sync::devices))
        .route("/api/stats", get(crate::stats::summary))
        .route("/api/stats/today", get(crate::stats::today))
        .route("/admin/stats", get(admin_stats).layer(CompressionLayer::new()))
        .route("/api/annotations", get(sync::list_annotations))
        .route("/api/highlights", get(crate::highlights::list))
        .route("/api/highlights/review", get(crate::highlights::daily))
        .route("/api/highlights/export", get(crate::highlights::export))
        .route(
            "/api/annotations/{document}",
            get(sync::get_annotations).put(sync::put_annotations),
        )
        .route("/api/annotations/{document}/export", get(sync::export_annotations))
        .route(
            "/api/bookmarks/{document}",
            get(sync::get_bookmarks).put(sync::put_bookmarks),
        )
        .route("/api/marks", axum::routing::put(sync::put_marks))
        .route("/api/devices/{device}", axum::routing::delete(sync::forget_device))
        .route(
            "/api/progress/{document}",
            get(sync::get_progress).put(sync::put_progress),
        )
        .route("/kosync/users/create", post(sync::kosync_register))
        .route("/kosync/users/auth", get(sync::kosync_auth))
        .route("/kosync/syncs/progress", axum::routing::put(sync::kosync_put))
        .route("/kosync/syncs/progress/{document}", get(sync::kosync_get))
        .route("/kosync/healthcheck", get(sync::kosync_health))
        .route("/static/{*path}", get(static_file))
        .route("/healthz", get(|| async { "ok" }))
        .layer(middleware::from_fn_with_state(state.clone(), refuse_when_read_only))
        .layer(middleware::from_fn_with_state(state.clone(), not_found_page))
        .layer(middleware::from_fn_with_state(state.clone(), require_auth))
        .with_state(state)
}

pub const READ_ONLY: &str = "The books folder is read-only: books cannot be uploaded, moved, renamed or deleted";

/// What changes the files in BOOKS_DIR. Edits, covers and progress go to
/// DATA_DIR, so they keep working when the books folder is read-only.
fn changes_books(method: &axum::http::Method, path: &str) -> bool {
    const PATHS: &[&str] = &[
        "/admin/upload",
        "/admin/delete",
        "/admin/rename",
        "/admin/bulk",
        "/admin/folder",
        "/admin/maintenance/delete",
        "/api/folders",
        "/api/folders/rename",
        "/api/folders/move",
        "/api/folders/delete",
        "/api/books/move",
        "/api/books/rename",
        "/api/books/delete",
    ];
    match *method {
        axum::http::Method::POST => PATHS.contains(&path),
        axum::http::Method::PUT => path.starts_with("/api/upload/"),
        _ => false,
    }
}

/// A read-only books folder (a NAS share mounted read-only, a container
/// volume with `:ro`) refuses every change with one clear message, instead
/// of the system's error for each file halfway through.
async fn refuse_when_read_only(State(state): State<AppState>, request: Request, next: Next) -> Response {
    if !changes_books(request.method(), request.uri().path()) || library_writable(&state).await {
        return next.run(request).await;
    }
    let path = request.uri().path();
    if path.starts_with("/api/") {
        return api_error(StatusCode::FORBIDDEN, "read_only", READ_ONLY);
    }
    let back = if path.starts_with("/admin/maintenance") {
        crate::maintenance::TAB
    } else {
        "/admin"
    };
    Redirect::to(&format!("{back}?error={}", paths::url_encode(READ_ONLY))).into_response()
}

pub async fn library_writable(state: &AppState) -> bool {
    let library = state.library.clone();
    tokio::task::spawn_blocking(move || library.writable())
        .await
        .unwrap_or(false)
}

/// A browser that opens a page that does not exist (a moved book, an old
/// link) gets a page of the panel with a way back instead of a blank one.
async fn not_found_page(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let wants_html = request.method() == axum::http::Method::GET
        && request
            .headers()
            .get(header::ACCEPT)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.contains("text/html"));
    let path = request.uri().path().to_string();
    let headers = request.headers().clone();
    let response = next.run(request).await;
    let raw = [
        "/static/",
        "/api/",
        "/kosync/",
        "/opds",
        "/download/",
        "/cover/",
        "/thumb/",
        "/page/",
        "/pages/",
        "/apps/",
    ];
    if response.status() != StatusCode::NOT_FOUND || !wants_html || raw.iter().any(|p| path.starts_with(p)) {
        return response;
    }
    let mut page = page(&state, &headers, "not_found", "Not found - OPDShelf", json!({}));
    *page.status_mut() = StatusCode::NOT_FOUND;
    page
}

async fn require_auth(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let path = request.uri().path();
    // KOReader's sync plugin logs in with its own headers, checked by each /kosync handler
    let public =
        path.starts_with("/static/") || path.starts_with("/kosync/") || path == "/user/login" || path == "/healthz";
    if public {
        return next.run(request).await;
    }
    if state.auth.is_authorized(request.headers()) {
        // Using the site keeps the session alive, so it only ends after 15 days without use
        let renew = crate::auth::cookie(request.headers(), crate::auth::SESSION_COOKIE).map(|_| {
            let secure = is_https(&state, request.headers());
            state.auth.session_cookie(secure)
        });
        let mut response = next.run(request).await;
        if let Some(cookie) = renew.and_then(|c| HeaderValue::from_str(&c).ok()) {
            if !response.headers().contains_key(header::SET_COOKIE) {
                response.headers_mut().append(header::SET_COOKIE, cookie);
            }
        }
        return response;
    }
    let wants_html = request
        .headers()
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.contains("text/html"));
    if wants_html {
        // After signing in, back to the page asked for
        let asked = request.uri().path_and_query().map_or("/admin", |p| p.as_str());
        let login = match safe_next(Some(asked)) {
            "/admin" => "/user/login".to_string(),
            next => format!("/user/login?next={}", paths::url_encode(next)),
        };
        return Redirect::to(&login).into_response();
    }
    // A page's own requests (fetch, EventSource) must not make the browser open its
    // username and password window: they are told to send the person to the login page
    let from_page = request
        .headers()
        .get("sec-fetch-dest")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v == "empty");
    if from_page {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Sign in again", "code": "login" })),
        )
            .into_response();
    }
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "Basic realm=\"OPDShelf\", charset=\"UTF-8\"")],
        "Unauthorized",
    )
        .into_response()
}

/// Public URL of the server, from BASE_URL or the request headers.
pub fn base_url(state: &AppState, headers: &HeaderMap) -> String {
    if let Some(url) = &state.config.base_url {
        return url.clone();
    }
    let header = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.split(',').next().unwrap_or(v).trim().to_string())
    };
    let proto = header("x-forwarded-proto").unwrap_or_else(|| "http".into());
    let host = header("x-forwarded-host")
        .or_else(|| header("host"))
        .unwrap_or_else(|| "localhost".into());
    format!("{proto}://{host}")
}

fn is_https(state: &AppState, headers: &HeaderMap) -> bool {
    base_url(state, headers).starts_with("https://")
}

pub fn page(state: &AppState, headers: &HeaderMap, view: &str, title: &str, data: serde_json::Value) -> Response {
    page_with_layout(state, headers, view, "layouts/main", title, data)
}

pub fn page_with_layout(
    state: &AppState,
    headers: &HeaderMap,
    view: &str,
    layout: &str,
    title: &str,
    data: serde_json::Value,
) -> Response {
    let mut data = data;
    let lang = Lang::from_headers(headers);
    let map = data.as_object_mut().expect("page data is an object");
    for key in ["notice", "error"] {
        if let Some(text) = map.get(key).and_then(|v| v.as_str()).map(|t| lang.message(t)) {
            map.insert(key.into(), json!(text));
        }
    }
    map.insert("title".into(), json!(lang.message(title)));
    map.insert("lang".into(), json!(lang.code()));
    map.insert("v".into(), json!(templates::asset_version()));
    map.insert("i18n".into(), json!(lang.table().to_string()));
    map.insert("username".into(), json!(state.config.admin_username));
    map.insert("csrf".into(), json!(state.auth.csrf_token()));
    map.insert("authEnabled".into(), json!(state.auth.enabled()));
    map.insert("baseUrl".into(), json!(base_url(state, headers)));
    let html = templates::render_with_layout(&state.templates, view, layout, data);
    (
        [
            (header::CACHE_CONTROL, "no-store"),
            (header::X_FRAME_OPTIONS, "DENY"),
            (header::REFERRER_POLICY, "same-origin"),
        ],
        Html(html),
    )
        .into_response()
}

/// Renders `view` on its own, for the parts of the panel that are swapped in with fetch.
pub fn fragment(state: &AppState, headers: &HeaderMap, view: &str, data: serde_json::Value) -> Response {
    let mut data = data;
    let map = data.as_object_mut().expect("fragment data is an object");
    map.insert("csrf".into(), json!(state.auth.csrf_token()));
    map.insert("lang".into(), json!(Lang::from_headers(headers).code()));
    let html = state.templates.render(view, &data).unwrap_or_else(|err| {
        tracing::error!("template {view}: {err}");
        String::from("Template error")
    });
    ([(header::CACHE_CONTROL, "no-store")], Html(html)).into_response()
}

pub fn admin_redirect(folder: &str, notice: Option<&str>, error: Option<&str>) -> Response {
    let mut url = String::from("/admin");
    let mut params = Vec::new();
    if !folder.is_empty() {
        params.push(format!("folder={}", paths::url_encode(folder)));
    }
    if let Some(n) = notice {
        params.push(format!("notice={}", paths::url_encode(n)));
    }
    if let Some(e) = error {
        params.push(format!("error={}", paths::url_encode(e)));
    }
    if !params.is_empty() {
        url = format!("{url}?{}", params.join("&"));
    }
    Redirect::to(&url).into_response()
}

/// The books of the library page as JSON for a `<script>` block: the page
/// builds only the rows it shows, which keeps it light with thousands of books.
/// A book as a row of the library page.
fn book_row(book: &Book) -> serde_json::Value {
    let flag = |on: bool| if on { "1" } else { "" };
    json!({
        "path": book.path,
        "title": book.title,
        "authors": book.author_line(),
        "authorList": book.authors.join("\n"),
        "folder": book.folder,
        "format": book.format,
        "added": book.modified.to_rfc3339(),
        "bytes": book.size.to_string(),
        "series": book.meta.series.as_deref().unwrap_or_default(),
        "index": book.meta.series_index.as_deref().unwrap_or_default(),
        "incomplete": flag(book.authors.is_empty() || !book.has_cover),
        "readable": flag(book.readable),
        "cover": if book.has_cover {
            format!("/thumb/{}?v={}", paths::url_encode(&book.path), covers::version(book))
        } else {
            String::new()
        },
    })
}

fn books_script(books: &[&Book]) -> String {
    let rows: Vec<_> = books.iter().map(|book| book_row(book)).collect();
    // No `<` at all, so no text can end the script block early
    serde_json::to_string(&rows)
        .unwrap_or_else(|_| "[]".into())
        .replace('<', "\\u003c")
}

pub fn book_json(book: &Book) -> serde_json::Value {
    json!({
        "path": book.path,
        "folder": book.folder,
        "fileName": book.file_name,
        "title": book.title,
        "authors": book.author_line(),
        // One per line, for the Authors shelf (a name can contain a comma)
        "authorList": book.authors.join("\n"),
        "authorNames": book.authors,
        "format": book.format,
        "size": format_size(book.size),
        "bytes": book.size,
        "series": book.meta.series,
        "seriesIndex": book.meta.series_index,
        "modified": book.modified.to_rfc3339(),
        "hasCover": book.has_cover,
        "coverVersion": covers::version(book),
        "readable": book.readable,
        "edited": book.edited,
        "noAuthors": book.authors.is_empty(),
        "incomplete": book.authors.is_empty() || !book.has_cover,
    })
}

// ---------------------------------------------------------------------------
// Admin

#[derive(Deserialize, Default)]
pub struct AdminQuery {
    sort: Option<String>,
    folder: Option<String>,
    notice: Option<String>,
    error: Option<String>,
    /// Set when the inspector asks for just its own HTML.
    panel: Option<String>,
    /// The inspector's tab: `notes` for the highlights and notes of the book.
    tab: Option<String>,
}

impl AdminQuery {
    pub fn wants_panel(&self) -> bool {
        self.panel.is_some()
    }
}

/// Everything the library page needs: the books, the folder tree and format counts.
pub async fn library_data(state: &AppState, query: &AdminQuery) -> serde_json::Value {
    let snapshot = state.library.snapshot().await;
    let mut books: Vec<&Book> = snapshot.books.iter().collect();
    let sort = SortMode::parse(query.sort.as_deref());
    sort.sort(&mut books);

    let mut extensions: Vec<String> = state
        .config
        .allowed_extensions
        .iter()
        .map(|e| format!(".{e}"))
        .collect();
    extensions.sort();
    let folder = query
        .folder
        .as_deref()
        .and_then(paths::sanitize)
        .filter(|f| snapshot.folders.contains(f))
        .unwrap_or_default();

    // Books under each folder, subfolders included, counted in one pass
    let mut counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for book in &books {
        let mut folder = book.folder.as_str();
        while !folder.is_empty() {
            *counts.entry(folder).or_default() += 1;
            folder = paths::parent(folder);
        }
    }
    let folder_tree: Vec<_> = snapshot
        .folders
        .iter()
        .map(|f| {
            let count = counts.get(f.as_str()).copied().unwrap_or(0);
            json!({
                "path": f,
                "name": paths::file_name(f),
                "depth": f.matches('/').count(),
                "count": count,
            })
        })
        .collect();
    let mut formats: Vec<(String, usize)> = Vec::new();
    for book in &books {
        match formats.iter_mut().find(|(name, _)| *name == book.format) {
            Some((_, count)) => *count += 1,
            None => formats.push((book.format.clone(), 1)),
        }
    }
    formats.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    json!({
        "books": !books.is_empty(),
        "booksJson": books_script(&books),
        "total": books.len(),
        "incompleteCount": books.iter().filter(|b| b.authors.is_empty() || !b.has_cover).count(),
        "folders": snapshot.folders,
        "folderTree": folder_tree,
        "formatCounts": formats.iter().map(|(name, count)| json!({ "name": name, "count": count })).collect::<Vec<_>>(),
        "folder": folder,
        "sortMode": sort.as_str(),
        "accept": extensions.join(","),
        "formats": extensions.iter().map(|e| e.trim_start_matches('.').to_uppercase()).collect::<Vec<_>>().join(", "),
        "notice": query.notice,
        "error": query.error,
        "readOnly": !library_writable(state).await,
        "apkLocal": apps::is_local(state, &apps::ANDROID),
        "pluginLocal": apps::is_local(state, &apps::KOREADER),
        "appsBeta": state.config.apps_channel == "beta",
        "about": about_data(state),
        "settingsTab": "general",
        "generalTab": true,
    })
}

/// The About tab of the settings: this server's version and folders.
fn about_data(state: &AppState) -> serde_json::Value {
    let config = &state.config;
    let full = |path: &std::path::Path| {
        std::fs::canonicalize(path)
            .unwrap_or_else(|_| path.to_path_buf())
            .display()
            .to_string()
    };
    json!({
        "version": env!("CARGO_PKG_VERSION"),
        "channel": config.apps_channel,
        "stable": config.apps_channel == "latest",
        "booksDir": full(&config.books_dir),
        "dataDir": full(&config.data_dir),
        "watch": config.watch,
        "scanMinutes": config.scan_interval_secs.div_ceil(60),
    })
}

/// Adds the selected book to the library data, for the inspector.
pub fn with_selection(mut data: serde_json::Value, selection: serde_json::Value) -> serde_json::Value {
    if let (Some(map), serde_json::Value::Object(extra)) = (data.as_object_mut(), selection) {
        map.extend(extra);
    }
    data
}

async fn admin(State(state): State<AppState>, headers: HeaderMap, Query(query): Query<AdminQuery>) -> Response {
    let data = library_data(&state, &query).await;
    page(&state, &headers, "admin", "OPDShelf", data)
}

/// The library page opened on reading sync: devices and live activity.
async fn admin_sync(State(state): State<AppState>, headers: HeaderMap, Query(query): Query<AdminQuery>) -> Response {
    let mut data = library_data(&state, &query).await;
    data["syncView"] = json!(true);
    data["shellPage"] = json!(true);
    page(&state, &headers, "admin", "Devices - OPDShelf", data)
}

/// The tabs of the settings, each with its own address.
const SETTINGS_TABS: [&str; 4] = ["general", "apps", "maintenance", "about"];

async fn admin_settings(state: State<AppState>, headers: HeaderMap, query: Query<AdminQuery>) -> Response {
    settings(state, headers, query, "general").await
}

async fn admin_settings_tab(
    state: State<AppState>,
    headers: HeaderMap,
    Path(tab): Path<String>,
    query: Query<AdminQuery>,
) -> Response {
    match SETTINGS_TABS.iter().find(|t| **t == tab && tab != "general") {
        Some(tab) => settings(state, headers, query, tab).await,
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// The library page opened on its settings. The Maintenance tab is worked out
/// only when it is asked for: on its own address, or as a fragment
/// (`?panel=1`) when the tab is opened from another page of the panel.
async fn settings(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AdminQuery>,
    tab: &str,
) -> Response {
    let maintenance = if tab == "maintenance" {
        match crate::maintenance::tab_data(&state).await {
            Ok(data) => Some(data),
            Err(err) => return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
        }
    } else {
        None
    };
    if query.wants_panel() {
        return match maintenance {
            Some(maint) => fragment(&state, &headers, "maintenance", json!({ "maint": maint })),
            None => StatusCode::NOT_FOUND.into_response(),
        };
    }
    let mut data = library_data(&state, &query).await;
    data["settingsView"] = json!(true);
    data["shellPage"] = json!(true);
    data["settingsTab"] = json!(tab);
    data["generalTab"] = json!(false);
    data[format!("{tab}Tab")] = json!(true);
    if let Some(maint) = maintenance {
        data["maint"] = maint;
    }
    page(&state, &headers, "admin", "Settings - OPDShelf", data)
}

async fn admin_highlights(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AdminQuery>,
) -> Response {
    let mut data = library_data(&state, &query).await;
    data["highlightsView"] = json!(true);
    data["shellPage"] = json!(true);
    page(&state, &headers, "admin", "Highlights - OPDShelf", data)
}

async fn admin_stats(State(state): State<AppState>, headers: HeaderMap, Query(query): Query<AdminQuery>) -> Response {
    let mut data = library_data(&state, &query).await;
    data["statsView"] = json!(true);
    data["shellPage"] = json!(true);
    page(&state, &headers, "admin", "Statistics - OPDShelf", data)
}

fn unique_destination(dir: &std::path::Path, name: &str) -> PathBuf {
    let candidate = dir.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let stem = paths::stem(name);
    let ext = paths::extension(name);
    (1..)
        .map(|i| {
            dir.join(if ext.is_empty() {
                format!("{stem} ({i})")
            } else {
                format!("{stem} ({i}).{ext}")
            })
        })
        .find(|p| !p.exists())
        .expect("some name is free")
}

/// Saves an uploaded book in `folder` (already sanitized) under a free name, answering where it went.
async fn save_book(
    state: &AppState,
    folder: &str,
    name: &str,
    body: impl futures_util::Stream<Item = std::io::Result<axum::body::Bytes>>,
) -> std::io::Result<String> {
    use futures_util::StreamExt;
    let dir = state.library.root().join(folder);
    tokio::fs::create_dir_all(&dir).await?;
    // Write to a hidden temp file first so half uploaded books never show up
    let tmp = dir.join(format!(".upload-{:016x}", rand::random::<u64>()));
    let result = async {
        let mut file = tokio::fs::File::create(&tmp).await?;
        let mut body = std::pin::pin!(body);
        while let Some(chunk) = body.next().await {
            file.write_all(&chunk?).await?;
        }
        file.flush().await?;
        let dest = unique_destination(&dir, name);
        tokio::fs::rename(&tmp, &dest).await?;
        Ok::<_, std::io::Error>(dest)
    }
    .await;
    match result {
        Ok(dest) => {
            state.library.invalidate();
            tracing::info!("uploaded {}", dest.display());
            let file = dest.file_name().and_then(|n| n.to_str()).unwrap_or(name);
            Ok(if folder.is_empty() {
                file.to_string()
            } else {
                format!("{folder}/{file}")
            })
        }
        Err(err) => {
            let _ = tokio::fs::remove_file(&tmp).await;
            Err(err)
        }
    }
}

/// Upload for apps: `PUT /api/upload/{folder/name.epub}` with the file as the body.
/// No CSRF token is needed: a PUT from another site needs a CORS preflight this server never allows.
/// Never overwrites; answers the path the book was saved under.
async fn api_upload(State(state): State<AppState>, Path(path): Path<String>, body: Body) -> Response {
    let Some(rel) = paths::sanitize(&path) else {
        return (StatusCode::BAD_REQUEST, axum::Json(json!({"error": "Invalid path"}))).into_response();
    };
    let folder = paths::parent(&rel).to_string();
    let name = paths::file_name(&rel).to_string();
    if !state.library.is_allowed(&name) {
        return (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            axum::Json(json!({"error": format!("Not a supported format: {name}")})),
        )
            .into_response();
    }
    // Raw bodies skip DefaultBodyLimit, so the limit is counted here
    let limit = state.config.max_upload_bytes;
    let mut received = 0usize;
    let stream = futures_util::StreamExt::map(body.into_data_stream(), move |chunk| {
        let chunk = chunk.map_err(std::io::Error::other)?;
        received += chunk.len();
        if received > limit {
            return Err(std::io::Error::other("The file is larger than the upload limit"));
        }
        Ok(chunk)
    });
    match save_book(&state, &folder, &name, stream).await {
        Ok(saved) => (StatusCode::CREATED, axum::Json(json!({"path": saved}))).into_response(),
        Err(err) => {
            tracing::warn!("upload of {rel} failed: {err}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(json!({"error": err.to_string()})),
            )
                .into_response()
        }
    }
}

async fn upload(State(state): State<AppState>, mut multipart: Multipart) -> Response {
    let mut csrf_ok = false;
    let mut folder = String::new();
    let mut saved = Vec::new();
    let mut rejected = Vec::new();

    loop {
        let mut field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(err) => return admin_redirect(&folder, None, Some(&format!("Upload failed: {err}"))),
        };
        match field.name() {
            Some("csrf") => csrf_ok = field.text().await.is_ok_and(|t| state.auth.check_csrf(&t)),
            Some("folder") => {
                let value = field.text().await.unwrap_or_default();
                if value.trim().is_empty() {
                    folder.clear();
                } else {
                    match paths::sanitize(&value) {
                        Some(f) => folder = f,
                        None => return admin_redirect("", None, Some("Invalid folder name")),
                    }
                }
            }
            Some("books") | Some("book") => {
                // The CSRF token and folder come before the files in the form
                if !csrf_ok {
                    return (StatusCode::FORBIDDEN, "Invalid CSRF token").into_response();
                }
                let Some(name) = field.file_name().and_then(paths::sanitize_name) else {
                    continue;
                };
                if !state.library.is_allowed(&name) {
                    rejected.push(name);
                    continue;
                }
                match save_book(
                    &state,
                    &folder,
                    &name,
                    futures_util::StreamExt::map(&mut field, |c| c.map_err(std::io::Error::other)),
                )
                .await
                {
                    Ok(_) => saved.push(name),
                    Err(err) => return admin_redirect(&folder, None, Some(&format!("Upload of {name} failed: {err}"))),
                }
            }
            _ => {}
        }
    }

    if !csrf_ok {
        return (StatusCode::FORBIDDEN, "Invalid CSRF token").into_response();
    }
    let notice = (!saved.is_empty()).then(|| format!("Uploaded {} book(s)", saved.len()));
    let error = (!rejected.is_empty()).then(|| format!("Not a supported format: {}", rejected.join(", ")));
    admin_redirect(&folder, notice.as_deref(), error.as_deref())
}

#[derive(Deserialize)]
struct DeleteForm {
    csrf: String,
    path: String,
}

async fn delete(State(state): State<AppState>, Form(form): Form<DeleteForm>) -> Response {
    if !state.auth.check_csrf(&form.csrf) {
        return (StatusCode::FORBIDDEN, "Invalid CSRF token").into_response();
    }
    let Some((rel, path)) = state.library.book_path(&form.path) else {
        return admin_redirect("", None, Some("Book not found"));
    };
    match delete_book(&state, &rel, &path).await {
        Ok(()) => admin_redirect(
            paths::parent(&rel),
            Some(&format!("Deleted {}", paths::file_name(&rel))),
            None,
        ),
        Err(err) => admin_redirect(paths::parent(&rel), None, Some(&format!("Delete failed: {err}"))),
    }
}

pub(crate) async fn delete_book(state: &AppState, rel: &str, path: &std::path::Path) -> std::io::Result<()> {
    tokio::fs::remove_file(path).await?;
    state.library.forget(rel);
    state.library.invalidate();
    if let Err(err) = state.library.store().remove(rel) {
        tracing::warn!("cannot remove edited metadata of {rel}: {err}");
    }
    tracing::info!("deleted {rel}");
    Ok(())
}

/// Moves a book to `new_rel`, taking along everything DATA_DIR keeps by its path.
async fn move_book(state: &AppState, rel: &str, old_path: &std::path::Path, new_rel: &str) -> Result<(), String> {
    let new_path = state.library.root().join(new_rel);
    if new_path.exists() && !same_entry(old_path, &new_path) {
        return Err(format!("{new_rel} already exists"));
    }
    let book = state.library.book(rel).await;
    let result = async {
        if let Some(parent) = new_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::rename(old_path, &new_path).await
    }
    .await;
    result.map_err(|err| err.to_string())?;
    carry_over(state, book.as_slice(), rel, new_rel);
    tracing::info!("renamed {rel} to {new_rel}");
    Ok(())
}

/// Whether two paths are the same file or folder, as when only the case of a
/// name changes on a disk that ignores case.
fn same_entry(a: &std::path::Path, b: &std::path::Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        match (std::fs::metadata(a), std::fs::metadata(b)) {
            (Ok(x), Ok(y)) => x.dev() == y.dev() && x.ino() == y.ino(),
            _ => false,
        }
    }
    #[cfg(not(unix))]
    {
        matches!((a.canonicalize(), b.canonicalize()), (Ok(x), Ok(y)) if x == y)
    }
}

/// After a book, or a folder with books in it, moved from `from` to `to`:
/// everything DATA_DIR keeps by path follows it. Edits, custom covers and
/// "not duplicates" choices are kept by path; the ids of the books and the
/// covers drawn from them are cached by path. Reading positions, highlights
/// and read marks are kept by the book's content, so they follow by themselves.
/// `books` are the books that moved, as they were before.
fn carry_over(state: &AppState, books: &[Book], from: &str, to: &str) {
    let store = state.library.store();
    state.library.rename_cached(from, to);
    if let Err(err) = store.rename(from, to) {
        tracing::warn!("cannot move the edited metadata of {from}: {err}");
    }
    state.documents.rename(from, to);
    crate::maintenance::rename_dismissed(store.dir(), from, to);
    for book in books {
        if let Some(new) = paths::relocate(&book.path, from, to) {
            covers::rename_cached(store, book, &new);
        }
    }
    state.library.invalidate();
}

/// Where a book goes when it is renamed to `name` in `folder` (`None` keeps
/// its folder, `""` is the root). The extension is kept.
fn renamed_path(rel: &str, name: &str, folder: Option<&str>) -> Result<String, &'static str> {
    let Some(mut name) = paths::sanitize_name(name.trim()) else {
        return Err("Invalid name");
    };
    let old_ext = paths::extension(rel);
    if !name.to_lowercase().ends_with(&format!(".{old_ext}")) {
        name = format!("{name}.{old_ext}");
    }
    let folder = match folder.map(str::trim) {
        None => paths::parent(rel).to_string(),
        Some("") => String::new(),
        Some(f) => paths::sanitize(f).ok_or("Invalid folder name")?,
    };
    Ok(if folder.is_empty() {
        name
    } else {
        format!("{folder}/{name}")
    })
}

#[derive(Deserialize)]
struct RenameForm {
    csrf: String,
    path: String,
    name: String,
    folder: Option<String>,
}

async fn rename(State(state): State<AppState>, Form(form): Form<RenameForm>) -> Response {
    if !state.auth.check_csrf(&form.csrf) {
        return (StatusCode::FORBIDDEN, "Invalid CSRF token").into_response();
    }
    let Some((rel, old_path)) = state.library.book_path(&form.path) else {
        return admin_redirect("", None, Some("Book not found"));
    };
    let new_rel = match renamed_path(&rel, &form.name, form.folder.as_deref()) {
        Ok(new_rel) => new_rel,
        Err(err) => return admin_redirect(paths::parent(&rel), None, Some(err)),
    };
    let folder = paths::parent(&new_rel).to_string();
    if new_rel == rel {
        return admin_redirect(&folder, None, None);
    }
    match move_book(&state, &rel, &old_path, &new_rel).await {
        Ok(()) => admin_redirect(&folder, Some(&format!("Renamed to {new_rel}")), None),
        Err(err) => admin_redirect(paths::parent(&rel), None, Some(&format!("Rename failed: {err}"))),
    }
}

#[derive(Deserialize)]
struct RenameRequest {
    path: String,
    name: String,
    /// Another folder to move it to (`""` for the root); left out, it stays in its own.
    folder: Option<String>,
}

/// Renames or moves one book for the panel and the apps. Answers its new path.
async fn api_rename_book(State(state): State<AppState>, Json(request): Json<RenameRequest>) -> Response {
    let Some((rel, old_path)) = state.library.book_path(&request.path) else {
        return api_error(StatusCode::NOT_FOUND, "not_found", "Book not found");
    };
    let new_rel = match renamed_path(&rel, &request.name, request.folder.as_deref()) {
        Ok(new_rel) => new_rel,
        Err(err) => return api_error(StatusCode::BAD_REQUEST, "invalid_name", err),
    };
    if new_rel != rel {
        if let Err(err) = move_book(&state, &rel, &old_path, &new_rel).await {
            let exists = state.library.root().join(&new_rel).exists();
            return if exists {
                api_error(StatusCode::CONFLICT, "exists", &err)
            } else {
                api_error(StatusCode::INTERNAL_SERVER_ERROR, "io", &err)
            };
        }
    }
    Json(json!({ "path": new_rel, "from": rel })).into_response()
}

#[derive(Deserialize)]
struct PathsRequest {
    paths: Vec<String>,
}

/// Deletes books for the panel, which then takes them out of the page without
/// reloading it. Answers the paths deleted and what failed.
async fn api_delete_books(State(state): State<AppState>, Json(request): Json<PathsRequest>) -> Response {
    let mut deleted = Vec::new();
    let mut failed = Vec::new();
    for path in &request.paths {
        let Some((rel, file)) = state.library.book_path(path) else {
            failed.push(format!("{path}: not found"));
            continue;
        };
        match delete_book(&state, &rel, &file).await {
            Ok(()) => deleted.push(rel),
            Err(err) => failed.push(format!("{}: {err}", paths::file_name(&rel))),
        }
    }
    let status = if deleted.is_empty() && !failed.is_empty() {
        StatusCode::BAD_REQUEST
    } else {
        StatusCode::OK
    };
    (status, Json(json!({ "deleted": deleted, "failed": failed }))).into_response()
}

/// The rows of the library page for some books, such as the ones just
/// uploaded, so the page adds them without reloading.
async fn api_book_rows(State(state): State<AppState>, Json(request): Json<PathsRequest>) -> Response {
    let snapshot = state.library.snapshot().await;
    let wanted: std::collections::HashSet<&str> = request.paths.iter().map(String::as_str).collect();
    let rows: Vec<_> = snapshot
        .books
        .iter()
        .filter(|b| wanted.contains(b.path.as_str()))
        .map(book_row)
        .collect();
    Json(json!({ "books": rows, "folders": snapshot.folders })).into_response()
}

/// A JSON error for the apps and the panel: `code` says what went wrong in
/// a way a program can tell apart, `error` says it in words.
fn api_error(status: StatusCode, code: &str, error: &str) -> Response {
    (status, Json(json!({ "error": error, "code": code }))).into_response()
}

/// Deletes or moves several books at once. The form repeats `path` once per book.
async fn bulk(State(state): State<AppState>, body: axum::body::Bytes) -> Response {
    let mut csrf = String::new();
    let mut action = String::new();
    let mut target = String::new();
    let mut selected = Vec::new();
    for (key, value) in form_urlencoded::parse(&body) {
        match key.as_ref() {
            "csrf" => csrf = value.into_owned(),
            "action" => action = value.into_owned(),
            "folder" => target = value.trim().to_string(),
            "path" => selected.push(value.into_owned()),
            _ => {}
        }
    }
    if !state.auth.check_csrf(&csrf) {
        return (StatusCode::FORBIDDEN, "Invalid CSRF token").into_response();
    }
    let folder = if target.is_empty() {
        String::new()
    } else {
        match paths::sanitize(&target) {
            Some(f) => f,
            None => return admin_redirect("", None, Some("Invalid folder name")),
        }
    };

    let (done, failed) = match action.as_str() {
        "move" => {
            let (moved, failed) = move_books(&state, &selected, &folder).await;
            (moved.len(), failed)
        }
        "delete" => {
            let mut done = 0;
            let mut failed = Vec::new();
            for path in &selected {
                let Some((rel, file)) = state.library.book_path(path) else {
                    failed.push(format!("{path}: not found"));
                    continue;
                };
                match delete_book(&state, &rel, &file).await {
                    Ok(()) => done += 1,
                    Err(err) => failed.push(format!("{}: {err}", paths::file_name(&rel))),
                }
            }
            (done, failed)
        }
        _ => return (StatusCode::BAD_REQUEST, "Unknown action").into_response(),
    };
    let notice = (done > 0).then(|| match action.as_str() {
        "delete" => format!("Deleted {done} book(s)"),
        _ => format!(
            "Moved {done} book(s) to {}",
            if folder.is_empty() { "the library root" } else { &folder }
        ),
    });
    let error = (!failed.is_empty()).then(|| failed.join("; "));
    let back = if action == "move" { folder.as_str() } else { "" };
    admin_redirect(back, notice.as_deref(), error.as_deref())
}

/// Creates `name` inside the folder `parent` (`""` for the root) and answers its path.
/// A folder that already exists is fine.
async fn create_folder(state: &AppState, parent: &str, name: &str) -> Result<String, String> {
    let Some((parent, _)) = state.library.folder_path(parent) else {
        return Err("Folder not found".into());
    };
    let Some(name) = paths::sanitize_name(name.trim()) else {
        return Err("Invalid folder name".into());
    };
    let rel = if parent.is_empty() {
        name
    } else {
        format!("{parent}/{name}")
    };
    let dir = state.library.root().join(&rel);
    if dir.is_dir() {
        return Ok(rel);
    }
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|err| format!("Cannot create folder: {err}"))?;
    state.library.invalidate();
    tracing::info!("created folder {rel}");
    Ok(rel)
}

/// Moves books into `folder` (`""` for the root). Answers where each book went
/// (from, to; the same for a book already there) and what failed.
async fn move_books(state: &AppState, selected: &[String], folder: &str) -> (Vec<(String, String)>, Vec<String>) {
    let mut moved = Vec::new();
    let mut failed = Vec::new();
    for path in selected {
        let Some((rel, file)) = state.library.book_path(path) else {
            failed.push(format!("{path}: not found"));
            continue;
        };
        let name = paths::file_name(&rel);
        let new_rel = if folder.is_empty() {
            name.to_string()
        } else {
            format!("{folder}/{name}")
        };
        if new_rel != rel {
            if let Err(err) = move_book(state, &rel, &file, &new_rel).await {
                failed.push(format!("{name}: {err}"));
                continue;
            }
        }
        moved.push((rel, new_rel));
    }
    (moved, failed)
}

// ---------------------------------------------------------------------------
// Folders for the apps

/// Every folder of the library, with how many books it holds (its subfolders included).
async fn api_folders(State(state): State<AppState>) -> Response {
    let snapshot = state.library.snapshot().await;
    let folders: Vec<_> = snapshot
        .folders
        .iter()
        .map(|f| json!({ "path": f, "name": paths::file_name(f), "count": snapshot.count_under(f) }))
        .collect();
    Json(json!({ "folders": folders })).into_response()
}

#[derive(Deserialize)]
struct NewFolder {
    #[serde(default)]
    parent: String,
    name: String,
}

async fn api_create_folder(State(state): State<AppState>, Json(request): Json<NewFolder>) -> Response {
    match create_folder(&state, &request.parent, &request.name).await {
        Ok(path) => (StatusCode::CREATED, Json(json!({ "path": path }))).into_response(),
        Err(err) => (StatusCode::BAD_REQUEST, Json(json!({ "error": err }))).into_response(),
    }
}

#[derive(Deserialize)]
struct MoveRequest {
    paths: Vec<String>,
    /// `""` for the root.
    #[serde(default)]
    folder: String,
}

/// Moves books into a folder, creating it if needed. Answers each book's new path.
async fn api_move_books(State(state): State<AppState>, Json(request): Json<MoveRequest>) -> Response {
    let folder = match request.folder.trim() {
        "" => String::new(),
        f => match paths::sanitize(f) {
            Some(f) => f,
            None => return (StatusCode::BAD_REQUEST, Json(json!({ "error": "Invalid folder name" }))).into_response(),
        },
    };
    let (moved, failed) = move_books(&state, &request.paths, &folder).await;
    let status = if moved.is_empty() && !failed.is_empty() {
        StatusCode::BAD_REQUEST
    } else {
        StatusCode::OK
    };
    let paths: Vec<_> = moved
        .iter()
        .map(|(from, to)| json!({ "from": from, "to": to }))
        .collect();
    (
        status,
        Json(json!({ "moved": moved.len(), "folder": folder, "failed": failed, "paths": paths })),
    )
        .into_response()
}

/// Why a folder cannot be renamed, moved or deleted: an HTTP status, a code
/// for programs and a message in English (translated by the panel).
struct Refusal(StatusCode, &'static str, String);

impl Refusal {
    fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self(status, code, message.into())
    }

    fn into_api(self) -> Response {
        api_error(self.0, self.1, &self.2)
    }
}

/// Moves the folder `from` into `parent` (`""` for the root) as `name`, which
/// renames it when the parent stays the same. Every book inside, in any
/// subfolder, takes its saved data along. Answers the folder's new path.
async fn move_folder(state: &AppState, from: &str, parent: &str, name: &str) -> Result<String, Refusal> {
    let not_found = || Refusal::new(StatusCode::NOT_FOUND, "not_found", "Folder not found");
    let Some((from, source)) = state.library.folder_path(from).filter(|(rel, _)| !rel.is_empty()) else {
        return Err(not_found());
    };
    let Some((parent, _)) = state.library.folder_path(parent) else {
        return Err(not_found());
    };
    let Some(name) = paths::sanitize_name(name.trim()) else {
        return Err(Refusal::new(
            StatusCode::BAD_REQUEST,
            "invalid_name",
            "Invalid folder name",
        ));
    };
    let to = if parent.is_empty() {
        name
    } else {
        format!("{parent}/{name}")
    };
    if to == from {
        return Ok(to);
    }
    if paths::is_within(&to, &from) {
        return Err(Refusal::new(
            StatusCode::BAD_REQUEST,
            "into_itself",
            "A folder cannot be moved into itself",
        ));
    }
    let dest = state.library.root().join(&to);
    // Only a change of case on a disk that ignores case finds itself here
    if dest.exists() && !same_entry(&source, &dest) {
        return Err(Refusal::new(
            StatusCode::CONFLICT,
            "exists",
            format!("{to} already exists"),
        ));
    }
    let snapshot = state.library.snapshot().await;
    let books: Vec<Book> = snapshot
        .books
        .iter()
        .filter(|b| paths::is_within(&b.folder, &from))
        .cloned()
        .collect();
    if let Err(err) = tokio::fs::rename(&source, &dest).await {
        tracing::warn!("cannot move folder {from} to {to}: {err}");
        return Err(Refusal::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "io",
            format!("Cannot move the folder: {err}"),
        ));
    }
    carry_over(state, &books, &from, &to);
    tracing::info!("moved folder {from} to {to} with {} book(s)", books.len());
    Ok(to)
}

/// Deletes a folder that holds no files at all, only maybe empty subfolders.
/// A folder with books, or any other file, is never deleted.
async fn delete_folder(state: &AppState, path: &str) -> Result<String, Refusal> {
    let Some((rel, dir)) = state.library.folder_path(path).filter(|(rel, _)| !rel.is_empty()) else {
        return Err(Refusal::new(StatusCode::NOT_FOUND, "not_found", "Folder not found"));
    };
    let result = tokio::task::spawn_blocking(move || -> Result<(), Refusal> {
        let not_empty = || Refusal::new(StatusCode::CONFLICT, "not_empty", "Only empty folders can be deleted");
        let mut dirs = Vec::new();
        for entry in walkdir::WalkDir::new(&dir).contents_first(true) {
            let entry = entry.map_err(|_| not_empty())?;
            if !entry.file_type().is_dir() {
                return Err(not_empty());
            }
            dirs.push(entry.into_path());
        }
        // Children come first; remove_dir refuses a folder a file was just added to
        for dir in dirs {
            std::fs::remove_dir(&dir).map_err(|err| match err.kind() {
                std::io::ErrorKind::DirectoryNotEmpty => not_empty(),
                _ => Refusal::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "io",
                    format!("Cannot delete the folder: {err}"),
                ),
            })?;
        }
        Ok(())
    })
    .await
    .unwrap_or_else(|_| {
        Err(Refusal::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "io",
            "Cannot delete the folder",
        ))
    });
    state.library.invalidate();
    result?;
    tracing::info!("deleted folder {rel}");
    Ok(rel)
}

#[derive(Deserialize)]
struct FolderRename {
    path: String,
    name: String,
}

async fn api_rename_folder(State(state): State<AppState>, Json(request): Json<FolderRename>) -> Response {
    let parent = paths::sanitize(&request.path)
        .map(|p| paths::parent(&p).to_string())
        .unwrap_or_default();
    folder_moved(
        &request.path,
        move_folder(&state, &request.path, &parent, &request.name).await,
    )
}

#[derive(Deserialize)]
struct FolderMove {
    path: String,
    /// The folder to move it into, `""` for the root.
    #[serde(default)]
    parent: String,
    /// A new name; left out, it keeps its own.
    name: Option<String>,
}

async fn api_move_folder(State(state): State<AppState>, Json(request): Json<FolderMove>) -> Response {
    let own = paths::sanitize(&request.path)
        .map(|p| paths::file_name(&p).to_string())
        .unwrap_or_default();
    let name = request.name.unwrap_or(own);
    folder_moved(
        &request.path,
        move_folder(&state, &request.path, &request.parent, &name).await,
    )
}

fn folder_moved(from: &str, result: Result<String, Refusal>) -> Response {
    match result {
        Ok(path) => Json(json!({ "path": path, "from": paths::sanitize(from).unwrap_or_default() })).into_response(),
        Err(refusal) => refusal.into_api(),
    }
}

#[derive(Deserialize)]
struct FolderPath {
    path: String,
}

async fn api_delete_folder(State(state): State<AppState>, Json(request): Json<FolderPath>) -> Response {
    match delete_folder(&state, &request.path).await {
        Ok(path) => Json(json!({ "deleted": path })).into_response(),
        Err(refusal) => refusal.into_api(),
    }
}

#[derive(Deserialize)]
struct FolderForm {
    csrf: String,
    action: String,
    /// The folder to delete, rename or move, or the one to create the new folder in (`""` for the root).
    #[serde(default)]
    path: String,
    #[serde(default)]
    name: String,
    /// Where `move` takes the folder (`""` for the root).
    #[serde(default)]
    parent: String,
}

/// Creates, renames or moves a folder, or deletes one that has nothing in it.
async fn folder_action(State(state): State<AppState>, Form(form): Form<FolderForm>) -> Response {
    if !state.auth.check_csrf(&form.csrf) {
        return (StatusCode::FORBIDDEN, "Invalid CSRF token").into_response();
    }
    let here = paths::sanitize(&form.path).unwrap_or_default();
    match form.action.as_str() {
        "create" => match create_folder(&state, &form.path, &form.name).await {
            Ok(rel) => admin_redirect(&rel, Some(&format!("Created folder {rel}")), None),
            Err(err) => admin_redirect(&here, None, Some(&err)),
        },
        "rename" | "move" => {
            let (parent, name) = if form.action == "rename" {
                (paths::parent(&here).to_string(), form.name.clone())
            } else {
                (form.parent.clone(), paths::file_name(&here).to_string())
            };
            match move_folder(&state, &form.path, &parent, &name).await {
                Ok(rel) => admin_redirect(&rel, Some(&format!("Moved folder to {rel}")), None),
                Err(Refusal(_, _, err)) => admin_redirect(&here, None, Some(&err)),
            }
        }
        "delete" => match delete_folder(&state, &form.path).await {
            Ok(rel) => admin_redirect(paths::parent(&rel), Some(&format!("Deleted folder {rel}")), None),
            Err(Refusal(status, _, err)) if status == StatusCode::NOT_FOUND => admin_redirect("", None, Some(&err)),
            Err(Refusal(_, _, err)) => admin_redirect(&here, None, Some(&err)),
        },
        _ => (StatusCode::BAD_REQUEST, "Unknown action").into_response(),
    }
}

// ---------------------------------------------------------------------------
// Books

async fn download(State(state): State<AppState>, Path(path): Path<String>, request: Request) -> Response {
    let Some((rel, file)) = state.library.book_path(&path) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    // ?inline=1 lets the web reader open the file instead of downloading it
    let inline = request
        .uri()
        .query()
        .is_some_and(|q| q.split('&').any(|p| p == "inline=1"));
    let mime = crate::library::mime_for(&rel);
    // Serving a file never fails: errors become responses (the error type is Infallible)
    let Ok(response) = ServeFile::new_with_mime(
        &file,
        &mime.parse().unwrap_or(mime_guess::mime::APPLICATION_OCTET_STREAM),
    )
    .oneshot(request)
    .await;
    let mut response = response.map(Body::new);
    let name = paths::file_name(&rel);
    let ascii: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_graphic() && c != '"' && c != '\\' || c == ' ' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let disposition = format!(
        "{}; filename=\"{ascii}\"; filename*=UTF-8''{}",
        if inline { "inline" } else { "attachment" },
        paths::url_encode(name)
    );
    if let Ok(value) = HeaderValue::from_str(&disposition) {
        response.headers_mut().insert(header::CONTENT_DISPOSITION, value);
    }
    response
}

async fn cover(state: State<AppState>, headers: HeaderMap, path: Path<String>) -> Response {
    serve_cover(state, headers, path, false).await
}

async fn thumb(state: State<AppState>, headers: HeaderMap, path: Path<String>) -> Response {
    serve_cover(state, headers, path, true).await
}

async fn serve_cover(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(path): Path<String>,
    thumbnail: bool,
) -> Response {
    let Some(book) = state.library.book(&path).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !book.has_cover {
        return StatusCode::NOT_FOUND.into_response();
    }
    let etag = covers::etag(&book);
    if headers.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok()) == Some(etag.as_str()) {
        return StatusCode::NOT_MODIFIED.into_response();
    }
    let Some((_, file)) = state.library.book_path(&book.path) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let store = state.library.store().clone();
    let image = tokio::task::spawn_blocking(move || {
        if thumbnail {
            covers::thumbnail(&store, &book, &file)
        } else {
            covers::cover(&store, &book, &file)
        }
    })
    .await
    .ok()
    .flatten();
    match image {
        Some((image, mime)) => (
            [
                (header::CONTENT_TYPE, mime),
                (header::CACHE_CONTROL, "private, max-age=86400".to_string()),
                (header::ETAG, etag),
            ],
            image,
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn info(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(path): Path<String>,
    Query(query): Query<AdminQuery>,
) -> Response {
    let Some(book) = state.library.book(&path).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let document = state.documents.id(&state.library, &book).await;
    let notes = query.tab.as_deref() == Some("notes");
    let highlights = document
        .as_deref()
        .map_or(0, |d| state.annotations.list(d).iter().filter(|a| !a.deleted).count());
    // A placeholder date (0101-01-01) is no date
    let mut meta = book.meta.clone();
    meta.date = meta.date.filter(|d| crate::metadata::plausible_date(d));
    let selection = json!({
        "book": book_json(&book),
        "document": document,
        "meta": meta,
        "mime": book.mime,
        "tab": if notes { "notes" } else { "info" },
        "infoTab": !notes,
        "notesTab": notes,
        "highlightCount": highlights,
        // Only an ISBN is labelled ISBN; a uuid or another id is an identifier
        "identifierIsIsbn": book.meta.identifier.as_deref().is_none_or(is_isbn),
    });
    if query.wants_panel() {
        return fragment(&state, &headers, "inspector", selection);
    }
    let data = with_selection(library_data(&state, &query).await, selection);
    page(&state, &headers, "admin", &format!("{} - OPDShelf", book.title), data)
}

/// Whether a book's identifier is an ISBN: 10 or 13 digits (an ISBN-10 may end
/// in X), with or without hyphens, spaces or an `isbn:` prefix.
pub fn is_isbn(identifier: &str) -> bool {
    let lower = identifier.trim().to_ascii_lowercase();
    let number = ["urn:isbn:", "isbn:", "isbn"]
        .iter()
        .find_map(|prefix| lower.strip_prefix(prefix))
        .unwrap_or(&lower);
    let chars: Vec<char> = number.chars().filter(|c| !matches!(c, '-' | ' ')).collect();
    match chars.len() {
        13 => chars.iter().all(char::is_ascii_digit),
        10 => chars[..9].iter().all(char::is_ascii_digit) && (chars[9].is_ascii_digit() || chars[9] == 'x'),
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Login

#[derive(Deserialize, Default)]
struct LoginQuery {
    error: Option<String>,
    username: Option<String>,
    next: Option<String>,
}

/// Where to go after signing in: a page of this server (a path, never another
/// site such as `//example.com`), or the panel.
fn safe_next(next: Option<&str>) -> &str {
    match next {
        // Kept as the browser sent it, percent-encoded, so it is a valid Location header
        Some(n)
            if n.starts_with('/')
                && !n.starts_with("//")
                && !n.starts_with("/user/")
                && n.bytes().all(|b| b.is_ascii_graphic() && b != b'\\') =>
        {
            n
        }
        _ => "/admin",
    }
}

async fn login_page(State(state): State<AppState>, headers: HeaderMap, Query(query): Query<LoginQuery>) -> Response {
    let next = safe_next(query.next.as_deref());
    if state.auth.is_authorized(&headers) {
        return Redirect::to(next).into_response();
    }
    page(
        &state,
        &headers,
        "login",
        "Sign in - OPDShelf",
        json!({ "error": query.error, "typedUsername": query.username, "next": (next != "/admin").then_some(next) }),
    )
}

#[derive(Deserialize)]
struct LoginForm {
    username: String,
    password: String,
    next: Option<String>,
}

async fn login(State(state): State<AppState>, headers: HeaderMap, Form(form): Form<LoginForm>) -> Response {
    let next = safe_next(form.next.as_deref());
    if !state.auth.check_credentials(&form.username, &form.password) {
        // Slow down password guessing
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        // Keep what was typed as the username so only the password has to be typed again
        let mut again = format!(
            "/user/login?error=Invalid%20username%20or%20password&username={}",
            paths::url_encode(form.username.chars().take(100).collect::<String>().as_str())
        );
        if next != "/admin" {
            again.push_str(&format!("&next={}", paths::url_encode(next)));
        }
        return Redirect::to(&again).into_response();
    }
    let cookie = state.auth.session_cookie(is_https(&state, &headers));
    ([(header::SET_COOKIE, cookie)], Redirect::to(next)).into_response()
}

#[derive(Deserialize)]
struct CsrfForm {
    csrf: String,
}

async fn logout(State(state): State<AppState>, Form(form): Form<CsrfForm>) -> Response {
    if !state.auth.check_csrf(&form.csrf) {
        return (StatusCode::FORBIDDEN, "Invalid CSRF token").into_response();
    }
    // A copy of the cookie stops working too
    state.auth.revoke_sessions();
    (
        [(header::SET_COOKIE, Auth::clear_cookie())],
        Redirect::to("/user/login"),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// Static files embedded in the binary

/// How long a static file may be kept. The pages ask for the app's own scripts and styles with
/// `?v=<version of this build>`, so those never change under one address and are kept for good;
/// the vendored libraries and images change only with an update, so a day is fine; anything else
/// of ours asked for without a version is checked every time, so a new build is never hidden
/// behind a day-old copy.
fn static_cache(path: &str, versioned: bool) -> &'static str {
    if versioned {
        "public, max-age=31536000, immutable"
    } else if path.starts_with("vendor/")
        || !(path.ends_with(".js") || path.ends_with(".css") || path.ends_with(".mjs"))
    {
        "public, max-age=86400"
    } else {
        "no-cache"
    }
}

async fn static_file(
    Path(path): Path<String>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Response {
    match Static::get(&path) {
        Some(file) => {
            let mime = match paths::extension(&path).as_str() {
                "mjs" => mime_guess::mime::APPLICATION_JAVASCRIPT,
                _ => mime_guess::from_path(&path).first_or_octet_stream(),
            };
            (
                [
                    (header::CONTENT_TYPE, mime.to_string()),
                    (
                        header::CACHE_CONTROL,
                        static_cache(&path, query.contains_key("v")).to_string(),
                    ),
                ],
                file.data,
            )
                .into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::{is_isbn, static_cache};

    #[test]
    fn a_new_build_is_never_hidden_behind_a_cached_script() {
        // Asked for by version: kept for good
        assert!(static_cache("app/notes.js", true).contains("immutable"));
        // Ours without a version: checked every time
        assert_eq!(static_cache("reader/xpointer.js", false), "no-cache");
        assert_eq!(static_cache("styles/app.css", false), "no-cache");
        // Libraries and images: a day
        assert_eq!(
            static_cache("vendor/epubjs/epub.min.js", false),
            "public, max-age=86400"
        );
        assert_eq!(static_cache("icon.png", false), "public, max-age=86400");
    }

    #[test]
    fn tells_isbns_from_other_identifiers() {
        assert!(is_isbn("9780316769488"));
        assert!(is_isbn("978-0-316-76948-8"));
        assert!(is_isbn("urn:isbn:0-316-76948-X"));
        assert!(is_isbn("ISBN 0316769487"));
        assert!(!is_isbn("urn:uuid:8f1d4b3c-0e3a-4a6f-9c1e-2b7d5a9e6f10"));
        assert!(!is_isbn("12345"));
        assert!(!is_isbn("calibre:42"));
    }
}
