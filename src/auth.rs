use axum::http::{header, HeaderMap};
use base64::Engine;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use subtle::ConstantTimeEq;

use crate::config::Config;

pub const SESSION_COOKIE: &str = "opdshelf_session";
/// Sessions last 15 days.
pub const SESSION_MAX_AGE: u64 = 15 * 24 * 60 * 60;

#[derive(Clone)]
pub struct Auth {
    credentials: Option<(String, String)>,
    secret: Vec<u8>,
    /// Signing out raises it, which revokes every session made before. Kept in
    /// DATA_DIR so it survives restarts.
    generation: Arc<AtomicU64>,
    generation_file: Option<PathBuf>,
}

impl Auth {
    pub fn new(config: &Config) -> Self {
        let credentials = config.admin_username.clone().zip(config.admin_password.clone());
        // With credentials the secret is derived from them, so sessions survive restarts and are
        // revoked by changing the password. Without them a random per-process secret still
        // protects the admin forms against cross-site requests.
        let secret = match &credentials {
            Some((user, pass)) => hmac(pass.as_bytes(), format!("opdshelf-secret:{user}").as_bytes()),
            None => rand::random::<[u8; 32]>().to_vec(),
        };
        Self {
            credentials,
            secret,
            generation: Arc::default(),
            generation_file: None,
        }
    }

    /// Like `new`, but keeps what has to outlive a restart in `dir`: the secret that signs the
    /// forms' tokens when there are no credentials (so a page left open keeps working after an
    /// update) and the count of sign-outs.
    pub fn open(config: &Config, dir: &std::path::Path) -> Self {
        let mut auth = Self::new(config);
        if auth.credentials.is_none() {
            let file = dir.join("session-secret");
            let saved = std::fs::read_to_string(&file)
                .ok()
                .map(|text| text.trim().to_string())
                .filter(|text| text.len() >= 32);
            auth.secret = match saved {
                Some(text) => text.into_bytes(),
                None => {
                    let fresh = hex(&rand::random::<[u8; 32]>());
                    let _ = std::fs::create_dir_all(dir);
                    // Without a place to keep it the secret lasts as long as the process
                    let _ = std::fs::write(&file, &fresh);
                    fresh.into_bytes()
                }
            };
        }
        let file = dir.join("session-generation");
        let saved = std::fs::read_to_string(&file)
            .ok()
            .and_then(|text| text.trim().parse::<u64>().ok())
            .unwrap_or(0);
        auth.generation.store(saved, Ordering::SeqCst);
        auth.generation_file = Some(file);
        auth
    }

    /// Makes every session made so far stop working (signing out).
    pub fn revoke_sessions(&self) {
        let next = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        if let Some(file) = &self.generation_file {
            let _ = std::fs::write(file, next.to_string());
        }
    }

    pub fn enabled(&self) -> bool {
        self.credentials.is_some()
    }

    pub fn check_credentials(&self, username: &str, password: &str) -> bool {
        let Some((user, pass)) = &self.credentials else {
            return false;
        };
        // Compare both so the timing does not reveal which one was wrong
        let user_ok = hmac(&self.secret, username.as_bytes()).ct_eq(&hmac(&self.secret, user.as_bytes()));
        let pass_ok = hmac(&self.secret, password.as_bytes()).ct_eq(&hmac(&self.secret, pass.as_bytes()));
        (user_ok & pass_ok).into()
    }

    /// KOReader's sync plugin sends the MD5 of the password (in hex) instead of the password.
    /// Accepts anything when authentication is off.
    pub fn check_hashed_password(&self, username: &str, key: &str) -> bool {
        use md5::{Digest, Md5};
        let Some((user, pass)) = &self.credentials else {
            return true;
        };
        let expected = hex(&Md5::digest(pass.as_bytes()));
        let key = key.to_ascii_lowercase();
        let user_ok = hmac(&self.secret, username.as_bytes()).ct_eq(&hmac(&self.secret, user.as_bytes()));
        let key_ok = hmac(&self.secret, key.as_bytes()).ct_eq(&hmac(&self.secret, expected.as_bytes()));
        (user_ok & key_ok).into()
    }

    pub fn session_token(&self) -> String {
        let generation = self.generation.load(Ordering::SeqCst);
        hex(&hmac(&self.secret, format!("session:{generation}").as_bytes()))
    }

    pub fn csrf_token(&self) -> String {
        hex(&hmac(&self.secret, b"csrf"))
    }

    pub fn check_csrf(&self, token: &str) -> bool {
        token.as_bytes().ct_eq(self.csrf_token().as_bytes()).into()
    }

    /// Whether the request carries a valid session cookie or Basic credentials.
    pub fn is_authorized(&self, headers: &HeaderMap) -> bool {
        if !self.enabled() {
            return true;
        }
        let session_ok = cookie(headers, SESSION_COOKIE)
            .is_some_and(|value| bool::from(value.as_bytes().ct_eq(self.session_token().as_bytes())));
        session_ok || basic_credentials(headers).is_some_and(|(u, p)| self.check_credentials(&u, &p))
    }

