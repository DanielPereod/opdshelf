//! Books marked as read or unread in the web panel.
//!
//! A mark only changes how the panel shows a book: reading positions, and so
//! KOReader and the Android app, are left alone. "Read" shows the book as
//! finished. "Unread" hides the positions saved until then, so the book is
//! back on no shelf until someone reads it again. Marks are kept in
//! DATA_DIR/marks.json, keyed by document id, so they follow a book that is
//! renamed or moved.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

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

#[derive(Clone)]
pub struct Marks {
    file: PathBuf,
    marks: Arc<Mutex<HashMap<String, Mark>>>,
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
        Self {
            file,
            marks: Arc::new(Mutex::new(marks)),
        }
    }

    pub fn get(&self, document: &str) -> Option<Mark> {
        self.marks.lock().unwrap().get(document).copied()
    }

    pub fn all(&self) -> HashMap<String, Mark> {
        self.marks.lock().unwrap().clone()
    }

    /// Marks the documents, or clears their mark with `None`.
    pub fn set(&self, documents: &[String], state: Option<State>, now: i64) -> std::io::Result<()> {
        {
            let mut marks = self.marks.lock().unwrap();
            for document in documents {
                match state {
                    Some(state) => marks.insert(document.clone(), Mark { state, at: now }),
                    None => marks.remove(document),
                };
            }
        }
        self.save()
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
        Ok(removed)
    }

    fn save(&self) -> std::io::Result<()> {
        let json = serde_json::to_vec_pretty(&*self.marks.lock().unwrap()).map_err(std::io::Error::other)?;
        if let Some(dir) = self.file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.file.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(tmp, &self.file)
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
        reopened.set(&["a".into()], None, 30).unwrap();
        assert_eq!(Marks::open(&dir).get("a"), None);
        let _ = std::fs::remove_dir_all(dir);
    }
}
