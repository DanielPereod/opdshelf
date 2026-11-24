//! Read-only API keys, for programs that read the highlights, the positions and
//! the covers (a notes app, a script) without knowing the admin password.
//!
//! Kept in `DATA_DIR/api-keys.json` with the SHA-256 of each key, never the key
//! itself: the key is shown once, when it is made. A key only opens the routes
//! in [`allowed`], for reading, and stops working only when it is revoked
//! (changing the admin password leaves it alone).

use axum::extract::{Path as UrlPath, State};
use axum::http::{header, HeaderMap, Method, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use subtle::ConstantTimeEq;

use crate::web::{api_error, AppState};

/// Every key starts with it, so it can be told apart (and found by secret scanners).
pub const PREFIX: &str = "opds_";
/// The longest name a key can have.
pub const MAX_NAME: usize = 80;
/// How often the last use of a key is written to disk: every request would be too many writes.
const SAVE_USE_EVERY: i64 = 60;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Stored {
    id: String,
    name: String,
    /// SHA-256 of the key, in hex.
    hash: String,
    created: i64,
    #[serde(default)]
    last_used: Option<i64>,
}

/// What the panel shows of a key: everything but its hash.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct KeyInfo {
    pub id: String,
    pub name: String,
    pub created: i64,
    pub last_used: Option<i64>,
}

impl From<&Stored> for KeyInfo {
    fn from(key: &Stored) -> Self {
        Self {
            id: key.id.clone(),
            name: key.name.clone(),
            created: key.created,
            last_used: key.last_used,
        }
    }
}

