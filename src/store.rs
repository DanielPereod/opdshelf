//! Metadata edited in the admin panel. Book files are never modified: edits are
//! kept in DATA_DIR/metadata.json, keyed by the book's relative path, and custom
//! covers in DATA_DIR/covers.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::metadata::{BookMeta, CoverSource};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MetaOverride {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authors: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subjects: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identifier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series_index: Option<String>,
    /// File name inside DATA_DIR/covers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover: Option<String>,
    /// The book's document id, noted by `Store::relink`: the edit follows the book when the file
    /// is renamed or moved outside the panel (in the NAS, in Calibre).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document: Option<String>,
}

impl MetaOverride {
    /// Applies the edited fields on top of the metadata read from the file.
    /// An empty string clears a field; the title falls back to the file's.
    pub fn apply(&self, meta: &mut BookMeta) {
        let non_empty = |v: &String| (!v.trim().is_empty()).then(|| v.trim().to_string());
        if let Some(title) = self.title.as_ref().and_then(non_empty) {
            meta.title = Some(title);
        }
        if let Some(authors) = &self.authors {
            meta.authors = authors.clone();
        }
        for (field, value) in [
            (&mut meta.description, &self.description),
            (&mut meta.language, &self.language),
            (&mut meta.publisher, &self.publisher),
            (&mut meta.date, &self.date),
            (&mut meta.identifier, &self.identifier),
            (&mut meta.series, &self.series),
            (&mut meta.series_index, &self.series_index),
        ] {
            if let Some(value) = value {
                *field = non_empty(value);
            }
        }
        if let Some(subjects) = &self.subjects {
            meta.subjects = subjects.clone();
        }
        if let Some(file) = &self.cover {
            let mime = crate::metadata::image_mime(file).unwrap_or("image/jpeg").to_string();
            meta.cover = Some(CoverSource::Custom {
                file: file.clone(),
                mime,
            });
        }
    }
}

#[derive(Clone)]
pub struct Store {
    dir: PathBuf,
    overrides: Arc<Mutex<HashMap<String, MetaOverride>>>,
}

