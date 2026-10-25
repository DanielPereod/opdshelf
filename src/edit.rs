//! Editing a book's metadata and cover from the admin panel, and the ISBN lookup.

use axum::extract::{Multipart, Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Redirect, Response};
use axum::Form;
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;

use crate::lookup::normalize_isbn;
use crate::metadata::BookMeta;
use crate::paths;
use crate::store::MetaOverride;
use crate::web::{admin_redirect, book_json, fragment, library_data, page, with_selection, AdminQuery, AppState};

fn split_list(value: &str, separator: char) -> Vec<String> {
    value
        .split(separator)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .collect()
}

fn edit_url(rel: &str) -> String {
    format!("/admin/edit/{}", paths::url_encode(rel))
}

pub async fn edit_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(path): Path<String>,
    Query(query): Query<AdminQuery>,
) -> Response {
    let Some(book) = state.library.book(&path).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let meta = &book.meta;
    let isbn = meta.identifier.as_deref().and_then(normalize_isbn);
    let selection = json!({
        "book": book_json(&book),
        "tab": "edit",
        "editTab": true,
        "fields": {
            "title": book.title,
            "authors": meta.authors.join("; "),
            "description": meta.description,
            "language": meta.language,
            "publisher": meta.publisher,
            "subjects": meta.subjects.join(", "),
            "date": meta.date,
            "identifier": meta.identifier,
        },
        "isbn": isbn,
    });
    if query.wants_panel() {
        return fragment(&state, "inspector", selection);
    }
    let data = with_selection(library_data(&state, &query).await, selection);
    page(
        &state,
        &headers,
        "admin",
        &format!("Edit {} - OPDShelf", book.title),
        data,
    )
}

/// Builds the override from the submitted form, keeping only the fields that
/// differ from the file, so later changes to the file still show up.
fn diff(form: &HashMap<String, String>, file: &BookMeta, file_title: &str) -> MetaOverride {
    let field = |name: &str| form.get(name).map(|v| v.trim().to_string()).unwrap_or_default();
    // Fields missing from the form are left untouched
    let present = |name: &str| form.contains_key(name);
    let changed = |name: &str, original: &Option<String>| {
        let value = field(name);
        (present(name) && value != original.clone().unwrap_or_default()).then_some(value)
    };
    let authors = split_list(&field("authors"), ';');
    let subjects = split_list(&field("subjects"), ',');
    let title = field("title");
    MetaOverride {
        title: (!title.is_empty() && title != file_title).then_some(title),
        authors: (present("authors") && authors != file.authors).then_some(authors),
        description: changed("description", &file.description),
        language: changed("language", &file.language),
        publisher: changed("publisher", &file.publisher),
        subjects: (present("subjects") && subjects != file.subjects).then_some(subjects),
        date: changed("date", &file.date),
        identifier: changed("identifier", &file.identifier),
        cover: None,
    }
}

