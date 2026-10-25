//! OPDS 1.2 catalog: navigation root, paginated acquisition feeds, folders, authors and search.

use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::Deserialize;
use std::fmt::Write;

use crate::library::{Book, SortMode};
use crate::paths;
use crate::web::{base_url, AppState};

const NAV: &str = "application/atom+xml;profile=opds-catalog;kind=navigation";
const ACQ: &str = "application/atom+xml;profile=opds-catalog;kind=acquisition";

#[derive(Deserialize, Default)]
pub struct FeedQuery {
    sort: Option<String>,
    page: Option<usize>,
    q: Option<String>,
}

pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // Control characters are not allowed in XML 1.0
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => out.push(c),
        }
    }
    out
}

fn date(d: DateTime<Utc>) -> String {
    d.to_rfc3339_opts(SecondsFormat::Secs, true)
}

struct Feed {
    base: String,
    id: String,
    title: String,
    kind: &'static str,
    path_and_query: String,
    links: Vec<String>,
    extra: Vec<String>,
    entries: Vec<String>,
}

impl Feed {
    fn new(base: &str, id: &str, title: &str, kind: &'static str, path_and_query: String) -> Self {
        Self {
            base: base.to_string(),
            id: format!("urn:opdshelf:{id}"),
            title: title.to_string(),
            kind,
            path_and_query,
            links: Vec::new(),
            extra: Vec::new(),
            entries: Vec::new(),
        }
    }

    fn link(&mut self, rel: &str, href: &str, kind: &str) {
        self.links.push(format!(
            r#"<link rel="{}" href="{}{}" type="{}"/>"#,
            escape(rel),
            escape(&self.base),
            escape(href),
            kind
        ));
    }

    fn nav_entry(&mut self, id: &str, title: &str, content: &str, href: &str, kind: &str, updated: DateTime<Utc>) {
        self.entries.push(format!(
            r#"<entry><title>{}</title><id>urn:opdshelf:{}</id><updated>{}</updated><content type="text">{}</content><link rel="subsection" href="{}{}" type="{kind}"/></entry>"#,
            escape(title),
            escape(id),
            date(updated),
            escape(content),
            escape(&self.base),
            escape(href),
        ));
    }

