//! Downloads of OPDShelf's own reading apps from the web panel.
//!
//! The KOReader plugin is embedded in the binary and zipped on request, so it
//! always matches the server. The Android app is too big to embed: the
//! Docker image carries it in /app/apps (built by the publish workflow), and
//! an APK copied to DATA_DIR/apps/opdshelf.apk takes its place.

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;
use tower::ServiceExt;
use tower_http::services::ServeFile;

use crate::web::AppState;

#[derive(RustEmbed)]
#[folder = "koreader/opdshelf.koplugin/"]
struct Plugin;

pub const PLUGIN_FILE: &str = "opdshelf.koplugin.zip";
pub const APK_FILE: &str = "opdshelf.apk";

/// Where the Docker image keeps the Android app built with it.
const BUNDLED_APPS: &str = "/app/apps";

/// Where the Android app is: one added to DATA_DIR wins over the bundled one.
pub fn apk_path(state: &AppState) -> PathBuf {
    let own = state.config.data_dir.join("apps").join(APK_FILE);
    let bundled = std::path::Path::new(BUNDLED_APPS).join(APK_FILE);
    if !own.is_file() && bundled.is_file() {
        return bundled;
    }
    own
}

/// Size in bytes of the Android app, when it is there to download.
pub fn apk_size(state: &AppState) -> Option<u64> {
    std::fs::metadata(apk_path(state))
        .ok()
        .filter(|m| m.is_file() && m.len() > 0)
        .map(|m| m.len())
}

/// The plugin as KOReader expects it: an `opdshelf.koplugin` folder inside the zip.
pub fn plugin_zip() -> &'static [u8] {
    static ZIP: OnceLock<Vec<u8>> = OnceLock::new();
    ZIP.get_or_init(|| {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let mut names: Vec<_> = Plugin::iter().collect();
        names.sort();
        for name in names {
            let file = Plugin::get(&name).expect("listed file exists");
            zip.start_file(format!("opdshelf.koplugin/{name}"), options)
                .and_then(|()| zip.write_all(&file.data).map_err(Into::into))
                .expect("writing to memory works");
        }
        zip.finish().expect("writing to memory works").into_inner()
    })
}

pub async fn koreader_plugin() -> Response {
    (
        [
            (header::CONTENT_TYPE, "application/zip".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{PLUGIN_FILE}\""),
            ),
            (header::CACHE_CONTROL, "no-cache".to_string()),
        ],
        plugin_zip(),
    )
        .into_response()
}

pub async fn android_app(State(state): State<AppState>, request: Request) -> Response {
    let path = apk_path(&state);
    if !path.is_file() {
        return (
            StatusCode::NOT_FOUND,
            "The Android app has not been added to this server",
        )
            .into_response();
    }
    let mime = "application/vnd.android.package-archive".parse().expect("valid mime");
    let mut response = match ServeFile::new_with_mime(&path, &mime).oneshot(request).await {
        Ok(response) => response.map(Body::new),
        Err(err) => return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response(),
    };
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=\"opdshelf.apk\""),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_zip_holds_the_plugin_folder() {
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(plugin_zip())).unwrap();
        let names: Vec<String> = (0..zip.len())
            .map(|i| zip.by_index(i).unwrap().name().to_string())
            .collect();
        assert!(names.contains(&"opdshelf.koplugin/_meta.lua".to_string()), "{names:?}");
        assert!(names.contains(&"opdshelf.koplugin/main.lua".to_string()));
        assert!(names.iter().all(|n| n.starts_with("opdshelf.koplugin/")));
    }
}