pub async fn save(State(state): State<AppState>, mut multipart: Multipart) -> Response {
    let mut form: HashMap<String, String> = HashMap::new();
    let mut cover_upload: Option<Vec<u8>> = None;
    loop {
        let field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(err) => return (StatusCode::BAD_REQUEST, err.to_string()).into_response(),
        };
        let name = field.name().unwrap_or_default().to_string();
        if name == "cover" {
            match field.bytes().await {
                Ok(bytes) if !bytes.is_empty() => cover_upload = Some(bytes.to_vec()),
                Ok(_) => {}
                Err(err) => return (StatusCode::BAD_REQUEST, err.to_string()).into_response(),
            }
        } else if let Ok(text) = field.text().await {
            form.insert(name, text);
        }
    }

    if !form.get("csrf").is_some_and(|t| state.auth.check_csrf(t)) {
        return (StatusCode::FORBIDDEN, "Invalid CSRF token").into_response();
    }
    let path = form.get("path").cloned().unwrap_or_default();
    let Some((rel, _)) = state.library.book_path(&path) else {
        return admin_redirect("", None, Some("Book not found"));
    };
    let Some(file_meta) = state.library.file_meta(&rel).await else {
        return admin_redirect("", None, Some("Book not found"));
    };
    let file_title = file_meta.title.clone().unwrap_or_else(|| paths::stem(&rel));
    let store = state.library.store().clone();
    let previous = store.get(&rel).unwrap_or_default();

    let mut edit = diff(&form, &file_meta, &file_title);
    let cover_url = form.get("cover_url").map(|u| u.trim()).unwrap_or_default();
    let new_cover = if let Some(image) = cover_upload {
        Some(image)
    } else if !cover_url.is_empty() {
        match state.lookup.download_cover(cover_url).await {
            Ok(image) => Some(image),
            Err(err) => {
                let error = format!("Cannot download the cover: {err}");
                return Redirect::to(&format!("{}?error={}", edit_url(&rel), paths::url_encode(&error)))
                    .into_response();
            }
        }
    } else {
        None
    };
    edit.cover = match new_cover {
        Some(image) => match store.save_cover(&image) {
            Ok(file) => Some(file),
            Err(err) => {
                let error = format!("Cannot save the cover: {err}");
                return Redirect::to(&format!("{}?error={}", edit_url(&rel), paths::url_encode(&error)))
                    .into_response();
            }
        },
        None if form.get("remove_cover").is_some_and(|v| v == "1") => None,
        None => previous.cover,
    };

    let result = if edit == MetaOverride::default() {
        store.remove(&rel)
    } else {
        store.set(&rel, edit)
    };
    if let Err(err) = result {
        let error = format!("Cannot save: {err}");
        return Redirect::to(&format!("{}?error={}", edit_url(&rel), paths::url_encode(&error))).into_response();
    }
    state.library.invalidate();
    tracing::info!("edited metadata of {rel}");
    Redirect::to(&format!("{}?notice=Saved", edit_url(&rel))).into_response()
}

#[derive(Deserialize)]
pub struct ResetForm {
    csrf: String,
    path: String,
}

pub async fn reset(State(state): State<AppState>, Form(form): Form<ResetForm>) -> Response {
    if !state.auth.check_csrf(&form.csrf) {
        return (StatusCode::FORBIDDEN, "Invalid CSRF token").into_response();
    }
    let Some(rel) = paths::sanitize(&form.path) else {
        return admin_redirect("", None, Some("Book not found"));
    };
    if let Err(err) = state.library.store().remove(&rel) {
        return admin_redirect(paths::parent(&rel), None, Some(&format!("Cannot reset: {err}")));
    }
    state.library.invalidate();
    Redirect::to(&format!(
        "{}?notice={}",
        edit_url(&rel),
        paths::url_encode("Restored the file's metadata")
    ))
    .into_response()
}

#[derive(Deserialize)]
pub struct LookupQuery {
    isbn: String,
}

pub async fn lookup(State(state): State<AppState>, Query(query): Query<LookupQuery>) -> Response {
    match state.lookup.isbn(&query.isbn).await {
        Ok(Some(found)) => Json(found).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "No book found for this ISBN" })),
        )
            .into_response(),
        Err(err) => (StatusCode::BAD_GATEWAY, Json(json!({ "error": err }))).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_changed_fields_are_kept() {
        let file = BookMeta {
            title: Some("T".into()),
            authors: vec!["A".into()],
            description: Some("D".into()),
            ..Default::default()
        };
        let form: HashMap<String, String> = [
            ("title", "T"),
            ("authors", "A; B"),
            ("description", " D "),
            ("language", "es"),
            ("subjects", ""),
            ("date", ""),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        let edit = diff(&form, &file, "T");
        assert_eq!(
            edit,
            MetaOverride {
                authors: Some(vec!["A".into(), "B".into()]),
                language: Some("es".into()),
                ..Default::default()
            }
        );
    }
}
