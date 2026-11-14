//! OPDS 1.2 catalog: navigation root, paginated acquisition feeds, folders,
//! authors, series, search, reading-status/language/format facets, books in
//! progress, a random pick, and OPDS-PSE page streaming for comics.

use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;

use crate::i18n::{fill, Lang};
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
    /// Facets: `unread`, `reading` or `finished`; a language code; a format.
    status: Option<String>,
    lang: Option<String>,
    format: Option<String>,
}

/// Where a book is at, worked out as the web panel does: from its newest
/// position, unless it was marked as read or unread in the panel.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    Reading,
    Finished,
}

/// The same threshold as the web panel: the last page often starts before 100 %.
const FINISHED: f64 = 0.98;

/// The status of every started or finished book (by path), with when it was last read.
async fn statuses(state: &AppState) -> HashMap<String, (Status, i64)> {
    use crate::marks::State as Mark;
    let index = state.documents.index(&state.library).await;
    let marks = state.marks.all();
    let mut out = HashMap::new();
    for (document, record) in state.progress.all_current() {
        let Some(path) = index.get(&document) else { continue };
        let status = match marks.get(&document) {
            Some(m) if m.state == Mark::Read => (Status::Finished, m.at.max(record.timestamp)),
            Some(m) if m.state == Mark::Unread && record.timestamp <= m.at => continue,
            _ if record.percentage >= FINISHED => (Status::Finished, record.timestamp),
            _ if record.percentage > 0.0 => (Status::Reading, record.timestamp),
            _ => continue,
        };
        out.insert(path.clone(), status);
    }
    for (document, mark) in marks {
        if mark.state == Mark::Read {
            if let Some(path) = index.get(&document) {
                out.entry(path.clone()).or_insert((Status::Finished, mark.at));
            }
        }
    }
    out
}

fn language_of(book: &Book) -> Option<String> {
    let lang = book.meta.language.as_deref()?.trim().to_lowercase();
    (!lang.is_empty()).then_some(lang)
}

/// Appends query parameters, skipping empty ones.
fn with_params(href: &str, params: &[(&str, Option<&str>)]) -> String {
    let mut out = href.to_string();
    for (key, value) in params {
        if let Some(value) = value.filter(|v| !v.is_empty()) {
            out.push(if out.contains('?') { '&' } else { '?' });
            let _ = write!(out, "{key}={}", paths::url_encode(value));
        }
    }
    out
}

/// Adds the facet links of an acquisition feed (reading status, and language
/// and format when the books have more than one) and returns the books the
/// chosen facets select, with the feed's href carrying them.
async fn facets(
    state: &AppState,
    feed: &mut Feed,
    lang: Lang,
    books: Vec<Book>,
    query: &FeedQuery,
    href: &str,
) -> (Vec<Book>, String) {
    let status = query
        .status
        .as_deref()
        .filter(|s| matches!(*s, "unread" | "reading" | "finished"));
    let language = query.lang.as_deref().map(str::to_lowercase);
    let format = query.format.as_deref().map(str::to_lowercase);
    let chosen = [
        ("status", status),
        ("lang", language.as_deref()),
        ("format", format.as_deref()),
    ];
    let link = |key: &str, value: Option<&str>| {
        let params: Vec<_> = chosen
            .iter()
            .map(|&(k, v)| (k, if k == key { value } else { v }))
            .collect();
        with_params(href, &params)
    };

    let group = lang.text("Reading status");
    for (value, label) in [
        (None, "All"),
        (Some("unread"), "Not started"),
        (Some("reading"), "Reading"),
        (Some("finished"), "Finished"),
    ] {
        feed.facet(&link("status", value), &lang.text(label), &group, status == value, None);
    }

    let mut languages: BTreeMap<String, usize> = BTreeMap::new();
    let mut formats: BTreeMap<String, usize> = BTreeMap::new();
    for book in &books {
        if let Some(l) = language_of(book) {
            *languages.entry(l).or_default() += 1;
        }
        if !book.format.is_empty() {
            *formats.entry(book.format.to_lowercase()).or_default() += 1;
        }
    }
    for (key, title, values, active) in [
        ("lang", "Language", &languages, &language),
        ("format", "Format", &formats, &format),
    ] {
        if values.len() < 2 && active.is_none() {
            continue;
        }
        let group = lang.text(title);
        feed.facet(&link(key, None), &lang.text("All"), &group, active.is_none(), None);
        for (value, count) in values {
            let label = if key == "format" {
                value.to_uppercase()
            } else {
                value.clone()
            };
            feed.facet(
                &link(key, Some(value)),
                &label,
                &group,
                active.as_deref() == Some(value.as_str()),
                Some(*count),
            );
        }
    }

    let statuses = match status {
        Some(_) => statuses(state).await,
        None => HashMap::new(),
    };
    let books = books
        .into_iter()
        .filter(|b| match status {
            Some("unread") => !statuses.contains_key(&b.path),
            Some("reading") => statuses.get(&b.path).is_some_and(|s| s.0 == Status::Reading),
            Some("finished") => statuses.get(&b.path).is_some_and(|s| s.0 == Status::Finished),
            _ => true,
        })
        .filter(|b| language.is_none() || language_of(b) == language)
        .filter(|b| format.as_ref().is_none_or(|f| b.format.eq_ignore_ascii_case(f)))
        .collect();
    let href = with_params(href, &chosen);
    feed.path_and_query = href.clone();
    (books, href)
}

