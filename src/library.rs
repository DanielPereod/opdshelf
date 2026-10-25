//! Scans BOOKS_DIR recursively and keeps a metadata cache keyed by path, size and mtime.

use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use walkdir::WalkDir;

use crate::metadata::{self, natural_cmp, BookMeta};
use crate::paths;

#[derive(Clone, Debug, Serialize)]
pub struct Book {
    /// Path relative to BOOKS_DIR, `/` separated. Also the book's identifier.
    pub path: String,
    pub folder: String,
    pub file_name: String,
    pub title: String,
    pub authors: Vec<String>,
    pub size: u64,
    #[serde(serialize_with = "serialize_date")]
    pub modified: DateTime<Utc>,
    pub mime: String,
    pub format: String,
    pub has_cover: bool,
    pub meta: BookMeta,
}

fn serialize_date<S: serde::Serializer>(date: &DateTime<Utc>, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&date.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
}

impl Book {
    pub fn author_line(&self) -> String {
        self.authors.join(", ")
    }

    pub fn cover_mime(&self) -> Option<&str> {
        self.meta.cover.as_ref().map(|c| c.mime())
    }

    fn matches(&self, terms: &[String]) -> bool {
        let haystack = format!(
            "{} {} {} {}",
            self.title,
            self.authors.join(" "),
            self.path,
            self.meta.subjects.join(" ")
        )
        .to_lowercase();
        terms.iter().all(|t| haystack.contains(t))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SortMode {
    NameAsc,
    NameDesc,
    DateAsc,
    #[default]
    DateDesc,
}

impl SortMode {
    pub fn parse(value: Option<&str>) -> Self {
        match value {
            Some("name-asc") => Self::NameAsc,
            Some("name-desc") => Self::NameDesc,
            Some("date-asc") => Self::DateAsc,
            _ => Self::DateDesc,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::NameAsc => "name-asc",
            Self::NameDesc => "name-desc",
            Self::DateAsc => "date-asc",
            Self::DateDesc => "date-desc",
        }
    }

    pub fn sort(self, books: &mut [Book]) {
        books.sort_by(|a, b| {
            let by_name = || natural_cmp(&a.title, &b.title).then_with(|| a.path.cmp(&b.path));
            match self {
                Self::NameAsc => by_name(),
                Self::NameDesc => by_name().reverse(),
                Self::DateAsc => a.modified.cmp(&b.modified).then_with(by_name),
                Self::DateDesc => b.modified.cmp(&a.modified).then_with(by_name),
            }
        });
    }
}

/// Result of a scan: every book plus every folder (relative paths, sorted).
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub books: Vec<Book>,
    pub folders: Vec<String>,
}

impl Snapshot {
    pub fn search(&self, query: &str) -> Vec<Book> {
        let terms: Vec<String> = query.to_lowercase().split_whitespace().map(str::to_string).collect();
        self.books.iter().filter(|b| b.matches(&terms)).cloned().collect()
    }

    /// Books directly inside `folder` (`""` for the root).
    pub fn in_folder(&self, folder: &str) -> Vec<Book> {
        self.books.iter().filter(|b| b.folder == folder).cloned().collect()
    }

    /// Direct subfolders of `folder`.
    pub fn subfolders(&self, folder: &str) -> Vec<String> {
        self.folders
            .iter()
            .filter(|f| paths::parent(f) == folder)
            .cloned()
            .collect()
    }

    /// Number of books inside `folder`, including its subfolders.
    pub fn count_under(&self, folder: &str) -> usize {
        let prefix = format!("{folder}/");
        self.books
            .iter()
            .filter(|b| b.folder == folder || b.folder.starts_with(&prefix))
            .count()
    }

    pub fn authors(&self) -> Vec<(String, usize)> {
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for author in self.books.iter().flat_map(|b| &b.authors) {
            *counts.entry(author).or_default() += 1;
        }
        let mut authors: Vec<_> = counts.into_iter().map(|(a, c)| (a.to_string(), c)).collect();
        authors.sort_by(|a, b| natural_cmp(&a.0, &b.0));
        authors
    }

