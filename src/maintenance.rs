//! The Maintenance tab of the panel's settings: books that look duplicated, and saved data
//! (edits, covers, positions, highlights, read marks) of books that are no
//! longer in the library. Nothing is changed until it is asked for.

use axum::extract::State;
use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use crate::library::{format_size, Book};
use crate::paths;
use crate::web::AppState;

/// Why the books of a group look like the same book.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// The same file content (KOReader's partial MD5).
    Same,
    /// The same title and authors, maybe in another format or edition.
    Title,
}

#[derive(Clone, Debug)]
pub struct Group {
    pub kind: Kind,
    /// Stays the same while the group has the same books, to remember it was dismissed.
    pub key: String,
    pub books: Vec<Book>,
}

/// Lowercase letters and digits only, words in their order.
fn normalize(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Authors compared whatever their order, and their words too, so
/// "Pratchett, Terry" and "Terry Pratchett" are the same person.
fn authors_key(authors: &[String]) -> String {
    let mut names: Vec<String> = authors
        .iter()
        .map(|a| {
            let mut words: Vec<String> = normalize(a).split(' ').map(str::to_string).collect();
            words.sort();
            words.join(" ")
        })
        .filter(|a| !a.is_empty())
        .collect();
    names.sort();
    names.dedup();
    names.join("|")
}

fn group_key(kind: Kind, books: &[Book]) -> String {
    let mut paths: Vec<&str> = books.iter().map(|b| b.path.as_str()).collect();
    paths.sort();
    let kind = match kind {
        Kind::Same => "same",
        Kind::Title => "title",
    };
    format!("{kind}:{}", paths.join("\n"))
}

/// Groups of books that look like the same book. `ids` maps a book's path to
/// its document id. A title group that only repeats a same-content group is left out.
pub fn find_duplicates(books: &[Book], ids: &HashMap<String, String>) -> Vec<Group> {
    let mut groups = Vec::new();
    let mut by_id: BTreeMap<&str, Vec<&Book>> = BTreeMap::new();
    let mut by_title: BTreeMap<(String, String), Vec<&Book>> = BTreeMap::new();
    for book in books {
        if let Some(id) = ids.get(&book.path) {
            by_id.entry(id).or_default().push(book);
        }
        let title = normalize(&book.title);
        if !title.is_empty() {
            by_title
                .entry((title, authors_key(&book.authors)))
                .or_default()
                .push(book);
        }
    }
    let mut same_sets: HashSet<Vec<&str>> = HashSet::new();
    let add = |kind: Kind, list: Vec<&Book>, groups: &mut Vec<Group>| {
        let mut books: Vec<Book> = list.into_iter().cloned().collect();
        books.sort_by(|a, b| a.path.cmp(&b.path));
        groups.push(Group {
            kind,
            key: group_key(kind, &books),
            books,
        });
    };
    for list in by_id.into_values().filter(|l| l.len() > 1) {
        let mut set: Vec<&str> = list.iter().map(|b| b.path.as_str()).collect();
        set.sort();
        same_sets.insert(set);
        add(Kind::Same, list, &mut groups);
    }
    for list in by_title.into_values().filter(|l| l.len() > 1) {
        let mut set: Vec<&str> = list.iter().map(|b| b.path.as_str()).collect();
        set.sort();
        if !same_sets.contains(&set) {
            add(Kind::Title, list, &mut groups);
        }
    }
    groups
}

/// Groups marked as "not duplicates", kept in DATA_DIR/duplicates-dismissed.json.
fn dismissed_file(dir: &Path) -> std::path::PathBuf {
    dir.join("duplicates-dismissed.json")
}

fn dismissed(dir: &Path) -> HashSet<String> {
    std::fs::read(dismissed_file(dir))
        .ok()
        .and_then(|json| serde_json::from_slice(&json).ok())
        .unwrap_or_default()
}

fn save_dismissed(dir: &Path, keys: &HashSet<String>) -> std::io::Result<()> {
    let mut list: Vec<&String> = keys.iter().collect();
    list.sort();
    std::fs::create_dir_all(dir)?;
    crate::store::write_atomic(
        &dismissed_file(dir),
        &serde_json::to_vec_pretty(&list).map_err(std::io::Error::other)?,
    )
}

/// One change at a time to the file of dismissed groups: each reads it, changes it and writes it.
static DISMISSED_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Keeps the groups marked as "not duplicates" when a book, or a folder with
/// books in it, moves: their keys name the books by path.
pub fn rename_dismissed(dir: &Path, from: &str, to: &str) {
    let _one_at_a_time = DISMISSED_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let keys = dismissed(dir);
    let mut changed = false;
    let renamed: HashSet<String> = keys
        .into_iter()
        .map(|key| {
            let Some((kind, list)) = key.split_once(':') else {
                return key;
            };
            let mut paths: Vec<String> = list
                .split('\n')
                .map(|p| crate::paths::relocate(p, from, to).unwrap_or_else(|| p.to_string()))
                .collect();
            paths.sort();
            let new = format!("{kind}:{}", paths.join("\n"));
            changed |= new != key;
            new
        })
        .collect();
    if changed {
        if let Err(err) = save_dismissed(dir, &renamed) {
            tracing::warn!("cannot save the dismissed duplicates: {err}");
        }
    }
}

/// Saved data of books that are no longer in the library.
#[derive(Debug, Default, PartialEq, Serialize)]
pub struct Orphans {
    pub metadata: usize,
    pub covers: usize,
    pub progress: usize,
    pub highlights: usize,
    pub bookmarks: usize,
    pub marks: usize,
    /// Books whose id could not be read just now (a share that failed, a locked file). While there
    /// are any, the data kept by id is neither counted nor removed: it may belong to them.
    pub unreadable: usize,
}

/// Counts what is left of books that are gone, or removes it: only the parts
/// named in `clean` (`metadata`, `covers`, `progress`, `highlights`,
/// `bookmarks`, `marks`).
/// With an empty library nothing is removed: the books folder may just be
/// unavailable (an unplugged disk), and its books would all look gone.
async fn orphans(state: &AppState, clean: &[&str]) -> std::io::Result<Orphans> {
    let index = state.documents.index(&state.library).await;
    let snapshot = state.library.snapshot().await;
    let paths: HashSet<&str> = snapshot.books.iter().map(|b| b.path.as_str()).collect();
    let safe = !snapshot.books.is_empty();
    let known = state.documents.ids_by_path();
    let unreadable = snapshot.books.iter().filter(|b| !known.contains_key(&b.path)).count();
    let dry = |what: &str| !(safe && clean.contains(&what));
    // The data kept by id is only judged when every book's id is known
    let dry_by_id = |what: &str| dry(what) || unreadable > 0;
    let present = |doc: &str| index.contains_key(doc);
    let store = state.library.store();
    let metadata = store.retain_books(|p| paths.contains(p), dry("metadata"))?;
    let by_id = |count: usize| if unreadable > 0 { 0 } else { count };
    Ok(Orphans {
        metadata,
        covers: store.unused_covers(dry("covers")),
        progress: by_id(state.progress.retain_documents(present, dry_by_id("progress"))?),
        highlights: by_id(state.annotations.retain_documents(present, dry_by_id("highlights"))?),
        bookmarks: by_id(state.bookmarks.retain_documents(present, dry_by_id("bookmarks"))?),
        marks: by_id(state.marks.retain_documents(present, dry_by_id("marks"))?),
        unreadable,
    })
}

fn book_json(book: &Book, ids: &HashMap<String, String>, state: &AppState) -> serde_json::Value {
    let progress = ids
        .get(&book.path)
        .and_then(|id| state.progress.current(id))
        .map(|r| (r.percentage * 100.0).round() as u32);
    json!({
        "path": book.path,
        "title": book.title,
        "authors": book.author_line(),
        "format": book.format,
        "size": format_size(book.size),
        "bytes": book.size,
        // Shown in the page's language by the browser
        "modified": book.modified.to_rfc3339(),
        "day": book.modified.format("%Y-%m-%d").to_string(),
        "folder": book.folder,
        "hasCover": book.has_cover,
        "coverVersion": crate::covers::version(book),
        "edited": book.edited,
        "readable": book.readable,
        "progress": progress,
    })
}

/// What the Maintenance tab of the settings shows: the groups of possible
/// duplicates and the saved data of books that are gone. It reads every
/// book's id and the saved data, so it is only worked out for that tab.
pub async fn tab_data(state: &AppState) -> std::io::Result<serde_json::Value> {
    let counts = orphans(state, &[]).await?;
    let snapshot = state.library.snapshot().await;
    let ids = state.documents.ids_by_path();
    let dismissed = dismissed(state.library.store().dir());
    let groups: Vec<_> = find_duplicates(&snapshot.books, &ids)
        .into_iter()
        .filter(|g| !dismissed.contains(&g.key))
        .map(|g| {
            json!({
                "same": g.kind == Kind::Same,
                "key": g.key,
                "books": g.books.iter().map(|b| book_json(b, &ids, state)).collect::<Vec<_>>(),
            })
        })
        .collect();
    let rows = [
        ("metadata", "Edited details", counts.metadata),
        ("covers", "Uploaded covers", counts.covers),
        ("progress", "Reading positions", counts.progress),
        ("highlights", "Highlights and notes", counts.highlights),
        ("bookmarks", "Bookmarks", counts.bookmarks),
        ("marks", "Read marks", counts.marks),
    ];
    let orphan_total = rows.iter().map(|r| r.2).sum::<usize>();
    Ok(json!({
        "oneGroup": groups.len() == 1,
        "groupCount": groups.len(),
        "groups": groups,
        "dismissedCount": dismissed.len(),
        "orphans": rows.iter().map(|(what, label, count)| json!({ "what": what, "label": label, "count": count })).collect::<Vec<_>>(),
        "orphanTotal": orphan_total,
        "oneOrphan": orphan_total == 1,
        "emptyLibrary": snapshot.books.is_empty(),
        "unreadable": counts.unreadable,
        "someUnreadable": counts.unreadable > 0,
        "oneUnreadable": counts.unreadable == 1,
    }))
}

/// The old address of the maintenance page: it is a tab of the settings now.
pub async fn old_page(uri: Uri) -> Response {
    let query = uri.query().map(|q| format!("?{q}")).unwrap_or_default();
    Redirect::to(&format!("{TAB}{query}")).into_response()
}

pub(crate) const TAB: &str = "/admin/settings/maintenance";

fn redirect(notice: Option<&str>, error: Option<&str>) -> Response {
    let mut url = String::from(TAB);
    if let Some(n) = notice {
        url.push_str(&format!("?notice={}", paths::url_encode(n)));
    } else if let Some(e) = error {
        url.push_str(&format!("?error={}", paths::url_encode(e)));
    }
    Redirect::to(&url).into_response()
}

#[derive(Deserialize)]
pub struct DeleteForm {
    csrf: String,
    path: String,
}

/// Deletes one book of a duplicate group.
pub async fn delete(State(state): State<AppState>, Form(form): Form<DeleteForm>) -> Response {
    if !state.auth.check_csrf(&form.csrf) {
        return (StatusCode::FORBIDDEN, "Invalid CSRF token").into_response();
    }
    let Some((rel, path)) = state.library.book_path(&form.path) else {
        return redirect(None, Some("Book not found"));
    };
    match crate::web::delete_book(&state, &rel, &path).await {
        Ok(()) => redirect(Some(&format!("Deleted {}", paths::file_name(&rel))), None),
        Err(err) => redirect(None, Some(&format!("Delete failed: {err}"))),
    }
}

#[derive(Deserialize)]
pub struct DismissForm {
    csrf: String,
    key: String,
}

/// Remembers that the books of a group are not duplicates.
pub async fn dismiss(State(state): State<AppState>, Form(form): Form<DismissForm>) -> Response {
    if !state.auth.check_csrf(&form.csrf) {
        return (StatusCode::FORBIDDEN, "Invalid CSRF token").into_response();
    }
    let dir = state.library.store().dir().to_path_buf();
    // Every book's id is needed to know which groups there are
    state.documents.index(&state.library).await;
    let snapshot = state.library.snapshot().await;
    let ids = state.documents.ids_by_path();
    let complete = !snapshot.books.is_empty() && snapshot.books.iter().all(|b| ids.contains_key(&b.path));
    let current: HashSet<String> = find_duplicates(&snapshot.books, &ids)
        .into_iter()
        .map(|g| g.key)
        .collect();
    let _one_at_a_time = DISMISSED_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut keys = dismissed(&dir);
    if form.key.len() < 100_000 {
        // The key lists the paths one per line, and browsers send the line
        // breaks of a form field as CRLF
        keys.insert(form.key.replace("\r\n", "\n"));
    }
    // Forget groups that no longer exist, so the file does not grow forever. Not while a book's
    // id could not be read: its group is missing from the list for that reason only
    if complete {
        keys.retain(|k| current.contains(k));
    }
    match save_dismissed(&dir, &keys) {
        Ok(()) => redirect(Some("Marked as not duplicates"), None),
        Err(err) => redirect(None, Some(&format!("Cannot save: {err}"))),
    }
}

/// Shows dismissed groups again.
pub async fn undismiss(State(state): State<AppState>, Form(form): Form<CsrfForm>) -> Response {
    if !state.auth.check_csrf(&form.csrf) {
        return (StatusCode::FORBIDDEN, "Invalid CSRF token").into_response();
    }
    let _one_at_a_time = DISMISSED_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _ = std::fs::remove_file(dismissed_file(state.library.store().dir()));
    redirect(None, None)
}

#[derive(Deserialize)]
pub struct CsrfForm {
    csrf: String,
}

#[derive(Deserialize)]
pub struct CleanForm {
    csrf: String,
    /// One of the parts, or `all`.
    what: String,
}

pub async fn clean(State(state): State<AppState>, Form(form): Form<CleanForm>) -> Response {
    if !state.auth.check_csrf(&form.csrf) {
        return (StatusCode::FORBIDDEN, "Invalid CSRF token").into_response();
    }
    const ALL: &[&str] = &["metadata", "covers", "progress", "highlights", "bookmarks", "marks"];
    let parts: Vec<&str> = match form.what.as_str() {
        "all" => ALL.to_vec(),
        what if ALL.contains(&what) => vec![what],
        _ => return redirect(None, Some("Nothing to clean")),
    };
    if state.library.snapshot().await.books.is_empty() {
        return redirect(None, Some("The library is empty: nothing was cleaned"));
    }
    match orphans(&state, &parts).await {
        Ok(found) => {
            let removed: usize = [
                ("metadata", found.metadata),
                ("covers", found.covers),
                ("progress", found.progress),
                ("highlights", found.highlights),
                ("bookmarks", found.bookmarks),
                ("marks", found.marks),
            ]
            .iter()
            .filter(|(what, _)| parts.contains(what))
            .map(|(_, count)| count)
            .sum();
            tracing::info!("cleaned {removed} entries of books no longer in the library ({parts:?})");
            redirect(Some(&format!("Removed {removed} entries")), None)
        }
        Err(err) => redirect(None, Some(&format!("Cannot clean: {err}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::BookMeta;

    #[test]
    fn dismissed_groups_follow_moved_books() {
        let dir = tempfile::tempdir().unwrap();
        let keys: HashSet<String> = [
            "same:A/x.epub\nB/y.epub".to_string(),
            "title:C/z.epub\nD/w.epub".to_string(),
        ]
        .into();
        save_dismissed(dir.path(), &keys).unwrap();
        rename_dismissed(dir.path(), "A", "Z/A");
        let mut after: Vec<String> = dismissed(dir.path()).into_iter().collect();
        after.sort();
        // The paths of a group stay sorted, as group_key makes them
        assert_eq!(after, ["same:B/y.epub\nZ/A/x.epub", "title:C/z.epub\nD/w.epub"]);
    }

    fn book(path: &str, title: &str, authors: &[&str]) -> Book {
        Book {
            path: path.into(),
            folder: paths::parent(path).into(),
            file_name: paths::file_name(path).into(),
            title: title.into(),
            authors: authors.iter().map(|a| a.to_string()).collect(),
            size: 1,
            modified: Default::default(),
            mime: String::new(),
            format: paths::extension(path).to_uppercase(),
            has_cover: false,
            readable: true,
            edited: false,
            meta: BookMeta::default(),
        }
    }

    #[test]
    fn finds_duplicates() {
        let books = vec![
            book("a/Mort.epub", "Mort", &["Terry Pratchett"]),
            book("b/mort.pdf", "MORT!", &["Pratchett, Terry"]),
            book("c/copy.epub", "Something else", &[]),
            book("d/copy.epub", "Another title", &[]),
            book("e/x.epub", "Same", &["A"]),
            book("f/x.epub", "Same", &["A"]),
            book("g/other.epub", "Mort", &["Someone Else"]),
        ];
        let ids: HashMap<String, String> = [
            ("a/Mort.epub", "1"),
            ("b/mort.pdf", "2"),
            ("c/copy.epub", "3"),
            ("d/copy.epub", "3"),
            ("e/x.epub", "4"),
            ("f/x.epub", "4"),
            ("g/other.epub", "5"),
        ]
        .into_iter()
        .map(|(p, i)| (p.to_string(), i.to_string()))
        .collect();
        let groups = find_duplicates(&books, &ids);
        let summary: Vec<(Kind, Vec<&str>)> = groups
            .iter()
            .map(|g| (g.kind, g.books.iter().map(|b| b.path.as_str()).collect()))
            .collect();
        assert_eq!(
            summary,
            vec![
                (Kind::Same, vec!["c/copy.epub", "d/copy.epub"]),
                (Kind::Same, vec!["e/x.epub", "f/x.epub"]),
                (Kind::Title, vec!["a/Mort.epub", "b/mort.pdf"]),
            ]
        );
        assert_ne!(groups[0].key, groups[1].key);
    }
}