    pub fn session_cookie(&self, secure: bool) -> String {
        let secure = if secure { "; Secure" } else { "" };
        format!(
            "{SESSION_COOKIE}={}; Path=/; HttpOnly; SameSite=Lax; Max-Age={SESSION_MAX_AGE}{secure}",
            self.session_token()
        )
    }

    pub fn clear_cookie() -> String {
        format!("{SESSION_COOKIE}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0")
    }
}

fn hmac(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value.to_string())
}

fn basic_credentials(headers: &HeaderMap) -> Option<(String, String)> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, encoded) = value.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("basic") {
        return None;
    }
    let decoded = base64::engine::general_purpose::STANDARD.decode(encoded.trim()).ok()?;
    let decoded = String::from_utf8(decoded).ok()?;
    let (user, pass) = decoded.split_once(':')?;
    Some((user.to_string(), pass.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn auth(user: Option<&str>, pass: Option<&str>) -> Auth {
        Auth::new(&Config::from_lookup(|k| match k {
            "ADMIN_USERNAME" => user.map(str::to_string),
            "ADMIN_PASSWORD" => pass.map(str::to_string),
            _ => None,
        }))
    }

    fn headers(name: header::HeaderName, value: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(name, HeaderValue::from_str(value).unwrap());
        h
    }

    #[test]
    fn open_when_disabled() {
        assert!(auth(None, None).is_authorized(&HeaderMap::new()));
        assert!(auth(Some("a"), None).is_authorized(&HeaderMap::new()));
    }

    #[test]
    fn session_cookie_must_match() {
        let a = auth(Some("a"), Some("b"));
        assert!(!a.is_authorized(&HeaderMap::new()));
        assert!(!a.is_authorized(&headers(header::COOKIE, "opdshelf_session=valid")));
        assert!(!a.is_authorized(&headers(header::COOKIE, "auth_session=valid")));
        let good = format!("x=1; {SESSION_COOKIE}={}", a.session_token());
        assert!(a.is_authorized(&headers(header::COOKIE, &good)));
        // Changing the password revokes existing sessions
        assert!(!auth(Some("a"), Some("c")).is_authorized(&headers(header::COOKIE, &good)));
    }

    #[test]
    fn basic_auth() {
        let a = auth(Some("a"), Some("b:c"));
        let encode = |s: &str| format!("Basic {}", base64::engine::general_purpose::STANDARD.encode(s));
        assert!(a.is_authorized(&headers(header::AUTHORIZATION, &encode("a:b:c"))));
        assert!(!a.is_authorized(&headers(header::AUTHORIZATION, &encode("a:b"))));
        assert!(!a.is_authorized(&headers(header::AUTHORIZATION, &encode("x:b:c"))));
    }

    #[test]
    fn hashed_password() {
        let a = auth(Some("a"), Some("secret"));
        // md5("secret")
        assert!(a.check_hashed_password("a", "5ebe2294ecd0e0f08eab7690d2a6ee69"));
        assert!(a.check_hashed_password("a", "5EBE2294ECD0E0F08EAB7690D2A6EE69"));
        assert!(!a.check_hashed_password("a", "secret"));
        assert!(!a.check_hashed_password("b", "5ebe2294ecd0e0f08eab7690d2a6ee69"));
        assert!(auth(None, None).check_hashed_password("", ""));
    }

    #[test]
    fn signing_out_revokes_the_sessions_made_before() {
        let a = auth(Some("a"), Some("b"));
        let good = format!("{SESSION_COOKIE}={}", a.session_token());
        assert!(a.is_authorized(&headers(header::COOKIE, &good)));
        a.revoke_sessions();
        assert!(!a.is_authorized(&headers(header::COOKIE, &good)));
        let again = format!("{SESSION_COOKIE}={}", a.session_token());
        assert!(a.is_authorized(&headers(header::COOKIE, &again)));
    }

    #[test]
    fn the_secret_and_the_sign_outs_outlive_a_restart() {
        let dir = std::env::temp_dir().join(format!("opdshelf-auth-{}", rand::random::<u32>()));
        let config = Config::from_lookup(|_| None);
        let first = Auth::open(&config, &dir);
        let second = Auth::open(&config, &dir);
        assert_eq!(first.csrf_token(), second.csrf_token());
        first.revoke_sessions();
        let third = Auth::open(&config, &dir);
        assert_eq!(third.session_token(), first.session_token());
        assert_ne!(third.session_token(), second.session_token());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn csrf() {
        let a = auth(None, None);
        assert!(a.check_csrf(&a.csrf_token()));
        assert!(!a.check_csrf("nope"));
        assert_ne!(a.csrf_token(), auth(None, None).csrf_token());
    }
}
