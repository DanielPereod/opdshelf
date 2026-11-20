//! Downloads of OPDShelf's own reading apps from the web panel.
//!
//! The Android app and the KOReader plugin are published as GitHub releases
//! (see .github/workflows/release-apps.yml), so they can be updated without
//! updating the server: the panel's download links send the browser to the
//! latest release. A file copied to DATA_DIR/apps takes its place, for
//! servers whose devices cannot reach GitHub or that want another build.
//!
//! Each channel has its own releases: `android-latest` and `koreader-latest`
//! come from main, `android-beta` and `koreader-beta` from the beta branch.
//! The Docker image of each branch sets APPS_CHANNEL to match.
//!
//! The KOReader plugin is served by the server itself: it keeps a copy of the
//! release in DATA_DIR/apps/cache and adds to each download a file with this
//! server's address and login, which the plugin reads the first time it
//! starts. The plugin updates itself from here too, after asking
//! /apps/versions.json which version the channel is on.

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Json, Redirect, Response};
use serde_json::json;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tower::ServiceExt;
use tower_http::services::ServeFile;

use crate::web::AppState;

/// Where the releases are published, unless APPS_URL says otherwise.
pub const DEFAULT_APPS_URL: &str = "https://github.com/DanielPereod/opdshelf/releases/download";

pub struct App {
    pub file: &'static str,
    /// The release tag is `<name>-<channel>`.
    name: &'static str,
    mime: &'static str,
}

pub const ANDROID: App = App {
    file: "opdshelf.apk",
    name: "android",
    mime: "application/vnd.android.package-archive",
};

pub const KOREADER: App = App {
    file: "opdshelf.koplugin.zip",
    name: "koreader",
    mime: "application/zip",
};

/// A copy of the app on this server, when there is one.
fn local(state: &AppState, app: &App) -> Option<PathBuf> {
    let path = state.config.data_dir.join("apps").join(app.file);
    path.is_file().then_some(path)
}

/// Where the latest release of an app is downloaded from.
pub fn release_url(state: &AppState, app: &App) -> String {
    let config = &state.config;
    format!("{}/{}-{}/{}", config.apps_url, app.name, config.apps_channel, app.file)
}

/// Whether the panel offers the app from this server instead of the release.
pub fn is_local(state: &AppState, app: &App) -> bool {
    local(state, app).is_some()
}

async fn serve(state: &AppState, app: &App, request: Request) -> Response {
    let Some(path) = local(state, app) else {
        return Redirect::temporary(&release_url(state, app)).into_response();
    };
    let mime = app.mime.parse().expect("valid mime");
    // Serving a file never fails: errors become responses (the error type is Infallible)
    let Ok(response) = ServeFile::new_with_mime(&path, &mime).oneshot(request).await;
    let mut response = response.map(Body::new);
    let headers = response.headers_mut();
    if let Ok(value) = HeaderValue::from_str(&format!("attachment; filename=\"{}\"", app.file)) {
        headers.insert(header::CONTENT_DISPOSITION, value);
    }
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    response
}

pub async fn android_app(State(state): State<AppState>, request: Request) -> Response {
    serve(&state, &ANDROID, request).await
}

/// The file added to the plugin with the server's address and login.
const SETUP_FILE: &str = "opdshelf_setup.json";
/// How long a copy of the release is used before asking GitHub again.
const CACHE_FRESH: Duration = Duration::from_secs(10 * 60);
/// One download of the release at a time.
static REFRESH: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn cached(state: &AppState, app: &App) -> PathBuf {
    state.config.data_dir.join("apps").join("cache").join(app.file)
}

fn is_fresh(path: &Path) -> bool {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_some_and(|age| age < CACHE_FRESH)
}

async fn download(url: &str, path: &Path) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;
    let response = client.get(url).send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }
    let bytes = response.bytes().await.map_err(|e| e.to_string())?;
    zip::ZipArchive::new(Cursor::new(&bytes)).map_err(|e| e.to_string())?;
    let dir = path.parent().expect("cache file has a folder");
    tokio::fs::create_dir_all(dir).await.map_err(|e| e.to_string())?;
    let tmp = path.with_extension("part");
    tokio::fs::write(&tmp, &bytes).await.map_err(|e| e.to_string())?;
    tokio::fs::rename(&tmp, path).await.map_err(|e| e.to_string())
}

/// The plugin as released: the copy in DATA_DIR/apps, or the latest release,
/// kept in the cache (an older copy if GitHub cannot be reached).
async fn plugin_zip(state: &AppState) -> Option<Vec<u8>> {
    let path = match local(state, &KOREADER) {
        Some(path) => path,
        None => {
            let path = cached(state, &KOREADER);
            let _guard = REFRESH.lock().await;
            if !is_fresh(&path) {
                if let Err(err) = download(&release_url(state, &KOREADER), &path).await {
                    tracing::warn!("cannot download the KOReader plugin: {err}");
                }
            }
            path
        }
    };
    tokio::fs::read(path).await.ok()
}

/// The folder inside the zip that holds the plugin (`opdshelf.koplugin/`).
fn plugin_root(archive: &zip::ZipArchive<Cursor<&[u8]>>) -> Option<String> {
    archive
        .file_names()
        .filter_map(|name| name.strip_suffix("main.lua"))
        .filter(|root| root.is_empty() || root.ends_with('/'))
        .min_by_key(|root| root.len())
        .map(str::to_string)
}

