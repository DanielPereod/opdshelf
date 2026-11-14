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
use tokio::sync::broadcast;

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
    /// The device's clock when it sent the update (Unix seconds). With it the
    /// server corrects `timestamp` for a device whose clock is wrong, which is
    /// common on e-readers, so positions from different devices compare fairly.
    #[serde(default)]
    pub sent_at: Option<i64>,
}

/// A change of a book's current position, sent to clients listening for them.
#[derive(Clone, Debug, Serialize)]
pub struct Change {
    pub document: String,
    pub current: Record,
}

/// What became of a saved position.
#[derive(Clone, Debug)]
pub struct Saved {
    /// The book's position to continue from after saving.
    pub current: Record,
    /// Whether the position sent is now the current one. When it is not, a
    /// newer position from another device won and the client should offer it.
    pub accepted: bool,
}

#[derive(Clone)]
pub struct Progress {
    file: PathBuf,
    books: Arc<Mutex<HashMap<String, Vec<Record>>>>,
    changes: broadcast::Sender<Change>,
    stats: crate::stats::Stats,
}

impl Progress {
    pub fn open(dir: &Path) -> Self {
        let file = dir.join("progress.json");
        let books: HashMap<String, Vec<Record>> = match std::fs::read(&file) {
            Ok(json) => serde_json::from_slice(&json).unwrap_or_else(|err| {
                tracing::error!("ignoring unreadable {}: {err}", file.display());
                HashMap::new()
            }),
            Err(_) => HashMap::new(),
        };
        let stats = crate::stats::Stats::open(dir, || {
            books
                .iter()
                .flat_map(|(doc, history)| {
                    history.iter().map(move |r| {
                        (
                            r.device_id.clone(),
                            doc.clone(),
                            r.progress.clone(),
                            r.percentage,
                            r.timestamp,
                        )
                    })
                })
                .collect()
        });
        Self {
            file,
            books: Arc::new(Mutex::new(books)),
            changes: broadcast::channel(256).0,
            stats,
        }
    }

    /// Reading statistics, kept up to date with every position saved.
    pub fn stats(&self) -> &crate::stats::Stats {
        &self.stats
    }

    /// Changes of current positions, as they happen.
    pub fn subscribe(&self) -> broadcast::Receiver<Change> {
        self.changes.subscribe()
    }

    /// Saves a position and returns the one now current.
    pub fn update(&self, document: &str, update: Update, now: i64) -> std::io::Result<Record> {
        self.apply(document, update, now).map(|saved| saved.current)
    }

    /// Saves a position, says whether it won and tells listeners when the current one changed.
    pub fn apply(&self, document: &str, update: Update, now: i64) -> std::io::Result<Saved> {
        // How far the device's clock is from the server's
        let skew = update.sent_at.map_or(0, |sent| now - sent);
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
            timestamp: update.timestamp.map_or(now, |t| t.saturating_add(skew).min(now)),
        };
        let (device_id, progress) = (record.device_id.clone(), record.progress.clone());
        self.stats
            .record(&device_id, document, &progress, record.percentage, record.timestamp);
        let (before, current) = {
            let mut books = self.books.lock().unwrap();
            let history = books.entry(document.to_string()).or_default();
            let before = current(history).cloned();
            add(history, record);
            (before, current(history).cloned().expect("a position was just added"))
        };
        self.save()?;
        let accepted = current.device_id == device_id && current.progress == progress;
        if before.as_ref() != Some(&current) {
            // Nobody listening is fine
            let _ = self.changes.send(Change {
                document: document.to_string(),
                current: current.clone(),
            });
        }
        Ok(Saved { current, accepted })
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

    /// The current position of every book that has one.
    pub fn all_current(&self) -> Vec<(String, Record)> {
        self.books
            .lock()
            .unwrap()
            .iter()
            .filter_map(|(doc, h)| current(h).map(|r| (doc.clone(), r.clone())))
            .collect()
    }

    /// Every saved position of every book, for summaries such as the device list.
    pub fn all_history(&self) -> HashMap<String, Vec<Record>> {
        self.books.lock().unwrap().clone()
    }