impl Store {
    pub fn open(dir: &Path) -> Self {
        let overrides = match std::fs::read(dir.join("metadata.json")) {
            Ok(json) => serde_json::from_slice(&json).unwrap_or_else(|err| {
                tracing::error!("ignoring unreadable {}/metadata.json: {err}", dir.display());
                HashMap::new()
            }),
            Err(_) => HashMap::new(),
        };
        Self {
            dir: dir.to_path_buf(),
            overrides: Arc::new(Mutex::new(overrides)),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn covers_dir(&self) -> PathBuf {
        self.dir.join("covers")
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.dir.join("cache")
    }

    pub fn get(&self, rel: &str) -> Option<MetaOverride> {
        self.overrides.lock().unwrap().get(rel).cloned()
    }

    pub fn set(&self, rel: &str, value: MetaOverride) -> std::io::Result<()> {
        let old_cover = {
            let mut map = self.overrides.lock().unwrap();
            let old = map.insert(rel.to_string(), value.clone()).and_then(|o| o.cover);
            old.filter(|c| Some(c) != value.cover.as_ref())
        };
        self.remove_cover_file(old_cover);
        self.save()
    }

    pub fn remove(&self, rel: &str) -> std::io::Result<()> {
        let removed = self.overrides.lock().unwrap().remove(rel);
        match removed {
            Some(old) => {
                self.remove_cover_file(old.cover);
                self.save()
            }
            None => Ok(()),
        }
    }

    /// Moves the edits of a book, or with a folder those of every book in it,
    /// to the new path. Covers are kept by file name, so they follow too.
    pub fn rename(&self, from: &str, to: &str) -> std::io::Result<()> {
        let moved = {
            let mut map = self.overrides.lock().unwrap();
            let keys: Vec<(String, String)> = map
                .keys()
                .filter_map(|k| crate::paths::relocate(k, from, to).map(|n| (k.clone(), n)))
                .collect();
            let moved: Vec<_> = keys
                .iter()
                .filter_map(|(k, n)| Some((n.clone(), map.remove(k)?)))
                .collect();
            let any = !moved.is_empty();
            map.extend(moved);
            any
        };
        if moved {
            self.save()
        } else {
            Ok(())
        }
    }

    /// Keeps the edits with their books: notes which book (by document id) each edit belongs to,
    /// and moves an edit whose file is gone to the new path of the book with that id, when it has
    /// none of its own there. `index` is document id → path, `ids` path → document id, of the
    /// books there are now. Returns whether anything changed.
    pub fn relink(&self, index: &HashMap<String, String>, ids: &HashMap<String, String>) -> std::io::Result<bool> {
        let changed = {
            let mut map = self.overrides.lock().unwrap();
            let mut changed = false;
            let paths: Vec<String> = map.keys().cloned().collect();
            for path in paths {
                if let Some(id) = ids.get(&path) {
                    // The book is there: note which one it is
                    let edit = map.get_mut(&path).expect("the key was just listed");
                    if edit.document.as_ref() != Some(id) {
                        edit.document = Some(id.clone());
                        changed = true;
                    }
                    continue;
                }
                // Not found by its path: the file was renamed or moved, or is only unreadable now
                let Some(document) = map.get(&path).and_then(|e| e.document.clone()) else {
                    continue;
                };
                let Some(now) = index.get(&document) else {
                    continue;
                };
                if now != &path && !map.contains_key(now) {
                    if let Some(edit) = map.remove(&path) {
                        map.insert(now.clone(), edit);
                        changed = true;
                    }
                }
            }
            changed
        };
        if changed {
            self.save()?;
        }
        Ok(changed)
    }

    /// Drops the edits of books `keep` rejects (no longer in the library) and
    /// their covers, or only counts them with `dry_run`. Returns how many.
    pub fn retain_books(&self, keep: impl Fn(&str) -> bool, dry_run: bool) -> std::io::Result<usize> {
        let removed: Vec<MetaOverride> = {
            let mut map = self.overrides.lock().unwrap();
            let gone: Vec<String> = map.keys().filter(|p| !keep(p)).cloned().collect();
            if dry_run {
                return Ok(gone.len());
            }
            gone.iter().filter_map(|p| map.remove(p)).collect()
        };
        if removed.is_empty() {
            return Ok(0);
        }
        let count = removed.len();
        for old in removed {
            self.remove_cover_file(old.cover);
        }
        self.save()?;
        Ok(count)
    }

    /// Files in DATA_DIR/covers that no edited book uses, or only counts them
    /// with `dry_run`. Covers added in the last hour are left alone: one may
    /// have just been uploaded and not saved with its book yet.
    pub fn unused_covers(&self, dry_run: bool) -> usize {
        let used: std::collections::HashSet<String> = self
            .overrides
            .lock()
            .unwrap()
            .values()
            .filter_map(|o| o.cover.clone())
            .collect();
        let Ok(entries) = std::fs::read_dir(self.covers_dir()) else {
            return 0;
        };
        let recent = |entry: &std::fs::DirEntry| {
            entry
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .is_none_or(|age| age < std::time::Duration::from_secs(3600))
        };
        let mut count = 0;
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if used.contains(&name) || !entry.path().is_file() || recent(&entry) {
                continue;
            }
            count += 1;
            if !dry_run {
                let _ = std::fs::remove_file(entry.path());
            }
        }
        count
    }

    /// Stores an uploaded cover image and returns its file name.
    pub fn save_cover(&self, image: &[u8]) -> std::io::Result<String> {
        let ext = match image::guess_format(image) {
            Ok(image::ImageFormat::Png) => "png",
            Ok(image::ImageFormat::WebP) => "webp",
            Ok(image::ImageFormat::Gif) => "gif",
            Ok(image::ImageFormat::Jpeg) => "jpg",
            _ => return Err(std::io::Error::other("not a JPEG, PNG, WebP or GIF image")),
        };
        std::fs::create_dir_all(self.covers_dir())?;
        let name = format!("{:016x}.{ext}", rand::random::<u64>());
        std::fs::write(self.covers_dir().join(&name), image)?;
        Ok(name)
    }

    fn remove_cover_file(&self, file: Option<String>) {
        if let Some(file) = file.filter(|f| crate::paths::sanitize_name(f).is_some()) {
            let _ = std::fs::remove_file(self.covers_dir().join(file));
        }
    }

    fn save(&self) -> std::io::Result<()> {
        // Kept locked until the file is written: two saves at once could
        // otherwise mix their files, or leave the older one on disk
        let overrides = self.overrides.lock().unwrap();
        let json = serde_json::to_vec_pretty(&*overrides).map_err(std::io::Error::other)?;
        write_atomic(&self.dir.join("metadata.json"), &json)
    }
}

/// Writes a file of DATA_DIR whole or not at all: to a temporary file first,
/// then renamed over the old one, so a crash never leaves half a file.
pub(crate) fn write_atomic(file: &Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // Its own temporary file, so two saves at once never write into the same one
    let mut tmp = file.as_os_str().to_owned();
    tmp.push(format!(".tmp-{:016x}", rand::random::<u64>()));
    let tmp = PathBuf::from(tmp);
    let result = std::fs::write(&tmp, data).and_then(|()| std::fs::rename(&tmp, file));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_at_the_same_time_keep_every_edit() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path());
        let threads: Vec<_> = (0..8)
            .map(|t| {
                let store = store.clone();
                std::thread::spawn(move || {
                    for i in 0..40 {
                        let edit = MetaOverride {
                            title: Some(format!("A long enough title to take a while to write {t} {i}")),
                            ..Default::default()
                        };
                        store.set(&format!("{t}/{i}.epub"), edit).unwrap();
                    }
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        assert_eq!(Store::open(dir.path()).overrides.lock().unwrap().len(), 320);
    }

    #[test]
    fn apply_overrides() {
        let mut meta = BookMeta {
            title: Some("File title".into()),
            authors: vec!["A".into()],
            description: Some("desc".into()),
            ..Default::default()
        };
        MetaOverride {
            title: Some(" ".into()),
            authors: Some(vec!["B".into(), "C".into()]),
            description: Some(String::new()),
            language: Some("es".into()),
            cover: Some("x.png".into()),
            ..Default::default()
        }
        .apply(&mut meta);
        assert_eq!(meta.title.as_deref(), Some("File title"));
        assert_eq!(meta.authors, vec!["B", "C"]);
        assert_eq!(meta.description, None);
        assert_eq!(meta.language.as_deref(), Some("es"));
        assert_eq!(meta.cover.unwrap().mime(), "image/png");
    }

    #[test]
    fn persists_renames_and_removes_covers() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path());
        let png = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0];
        let cover = store.save_cover(&png).unwrap();
        assert!(cover.ends_with(".png"));
        assert!(store.save_cover(b"not an image").is_err());

        let value = MetaOverride {
            title: Some("T".into()),
            cover: Some(cover.clone()),
            ..Default::default()
        };
        store.set("a.epub", value.clone()).unwrap();
        store.rename("a.epub", "b/a.epub").unwrap();

        let reopened = Store::open(dir.path());
        assert_eq!(reopened.get("a.epub"), None);
        assert_eq!(reopened.get("b/a.epub"), Some(value.clone()));

        // A folder takes the edits of every book inside it along
        let plain = MetaOverride {
            title: Some("U".into()),
            ..Default::default()
        };
        reopened.set("b/c/d.epub", plain.clone()).unwrap();
        reopened.set("bb/e.epub", plain.clone()).unwrap();
        reopened.rename("b", "x/b").unwrap();
        let reopened = Store::open(dir.path());
        assert_eq!(reopened.get("x/b/a.epub"), Some(value.clone()));
        assert_eq!(reopened.get("x/b/c/d.epub"), Some(plain.clone()));
        assert_eq!(reopened.get("bb/e.epub"), Some(plain));
        assert_eq!(reopened.get("b/a.epub"), None);
        assert!(dir.path().join("covers").join(&cover).exists());
        reopened.rename("x/b", "b").unwrap();
        reopened.remove("b/a.epub").unwrap();
        assert!(!dir.path().join("covers").join(cover).exists());
    }
}