/// The 1-based page shown and the index of its first book.
fn page_range(len: usize, page: usize, page_size: usize) -> (usize, usize, usize) {
    let pages = len.div_ceil(page_size).max(1);
    let page = page.clamp(1, pages);
    (page, pages, (page - 1) * page_size)
}

/// Fills `feed.pages` with the page count of the comics shown on this page,
/// for their OPDS-PSE stream links.
async fn count_pages(state: &AppState, feed: &mut Feed, books: &[Book], page: usize) {
    let page_size = state.config.page_size;
    let (_, _, start) = page_range(books.len(), page, page_size);
    let comics: Vec<&Book> = books
        .iter()
        .skip(start)
        .take(page_size)
        .filter(|b| crate::reader::is_comic(b))
        .collect();
    let counts = futures_util::future::join_all(comics.iter().map(|b| crate::reader::page_list(state, b))).await;
    for (book, pages) in comics.into_iter().zip(counts) {
        if let Some(pages) = pages.filter(|p| !p.is_empty()) {
            feed.pages.insert(book.path.clone(), pages.len());
        }
    }
}

/// Facets, page counts and one page of `books`: the body of an acquisition feed.
async fn fill_feed(
    state: &AppState,
    feed: &mut Feed,
    lang: Lang,
    books: Vec<Book>,
    query: &FeedQuery,
    href: &str,
) -> Vec<Book> {
    let (books, href) = facets(state, feed, lang, books, query, href).await;
    let page = query.page.unwrap_or(1);
    count_pages(state, feed, &books, page).await;
    feed.paginate(&books, page, state.config.page_size, &href);
    books
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
    /// Page counts of the comics in the feed, for OPDS-PSE.
    pages: HashMap<String, usize>,
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
            pages: HashMap::new(),
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

    fn facet(&mut self, href: &str, title: &str, group: &str, active: bool, count: Option<usize>) {
        let mut link = format!(
            r#"<link rel="http://opds-spec.org/facet" href="{}{}" type="{}" title="{}" opds:facetGroup="{}""#,
            escape(&self.base),
            escape(href),
            self.kind,
            escape(title),
            escape(group),
        );
        if active {
            link.push_str(r#" opds:activeFacet="true""#);
        }
        if let Some(count) = count {
            let _ = write!(link, r#" thr:count="{count}""#);
        }
        link.push_str("/>");
        self.links.push(link);
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
        if let Some(series) = &book.meta.series {
            let _ = write!(e, "<calibre:series>{}</calibre:series>", escape(series));
            if let Some(index) = &book.meta.series_index {
                let _ = write!(e, "<calibre:series_index>{}</calibre:series_index>", escape(index));
            }
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
        if let Some(count) = self.pages.get(&book.path) {
            // OPDS-PSE: page {pageNumber} counts from 0, as /page does
            let _ = write!(
                e,
                r#"<link rel="http://vaemendis.net/opds-pse/stream" href="{base}/page/{}?n={{pageNumber}}" type="image/jpeg" pse:count="{count}"/>"#,
                escape(&url),
            );
        }
        e.push_str("</entry>");
        self.entries.push(e);
    }

    /// Adds a page of `books` with first/previous/next/last links.
    fn paginate(&mut self, books: &[Book], page: usize, page_size: usize, href: &str) {
        let (page, pages, start) = page_range(books.len(), page, page_size);
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
        xml.push_str(r#"<feed xmlns="http://www.w3.org/2005/Atom" xmlns:dc="http://purl.org/dc/terms/" xmlns:opds="http://opds-spec.org/2010/catalog" xmlns:opensearch="http://a9.com/-/spec/opensearch/1.1/" xmlns:calibre="http://calibre.kovidgoyal.net/2009/metadata" xmlns:thr="http://purl.org/syndication/thread/1.0" xmlns:pse="http://vaemendis.net/opds-pse/ns">"#);
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

    let lang = Lang::from_headers(&headers);
    let count = |template: &str, n: usize| fill(&lang.text(template), &[("n".into(), n.to_string())]);
    let snapshot = state.library.scan().await;
    let base = base_url(&state, &headers);
    let updated = latest(&snapshot.books);
    let books = snapshot.books.len();
    let mut feed = Feed::new(&base, "root", "OPDShelf", NAV, "/".into());

    // Listed without counting, so the root does not wait for every book's id
    feed.nav_entry(
        "reading",
        &lang.text("Continue reading"),
        &lang.text("Books you have started, last read first"),
        "/opds/reading",
        ACQ,
        Utc::now(),
    );
    feed.nav_entry(
        "recent",
        &lang.text("Recently added"),
        &count("{n} books, newest first", books),
        "/opds/books",
        ACQ,
        updated,
    );
    feed.nav_entry(
        "title",
        &lang.text("All books by title"),
        &count("{n} books, A-Z", books),
        "/opds/books?sort=name-asc",
        ACQ,
        updated,
    );
    if !snapshot.folders.is_empty() {
        let folders = snapshot.subfolders("").len();
        feed.nav_entry(
            "folders",
            &lang.text("Folders"),
            &count("{n} folders", folders),
            "/opds/folders",
            ACQ,
            updated,
        );
    }
    let authors = snapshot.authors().len();
    if authors > 0 {
        feed.nav_entry(
            "authors",
            &lang.text("Authors"),
            &count("{n} authors", authors),
            "/opds/authors",
            NAV,
            updated,
        );
    }
    let series = snapshot.series().len();
    if series > 0 {
        feed.nav_entry(
            "series",
            "Series",
            &count("{n} series", series),
            "/opds/series",
            NAV,
            updated,
        );
    }
    if books > 0 {
        feed.nav_entry(
            "random",
            &lang.text("Random books"),
            &lang.text("A different selection each time"),
            "/opds/random",
            ACQ,
            Utc::now(),
        );
    }
    feed.render(updated)
}

pub async fn books(State(state): State<AppState>, headers: HeaderMap, Query(query): Query<FeedQuery>) -> Response {
    let lang = Lang::from_headers(&headers);
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
        &lang.text(title),
        ACQ,
        href.clone(),
    );
    feed.link("up", "/", NAV);
    let books = fill_feed(&state, &mut feed, lang, books, &query, &href).await;
    feed.render(latest(&books))
}

/// Started books that are not finished, last read first.
pub async fn reading(State(state): State<AppState>, headers: HeaderMap, Query(query): Query<FeedQuery>) -> Response {
    let lang = Lang::from_headers(&headers);
    let statuses = statuses(&state).await;
    let mut books: Vec<(i64, Book)> = state
        .library
        .scan()
        .await
        .books
        .into_iter()
        .filter_map(|b| match statuses.get(&b.path) {
            Some(&(Status::Reading, at)) => Some((at, b)),
            _ => None,
        })
        .collect();
    books.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| crate::library::by_title(&a.1, &b.1)));
    let books: Vec<Book> = books.into_iter().map(|(_, b)| b).collect();
    let href = "/opds/reading";
    let mut feed = Feed::new(
        &base_url(&state, &headers),
        "reading",
        &lang.text("Continue reading"),
        ACQ,
        href.into(),
    );
    feed.link("up", "/", NAV);
    let page = query.page.unwrap_or(1);
    count_pages(&state, &mut feed, &books, page).await;
    feed.paginate(&books, page, state.config.page_size, href);
    feed.render(Utc::now())
}

/// A page of books picked at random, different on every visit; the facets
/// narrow the pick (to books not started yet, say).
pub async fn random(State(state): State<AppState>, headers: HeaderMap, Query(query): Query<FeedQuery>) -> Response {
    use rand::seq::SliceRandom;
    let lang = Lang::from_headers(&headers);
    let books = state.library.scan().await.books;
    let href = "/opds/random";
    let mut feed = Feed::new(
        &base_url(&state, &headers),
        "random",
        &lang.text("Random books"),
        ACQ,
        href.into(),
    );
    feed.link("up", "/", NAV);
    let (mut books, href) = facets(&state, &mut feed, lang, books, &query, href).await;
    books.shuffle(&mut rand::thread_rng());
    books.truncate(state.config.page_size);
    count_pages(&state, &mut feed, &books, 1).await;
    feed.paginate(&books, 1, state.config.page_size, &href);
    feed.render(Utc::now())
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
    let lang = Lang::from_headers(&headers);
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
        lang.text("Folders")
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
                &fill(&lang.text("{n} books"), &[("n".into(), count.to_string())]),
                &sub_href,
                ACQ,
                updated,
            );
        }
    }
    let books = fill_feed(&state, &mut feed, lang, books, &query, &href).await;
    feed.render(latest(&books))
}