    /// Removes every position a device saved. Returns how many were removed.
    pub fn forget_device(&self, device_id: &str) -> std::io::Result<usize> {
        let mut removed = 0;
        let mut changes = Vec::new();
        {
            let mut books = self.books.lock().unwrap();
            for (doc, history) in books.iter_mut() {
                let before = current(history).cloned();
                let len = history.len();
                history.retain(|r| r.device_id != device_id);
                removed += len - history.len();
                if let Some(now) = current(history) {
                    if before.as_ref() != Some(now) {
                        changes.push(Change {
                            document: doc.clone(),
                            current: now.clone(),
                        });
                    }
                }
            }
            books.retain(|_, h| !h.is_empty());
        }
        if removed > 0 {
            self.save()?;
            for change in changes {
                let _ = self.changes.send(change);
            }
        }
        Ok(removed)
    }

    /// Drops the positions of documents `keep` rejects (books no longer in the
    /// library), or only counts them with `dry_run`. Returns how many.
    pub fn retain_documents(&self, keep: impl Fn(&str) -> bool, dry_run: bool) -> std::io::Result<usize> {
        let removed = {
            let mut all = self.books.lock().unwrap();
            let gone: Vec<String> = all.keys().filter(|d| !keep(d)).cloned().collect();
            if !dry_run {
                for document in &gone {
                    all.remove(document);
                }
            }
            gone.len()
        };
        if removed > 0 && !dry_run {
            self.save()?;
        }
        Ok(removed)
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

/// The position to continue from: the most recent one. Of two read in the
/// same second, the one that arrived last (first in the history).
fn current(history: &[Record]) -> Option<&Record> {
    history
        .iter()
        .reduce(|best, r| if r.timestamp > best.timestamp { r } else { best })
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
    fn of_two_in_the_same_second_the_last_one_wins() {
        let dir = tempfile::tempdir().unwrap();
        let p = Progress::open(dir.path());
        p.update("doc", update("kobo", "a", 100), 1000).unwrap();
        assert_eq!(p.update("doc", update("phone", "b", 100), 1000).unwrap().progress, "b");
    }

    #[test]
    fn a_wrong_clock_is_corrected() {
        let dir = tempfile::tempdir().unwrap();
        let p = Progress::open(dir.path());
        p.update("doc", update("web", "b", 1000), 1000).unwrap();
        // A Kobo whose clock is an hour behind read this 10 s before sending it at 2000 server time
        let mut late = update("kobo", "a", 2000 - 3600 - 10);
        late.sent_at = Some(2000 - 3600);
        let saved = p.apply("doc", late, 2000).unwrap();
        assert!(saved.accepted);
        assert_eq!(saved.current.timestamp, 1990);
    }

    #[test]
    fn changes_are_announced() {
        let dir = tempfile::tempdir().unwrap();
        let p = Progress::open(dir.path());
        let mut changes = p.subscribe();
        p.update("doc", update("kobo", "a", 100), 1000).unwrap();
        let change = changes.try_recv().unwrap();
        assert_eq!(
            (change.document.as_str(), change.current.progress.as_str()),
            ("doc", "a")
        );
        // An older position does not change the current one: nothing to announce, and it is not accepted
        let saved = p.apply("doc", update("phone", "z", 50), 1000).unwrap();
        assert!(!saved.accepted);
        assert!(changes.try_recv().is_err());
    }

    #[test]
    fn forgetting_a_device_removes_its_positions() {
        let dir = tempfile::tempdir().unwrap();
        let p = Progress::open(dir.path());
        p.update("doc", update("kobo", "a", 100), 1000).unwrap();
        p.update("doc", update("web", "b", 200), 1000).unwrap();
        p.update("other", update("web", "c", 300), 1000).unwrap();
        let mut changes = p.subscribe();
        assert_eq!(p.forget_device("web").unwrap(), 2);
        assert_eq!(p.current("doc").unwrap().progress, "a");
        assert_eq!(p.current("other"), None);
        assert_eq!(changes.try_recv().unwrap().current.progress, "a");
        assert_eq!(p.all_current().len(), 1);
        assert_eq!(p.forget_device("web").unwrap(), 0);
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
