use std::collections::HashSet;
use std::path::PathBuf;

/// Extensions served when ALLOWED_EXTENSIONS is not set.
pub const DEFAULT_EXTENSIONS: &[&str] = &[
    "epub", "kepub", "pdf", "fb2", "fbz", "mobi", "azw", "azw3", "azw4", "cbz", "cbr", "cb7", "cbt", "djvu", "rtf",
    "txt", "html", "htm", "doc", "docx", "odt", "zip",
];

#[derive(Clone, Debug)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub books_dir: PathBuf,
    pub admin_username: Option<String>,
    pub admin_password: Option<String>,
    /// Public URL of the server (e.g. `https://books.example.com`). Detected from the request when empty.
    pub base_url: Option<String>,
    pub allowed_extensions: HashSet<String>,
    pub page_size: usize,
    pub max_upload_bytes: usize,
    /// Where edited metadata, uploaded covers and the thumbnail cache are stored.
    pub data_dir: PathBuf,
    /// Watch BOOKS_DIR for changes instead of rescanning it on every request.
    pub watch: bool,
    /// Full rescan interval while watching, as a fallback for file systems without events.
    pub scan_interval_secs: u64,
    pub openlibrary_url: String,
    pub google_books_url: String,
}

impl Config {
    pub fn from_env() -> Self {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Self {
        let get = |key: &str| get(key).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());

        let mut allowed_extensions: HashSet<String> = match get("ALLOWED_EXTENSIONS") {
            Some(list) => parse_extensions(&list),
            None => DEFAULT_EXTENSIONS.iter().map(|e| e.to_string()).collect(),
        };
        if let Some(list) = get("EXCLUDE_EXTENSIONS") {
            for ext in parse_extensions(&list) {
                allowed_extensions.remove(&ext);
            }
        }

        // REVERSE_PROXY + REVERSE_PROXY_HOST are kept for compatibility with 1.x
        let legacy_proxy_url = (get("REVERSE_PROXY").as_deref() == Some("true"))
            .then(|| get("REVERSE_PROXY_HOST"))
            .flatten();

        let admin_username = get("ADMIN_USERNAME");
        let admin_password = get("ADMIN_PASSWORD");
        let auth_enabled = admin_username.is_some() && admin_password.is_some();

        let books_dir = PathBuf::from(get("BOOKS_DIR").unwrap_or_else(|| "./books".into()));
        let flag = |key: &str, default: bool| {
            get(key).map_or(default, |v| {
                !matches!(v.to_lowercase().as_str(), "0" | "false" | "no" | "off")
            })
        };

        Self {
            data_dir: get("DATA_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| books_dir.join(".opdshelf")),
            watch: flag("WATCH", true),
            scan_interval_secs: get("SCAN_INTERVAL").and_then(|v| v.parse().ok()).unwrap_or(600),
            openlibrary_url: get("OPENLIBRARY_URL").unwrap_or_else(|| "https://openlibrary.org".into()),
            google_books_url: get("GOOGLE_BOOKS_URL").unwrap_or_else(|| "https://www.googleapis.com".into()),
            host: get("HOST").unwrap_or_else(|| "0.0.0.0".into()),
            port: get("PORT").and_then(|p| p.parse().ok()).unwrap_or(3000),
            books_dir,
            admin_username: admin_username.filter(|_| auth_enabled),
            admin_password: admin_password.filter(|_| auth_enabled),
            base_url: get("BASE_URL")
                .or(legacy_proxy_url)
                .map(|url| url.trim_end_matches('/').to_string()),
            allowed_extensions,
            page_size: get("PAGE_SIZE")
                .and_then(|p| p.parse().ok())
                .filter(|p| *p > 0)
                .unwrap_or(50),
            max_upload_bytes: get("MAX_UPLOAD_MB")
                .and_then(|p| p.parse::<usize>().ok())
                .unwrap_or(1024)
                .saturating_mul(1024 * 1024),
        }
    }

    pub fn auth_enabled(&self) -> bool {
        self.admin_username.is_some() && self.admin_password.is_some()
    }
}

fn parse_extensions(list: &str) -> HashSet<String> {
    list.split(',')
        .map(|e| e.trim().trim_start_matches('.').to_lowercase())
        .filter(|e| !e.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn config(vars: &[(&str, &str)]) -> Config {
        let vars: HashMap<String, String> = vars.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        Config::from_lookup(|k| vars.get(k).cloned())
    }

    #[test]
    fn defaults_match_readme() {
        let cfg = config(&[]);
        assert_eq!(cfg.port, 3000);
        assert_eq!(cfg.host, "0.0.0.0");
        assert!(!cfg.auth_enabled());
        assert!(cfg.allowed_extensions.contains("epub"));
        assert!(!cfg.allowed_extensions.contains("mp3"));
        assert!(cfg.watch);
        assert_eq!(cfg.data_dir, PathBuf::from("./books/.opdshelf"));
        assert!(!config(&[("WATCH", "false")]).watch);
    }

    #[test]
    fn extension_filters() {
        let cfg = config(&[("ALLOWED_EXTENSIONS", "EPUB, .pdf,cbz"), ("EXCLUDE_EXTENSIONS", "cbz")]);
        let mut exts: Vec<_> = cfg.allowed_extensions.into_iter().collect();
        exts.sort();
        assert_eq!(exts, vec!["epub", "pdf"]);
    }

    #[test]
    fn auth_needs_both_credentials() {
        assert!(!config(&[("ADMIN_USERNAME", "a")]).auth_enabled());
        assert!(config(&[("ADMIN_USERNAME", "a"), ("ADMIN_PASSWORD", "b")]).auth_enabled());
    }

    #[test]
    fn legacy_reverse_proxy() {
        let cfg = config(&[("REVERSE_PROXY", "true"), ("REVERSE_PROXY_HOST", "https://x.org/")]);
        assert_eq!(cfg.base_url.as_deref(), Some("https://x.org"));
        let cfg = config(&[
            ("BASE_URL", "https://y.org"),
            ("REVERSE_PROXY", "true"),
            ("REVERSE_PROXY_HOST", "https://x.org"),
        ]);
        assert_eq!(cfg.base_url.as_deref(), Some("https://y.org"));
    }
}
