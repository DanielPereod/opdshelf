//! Reading positions shared between the web reader, KOReader and other apps.
//!
//! Books are identified by the same partial MD5 that KOReader uses, so a
//! position survives renaming or moving a file and KOReader can sync with its
//! built-in plugin. Positions are kept in DATA_DIR/progress.json.

use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// Positions kept per book, newest first.
const HISTORY: usize = 30;
const MAX_FIELD: usize = 2048;

/// Text around a position, used to find it again when the structural pointer
/// no longer matches (an edited book, a reader that splits the text otherwise).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Context {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub before: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub after: String,
    /// The text that was selected when the position was set by hand.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub selected: String,
}

impl Context {
    fn is_empty(&self) -> bool {
        self.before.is_empty() && self.after.is_empty() && self.selected.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Record {
    /// KOReader's format: an xpointer for reflowable books, the page number (from 1) for PDFs and comics.
    pub progress: String,
    /// From 0 to 1.
    pub percentage: f64,
    pub device: String,
    pub device_id: String,
    /// EPUB CFI of the same position, so the web reader can go back exactly where it was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cfi: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<Context>,
    /// Set by selecting a point in the text instead of by turning pages.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub manual: bool,
    /// When the position was reached on the device (Unix seconds). Updates
    /// sent after reading offline keep the time they were read at.
    pub timestamp: i64,
}

/// A position sent by a client.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Update {
    #[serde(default)]
    pub progress: String,
    #[serde(default)]
    pub percentage: f64,
    #[serde(default)]
    pub device: String,
    #[serde(default)]
    pub device_id: String,
    #[serde(default)]
    pub cfi: Option<String>,
    #[serde(default)]
    pub context: Option<Context>,
    #[serde(default)]
    pub manual: bool,
    /// When the position was reached. Defaults to now and is never in the future.
    #[serde(default)]
    pub timestamp: Option<i64>,
}

#[derive(Clone)]
pub struct Progress {
    file: PathBuf,
    books: Arc<Mutex<HashMap<String, Vec<Record>>>>,
}

impl Progress {
    pub fn open(dir: &Path) -> Self {
        let file = dir.join("progress.json");
        let books = match std::fs::read(&file) {
            Ok(json) => serde_json::from_slice(&json).unwrap_or_else(|err| {
                tracing::error!("ignoring unreadable {}: {err}", file.display());
                HashMap::new()
            }),
            Err(_) => HashMap::new(),
        };
        Self {
            file,
            books: Arc::new(Mutex::new(books)),
        }
    }

    /// Saves a position and returns the one now current.
    pub fn update(&self, document: &str, update: Update, now: i64) -> std::io::Result<Record> {
        let record = Record {
            progress: clip(update.progress),
            percentage: if update.percentage.is_finite() {
                update.percentage.clamp(0.0, 1.0)
            } else {
                0.0
            },
            device: clip(update.device),
            device_id: clip(update.device_id),
            cfi: update.cfi.map(clip).filter(|c| !c.is_empty()),
            context: update
                .context
                .map(|c| Context {
                    before: clip(c.before),
                    after: clip(c.after),
                    selected: clip(c.selected),
                })
                .filter(|c| !c.is_empty()),
            manual: update.manual,
            timestamp: update.timestamp.map_or(now, |t| t.min(now)),
        };
        let current = {
            let mut books = self.books.lock().unwrap();
            let history = books.entry(document.to_string()).or_default();
            add(history, record);
            current(history).cloned().expect("a position was just added")
        };
        self.save()?;
        Ok(current)
    }

    pub fn current(&self, document: &str) -> Option<Record> {
        self.books
            .lock()
            .unwrap()
            .get(document)
            .and_then(|h| current(h).cloned())
    }

    /// Every saved position of a book, newest first.
    pub fn history(&self, document: &str) -> Vec<Record> {
        self.books.lock().unwrap().get(document).cloned().unwrap_or_default()
    }