/// What a list of names (authors, series) is about.
struct NameList {
    /// Feed id, title and path under /opds.
    id: &'static str,
    title: &'static str,
    /// Prefix of each entry's id.
    entry: &'static str,
}

impl NameList {
    const AUTHORS: Self = Self {
        id: "authors",
        title: "Authors",
        entry: "author",
    };
    const SERIES: Self = Self {
        id: "series",
        title: "Series",
        entry: "series",
    };
}

/// A paginated navigation feed of names, each linking to its books.
fn name_index(
    state: &AppState,
    headers: &HeaderMap,
    query: &FeedQuery,
    list: NameList,
    names: &[(String, usize)],
    books_of: impl Fn(&str) -> Vec<Book>,
    updated: DateTime<Utc>,
) -> Response {
    let lang = Lang::from_headers(headers);
    let href = format!("/opds/{}", list.id);
    // "Series" is the same word in Spanish
    let title = if list.id == "series" {
        list.title.to_string()
    } else {
        lang.text(list.title)
    };
    let mut feed = Feed::new(&base_url(state, headers), list.id, &title, NAV, href.clone());
    feed.link("up", "/", NAV);

    let page_size = state.config.page_size;
    let pages = names.len().div_ceil(page_size).max(1);
    let page = query.page.unwrap_or(1).clamp(1, pages);
    if page > 1 {
        feed.link("previous", &format!("{href}?page={}", page - 1), NAV);
    }
    if page < pages {
        feed.link("next", &format!("{href}?page={}", page + 1), NAV);
    }
    for (name, count) in names.iter().skip((page - 1) * page_size).take(page_size) {
        let encoded = paths::url_encode(name);
        feed.nav_entry(
            &format!("{}:{encoded}", list.entry),
            name,
            &fill(&lang.text("{n} books"), &[("n".into(), count.to_string())]),
            &format!("{href}/{encoded}"),
            ACQ,
            latest(&books_of(name)),
        );
    }
    feed.render(updated)
}

