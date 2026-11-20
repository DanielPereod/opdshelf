//! Books marked as read or unread in the web panel.
//!
//! A mark only changes how the panel shows a book: reading positions, and so
//! KOReader and the Android app, are left alone. "Read" shows the book as
//! finished. "Unread" hides the positions saved until then, so the book is
//! back on no shelf until someone reads it again. Marks are kept in
//! DATA_DIR/marks.json, keyed by document id, so they follow a book that is
//! renamed or moved.
//!
//! The Android app marks books as finished or not finished here too, by
//! document id and with the time it was marked on the device, so the newest
//! mark wins whichever device sent it last. A mark that is cleared leaves a
//! tombstone (when it was cleared, in DATA_DIR/marks-cleared.json) for
//! [`TOMBSTONE_DAYS`] days, so an older mark sent late by a device that was
//! offline does not bring it back, and the apps can learn it was cleared.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

/// How long the time a mark was cleared is remembered.
const TOMBSTONE_DAYS: i64 = 400;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Read,
    Unread,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mark {
    pub state: State,
    /// When it was marked (Unix seconds); for "unread", positions saved
    /// after this count again.
    pub at: i64,
}

/// A book's mark that changed, sent to clients listening for changes; `mark`
/// is null when it was cleared.
#[derive(Clone, Debug, Serialize)]
pub struct Change {
    pub document: String,
    pub mark: Option<Mark>,
    /// When the mark was cleared, if it was.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cleared_at: Option<i64>,
}

#[derive(Clone)]
pub struct Marks {
    file: PathBuf,
    cleared_file: PathBuf,
    marks: Arc<Mutex<HashMap<String, Mark>>>,
    /// When each cleared mark was cleared (by document id).
    cleared: Arc<Mutex<HashMap<String, i64>>>,
    changes: broadcast::Sender<Change>,
}

impl Marks {
    pub fn open(dir: &Path) -> Self {
        let file = dir.join("marks.json");
        let marks = match std::fs::read(&file) {
            Ok(json) => serde_json::from_slice(&json).unwrap_or_else(|err| {
                tracing::error!("ignoring unreadable {}: {err}", file.display());
                HashMap::new()
            }),
            Err(_) => HashMap::new(),
        };
        let cleared_file = dir.join("marks-cleared.json");
        let mut cleared: HashMap<String, i64> = match std::fs::read(&cleared_file) {
            Ok(json) => serde_json::from_slice(&json).unwrap_or_else(|err| {
                tracing::error!("ignoring unreadable {}: {err}", cleared_file.display());
                HashMap::new()
            }),
            Err(_) => HashMap::new(),
        };
        let oldest = chrono::Utc::now().timestamp() - TOMBSTONE_DAYS * 86_400;
        cleared.retain(|_, at| *at >= oldest);
        Self {
            file,
            cleared_file,
            marks: Arc::new(Mutex::new(marks)),
            cleared: Arc::new(Mutex::new(cleared)),
            changes: broadcast::channel(256).0,
        }
    }

    /// Marks that change, as they happen.
    pub fn subscribe(&self) -> broadcast::Receiver<Change> {
        self.changes.subscribe()
    }

    pub fn get(&self, document: &str) -> Option<Mark> {
        self.marks.lock().unwrap().get(document).copied()
    }

    pub fn all(&self) -> HashMap<String, Mark> {
        self.marks.lock().unwrap().clone()
    }

    /// When each mark that was cleared (and not set again) was cleared.
    pub fn cleared(&self) -> HashMap<String, i64> {
        self.cleared.lock().unwrap().clone()
    }

    /// Marks the documents, or clears their mark with `None`.
    pub fn set(&self, documents: &[String], state: Option<State>, now: i64) -> std::io::Result<()> {
        self.set_at(documents, state, now, true).map(|_| ())
    }

    /// Marks the documents as of `at`, or clears their mark with `None`. Unless
    /// `force`, a document marked later than `at` keeps its mark: the newest
    /// mark wins, whatever order devices send them in. Answers each document's
    /// mark afterwards.
    pub fn set_at(
        &self,
        documents: &[String],
        state: Option<State>,
        at: i64,
        force: bool,
    ) -> std::io::Result<Vec<(String, Option<Mark>)>> {
        let mut changed = Vec::new();
        let mut tombstones_changed = false;
        let result: Vec<_> = {
            let mut marks = self.marks.lock().unwrap();
            let mut cleared = self.cleared.lock().unwrap();
            documents
                .iter()
                .map(|document| {
                    let before = marks.get(document).copied();
                    let before_cleared = cleared.get(document).copied();
                    // A mark made after this one, or a clear after it, wins
                    let newer = before.is_some_and(|m| m.at > at) || before_cleared.is_some_and(|c| c > at);
                    if force || !newer {
                        match state {
                            Some(state) => {
                                marks.insert(document.clone(), Mark { state, at });
                                cleared.remove(document);
                            }
                            None => {
                                marks.remove(document);
                                cleared.insert(document.clone(), at);
                            }
                        };
                    }
                    let after = marks.get(document).copied();
                    let after_cleared = cleared.get(document).copied();
                    tombstones_changed |= after_cleared != before_cleared;
                    if after != before || (after.is_none() && after_cleared != before_cleared) {
                        changed.push(Change {
                            document: document.clone(),
                            mark: after,
                            cleared_at: if after.is_none() { after_cleared } else { None },
                        });
                    }
                    (document.clone(), after)
                })
                .collect()
        };
        if tombstones_changed {
            self.save_cleared()?;
        }
        if !changed.is_empty() {
            self.save()?;
            for change in changed {
                // Nobody listening is fine
                let _ = self.changes.send(change);
            }
        }
        Ok(result)
    }

