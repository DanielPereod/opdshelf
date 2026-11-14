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
}

pub fn router(config: Config) -> Router {
    let state = AppState {
        library: Library::new(
            config.books_dir.clone(),
            config.allowed_extensions.clone(),
            Store::open(&config.data_dir),
        ),
        auth: Auth::new(&config),
        templates: Arc::new(templates::registry()),
        lookup: Lookup::new(&config.openlibrary_url, &config.google_books_url),
        progress: Progress::open(&config.data_dir),
        marks: crate::marks::Marks::open(&config.data_dir),
        documents: DocumentIds::open(&config.data_dir),
        presence: sync::Presence::default(),
        annotations: crate::annotations::Annotations::open(&config.data_dir),
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
        .route("/api/books/move", post(api_move_books))
        .route("/admin/edit/{*path}", get(edit::edit_page))
        .route(
            "/admin/edit",
            post(edit::save).layer(DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route("/admin/reset", post(edit::reset))
        .route("/admin/maintenance", get(crate::maintenance::maintenance_page))
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
        .route("/admin/stats", get(admin_stats))
        .route("/api/annotations", get(sync::list_annotations))
        .route("/api/highlights", get(crate::highlights::list))
        .route("/api/highlights/review", get(crate::highlights::daily))
        .route("/api/highlights/export", get(crate::highlights::export))
        .route(
            "/api/annotations/{document}",
            get(sync::get_annotations).put(sync::put_annotations),
        )
        .route("/api/annotations/{document}/export", get(sync::export_annotations))
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
        .layer(middleware::from_fn_with_state(state.clone(), not_found_page))
        .layer(middleware::from_fn_with_state(state.clone(), require_auth))
        .with_state(state)
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
    if public || state.auth.is_authorized(request.headers()) {
        return next.run(request).await;
    }
    let wants_html = request
        .headers()
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.contains("text/html"));
    if wants_html {
        return Redirect::to("/user/login").into_response();
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
fn books_script(books: &[&Book]) -> String {
    let flag = |on: bool| if on { "1" } else { "" };
    let rows: Vec<_> = books
        .iter()
        .map(|book| {
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
        })
        .collect();
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
        "apkLocal": apps::is_local(state, &apps::ANDROID),
        "pluginLocal": apps::is_local(state, &apps::KOREADER),
        "appsBeta": state.config.apps_channel == "beta",
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

/// The library page opened on reading sync: devices, live activity and how to connect apps.
async fn admin_sync(State(state): State<AppState>, headers: HeaderMap, Query(query): Query<AdminQuery>) -> Response {
    let mut data = library_data(&state, &query).await;
    data["syncView"] = json!(true);
    page(&state, &headers, "admin", "Sync - OPDShelf", data)
}

async fn admin_highlights(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AdminQuery>,
) -> Response {
    let mut data = library_data(&state, &query).await;
    data["highlightsView"] = json!(true);
    page(&state, &headers, "admin", "Highlights - OPDShelf", data)
}

async fn admin_stats(State(state): State<AppState>, headers: HeaderMap, Query(query): Query<AdminQuery>) -> Response {
    let mut data = library_data(&state, &query).await;
    data["statsView"] = json!(true);
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

/// Moves a book to `new_rel`, taking its edited metadata along.
async fn move_book(state: &AppState, rel: &str, old_path: &std::path::Path, new_rel: &str) -> Result<(), String> {
    let new_path = state.library.root().join(new_rel);
    if new_path.exists() {
        return Err(format!("{new_rel} already exists"));
    }
    let result = async {
        if let Some(parent) = new_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::rename(old_path, &new_path).await
    }
    .await;
    result.map_err(|err| err.to_string())?;
    state.library.forget(rel);
    state.library.invalidate();
    if let Err(err) = state.library.store().rename(rel, new_rel) {
        tracing::warn!("cannot move edited metadata of {rel}: {err}");
    }
    tracing::info!("renamed {rel} to {new_rel}");
    Ok(())
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
    let Some(mut name) = paths::sanitize_name(&form.name) else {
        return admin_redirect(paths::parent(&rel), None, Some("Invalid name"));
    };
    let old_ext = paths::extension(&rel);
    if !name.to_lowercase().ends_with(&format!(".{old_ext}")) {
        name = format!("{name}.{old_ext}");
    }
    let folder = match form.folder.as_deref().map(str::trim) {
        None => paths::parent(&rel).to_string(),
        Some("") => String::new(),
        Some(f) => match paths::sanitize(f) {
            Some(f) => f,
            None => return admin_redirect(paths::parent(&rel), None, Some("Invalid folder name")),
        },
    };
    let new_rel = if folder.is_empty() {
        name
    } else {
        format!("{folder}/{name}")
    };
    if new_rel == rel {
        return admin_redirect(&folder, None, None);
    }
    match move_book(&state, &rel, &old_path, &new_rel).await {
        Ok(()) => admin_redirect(&folder, Some(&format!("Renamed to {new_rel}")), None),
        Err(err) => admin_redirect(paths::parent(&rel), None, Some(&format!("Rename failed: {err}"))),
    }
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
        "move" => move_books(&state, &selected, &folder).await,
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

/// Moves books into `folder` (`""` for the root). Answers how many moved and what failed.
async fn move_books(state: &AppState, selected: &[String], folder: &str) -> (usize, Vec<String>) {
    let mut done = 0;
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
        if new_rel == rel {
            done += 1;
            continue;
        }
        match move_book(state, &rel, &file, &new_rel).await {
            Ok(()) => done += 1,
            Err(err) => failed.push(format!("{name}: {err}")),
        }
    }
    (done, failed)
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
    let status = if moved == 0 && !failed.is_empty() {
        StatusCode::BAD_REQUEST
    } else {
        StatusCode::OK
    };
    (
        status,
        Json(json!({ "moved": moved, "folder": folder, "failed": failed })),
    )
        .into_response()
}

#[derive(Deserialize)]
struct FolderForm {
    csrf: String,
    action: String,
    /// The folder to delete, or the one to create the new folder in (`""` for the root).
    #[serde(default)]
    path: String,
    #[serde(default)]
    name: String,
}

/// Creates a folder, or deletes one that has nothing in it.
async fn folder_action(State(state): State<AppState>, Form(form): Form<FolderForm>) -> Response {
    if !state.auth.check_csrf(&form.csrf) {
        return (StatusCode::FORBIDDEN, "Invalid CSRF token").into_response();
    }
    match form.action.as_str() {
        "create" => match create_folder(&state, &form.path, &form.name).await {
            Ok(rel) => admin_redirect(&rel, Some(&format!("Created folder {rel}")), None),
            Err(err) => admin_redirect(paths::sanitize(&form.path).as_deref().unwrap_or(""), None, Some(&err)),
        },
        "delete" => {
            let Some((rel, dir)) = state.library.folder_path(&form.path).filter(|(rel, _)| !rel.is_empty()) else {
                return admin_redirect("", None, Some("Folder not found"));
            };
            // remove_dir refuses folders with anything in them, so no book is ever lost here
            if let Err(err) = tokio::fs::remove_dir(&dir).await {
                tracing::warn!("cannot delete folder {rel}: {err}");
                return admin_redirect(&rel, None, Some("Only empty folders can be deleted"));
            }
            state.library.invalidate();
            tracing::info!("deleted folder {rel}");
            admin_redirect(paths::parent(&rel), Some(&format!("Deleted folder {rel}")), None)
        }
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
    let selection = json!({
        "book": book_json(&book),
        "document": document,
        "meta": book.meta,
        "mime": book.mime,
        "tab": if notes { "notes" } else { "info" },
        "infoTab": !notes,
        "notesTab": notes,
        "highlightCount": highlights,
    });
    if query.wants_panel() {
        return fragment(&state, &headers, "inspector", selection);
    }
    let data = with_selection(library_data(&state, &query).await, selection);
    page(&state, &headers, "admin", &format!("{} - OPDShelf", book.title), data)
}

// ---------------------------------------------------------------------------
// Login

#[derive(Deserialize, Default)]
struct LoginQuery {
    error: Option<String>,
    username: Option<String>,
}

async fn login_page(State(state): State<AppState>, headers: HeaderMap, Query(query): Query<LoginQuery>) -> Response {
    if state.auth.is_authorized(&headers) {
        return Redirect::to("/admin").into_response();
    }
    page(
        &state,
        &headers,
        "login",
        "Sign in - OPDShelf",
        json!({ "error": query.error, "typedUsername": query.username }),
    )
}

#[derive(Deserialize)]
struct LoginForm {
    username: String,
    password: String,
}

async fn login(State(state): State<AppState>, headers: HeaderMap, Form(form): Form<LoginForm>) -> Response {
    if !state.auth.check_credentials(&form.username, &form.password) {
        // Slow down password guessing
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        // Keep what was typed as the username so only the password has to be typed again
        return Redirect::to(&format!(
            "/user/login?error=Invalid%20username%20or%20password&username={}",
            paths::url_encode(form.username.chars().take(100).collect::<String>().as_str())
        ))
        .into_response();
    }
    let cookie = state.auth.session_cookie(is_https(&state, &headers));
    ([(header::SET_COOKIE, cookie)], Redirect::to("/admin")).into_response()
}

#[derive(Deserialize)]
struct CsrfForm {
    csrf: String,
}

async fn logout(State(state): State<AppState>, Form(form): Form<CsrfForm>) -> Response {
    if !state.auth.check_csrf(&form.csrf) {
        return (StatusCode::FORBIDDEN, "Invalid CSRF token").into_response();
    }
    (
        [(header::SET_COOKIE, Auth::clear_cookie())],
        Redirect::to("/user/login"),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// Static files embedded in the binary

async fn static_file(Path(path): Path<String>) -> Response {
    match Static::get(&path) {
        Some(file) => {
            let mime = match paths::extension(&path).as_str() {
                "mjs" => mime_guess::mime::APPLICATION_JAVASCRIPT,
                _ => mime_guess::from_path(&path).first_or_octet_stream(),
            };
            (
                [
                    (header::CONTENT_TYPE, mime.to_string()),
                    (header::CACHE_CONTROL, "public, max-age=86400".to_string()),
                ],
                file.data,
            )
                .into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