/// The key a request was let in with, for the handlers (the live feed names the
/// connection after it).
#[derive(Clone, Debug)]
pub struct KeyUse {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Default)]
pub struct ApiKeys {
    keys: Arc<Mutex<Vec<Stored>>>,
    /// When the last use was last written to disk.
    saved_use: Arc<Mutex<i64>>,
    file: Option<PathBuf>,
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

fn hash(key: &str) -> String {
    Sha256::digest(key.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

impl ApiKeys {
    /// Keys kept only in memory (for tests).
    pub fn in_memory() -> Self {
        Self::default()
    }

    pub fn open(dir: &Path) -> Self {
        let file = dir.join("api-keys.json");
        let keys = std::fs::read(&file)
            .ok()
            .and_then(|data| serde_json::from_slice::<Vec<Stored>>(&data).ok())
            .unwrap_or_default();
        Self {
            keys: Arc::new(Mutex::new(keys)),
            saved_use: Arc::default(),
            file: Some(file),
        }
    }

    fn save(&self, keys: &[Stored]) -> std::io::Result<()> {
        let Some(file) = &self.file else { return Ok(()) };
        let json = serde_json::to_vec_pretty(keys).map_err(std::io::Error::other)?;
        crate::store::write_atomic(file, &json)
    }

    /// Every key, oldest first.
    pub fn list(&self) -> Vec<KeyInfo> {
        self.keys.lock().unwrap().iter().map(KeyInfo::from).collect()
    }

    /// Makes a key called `name`. Answers what the panel shows of it and the key itself,
    /// which is not kept anywhere.
    pub fn create(&self, name: &str) -> std::io::Result<(KeyInfo, String)> {
        let random: [u8; 32] = rand::random();
        let secret = format!(
            "{PREFIX}{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(random)
        );
        let id: String = rand::random::<[u8; 8]>().iter().map(|b| format!("{b:02x}")).collect();
        let stored = Stored {
            id,
            name: name.trim().chars().take(MAX_NAME).collect(),
            hash: hash(&secret),
            created: now(),
            last_used: None,
        };
        let info = KeyInfo::from(&stored);
        // Locked until the file is written, so two changes at once cannot lose one
        let mut keys = self.keys.lock().unwrap();
        keys.push(stored);
        if let Err(err) = self.save(&keys) {
            keys.pop();
            return Err(err);
        }
        Ok((info, secret))
    }

    /// Stops a key from working. False when there was no such key.
    pub fn revoke(&self, id: &str) -> std::io::Result<bool> {
        let mut keys = self.keys.lock().unwrap();
        let Some(at) = keys.iter().position(|k| k.id == id) else {
            return Ok(false);
        };
        let removed = keys.remove(at);
        if let Err(err) = self.save(&keys) {
            keys.insert(at, removed);
            return Err(err);
        }
        Ok(true)
    }

    /// The key `secret` is, if it is one, noting that it was used.
    pub fn check(&self, secret: &str) -> Option<KeyUse> {
        let wanted = hash(secret);
        let mut keys = self.keys.lock().unwrap();
        // Every key is compared, in constant time, so the timing says nothing about the hashes
        let mut found = None;
        for (at, key) in keys.iter().enumerate() {
            if bool::from(key.hash.as_bytes().ct_eq(wanted.as_bytes())) {
                found = Some(at);
            }
        }
        let key = &mut keys[found?];
        let now = now();
        key.last_used = Some(now);
        let found = KeyUse {
            id: key.id.clone(),
            name: key.name.clone(),
        };
        let mut saved = self.saved_use.lock().unwrap();
        if now - *saved >= SAVE_USE_EVERY {
            *saved = now;
            // The key still works if the last use cannot be written
            let _ = self.save(&keys);
        }
        Some(found)
    }
}

/// Whether a key may make this request: only reading, and only the highlights,
/// the positions (with their live feed), the covers and the books' details.
pub fn allowed(method: &Method, path: &str) -> bool {
    if method != Method::GET && method != Method::HEAD {
        return false;
    }
    if ["/cover/", "/thumb/", "/info/"].iter().any(|p| path.starts_with(p)) {
        return true;
    }
    let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    let named = |s: &str| !s.is_empty();
    match parts.as_slice() {
        ["api", "annotations"] | ["api", "highlights"] | ["api", "progress"] => true,
        ["api", "highlights", "review" | "export"] => true,
        // `/api/progress/events` too
        ["api", "annotations" | "progress", document] => named(document),
        ["api", "annotations", document, "export"] => named(document),
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// The panel's forms (Settings › Apps and connections). They need a session, like
// every page of the panel, and the CSRF token in `X-CSRF-Token`.

fn csrf_ok(state: &AppState, headers: &HeaderMap) -> bool {
    let token = headers
        .get("x-csrf-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    state.auth.check_csrf(token)
}

fn bad_csrf() -> Response {
    api_error(StatusCode::FORBIDDEN, "csrf", "Invalid CSRF token")
}

fn save_failed(err: std::io::Error) -> Response {
    tracing::warn!("cannot save the API keys: {err}");
    api_error(StatusCode::INTERNAL_SERVER_ERROR, "save", "Cannot save the API keys")
}

pub async fn list_keys(State(state): State<AppState>) -> Response {
    Json(json!({ "keys": state.auth.keys.list() })).into_response()
}

#[derive(Deserialize)]
pub struct NewKey {
    #[serde(default)]
    name: String,
}

/// Makes a key. The answer is the only time the key itself is shown.
pub async fn create_key(State(state): State<AppState>, headers: HeaderMap, Json(request): Json<NewKey>) -> Response {
    if !csrf_ok(&state, &headers) {
        return bad_csrf();
    }
    if request.name.trim().is_empty() {
        return api_error(StatusCode::BAD_REQUEST, "invalid_name", "Give the key a name");
    }
    match state.auth.keys.create(&request.name) {
        Ok((info, secret)) => (
            StatusCode::CREATED,
            [(header::CACHE_CONTROL, "no-store")],
            Json(json!({ "key": info, "secret": secret })),
        )
            .into_response(),
        Err(err) => save_failed(err),
    }
}

pub async fn revoke_key(State(state): State<AppState>, headers: HeaderMap, UrlPath(id): UrlPath<String>) -> Response {
    if !csrf_ok(&state, &headers) {
        return bad_csrf();
    }
    match state.auth.keys.revoke(&id) {
        Ok(true) => Json(json!({ "revoked": id })).into_response(),
        Ok(false) => api_error(StatusCode::NOT_FOUND, "not_found", "There is no such key"),
        Err(err) => save_failed(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_works_until_it_is_revoked_and_only_its_hash_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let keys = ApiKeys::open(dir.path());
        let (info, secret) = keys.create("  Notes app  ").unwrap();
        assert_eq!(info.name, "Notes app");
        assert!(secret.starts_with(PREFIX));
        assert_eq!(secret.len(), PREFIX.len() + 43);
        let file = std::fs::read_to_string(dir.path().join("api-keys.json")).unwrap();
        assert!(!file.contains(&secret[PREFIX.len()..]));
        assert!(file.contains(&hash(&secret)));

        assert_eq!(keys.check(&secret).unwrap().name, "Notes app");
        assert!(keys.check("opds_wrong").is_none());
        assert!(keys.check("").is_none());
        // Kept across restarts, with its last use
        let reopened = ApiKeys::open(dir.path());
        assert!(reopened.list()[0].last_used.is_some());
        assert_eq!(reopened.check(&secret).unwrap().id, info.id);

        assert!(reopened.revoke(&info.id).unwrap());
        assert!(!reopened.revoke(&info.id).unwrap());
        assert!(reopened.check(&secret).is_none());
        assert!(ApiKeys::open(dir.path()).list().is_empty());
    }

    #[test]
    fn keys_only_read() {
        let get = Method::GET;
        let doc = "502000418ae95849b554660b1ee1d388";
        for path in [
            "/api/annotations".to_string(),
            format!("/api/annotations/{doc}"),
            format!("/api/annotations/{doc}/export"),
            "/api/highlights".into(),
            "/api/highlights/review".into(),
            "/api/highlights/export".into(),
            "/api/progress".into(),
            format!("/api/progress/{doc}"),
            "/api/progress/events".into(),
            "/cover/a/b.epub".into(),
            "/thumb/a.epub".into(),
            "/info/a.epub".into(),
        ] {
            assert!(allowed(&get, &path), "{path}");
            assert!(allowed(&Method::HEAD, &path), "{path}");
        }
        assert!(!allowed(&Method::PUT, &format!("/api/annotations/{doc}")));
        assert!(!allowed(&Method::POST, "/api/progress"));
        assert!(!allowed(&Method::DELETE, "/api/devices/x"));
        for path in [
            "/admin",
            "/admin/settings",
            "/download/a.epub",
            "/read/a.epub",
            "/api/devices",
            "/api/keys",
            "/api/bookmarks/x",
            "/api/stats",
            "/api/annotations/",
            "/api/annotations/x/y",
            "/api/highlights/x",
            "/opds/books",
            "/",
        ] {
            assert!(!allowed(&get, path), "{path}");
        }
    }
}
