//! Every highlight of the library together, and a daily review of a few of them.
//!
//! Read-only views over `annotations.json`: nothing here changes a highlight
//! or a book.

use axum::extract::{Query, State};
use axum::http::{header, HeaderMap};
use axum::response::{IntoResponse, Json, Response};
use serde::Deserialize;
use serde_json::json;
use std::collections::{HashMap, HashSet};

use crate::annotations::{self, Annotation};
use crate::web::AppState;

/// Highlights in a daily review.
pub const REVIEW_SIZE: usize = 5;

struct Book {
    path: String,
    title: String,
    authors: String,
}

async fn books(state: &AppState) -> HashMap<String, Book> {
    let index = state.documents.index(&state.library).await;
    let snapshot = state.library.snapshot().await;
    let by_path: HashMap<&str, &crate::library::Book> = snapshot.books.iter().map(|b| (b.path.as_str(), b)).collect();
    index
        .into_iter()
        .map(|(id, path)| {
            let (title, authors) = by_path
                .get(path.as_str())
                .map(|b| (b.title.clone(), b.author_line()))
                .unwrap_or_default();
            (id, Book { path, title, authors })
        })
        .collect()
}

fn entry(document: &str, a: &Annotation, books: &HashMap<String, Book>) -> serde_json::Value {
    let mut value = serde_json::to_value(a).unwrap_or_default();
    value["document"] = json!(document);
    if let Some(book) = books.get(document) {
        value["path"] = json!(book.path);
        value["title"] = json!(book.title);
        value["authors"] = json!(book.authors);
    }
    value
}

/// A number made from the day and the highlight, the same on every device.
fn mix(day: &str, id: &str) -> u64 {
    // FNV-1a
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in day.bytes().chain([0]).chain(id.bytes()) {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// The highlights to look at again on `day` (YYYY-MM-DD): chosen at random but
/// the same all day, from as many books as possible.
pub fn review<'a>(all: &'a [(String, Vec<Annotation>)], day: &str, size: usize) -> Vec<(&'a str, &'a Annotation)> {
    let mut list: Vec<(&str, &Annotation)> = all
        .iter()
        .flat_map(|(doc, notes)| notes.iter().map(move |a| (doc.as_str(), a)))
        .filter(|(_, a)| !a.text.trim().is_empty() || !a.note.trim().is_empty())
        .collect();
    list.sort_by_key(|(_, a)| mix(day, &a.id));
    let mut seen = HashSet::new();
    let (mut picked, rest): (Vec<_>, Vec<_>) = list.into_iter().partition(|(doc, _)| seen.insert(*doc));
    picked.truncate(size);
    picked.extend(rest.into_iter().take(size - picked.len()));
    picked
}

fn today() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

#[derive(Deserialize)]
pub struct ReviewQuery {
    /// The reader's local day; today in UTC without it.
    #[serde(default)]
    date: Option<String>,
}

/// Every highlight not deleted, newest first, with its book's title.
pub async fn list(State(state): State<AppState>) -> Response {
    let books = books(&state).await;
    let mut list: Vec<_> = state
        .annotations
        .all()
        .iter()
        .flat_map(|(doc, notes)| notes.iter().map(|a| entry(doc, a, &books)).collect::<Vec<_>>())
        .collect();
    list.sort_by_key(|e| std::cmp::Reverse(e["created"].as_i64().unwrap_or(0)));
    Json(json!({ "highlights": list })).into_response()
}

/// Today's review.
pub async fn daily(State(state): State<AppState>, Query(query): Query<ReviewQuery>) -> Response {
    let day = query
        .date
        .filter(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").is_ok())
        .unwrap_or_else(today);
    let books = books(&state).await;
    let all = state.annotations.all();
    let list: Vec<_> = review(&all, &day, REVIEW_SIZE)
        .into_iter()
        .map(|(doc, a)| entry(doc, a, &books))
        .collect();
    Json(json!({ "date": day, "highlights": list })).into_response()
}

#[derive(Deserialize)]
pub struct ExportQuery {
    #[serde(default)]
    format: Option<String>,
}

/// Every book's highlights in one Markdown or JSON file.
pub async fn export(State(state): State<AppState>, headers: HeaderMap, Query(query): Query<ExportQuery>) -> Response {
    let books = books(&state).await;
    let mut all = state.annotations.all();
    let title = |doc: &str| books.get(doc).map_or_else(|| doc.to_string(), |b| b.title.clone());
    all.sort_by_key(|(doc, _)| title(doc).to_lowercase());
    let (body, mime, ext) = if query.format.as_deref() == Some("json") {
        let list: Vec<_> = all
            .iter()
            .map(|(doc, notes)| {
                json!({
                    "document": doc,
                    "title": title(doc),
                    "authors": books.get(doc.as_str()).map(|b| b.authors.clone()).unwrap_or_default(),
                    "annotations": notes,
                })
            })
            .collect();
        (
            serde_json::to_string_pretty(&json!({ "books": list })).unwrap_or_default(),
            "application/json; charset=utf-8",
            "json",
        )
    } else {
        let spanish = crate::i18n::Lang::from_headers(&headers) == crate::i18n::Lang::Es;
        let parts: Vec<String> = all
            .iter()
            .map(|(doc, notes)| {
                let info = annotations::BookInfo {
                    title: title(doc),
                    authors: books.get(doc.as_str()).map(|b| b.authors.clone()).unwrap_or_default(),
                };
                annotations::markdown(&info, notes, spanish)
                    .replace("\n#", "\n##")
                    .replacen("# ", "## ", 1)
            })
            .collect();
        let head = if spanish {
            "# Subrayados\n\n"
        } else {
            "# Highlights\n\n"
        };
        (
            format!("{head}{}", parts.join("\n")),
            "text/markdown; charset=utf-8",
            "md",
        )
    };
    (
        [
            (header::CONTENT_TYPE, mime.to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"highlights.{ext}\""),
            ),
        ],
        body,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(id: &str, text: &str) -> Annotation {
        serde_json::from_value(json!({ "id": id, "text": text })).unwrap()
    }

    #[test]
    fn the_review_is_stable_for_a_day_and_spread_over_books() {
        let all = vec![
            ("a".to_string(), (0..20).map(|i| note(&format!("a{i}"), "x")).collect()),
            ("b".to_string(), vec![note("b0", "y"), note("b1", "")]),
            ("c".to_string(), vec![note("c0", "z")]),
        ];
        let one = review(&all, "2026-11-14", 5);
        assert_eq!(one.len(), 5);
        assert_eq!(one, review(&all, "2026-11-14", 5));
        let docs: HashSet<&str> = one.iter().map(|(d, _)| *d).collect();
        assert_eq!(docs.len(), 3);
        assert!(one.iter().all(|(_, a)| a.id != "b1"));
        let ids = |r: Vec<(&str, &Annotation)>| r.iter().map(|(_, a)| a.id.clone()).collect::<Vec<_>>();
        assert_ne!(ids(one), ids(review(&all, "2026-11-15", 5)));
        assert!(review(&[], "2026-11-14", 5).is_empty());
    }
}