    pub fn by_author(&self, author: &str) -> Vec<Book> {
        self.books
            .iter()
            .filter(|b| b.authors.iter().any(|a| a == author))
            .cloned()
            .collect()
    }
}

struct CacheEntry {
    size: u64,
    modified: SystemTime,
    meta: BookMeta,
}

#[derive(Clone)]
pub struct Library {
    root: PathBuf,
    allowed_extensions: Arc<HashSet<String>>,
    cache: Arc<Mutex<HashMap<String, CacheEntry>>>,
}

impl Library {
    pub fn new(root: PathBuf, allowed_extensions: HashSet<String>) -> Self {
        Self {
            root,
            allowed_extensions: Arc::new(allowed_extensions),
            cache: Arc::default(),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn is_allowed(&self, rel: &str) -> bool {
        let ext = paths::extension(rel);
        self.allowed_extensions.contains(&ext) || (ext == "fb2.zip" && self.allowed_extensions.contains("zip"))
    }

    /// Absolute path of an existing, allowed book.
    pub fn book_path(&self, rel: &str) -> Option<(String, PathBuf)> {
        let rel = paths::sanitize(rel)?;
        let path = self.root.join(&rel);
        (self.is_allowed(&rel) && path.is_file()).then_some((rel, path))
    }

    /// Absolute path of an existing folder (`""` is the root).
    pub fn folder_path(&self, rel: &str) -> Option<(String, PathBuf)> {
        if rel.trim_matches('/').is_empty() {
            return Some((String::new(), self.root.clone()));
        }
        let rel = paths::sanitize(rel)?;
        let path = self.root.join(&rel);
        path.is_dir().then_some((rel, path))
    }

    pub async fn scan(&self) -> Snapshot {
        let lib = self.clone();
        tokio::task::spawn_blocking(move || lib.scan_blocking())
            .await
            .unwrap_or_default()
    }

    pub async fn book(&self, rel: &str) -> Option<Book> {
        let (rel, path) = self.book_path(rel)?;
        let lib = self.clone();
        tokio::task::spawn_blocking(move || {
            let stat = std::fs::metadata(&path).ok()?;
            Some(lib.load(&rel, &path, &stat))
        })
        .await
        .ok()
        .flatten()
    }

    pub fn forget(&self, rel: &str) {
        self.cache.lock().unwrap().remove(rel);
    }

    fn scan_blocking(&self) -> Snapshot {
        if let Err(err) = std::fs::create_dir_all(&self.root) {
            tracing::error!("cannot create {}: {err}", self.root.display());
            return Snapshot::default();
        }

        let mut books = Vec::new();
        let mut folders = BTreeSet::new();
        let walker = WalkDir::new(&self.root)
            .follow_links(true)
            .min_depth(1)
            .into_iter()
            .filter_entry(|e| !e.file_name().to_string_lossy().starts_with('.'));

        for entry in walker {
            let entry = match entry {
                Ok(entry) => entry,
                Err(err) => {
                    tracing::warn!("skipping unreadable entry: {err}");
                    continue;
                }
            };
            let Ok(rel) = entry.path().strip_prefix(&self.root) else {
                continue;
            };
            let Some(rel) = rel.to_str().map(|r| r.replace('\\', "/")) else {
                tracing::warn!("skipping non UTF-8 path {}", entry.path().display());
                continue;
            };
            if entry.file_type().is_dir() {
                folders.insert(rel);
            } else if self.is_allowed(&rel) {
                if let Ok(stat) = entry.metadata() {
                    books.push(self.load(&rel, entry.path(), &stat));
                }
            }
        }

        let seen: HashSet<&str> = books.iter().map(|b| b.path.as_str()).collect();
        self.cache
            .lock()
            .unwrap()
            .retain(|path, _| seen.contains(path.as_str()));

        Snapshot {
            books,
            folders: folders.into_iter().collect(),
        }
    }

    fn load(&self, rel: &str, path: &Path, stat: &std::fs::Metadata) -> Book {
        let modified = stat.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let size = stat.len();

        let cached = {
            let cache = self.cache.lock().unwrap();
            cache
                .get(rel)
                .filter(|e| e.size == size && e.modified == modified)
                .map(|e| e.meta.clone())
        };
        let meta = cached.unwrap_or_else(|| {
            let meta = metadata::read_metadata(path);
            self.cache.lock().unwrap().insert(
                rel.to_string(),
                CacheEntry {
                    size,
                    modified,
                    meta: meta.clone(),
                },
            );
            meta
        });

        let mime = mime_for(rel);
        Book {
            path: rel.to_string(),
            folder: paths::parent(rel).to_string(),
            file_name: paths::file_name(rel).to_string(),
            title: meta.title.clone().unwrap_or_else(|| paths::stem(rel)),
            authors: meta.authors.clone(),
            size,
            modified: modified.into(),
            format: format_label(rel),
            has_cover: meta.cover.is_some(),
            mime,
            meta,
        }
    }
}

pub fn mime_for(rel: &str) -> String {
    match paths::extension(rel).as_str() {
        "epub" | "kepub" => "application/epub+zip",
        "fb2" => "application/x-fictionbook+xml",
        "fb2.zip" | "fbz" => "application/x-zip-compressed-fb2",
        "cbz" => "application/vnd.comicbook+zip",
        "cbr" => "application/vnd.comicbook-rar",
        "cb7" => "application/x-cb7",
        "cbt" => "application/x-cbt",
        "mobi" => "application/x-mobipocket-ebook",
        "azw" | "azw3" | "azw4" => "application/vnd.amazon.ebook",
        "djvu" => "image/vnd.djvu",
        _ => return mime_guess::from_path(rel).first_or_octet_stream().to_string(),
    }
    .to_string()
}

fn format_label(rel: &str) -> String {
    match paths::extension(rel).as_str() {
        "fb2.zip" | "fbz" => "FB2".into(),
        "kepub" => "EPUB".into(),
        "htm" => "HTML".into(),
        ext => ext.to_uppercase(),
    }
}

pub fn format_size(bytes: u64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < units.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    format!("{size:.1} {}", units[unit])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DEFAULT_EXTENSIONS;

    fn library(dir: &Path) -> Library {
        Library::new(
            dir.to_path_buf(),
            DEFAULT_EXTENSIONS.iter().map(|e| e.to_string()).collect(),
        )
    }

    #[tokio::test]
    async fn scans_subfolders_and_filters_extensions() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("Saga/Tomo 1")).unwrap();
        std::fs::create_dir_all(dir.path().join(".hidden")).unwrap();
        std::fs::write(dir.path().join("root.pdf"), b"x").unwrap();
        std::fs::write(dir.path().join("Saga/Tomo 1/libro.txt"), b"x").unwrap();
        std::fs::write(dir.path().join("Saga/audio.mp3"), b"x").unwrap();
        std::fs::write(dir.path().join("Saga/metadata.opf"), b"x").unwrap();
        std::fs::write(dir.path().join(".hidden/secret.pdf"), b"x").unwrap();
        std::fs::write(dir.path().join(".dotfile.pdf"), b"x").unwrap();

        let snapshot = library(dir.path()).scan().await;
        let mut paths: Vec<_> = snapshot.books.iter().map(|b| b.path.as_str()).collect();
        paths.sort();
        assert_eq!(paths, vec!["Saga/Tomo 1/libro.txt", "root.pdf"]);
        assert_eq!(snapshot.folders, vec!["Saga", "Saga/Tomo 1"]);
        assert_eq!(snapshot.subfolders(""), vec!["Saga"]);
        assert_eq!(snapshot.count_under("Saga"), 1);
        assert_eq!(snapshot.in_folder("").len(), 1);
    }

