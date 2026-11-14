//! Book paths are always handled as `/`-separated paths relative to BOOKS_DIR.
//! Everything coming from a request goes through [`sanitize`] before touching the disk.

use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use std::path::{Component, Path, PathBuf};

/// Characters left unescaped in a path segment (RFC 3986 unreserved).
const SEGMENT: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'.').remove(b'_').remove(b'~');

/// Normalises a user supplied relative path. Rejects anything that could leave
/// BOOKS_DIR (`..`, absolute paths, drive prefixes) or reach hidden files.
pub fn sanitize(input: &str) -> Option<String> {
    let input = input.replace('\\', "/");
    let mut parts = Vec::new();
    for component in Path::new(&input).components() {
        match component {
            Component::Normal(part) => {
                let part = part.to_str()?;
                if part.starts_with('.') || part.contains('\0') {
                    return None;
                }
                parts.push(part);
            }
            Component::CurDir => {}
            _ => return None,
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// Same as [`sanitize`] but for a single file or folder name (no `/` allowed).
pub fn sanitize_name(input: &str) -> Option<String> {
    let name = sanitize(input)?;
    (!name.contains('/')).then_some(name)
}

/// Resolves a user supplied relative path to an absolute path inside `root`.
pub fn resolve(root: &Path, input: &str) -> Option<PathBuf> {
    sanitize(input).map(|rel| root.join(rel))
}

/// Percent-encodes every segment of a relative path, keeping the `/` separators.
pub fn url_encode(rel: &str) -> String {
    rel.split('/')
        .map(|segment| utf8_percent_encode(segment, SEGMENT).to_string())
        .collect::<Vec<_>>()
        .join("/")
}

/// Folder part of a relative path (`""` for files in the root).
pub fn parent(rel: &str) -> &str {
    rel.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("")
}

/// Last segment of a relative path.
pub fn file_name(rel: &str) -> &str {
    rel.rsplit_once('/').map(|(_, name)| name).unwrap_or(rel)
}

/// Where `path` ends up when `from` (a book or a folder) moves to `to`:
/// `from` itself and everything inside it, or `None` for anything else.
pub fn relocate(path: &str, from: &str, to: &str) -> Option<String> {
    if path == from {
        return Some(to.to_string());
    }
    let rest = path.strip_prefix(from)?.strip_prefix('/')?;
    Some(if to.is_empty() {
        rest.to_string()
    } else {
        format!("{to}/{rest}")
    })
}

/// Whether `path` is `folder` or inside it (`""`, the root, holds everything).
pub fn is_within(path: &str, folder: &str) -> bool {
    folder.is_empty() || relocate(path, folder, "").is_some()
}

/// Lower-case extension, treating `.fb2.zip` as a single extension.
pub fn extension(rel: &str) -> String {
    let name = file_name(rel).to_lowercase();
    if name.ends_with(".fb2.zip") {
        return "fb2.zip".into();
    }
    name.rsplit_once('.')
        .map(|(_, ext)| ext.to_string())
        .unwrap_or_default()
}

/// File name without its extension, used as the fallback title.
pub fn stem(rel: &str) -> String {
    let name = file_name(rel);
    let ext = extension(rel);
    if ext.is_empty() {
        return name.to_string();
    }
    name[..name.len() - ext.len() - 1].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_traversal() {
        for bad in [
            "../secret",
            "a/../../b",
            "/etc/passwd",
            "..",
            "",
            ".hidden",
            "a/.git/config",
            "a\\..\\..\\b",
        ] {
            assert_eq!(sanitize(bad), None, "{bad:?} should be rejected");
        }
    }

    #[test]
    fn normalises_valid_paths() {
        assert_eq!(sanitize("Saga/Vol 1.epub").as_deref(), Some("Saga/Vol 1.epub"));
        assert_eq!(sanitize("./a//b.pdf").as_deref(), Some("a/b.pdf"));
        assert_eq!(sanitize("a\\b.pdf").as_deref(), Some("a/b.pdf"));
        assert_eq!(sanitize_name("a/b.pdf"), None);
    }

    #[test]
    fn relocates_paths_under_a_moved_folder() {
        assert_eq!(
            relocate("a/b.epub", "a/b.epub", "c/b.epub").as_deref(),
            Some("c/b.epub")
        );
        assert_eq!(
            relocate("Saga/Vol 1/x.epub", "Saga", "Old/Saga").as_deref(),
            Some("Old/Saga/Vol 1/x.epub")
        );
        assert_eq!(relocate("Saga 2/x.epub", "Saga", "Old/Saga"), None);
        assert_eq!(relocate("Sagas/x.epub", "Saga", "Old"), None);
        assert!(is_within("Saga/Vol 1", "Saga") && is_within("Saga", "Saga") && is_within("x", ""));
        assert!(!is_within("Sagas", "Saga"));
    }

    #[test]
    fn encodes_segments() {
        assert_eq!(url_encode("Saga/it's #1?.epub"), "Saga/it%27s%20%231%3F.epub");
    }

    #[test]
    fn splits_names() {
        assert_eq!(extension("a/Book.FB2.zip"), "fb2.zip");
        assert_eq!(stem("a/Book.fb2.zip"), "Book");
        assert_eq!(stem("Book.epub"), "Book");
        assert_eq!(stem("README"), "README");
        assert_eq!(parent("a/b/c.epub"), "a/b");
        assert_eq!(parent("c.epub"), "");
    }
}