    fn save(&self) -> std::io::Result<()> {
        let json = serde_json::to_vec(&*self.books.lock().unwrap()).map_err(std::io::Error::other)?;
        if let Some(dir) = self.file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.file.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(tmp, &self.file)
    }
}

fn clip(mut value: String) -> String {
    if value.len() > MAX_FIELD {
        let mut end = MAX_FIELD;
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        value.truncate(end);
    }
    value
}

/// Adds a position to a book's history (newest first).
///
/// A device that sends the position it already had has not moved: readers
/// send their position again when they close or sleep, and that must not undo
/// what was read meanwhile on another device. So the old record keeps its time
/// and only takes the new details, unless the new one was set by hand.
fn add(history: &mut Vec<Record>, record: Record) {
    if let Some(i) = history.iter().position(|r| r.device_id == record.device_id) {
        let last = &history[i];
        if last.progress == record.progress
            && (record.cfi.is_none() || record.cfi == last.cfi)
            && !record.manual
            && record.timestamp >= last.timestamp
        {
            let merged = Record {
                timestamp: last.timestamp,
                manual: last.manual,
                cfi: record.cfi.or_else(|| last.cfi.clone()),
                context: record.context.or_else(|| last.context.clone()),
                ..record
            };
            history[i] = merged;
            return;
        }
    }
    let at = history
        .iter()
        .position(|r| r.timestamp <= record.timestamp)
        .unwrap_or(history.len());
    history.insert(at, record);
    history.truncate(HISTORY);
}

/// The position to continue from: the most recent one.
fn current(history: &[Record]) -> Option<&Record> {
    history.iter().max_by_key(|r| r.timestamp)
}

/// KOReader's document id: the MD5 of 1 KiB samples taken at 0, 1 KiB, 4 KiB,
/// 16 KiB… up to 1 GiB into the file.
pub fn partial_md5(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let len = file.metadata()?.len();
    let mut hasher = Md5::new();
    let mut buf = vec![0u8; 1024];
    for i in -1i32..=10 {
        let offset: u64 = if i < 0 { 0 } else { 1024u64 << (2 * i) };
        if offset >= len {
            break;
        }
        file.seek(SeekFrom::Start(offset))?;
        let mut read = 0;
        while read < buf.len() {
            match file.read(&mut buf[read..])? {
                0 => break,
                n => read += n,
            }
        }
        hasher.update(&buf[..read]);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

type IdCache = Mutex<HashMap<PathBuf, (u64, i64, String)>>;

/// The document id of a book, cached while the file does not change.
pub fn document_id(path: &Path, size: u64, modified: i64) -> Option<String> {
    static CACHE: OnceLock<IdCache> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some((s, m, id)) = cache.lock().unwrap().get(path) {
        if (*s, *m) == (size, modified) {
            return Some(id.clone());
        }
    }
    let id = partial_md5(path).ok()?;
    let mut cache = cache.lock().unwrap();
    if cache.len() > 4096 {
        cache.clear();
    }
    cache.insert(path.to_path_buf(), (size, modified, id.clone()));
    Some(id)
}

/// Document ids are hex MD5 digests (KOReader can also hash the file name, which looks the same).
pub fn valid_document(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn update(device: &str, progress: &str, timestamp: i64) -> Update {
        Update {
            progress: progress.into(),
            percentage: 0.5,
            device: device.into(),
            device_id: device.into(),
            timestamp: Some(timestamp),
            ..Default::default()
        }
    }

    #[test]
    fn newest_position_wins() {
        let dir = tempfile::tempdir().unwrap();
        let p = Progress::open(dir.path());
        p.update("doc", update("kobo", "a", 100), 1000).unwrap();
        p.update("doc", update("web", "b", 200), 1000).unwrap();
        // Read offline earlier and sent late: does not replace the newer one
        let current = p.update("doc", update("phone", "c", 150), 1000).unwrap();
        assert_eq!(current.progress, "b");
        assert_eq!(
            p.history("doc").iter().map(|r| r.progress.as_str()).collect::<Vec<_>>(),
            ["b", "c", "a"]
        );
        // Times in the future are taken as now
        assert_eq!(p.update("doc", update("web", "d", 5000), 1000).unwrap().timestamp, 1000);
    }

    #[test]
    fn resending_an_old_position_does_not_win() {
        let dir = tempfile::tempdir().unwrap();
        let p = Progress::open(dir.path());
        p.update("doc", update("kobo", "a", 100), 1000).unwrap();
        p.update("doc", update("web", "b", 200), 1000).unwrap();
        // The Kobo wakes up and sends the page it was left on
        assert_eq!(p.update("doc", update("kobo", "a", 300), 1000).unwrap().progress, "b");
        // Once it turns a page it is the newest again
        assert_eq!(p.update("doc", update("kobo", "a2", 400), 1000).unwrap().progress, "a2");
    }

    #[test]
    fn a_point_set_by_hand_wins_even_if_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let p = Progress::open(dir.path());
        p.update("doc", update("phone", "a", 100), 1000).unwrap();
        p.update("doc", update("web", "b", 200), 1000).unwrap();
        let mut mark = update("phone", "a", 300);
        mark.manual = true;
        let current = p.update("doc", mark, 1000).unwrap();
        assert!(current.manual);
        assert_eq!(current.progress, "a");
    }

    #[test]
    fn persists() {
        let dir = tempfile::tempdir().unwrap();
        let p = Progress::open(dir.path());
        let mut u = update("web", "/body/DocFragment[2]/body/p[3]/text()[1].4", 100);
        u.cfi = Some("epubcfi(/6/4!/4/6/1:4)".into());
        u.context = Some(Context {
            after: "It was".into(),
            ..Default::default()
        });
        p.update("doc", u, 1000).unwrap();
        let reopened = Progress::open(dir.path());
        let current = reopened.current("doc").unwrap();
        assert_eq!(current.cfi.as_deref(), Some("epubcfi(/6/4!/4/6/1:4)"));
        assert_eq!(current.context.unwrap().after, "It was");
        assert_eq!(reopened.current("other"), None);
    }

    #[test]
    fn partial_md5_samples_like_koreader() {
        let dir = tempfile::tempdir().unwrap();
        let data: Vec<u8> = (0..20_000u32).map(|i| (i * 7 % 251) as u8).collect();
        let file = dir.path().join("book.epub");
        std::fs::write(&file, &data).unwrap();
        // Samples at 0, 1024, 4096 and 16384 (the last one cut short by the end of the file)
        let mut expected = Md5::new();
        for offset in [0usize, 1024, 4096, 16384] {
            expected.update(&data[offset..(offset + 1024).min(data.len())]);
        }
        let expected: String = expected.finalize().iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(partial_md5(&file).unwrap(), expected);

        std::fs::write(&file, b"tiny").unwrap();
        assert_eq!(partial_md5(&file).unwrap(), "d60cadf1a41c651e1f0ade50136bad43");
    }
}
