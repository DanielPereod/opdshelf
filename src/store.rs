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
    /// File name inside DATA_DIR/covers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover: Option<String>,
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

    pub fn rename(&self, from: &str, to: &str) -> std::io::Result<()> {
        let moved = {
            let mut map = self.overrides.lock().unwrap();
            map.remove(from).map(|value| map.insert(to.to_string(), value))
        };
        match moved {
            Some(_) => self.save(),
            None => Ok(()),
        }
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
        let json = serde_json::to_vec_pretty(&*self.overrides.lock().unwrap()).map_err(std::io::Error::other)?;
        std::fs::create_dir_all(&self.dir)?;
        let tmp = self.dir.join("metadata.json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(tmp, self.dir.join("metadata.json"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(reopened.get("b/a.epub"), Some(value));

        reopened.remove("b/a.epub").unwrap();
        assert!(!dir.path().join("covers").join(cover).exists());
    }
}