    fn book_entry(&mut self, book: &Book) {
        let base = escape(&self.base);
        let url = paths::url_encode(&book.path);
        let mut e = String::from("<entry>");
        let _ = write!(e, "<title>{}</title>", escape(&book.title));
        let _ = write!(e, "<id>urn:opdshelf:book:{}</id>", escape(&url));
        let _ = write!(e, "<updated>{}</updated>", date(book.modified));
        for author in &book.authors {
            let _ = write!(e, "<author><name>{}</name></author>", escape(author));
        }
        if let Some(lang) = &book.meta.language {
            let _ = write!(e, "<dc:language>{}</dc:language>", escape(lang));
        }
        if let Some(publisher) = &book.meta.publisher {
            let _ = write!(e, "<dc:publisher>{}</dc:publisher>", escape(publisher));
        }
        if let Some(issued) = &book.meta.date {
            let _ = write!(e, "<dc:issued>{}</dc:issued>", escape(issued));
        }
        if let Some(identifier) = &book.meta.identifier {
            let _ = write!(e, "<dc:identifier>{}</dc:identifier>", escape(identifier));
        }
        for subject in &book.meta.subjects {
            let _ = write!(e, r#"<category term="{0}" label="{0}"/>"#, escape(subject));
        }
        if let Some(description) = &book.meta.description {
            let _ = write!(e, r#"<summary type="text">{}</summary>"#, escape(description));
        }
        if let Some(mime) = book.cover_mime() {
            let version = crate::covers::version(book);
            let _ = write!(
                e,
                r#"<link rel="http://opds-spec.org/image" href="{base}/cover/{}?v={version}" type="{}"/>"#,
                escape(&url),
                escape(mime)
            );
            let _ = write!(
                e,
                r#"<link rel="http://opds-spec.org/image/thumbnail" href="{base}/thumb/{}?v={version}" type="image/jpeg"/>"#,
                escape(&url)
            );
        }
        let _ = write!(
            e,
            r#"<link rel="http://opds-spec.org/acquisition" href="{base}/download/{}" type="{}" length="{}" title="{}"/>"#,
            escape(&url),
            escape(&book.mime),
            book.size,
            escape(&book.format),
        );
        e.push_str("</entry>");
        self.entries.push(e);
    }

    /// Adds a page of `books` with first/previous/next/last links.
    fn paginate(&mut self, books: &[Book], page: usize, page_size: usize, href: &str) {
        let pages = books.len().div_ceil(page_size).max(1);
        let page = page.clamp(1, pages);
        let sep = if href.contains('?') { '&' } else { '?' };
        let page_href = |p: usize| {
            if p == 1 {
                href.to_string()
            } else {
                format!("{href}{sep}page={p}")
            }
        };
        if pages > 1 {
            self.link("first", &page_href(1), self.kind);
            self.link("last", &page_href(pages), self.kind);
            if page > 1 {
                self.link("previous", &page_href(page - 1), self.kind);
            }
            if page < pages {
                self.link("next", &page_href(page + 1), self.kind);
            }
        }
        let start = (page - 1) * page_size;
        self.extra.push(format!(
            "<opensearch:totalResults>{}</opensearch:totalResults>",
            books.len()
        ));
        self.extra.push(format!(
            "<opensearch:itemsPerPage>{page_size}</opensearch:itemsPerPage>"
        ));
        self.extra
            .push(format!("<opensearch:startIndex>{}</opensearch:startIndex>", start + 1));
        for book in books.iter().skip(start).take(page_size) {
            self.book_entry(book);
        }
    }

    fn render(mut self, updated: DateTime<Utc>) -> Response {
        let self_href = self.path_and_query.clone();
        self.link("self", &self_href, self.kind);
        self.link("start", "/", NAV);
        self.link(
            "search",
            "/opds/opensearch.xml",
            "application/opensearchdescription+xml",
        );
        self.link("search", "/opds/search?q={searchTerms}", ACQ);

        let mut xml = String::from(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
        xml.push_str(r#"<feed xmlns="http://www.w3.org/2005/Atom" xmlns:dc="http://purl.org/dc/terms/" xmlns:opds="http://opds-spec.org/2010/catalog" xmlns:opensearch="http://a9.com/-/spec/opensearch/1.1/">"#);
        let _ = write!(
            xml,
            "<id>{}</id><title>{}</title><updated>{}</updated>",
            escape(&self.id),
            escape(&self.title),
            date(updated)
        );
        xml.push_str("<author><name>OPDShelf</name><uri>https://github.com/DanielPereod/opdshelf</uri></author>");
        let _ = write!(xml, r#"<icon>{}/static/icon.png</icon>"#, escape(&self.base));
        for part in self.links.iter().chain(&self.extra).chain(&self.entries) {
            xml.push_str(part);
        }
        xml.push_str("</feed>");
        let content_type = if self.kind == NAV { NAV } else { ACQ };
        ([(header::CONTENT_TYPE, format!("{content_type};charset=utf-8"))], xml).into_response()
    }
}

fn latest(books: &[Book]) -> DateTime<Utc> {
    books.iter().map(|b| b.modified).max().unwrap_or_else(Utc::now)
}

fn path_with_query(path: &str, sort: SortMode) -> String {
    if sort == SortMode::default() {
        path.to_string()
    } else {
        format!("{path}?sort={}", sort.as_str())
    }
}

pub async fn root(State(state): State<AppState>, headers: HeaderMap, Query(query): Query<FeedQuery>) -> Response {
    // 1.x served every book at `/`, optionally sorted with ?sort=
    if let Some(sort) = query.sort {
        return Redirect::temporary(&format!("/opds/books?sort={}", SortMode::parse(Some(&sort)).as_str()))
            .into_response();
    }

    let snapshot = state.library.scan().await;
    let base = base_url(&state, &headers);
    let updated = latest(&snapshot.books);
    let count = snapshot.books.len();
    let mut feed = Feed::new(&base, "root", "OPDShelf", NAV, "/".into());

    feed.nav_entry(
        "recent",
        "Recently added",
        &format!("{count} books, newest first"),
        "/opds/books",
        ACQ,
        updated,
    );
    feed.nav_entry(
        "title",
        "All books by title",
        &format!("{count} books, A-Z"),
        "/opds/books?sort=name-asc",
        ACQ,
        updated,
    );
    if !snapshot.folders.is_empty() {
        let folders = snapshot.subfolders("").len();
        feed.nav_entry(
            "folders",
            "Folders",
            &format!("{folders} folders"),
            "/opds/folders",
            ACQ,
            updated,
        );
    }
    let authors = snapshot.authors().len();
    if authors > 0 {
        feed.nav_entry(
            "authors",
            "Authors",
            &format!("{authors} authors"),
            "/opds/authors",
            NAV,
            updated,
        );
    }
    feed.render(updated)
}

pub async fn books(State(state): State<AppState>, headers: HeaderMap, Query(query): Query<FeedQuery>) -> Response {
    let mut books = state.library.scan().await.books;
    let sort = SortMode::parse(query.sort.as_deref());
    sort.sort(&mut books);
    let href = path_with_query("/opds/books", sort);
    let title = if sort == SortMode::NameAsc {
        "All books by title"
    } else {
        "All books"
    };
    let mut feed = Feed::new(
        &base_url(&state, &headers),
        &format!("books:{}", sort.as_str()),
        title,
        ACQ,
        href.clone(),
    );
    feed.link("up", "/", NAV);
    feed.paginate(&books, query.page.unwrap_or(1), state.config.page_size, &href);
    feed.render(latest(&books))
}

pub async fn folders_root(state: State<AppState>, headers: HeaderMap, query: Query<FeedQuery>) -> Response {
    folder_feed(state, headers, query, String::new()).await
}

pub async fn folder(
    state: State<AppState>,
    headers: HeaderMap,
    query: Query<FeedQuery>,
    Path(path): Path<String>,
) -> Response {
    folder_feed(state, headers, query, path).await
}

async fn folder_feed(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<FeedQuery>,
    path: String,
) -> Response {
    let Some((folder, _)) = state.library.folder_path(&path) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let snapshot = state.library.scan().await;
    let mut books = snapshot.in_folder(&folder);
    let sort = SortMode::parse(query.sort.as_deref().or(Some("name-asc")));
    sort.sort(&mut books);

    let base = base_url(&state, &headers);
    let href = if folder.is_empty() {
        "/opds/folders".to_string()
    } else {
        format!("/opds/folders/{}", paths::url_encode(&folder))
    };
    let title = if folder.is_empty() {
        "Folders".to_string()
    } else {
        paths::file_name(&folder).to_string()
    };
    let mut feed = Feed::new(
        &base,
        &format!("folder:{}", paths::url_encode(&folder)),
        &title,
        ACQ,
        href.clone(),
    );

    let up = match paths::parent(&folder) {
        _ if folder.is_empty() => "/".to_string(),
        "" => "/opds/folders".to_string(),
        parent => format!("/opds/folders/{}", paths::url_encode(parent)),
    };
    feed.link("up", &up, if folder.is_empty() { NAV } else { ACQ });

    let page = query.page.unwrap_or(1);
    if page <= 1 {
        for sub in snapshot.subfolders(&folder) {
            let count = snapshot.count_under(&sub);
            let sub_href = format!("/opds/folders/{}", paths::url_encode(&sub));
            let updated = latest(
                &snapshot
                    .books
                    .iter()
                    .filter(|b| b.path.starts_with(&format!("{sub}/")))
                    .cloned()
                    .collect::<Vec<_>>(),
            );
            feed.nav_entry(
                &format!("folder:{}", paths::url_encode(&sub)),
                &format!("📁 {}", paths::file_name(&sub)),
                &format!("{count} books"),
                &sub_href,
                ACQ,
                updated,
            );
        }
    }
    feed.paginate(&books, page, state.config.page_size, &href);
    feed.render(latest(&books))
}

pub async fn authors(State(state): State<AppState>, headers: HeaderMap, Query(query): Query<FeedQuery>) -> Response {
    let snapshot = state.library.scan().await;
    let authors = snapshot.authors();
    let mut feed = Feed::new(
        &base_url(&state, &headers),
        "authors",
        "Authors",
        NAV,
        "/opds/authors".into(),
    );
    feed.link("up", "/", NAV);

    let page_size = state.config.page_size;
    let pages = authors.len().div_ceil(page_size).max(1);
    let page = query.page.unwrap_or(1).clamp(1, pages);
    if page > 1 {
        feed.link("previous", &format!("/opds/authors?page={}", page - 1), NAV);
    }
    if page < pages {
        feed.link("next", &format!("/opds/authors?page={}", page + 1), NAV);
    }
    for (author, count) in authors.iter().skip((page - 1) * page_size).take(page_size) {
        let encoded = paths::url_encode(author);
        let updated = latest(&snapshot.by_author(author));
        feed.nav_entry(
            &format!("author:{encoded}"),
            author,
            &format!("{count} books"),
            &format!("/opds/authors/{encoded}"),
            ACQ,
            updated,
        );
    }
    feed.render(latest(&snapshot.books))
}

pub async fn author(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<FeedQuery>,
    Path(name): Path<String>,
) -> Response {
    let mut books = state.library.scan().await.by_author(&name);
    if books.is_empty() {
        return StatusCode::NOT_FOUND.into_response();
    }
    SortMode::NameAsc.sort(&mut books);
    let href = format!("/opds/authors/{}", paths::url_encode(&name));
    let mut feed = Feed::new(
        &base_url(&state, &headers),
        &format!("author:{}", paths::url_encode(&name)),
        &name,
        ACQ,
        href.clone(),
    );
    feed.link("up", "/opds/authors", NAV);
    feed.paginate(&books, query.page.unwrap_or(1), state.config.page_size, &href);
    feed.render(latest(&books))
}

pub async fn search(State(state): State<AppState>, headers: HeaderMap, Query(query): Query<FeedQuery>) -> Response {
    let terms = query.q.unwrap_or_default();
    let mut books = state.library.scan().await.search(&terms);
    SortMode::NameAsc.sort(&mut books);
    let href = format!("/opds/search?q={}", paths::url_encode(&terms));
    let mut feed = Feed::new(
        &base_url(&state, &headers),
        &format!("search:{}", paths::url_encode(&terms)),
        &format!("Search: {terms}"),
        ACQ,
        href.clone(),
    );
    feed.link("up", "/", NAV);
    feed.paginate(&books, query.page.unwrap_or(1), state.config.page_size, &href);
    feed.render(Utc::now())
}

pub async fn opensearch(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let base = escape(&base_url(&state, &headers));
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<OpenSearchDescription xmlns="http://a9.com/-/spec/opensearch/1.1/">
  <ShortName>OPDShelf</ShortName>
  <Description>Search the OPDShelf library</Description>
  <InputEncoding>UTF-8</InputEncoding>
  <OutputEncoding>UTF-8</OutputEncoding>
  <Url type="{ACQ}" template="{base}/opds/search?q={{searchTerms}}"/>
</OpenSearchDescription>"#
    );
    (
        [(
            header::CONTENT_TYPE,
            "application/opensearchdescription+xml;charset=utf-8",
        )],
        xml,
    )
        .into_response()
}