pub async fn authors(State(state): State<AppState>, headers: HeaderMap, Query(query): Query<FeedQuery>) -> Response {
    let snapshot = state.library.scan().await;
    let updated = latest(&snapshot.books);
    name_index(
        &state,
        &headers,
        &query,
        NameList::AUTHORS,
        &snapshot.authors(),
        |a| snapshot.by_author(a),
        updated,
    )
}

pub async fn author(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<FeedQuery>,
    Path(name): Path<String>,
) -> Response {
    let books = state.library.scan().await.by_author(&name);
    books_of(&state, &headers, &query, books, "author", &name, "/opds/authors").await
}

pub async fn series_index(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<FeedQuery>,
) -> Response {
    let snapshot = state.library.scan().await;
    let updated = latest(&snapshot.books);
    name_index(
        &state,
        &headers,
        &query,
        NameList::SERIES,
        &snapshot.series(),
        |s| snapshot.in_series(s),
        updated,
    )
}

pub async fn series(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<FeedQuery>,
    Path(name): Path<String>,
) -> Response {
    let books = state.library.scan().await.in_series(&name);
    books_of(&state, &headers, &query, books, "series", &name, "/opds/series").await
}

/// The books of one author or series, by title (so a series reads in volume order).
async fn books_of(
    state: &AppState,
    headers: &HeaderMap,
    query: &FeedQuery,
    mut books: Vec<Book>,
    kind: &str,
    name: &str,
    up: &str,
) -> Response {
    if books.is_empty() {
        return StatusCode::NOT_FOUND.into_response();
    }
    SortMode::NameAsc.sort(&mut books);
    let encoded = paths::url_encode(name);
    let href = format!("{up}/{encoded}");
    let mut feed = Feed::new(
        &base_url(state, headers),
        &format!("{kind}:{encoded}"),
        name,
        ACQ,
        href.clone(),
    );
    feed.link("up", up, NAV);
    let books = fill_feed(state, &mut feed, Lang::from_headers(headers), books, query, &href).await;
    feed.render(latest(&books))
}

pub async fn search(State(state): State<AppState>, headers: HeaderMap, Query(query): Query<FeedQuery>) -> Response {
    let lang = Lang::from_headers(&headers);
    let terms = query.q.clone().unwrap_or_default();
    let mut books = state.library.scan().await.search(&terms);
    SortMode::NameAsc.sort(&mut books);
    let href = format!("/opds/search?q={}", paths::url_encode(&terms));
    let mut feed = Feed::new(
        &base_url(&state, &headers),
        &format!("search:{}", paths::url_encode(&terms)),
        &fill(&lang.text("Search: {terms}"), &[("terms".into(), terms.clone())]),
        ACQ,
        href.clone(),
    );
    feed.link("up", "/", NAV);
    fill_feed(&state, &mut feed, lang, books, &query, &href).await;
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
