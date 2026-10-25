//! Keeps the library snapshot up to date by watching BOOKS_DIR, so requests no
//! longer walk the whole folder and new books are indexed as soon as they appear.

use notify::{RecursiveMode, Watcher};
use std::path::Path;
use std::time::Duration;

use crate::covers;
use crate::library::Library;

/// Waits for this long after the last change before rescanning, so copying many
/// files at once triggers a single scan.
const DEBOUNCE: Duration = Duration::from_millis(750);

/// Starts the watcher and the periodic rescan. Falls back to scanning on every
/// request if the platform cannot watch the folder.
pub fn start(library: Library, scan_interval: Duration) {
    let root = library.root().to_path_buf();
    if let Err(err) = std::fs::create_dir_all(&root) {
        tracing::error!("cannot create {}: {err}", root.display());
        return;
    }

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let events_root = root.clone();
    let watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if let Ok(event) = event {
            let relevant = event.need_rescan() || event.paths.iter().any(|p| !is_hidden(&events_root, p));
            if relevant && !event.kind.is_access() {
                let _ = tx.send(());
            }
        }
    })
    .and_then(|mut watcher| watcher.watch(&root, RecursiveMode::Recursive).map(|_| watcher));

    let watcher = match watcher {
        Ok(watcher) => watcher,
        Err(err) => {
            tracing::warn!(
                "cannot watch {}: {err}; the folder will be rescanned on every request",
                root.display()
            );
            return;
        }
    };
    library.set_watching(true);
    tracing::info!("watching {} for changes", root.display());

    let watched = library.clone();
    tokio::spawn(async move {
        // Keep the watcher alive for as long as this task runs
        let _watcher = watcher;
        while rx.recv().await.is_some() {
            loop {
                match tokio::time::timeout(DEBOUNCE, rx.recv()).await {
                    Ok(Some(())) => continue,
                    Ok(None) => return,
                    Err(_) => break,
                }
            }
            watched.invalidate();
            let books = watched.refresh().await.books.len();
            tracing::info!("library changed, {books} books");
        }
    });

    if !scan_interval.is_zero() {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(scan_interval);
            ticker.tick().await;
            loop {
                ticker.tick().await;
                library.invalidate();
                let snapshot = library.refresh().await;
                let store = library.store().clone();
                let _ = tokio::task::spawn_blocking(move || covers::prune(&store, &snapshot)).await;
            }
        });
    }
}

/// Changes inside hidden folders (like DATA_DIR's default `.opdshelf`) are ignored.
fn is_hidden(root: &Path, path: &Path) -> bool {
    path.strip_prefix(root)
        .map(|rel| {
            rel.components()
                .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
        })
        .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DEFAULT_EXTENSIONS;
    use crate::store::Store;

    #[test]
    fn hidden_paths() {
        let root = Path::new("/books");
        assert!(is_hidden(root, Path::new("/books/.opdshelf/cache/x.jpg")));
        assert!(is_hidden(root, Path::new("/books/a/.tmp")));
        assert!(!is_hidden(root, Path::new("/books/a/b.epub")));
        assert!(is_hidden(root, Path::new("/elsewhere/b.epub")));
    }

    #[tokio::test]
    async fn picks_up_new_books() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let library = Library::new(
            root.clone(),
            DEFAULT_EXTENSIONS.iter().map(|e| e.to_string()).collect(),
            Store::open(&root.join(".opdshelf")),
        );
        start(library.clone(), Duration::ZERO);
        assert!(library.snapshot().await.books.is_empty());

        std::fs::create_dir_all(root.join("Nueva")).unwrap();
        std::fs::write(root.join("Nueva/libro.txt"), b"x").unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while library.snapshot().await.books.is_empty() {
            assert!(std::time::Instant::now() < deadline, "new book not detected");
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert_eq!(library.snapshot().await.books[0].path, "Nueva/libro.txt");
    }
}