    #[tokio::test]
    async fn book_lookup_is_confined() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.epub"), b"x").unwrap();
        std::fs::write(dir.path().join("a.mp3"), b"x").unwrap();
        let lib = library(&dir.path().join("."));
        assert!(lib.book("a.epub").await.is_some());
        assert!(lib.book("a.mp3").await.is_none());
        assert!(lib.book("../a.epub").await.is_none());
        assert!(lib.book("missing.epub").await.is_none());
    }

    #[test]
    fn sort_modes() {
        let book = |title: &str, secs: i64| Book {
            path: title.into(),
            folder: String::new(),
            file_name: title.into(),
            title: title.into(),
            authors: vec![],
            size: 0,
            modified: DateTime::from_timestamp(secs, 0).unwrap(),
            mime: String::new(),
            format: String::new(),
            has_cover: false,
            meta: BookMeta::default(),
        };
        let mut books = vec![book("b 10", 1), book("B 2", 3), book("a", 2)];
        let titles = |b: &[Book]| b.iter().map(|b| b.title.clone()).collect::<Vec<_>>();
        SortMode::NameAsc.sort(&mut books);
        assert_eq!(titles(&books), ["a", "B 2", "b 10"]);
        SortMode::DateDesc.sort(&mut books);
        assert_eq!(titles(&books), ["B 2", "a", "b 10"]);
        SortMode::parse(Some("date-asc")).sort(&mut books);
        assert_eq!(titles(&books), ["b 10", "a", "B 2"]);
        assert_eq!(SortMode::parse(Some("bogus")), SortMode::DateDesc);
    }

    #[test]
    fn sizes() {
        assert_eq!(format_size(512), "512.0 B");
        assert_eq!(format_size(1536), "1.5 KB");
    }
}
