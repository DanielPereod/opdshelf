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

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use std::path::PathBuf;
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
    let mut response = match ServeFile::new_with_mime(&path, &mime).oneshot(request).await {
        Ok(response) => response.map(Body::new),
        Err(err) => return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    };
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

pub async fn koreader_plugin(State(state): State<AppState>, request: Request) -> Response {
    serve(&state, &KOREADER, request).await
}