    /// Drops the read marks of documents `keep` rejects (books no longer in the
    /// library), or only counts them with `dry_run`. Returns how many.
    pub fn retain_documents(&self, keep: impl Fn(&str) -> bool, dry_run: bool) -> std::io::Result<usize> {
        let removed = {
            let mut all = self.marks.lock().unwrap();
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
        if !dry_run {
            let mut cleared = self.cleared.lock().unwrap();
            let before = cleared.len();
            cleared.retain(|document, _| keep(document));
            let changed = cleared.len() != before;
            drop(cleared);
            if changed {
                self.save_cleared()?;
            }
        }
        Ok(removed)
    }

    fn save_cleared(&self) -> std::io::Result<()> {
        let cleared = self.cleared.lock().unwrap();
        let json = serde_json::to_vec(&*cleared).map_err(std::io::Error::other)?;
        crate::store::write_atomic(&self.cleared_file, &json)
    }

    fn save(&self) -> std::io::Result<()> {
        // Kept locked until the file is written: two saves at once could
        // otherwise mix their files, or leave the older one on disk
        let marks = self.marks.lock().unwrap();
        let json = serde_json::to_vec_pretty(&*marks).map_err(std::io::Error::other)?;
        crate::store::write_atomic(&self.file, &json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_are_saved_and_cleared() {
        let dir = std::env::temp_dir().join(format!("opdshelf-marks-{}", rand::random::<u64>()));
        let marks = Marks::open(&dir);
        marks.set(&["a".into(), "b".into()], Some(State::Read), 10).unwrap();
        marks.set(&["b".into()], Some(State::Unread), 20).unwrap();
        let reopened = Marks::open(&dir);
        assert_eq!(
            reopened.get("a"),
            Some(Mark {
                state: State::Read,
                at: 10
            })
        );
        assert_eq!(
            reopened.get("b"),
            Some(Mark {
                state: State::Unread,
                at: 20
            })
        );
        let now = chrono::Utc::now().timestamp();
        reopened.set(&["a".into()], None, now).unwrap();
        assert_eq!(Marks::open(&dir).get("a"), None);
        assert_eq!(Marks::open(&dir).cleared()["a"], now, "a cleared mark is remembered");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_newest_mark_wins() {
        let dir = tempfile::tempdir().unwrap();
        let marks = Marks::open(dir.path());
        let mut changes = marks.subscribe();
        let doc = vec!["doc".to_string()];
        marks.set_at(&doc, Some(State::Read), 100, false).unwrap();
        assert_eq!(changes.try_recv().unwrap().mark.unwrap().state, State::Read);
        // Marked earlier on a device that was offline: it loses
        let answer = marks.set_at(&doc, Some(State::Unread), 50, false).unwrap();
        assert_eq!(answer[0].1.unwrap().state, State::Read);
        assert!(changes.try_recv().is_err(), "nothing changed, nothing announced");
        // Nor can it clear the newer mark
        marks.set_at(&doc, None, 60, false).unwrap();
        assert_eq!(marks.get("doc").unwrap().at, 100);
        // A later one wins
        marks.set_at(&doc, Some(State::Unread), 150, false).unwrap();
        assert_eq!(
            marks.get("doc"),
            Some(Mark {
                state: State::Unread,
                at: 150
            })
        );
        assert_eq!(changes.try_recv().unwrap().mark.unwrap().at, 150);
        // A mark older than the clear cannot bring it back
        marks.set_at(&doc, None, 200, false).unwrap();
        let _ = changes.try_recv();
        let answer = marks.set_at(&doc, Some(State::Read), 180, false).unwrap();
        assert!(answer[0].1.is_none());
        // A newer one can
        let answer = marks.set_at(&doc, Some(State::Read), 250, false).unwrap();
        assert_eq!(answer[0].1.unwrap().at, 250);
        assert!(marks.cleared().is_empty());
        let _ = changes.try_recv();
        // The panel's marks are always the newest
        marks.set(&doc, None, 10).unwrap();
        assert_eq!(marks.get("doc"), None);
        assert!(changes.try_recv().unwrap().mark.is_none());
    }
}
