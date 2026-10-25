//! Cover images and thumbnails. Generated images (PDF covers and thumbnails) are
//! cached in DATA_DIR/cache, keyed by the book's path, size, mtime and cover source.

use image::codecs::jpeg::JpegEncoder;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::library::{Book, Snapshot};
use crate::metadata::{self, CoverSource};
use crate::store::Store;

pub const THUMB_WIDTH: u32 = 320;
pub const THUMB_HEIGHT: u32 = 480;

fn cache_key(book: &Book) -> String {
    let mut hash = Sha256::new();
    hash.update(format!(
        "{}\0{}\0{}\0{:?}",
        book.path,
        book.size,
        book.modified.timestamp(),
        book.meta.cover
    ));
    hash.finalize()[..16].iter().map(|b| format!("{b:02x}")).collect()
}

/// Changes whenever the cover changes; added to cover URLs so browsers and
/// readers do not keep showing a cached old cover.
pub fn version(book: &Book) -> String {
    cache_key(book)[..12].to_string()
}

/// Strong validator for HTTP caching of a book's cover and thumbnail.
pub fn etag(book: &Book) -> String {
    format!("\"{}\"", cache_key(book))
}

fn cached(path: &Path, generate: impl FnOnce() -> Option<Vec<u8>>) -> Option<Vec<u8>> {
    if let Ok(data) = std::fs::read(path) {
        return Some(data);
    }
    let data = generate()?;
    let write = || -> std::io::Result<()> {
        std::fs::create_dir_all(path.parent().expect("cache files have a parent"))?;
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, &data)?;
        std::fs::rename(tmp, path)
    };
    if let Err(err) = write() {
        tracing::debug!("cannot cache {}: {err}", path.display());
    }
    Some(data)
}

fn pdf_cover_path(store: &Store, book: &Book) -> PathBuf {
    store
        .cache_dir()
        .join("covers")
        .join(format!("{}.jpg", cache_key(book)))
}

fn thumb_path(store: &Store, book: &Book) -> PathBuf {
    store
        .cache_dir()
        .join("thumbs")
        .join(format!("{}.jpg", cache_key(book)))
}

/// Full size cover and its MIME type.
pub fn cover(store: &Store, book: &Book, file: &Path) -> Option<(Vec<u8>, String)> {
    let source = book.meta.cover.as_ref()?;
    let data = match source {
        CoverSource::Custom { file, .. } => std::fs::read(store.covers_dir().join(file)).ok(),
        CoverSource::Pdf => cached(&pdf_cover_path(store, book), || metadata::read_cover(file, source)),
        _ => metadata::read_cover(file, source),
    }?;
    Some((data, source.mime().to_string()))
}

/// JPEG thumbnail that fits in [`THUMB_WIDTH`]x[`THUMB_HEIGHT`].
pub fn thumbnail(store: &Store, book: &Book, file: &Path) -> Option<(Vec<u8>, String)> {
    let mut original = None;
    let thumb = cached(&thumb_path(store, book), || {
        let (data, mime) = cover(store, book, file)?;
        match resize(&data) {
            Ok(thumb) => Some(thumb),
            Err(err) => {
                tracing::debug!("cannot make thumbnail of {}: {err}", book.path);
                original = Some((data, mime));
                None
            }
        }
    });
    thumb.map(|t| (t, "image/jpeg".to_string())).or(original)
}

pub fn resize(data: &[u8]) -> Result<Vec<u8>, String> {
    let image = image::load_from_memory(data).map_err(|e| e.to_string())?;
    let thumb = image.thumbnail(THUMB_WIDTH, THUMB_HEIGHT).to_rgb8();
    let mut out = Vec::new();
    thumb
        .write_with_encoder(JpegEncoder::new_with_quality(&mut out, 80))
        .map_err(|e| e.to_string())?;
    Ok(out)
}

/// Deletes cached images of books that changed or no longer exist.
pub fn prune(store: &Store, snapshot: &Snapshot) {
    let keep: HashSet<String> = snapshot.books.iter().map(|b| format!("{}.jpg", cache_key(b))).collect();
    for dir in ["covers", "thumbs"] {
        let Ok(entries) = std::fs::read_dir(store.cache_dir().join(dir)) else {
            continue;
        };
        for entry in entries.flatten() {
            if !keep.contains(&*entry.file_name().to_string_lossy()) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resizes_to_jpeg() {
        let image = image::RgbImage::from_pixel(1000, 1500, image::Rgb([200, 10, 10]));
        let mut png = Vec::new();
        image::DynamicImage::ImageRgb8(image)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let thumb = resize(&png).unwrap();
        let decoded = image::load_from_memory(&thumb).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (THUMB_WIDTH, THUMB_HEIGHT));
        assert!(resize(b"junk").is_err());
    }
}