/// The plugin with the setup file added.
fn with_setup(zip: &[u8], setup: &[u8]) -> Result<Vec<u8>, String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(zip)).map_err(|e| e.to_string())?;
    let root = plugin_root(&archive).ok_or("the zip has no main.lua")?;
    let name = format!("{root}{SETUP_FILE}");
    let mut out = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for i in 0..archive.len() {
        let file = archive.by_index_raw(i).map_err(|e| e.to_string())?;
        if file.name() != name {
            out.raw_copy_file(file).map_err(|e| e.to_string())?;
        }
    }
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    out.start_file(name, options).map_err(|e| e.to_string())?;
    out.write_all(setup).map_err(|e| e.to_string())?;
    Ok(out.finish().map_err(|e| e.to_string())?.into_inner())
}

/// The version stamped in the plugin's `_meta.lua` by the release workflow.
pub fn plugin_version(zip: &[u8]) -> Option<u64> {
    let mut archive = zip::ZipArchive::new(Cursor::new(zip)).ok()?;
    let root = plugin_root(&archive)?;
    let mut meta = String::new();
    archive
        .by_name(&format!("{root}_meta.lua"))
        .ok()?
        .read_to_string(&mut meta)
        .ok()?;
    let rest = meta.split("version").nth(1)?;
    let rest = rest
        .trim_start()
        .strip_prefix('=')?
        .trim_start()
        .trim_start_matches('"');
    rest.split(|c: char| !c.is_ascii_digit()).next()?.parse().ok()
}

/// This server's address and login, as the apps need them.
fn login(state: &AppState, headers: &HeaderMap) -> (String, Option<String>, Option<String>) {
    let config = &state.config;
    let (username, password) = if state.auth.enabled() {
        (config.admin_username.clone(), config.admin_password.clone())
    } else {
        (None, None)
    };
    (crate::web::base_url(state, headers), username, password)
}

fn private(mut response: Response) -> Response {
    // It holds the password
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

pub async fn koreader_plugin(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(zip) = plugin_zip(&state).await else {
        return Redirect::temporary(&release_url(&state, &KOREADER)).into_response();
    };
    let (server, username, password) = login(&state, &headers);
    let setup = json!({ "server": server, "username": username, "password": password }).to_string();
    match tokio::task::spawn_blocking(move || with_setup(&zip, setup.as_bytes())).await {
        Ok(Ok(bytes)) => private(
            (
                [
                    (header::CONTENT_TYPE, KOREADER.mime.to_string()),
                    (
                        header::CONTENT_DISPOSITION,
                        format!("attachment; filename=\"{}\"", KOREADER.file),
                    ),
                ],
                bytes,
            )
                .into_response(),
        ),
        Ok(Err(err)) => {
            tracing::warn!("cannot prepare the KOReader plugin: {err}");
            (StatusCode::BAD_GATEWAY, err).into_response()
        }
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    }
}

/// The version of the plugin offered on this server's channel, for its updates.
pub async fn versions(State(state): State<AppState>) -> Response {
    let version = plugin_zip(&state).await.and_then(|zip| plugin_version(&zip));
    let mut response = Json(json!({ "koreader": { "version": version } })).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    response
}

/// The link the Android app opens from the panel's QR code.
pub fn connect_link(server: &str, username: Option<&str>, password: Option<&str>) -> String {
    let mut query = form_urlencoded::Serializer::new(String::new());
    query.append_pair("server", server);
    if let Some(username) = username {
        query.append_pair("username", username);
    }
    if let Some(password) = password {
        query.append_pair("password", password);
    }
    format!("opdshelf://connect?{}", query.finish())
}

/// A QR code with the server and login, scanned from the Android app's welcome screen.
pub async fn connect_qr(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let (server, username, password) = login(&state, &headers);
    let link = connect_link(&server, username.as_deref(), password.as_deref());
    let Ok(code) = qrcode::QrCode::new(link.as_bytes()) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let svg = code
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(240, 240)
        .dark_color(qrcode::render::svg::Color("#000000"))
        .light_color(qrcode::render::svg::Color("#ffffff"))
        .build();
    private(([(header::CONTENT_TYPE, "image/svg+xml")], svg).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plugin(meta: &str) -> Vec<u8> {
        let mut out = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        for (name, text) in [
            ("opdshelf.koplugin/main.lua", "return {}"),
            ("opdshelf.koplugin/_meta.lua", meta),
            ("opdshelf.koplugin/opdshelf_setup.json", "{}"),
        ] {
            out.start_file(name, options).unwrap();
            out.write_all(text.as_bytes()).unwrap();
        }
        out.finish().unwrap().into_inner()
    }

    #[test]
    fn setup_is_added_to_the_plugin() {
        let zip = with_setup(&plugin("return {}"), br#"{"server":"http://x"}"#).unwrap();
        let mut archive = zip::ZipArchive::new(Cursor::new(&zip[..])).unwrap();
        assert_eq!(archive.len(), 3);
        let mut setup = String::new();
        archive
            .by_name("opdshelf.koplugin/opdshelf_setup.json")
            .unwrap()
            .read_to_string(&mut setup)
            .unwrap();
        assert_eq!(setup, r#"{"server":"http://x"}"#);
    }

    #[test]
    fn version_comes_from_meta() {
        assert_eq!(
            plugin_version(&plugin(
                "return {\n    name = \"opdshelf\",\n    version = 202611141233,\n}"
            )),
            Some(202611141233)
        );
        assert_eq!(plugin_version(&plugin("return { name = \"opdshelf\" }")), None);
    }

    #[test]
    fn connect_link_encodes_the_login() {
        assert_eq!(
            connect_link("http://host:3000", Some("ana"), Some("p&ss word")),
            "opdshelf://connect?server=http%3A%2F%2Fhost%3A3000&username=ana&password=p%26ss+word"
        );
        assert_eq!(
            connect_link("http://h", None, None),
            "opdshelf://connect?server=http%3A%2F%2Fh"
        );
    }
}
