use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Multipart, Path, Query, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Form, Router};
use handlebars::Handlebars;
use serde::Deserialize;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tower::ServiceExt;
use tower_http::services::ServeFile;

use crate::auth::Auth;
use crate::config::Config;
use crate::library::{format_size, Book, Library, SortMode};
use crate::lookup::Lookup;
use crate::store::Store;
use crate::templates::{self, Static};
use crate::{covers, edit, opds, paths, reader};

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub library: Library,
    pub auth: Auth,
    pub templates: Arc<Handlebars<'static>>,
    pub lookup: Lookup,
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
        config: Arc::new(config),
    };
    let upload_limit = state.config.max_upload_bytes;

    // Read every book's metadata in the background so the first feed request is fast
    if let Ok(runtime) = tokio::runtime::Handle::try_current() {
        let library = state.library.clone();
        let watch = state.config.watch;
        let interval = std::time::Duration::from_secs(state.config.scan_interval_secs);
        runtime.spawn(async move {
            if watch {
                crate::watch::start(library.clone(), interval);
            }
            let books = library.snapshot().await.books.len();
            tracing::info!("library ready: {books} books");
        });
    }

    Router::new()
        .route("/", get(opds::root))
        .route("/opds/books", get(opds::books))
        .route("/opds/folders", get(opds::folders_root))
        .route("/opds/folders/{*path}", get(opds::folder))
        .route("/opds/authors", get(opds::authors))
        .route("/opds/authors/{name}", get(opds::author))
        .route("/opds/search", get(opds::search))
        .route("/opds/opensearch.xml", get(opds::opensearch))
        .route("/download/{*path}", get(download))
        .route("/cover/{*path}", get(cover))
        .route("/thumb/{*path}", get(thumb))
        .route("/info/{*path}", get(info))
        .route("/read/{*path}", get(reader::read))
        .route("/pages/{*path}", get(reader::pages))
        .route("/page/{*path}", get(reader::page))
        .route("/admin", get(admin))
        .route("/admin/upload", post(upload).layer(DefaultBodyLimit::max(upload_limit)))
        .route("/admin/delete", post(delete))
        .route("/admin/rename", post(rename))
        .route("/admin/edit/{*path}", get(edit::edit_page))
        .route(
            "/admin/edit",
            post(edit::save).layer(DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route("/admin/reset", post(edit::reset))
        .route("/admin/lookup", get(edit::lookup))
        .route("/user/login", get(login_page).post(login))
        .route("/user/logout", post(logout))
        .route("/static/{*path}", get(static_file))
        .route("/healthz", get(|| async { "ok" }))
        .layer(middleware::from_fn_with_state(state.clone(), require_auth))
        .with_state(state)
}

async fn require_auth(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let path = request.uri().path();
    let public = path.starts_with("/static/") || path == "/user/login" || path == "/healthz";
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
    let map = data.as_object_mut().expect("page data is an object");
    map.insert("title".into(), json!(title));
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

pub fn book_json(book: &Book) -> serde_json::Value {
    json!({
        "path": book.path,
        "folder": book.folder,
        "fileName": book.file_name,
        "title": book.title,
        "authors": book.author_line(),
        "format": book.format,
        "size": format_size(book.size),
        "modified": book.modified.to_rfc3339(),
        "hasCover": book.has_cover,
        "coverVersion": covers::version(book),
        "readable": book.readable,
        "edited": book.edited,
    })
}

// ---------------------------------------------------------------------------
// Admin

#[derive(Deserialize)]
struct AdminQuery {
    sort: Option<String>,
    folder: Option<String>,
    notice: Option<String>,
    error: Option<String>,
}

async fn admin(State(state): State<AppState>, headers: HeaderMap, Query(query): Query<AdminQuery>) -> Response {
    let snapshot = state.library.scan().await;
    let mut books = snapshot.books.clone();
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
        .and_then(|f| paths::sanitize(&f))
        .filter(|f| snapshot.folders.contains(f))
        .unwrap_or_default();

    let data = json!({
        "books": books.iter().map(book_json).collect::<Vec<_>>(),
        "folders": snapshot.folders,
        "folder": folder,
        "sortMode": sort.as_str(),
        "accept": extensions.join(","),
        "formats": extensions.iter().map(|e| e.trim_start_matches('.').to_uppercase()).collect::<Vec<_>>().join(", "),
        "notice": query.notice,
        "error": query.error,
    });
    page(&state, &headers, "admin", "OPDShelf Admin", data)
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
                let dir = state.library.root().join(&folder);
                if let Err(err) = tokio::fs::create_dir_all(&dir).await {
                    return admin_redirect(&folder, None, Some(&format!("Cannot create folder: {err}")));
                }
                // Write to a hidden temp file first so half uploaded books never show up
                let tmp = dir.join(format!(".upload-{:016x}", rand::random::<u64>()));
                let result = async {
                    let mut file = tokio::fs::File::create(&tmp).await?;
                    while let Some(chunk) = field.chunk().await.map_err(std::io::Error::other)? {
                        file.write_all(&chunk).await?;
                    }
                    file.flush().await?;
                    let dest = unique_destination(&dir, &name);
                    tokio::fs::rename(&tmp, &dest).await?;
                    Ok::<_, std::io::Error>(dest)
                }
                .await;
                match result {
                    Ok(dest) => {
                        state.library.invalidate();
                        tracing::info!("uploaded {}", dest.display());
                        saved.push(name);
                    }
                    Err(err) => {
                        let _ = tokio::fs::remove_file(&tmp).await;
                        return admin_redirect(&folder, None, Some(&format!("Upload of {name} failed: {err}")));
                    }
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
    match tokio::fs::remove_file(&path).await {
        Ok(()) => {
            state.library.forget(&rel);
            state.library.invalidate();
            if let Err(err) = state.library.store().remove(&rel) {
                tracing::warn!("cannot remove edited metadata of {rel}: {err}");
            }
            tracing::info!("deleted {rel}");
            admin_redirect(
                paths::parent(&rel),
                Some(&format!("Deleted {}", paths::file_name(&rel))),
                None,
            )
        }
        Err(err) => admin_redirect(paths::parent(&rel), None, Some(&format!("Delete failed: {err}"))),
    }
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
    let new_path = state.library.root().join(&new_rel);
    if new_path.exists() {
        return admin_redirect(paths::parent(&rel), None, Some(&format!("{new_rel} already exists")));
    }
    let result = async {
        if let Some(parent) = new_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::rename(&old_path, &new_path).await
    }
    .await;
    match result {
        Ok(()) => {
            state.library.forget(&rel);
            state.library.invalidate();
            if let Err(err) = state.library.store().rename(&rel, &new_rel) {
                tracing::warn!("cannot move edited metadata of {rel}: {err}");
            }
            tracing::info!("renamed {rel} to {new_rel}");
            admin_redirect(&folder, Some(&format!("Renamed to {new_rel}")), None)
        }
        Err(err) => admin_redirect(paths::parent(&rel), None, Some(&format!("Rename failed: {err}"))),
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
    let mut response = match ServeFile::new_with_mime(
        &file,
        &mime.parse().unwrap_or(mime_guess::mime::APPLICATION_OCTET_STREAM),
    )
    .oneshot(request)
    .await
    {
        Ok(response) => response.map(Body::new),
        Err(err) => return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    };
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

async fn info(State(state): State<AppState>, headers: HeaderMap, Path(path): Path<String>) -> Response {
    let Some(book) = state.library.book(&path).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let data = json!({
        "book": book_json(&book),
        "meta": book.meta,
        "mime": book.mime,
    });
    page(
        &state,
        &headers,
        "book_details",
        &format!("{} - OPDShelf", book.title),
        data,
    )
}

// ---------------------------------------------------------------------------
// Login

async fn login_page(State(state): State<AppState>, headers: HeaderMap, Query(query): Query<AdminQuery>) -> Response {
    if state.auth.is_authorized(&headers) {
        return Redirect::to("/admin").into_response();
    }
    page(
        &state,
        &headers,
        "login",
        "Login - OPDShelf",
        json!({ "error": query.error }),
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
        return Redirect::to("/user/login?error=Invalid%20username%20or%20password").into_response();
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
