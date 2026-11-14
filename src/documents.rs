//! Which book each synced document id belongs to.
//!
//! Positions are saved under KOReader's document id (a partial MD5 of the
//! file), which says nothing about the book. To show reading progress in the
//! library and to name the book each device is on, the ids of every book are
//! worked out once and kept in DATA_DIR/document-ids.json, so restarts and
//! large libraries on slow disks do not read every file again.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::library::{Book, Library};
use crate::progress;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Entry {
    size: u64,
    modified: i64,
    id: String,
}

#[derive(Clone)]
pub struct DocumentIds {
    file: PathBuf,
    /// Book path → its id, while the file keeps its size and date.
    known: Arc<Mutex<HashMap<String, Entry>>>,
    /// Only one pass over the library at a time.
    indexing: Arc<tokio::sync::Mutex<()>>,
}

impl DocumentIds {
    pub fn open(dir: &Path) -> Self {
        let file = dir.join("document-ids.json");
        let known = std::fs::read(&file)
            .ok()
            .and_then(|json| serde_json::from_slice(&json).ok())
            .unwrap_or_default();
        Self {
            file,
            known: Arc::new(Mutex::new(known)),
            indexing: Arc::default(),
        }
    }

    fn cached(&self, book: &Book) -> Option<String> {
        let known = self.known.lock().unwrap();
        let entry = known.get(&book.path)?;
        (entry.size == book.size && entry.modified == book.modified.timestamp()).then(|| entry.id.clone())
    }

    fn remember(&self, book: &Book, id: &str) {
        self.known.lock().unwrap().insert(
            book.path.clone(),
            Entry {
                size: book.size,
                modified: book.modified.timestamp(),
                id: id.to_string(),
            },
        );
    }

    /// Book path → document id, for the books whose id is known (see [`Self::index`]).
    pub fn ids_by_path(&self) -> HashMap<String, String> {
        self.known
            .lock()
            .unwrap()
            .iter()
            .map(|(path, entry)| (path.clone(), entry.id.clone()))
            .collect()
    }

    /// The id of one book, reading the file if it is not known yet.
    pub async fn id(&self, library: &Library, book: &Book) -> Option<String> {
        if let Some(id) = self.cached(book) {
            return Some(id);
        }
        let (_, file) = library.book_path(&book.path)?;
        let (size, modified) = (book.size, book.modified.timestamp());
        let id = tokio::task::spawn_blocking(move || progress::document_id(&file, size, modified))
            .await
            .ok()
            .flatten()?;
        self.remember(book, &id);
        Some(id)
    }

    /// Document id → book path for the whole library.
    pub async fn index(&self, library: &Library) -> HashMap<String, String> {
        let _one_at_a_time = self.indexing.lock().await;
        let snapshot = library.snapshot().await;
        let mut index = HashMap::new();
        let mut missing = Vec::new();
        for book in &snapshot.books {
            match self.cached(book) {
                Some(id) => {
                    index.insert(id, book.path.clone());
                }
                None => missing.push(book.clone()),
            }
        }
        let changed = !missing.is_empty() || self.known.lock().unwrap().len() != snapshot.books.len();
        for book in missing {
            if let Some(id) = self.id(library, &book).await {
                index.insert(id, book.path.clone());
            }
        }
        if changed {
            // Forget books that are gone so the file does not grow forever
            let paths: std::collections::HashSet<&str> = snapshot.books.iter().map(|b| b.path.as_str()).collect();
            self.known
                .lock()
                .unwrap()
                .retain(|path, _| paths.contains(path.as_str()));
            if let Err(err) = self.save() {
                tracing::warn!("cannot save {}: {err}", self.file.display());
            }
        }
        index
    }

    fn save(&self) -> std::io::Result<()> {
        let json = serde_json::to_vec(&*self.known.lock().unwrap()).map_err(std::io::Error::other)?;
        if let Some(dir) = self.file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.file.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(tmp, &self.file)
    }
}
