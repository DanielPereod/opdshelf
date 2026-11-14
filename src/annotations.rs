//! Highlights and notes, shared between the web reader, the Android app and KOReader.
//!
//! Each highlight has an id made by the device that created it, so every
//! device can change or delete the same one. Changes are merged one highlight
//! at a time: the newest change wins, by when it was made on its device (with
//! the device's clock corrected, as for positions). A deleted highlight stays
//! as a tombstone for a while so the deletion reaches devices that were
//! offline. Highlights are kept in DATA_DIR/annotations.json, by document id.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

use crate::progress::Context;

/// Highlights kept per book, tombstones included.
const MAX_PER_BOOK: usize = 5000;
/// Seconds a deleted highlight is remembered, so devices offline meanwhile delete it too.
const TOMBSTONE_SECONDS: i64 = 400 * 86400;
const MAX_TEXT: usize = 8000;
const MAX_NOTE: usize = 20000;
const MAX_FIELD: usize = 2048;

/// Colours every client can show.
pub const COLORS: &[&str] = &[
    "yellow", "orange", "red", "purple", "blue", "cyan", "green", "olive", "gray",
];
/// How a highlight is drawn: KOReader's drawers.
pub const STYLES: &[&str] = &["lighten", "underscore", "strikeout", "invert"];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Annotation {
    /// Made by the device that created it: letters, digits, `-` and `_`.
    pub id: String,
    /// The highlighted text.
    #[serde(default)]
    pub text: String,
    /// The reader's note, if any.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    #[serde(default = "default_color")]
    pub color: String,
    #[serde(default = "default_style")]
    pub style: String,
    /// Start and end: KOReader xpointers for reflowable books, the page number (from 1) for PDFs and comics.
    #[serde(default)]
    pub pos0: String,
    #[serde(default)]
    pub pos1: String,
    /// EPUB CFI of the range, so the web reader finds it exactly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cfi: Option<String>,
    /// Text before and after the highlight, to find it again when the pointers do not match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<Context>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub chapter: String,
    /// The page, for books with pages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    /// Where in the book, from 0 to 1, to sort highlights.
    #[serde(default)]
    pub percentage: f64,
    /// When it was made and last changed on its device (Unix seconds).
    #[serde(default)]
    pub created: i64,
    #[serde(default)]
    pub updated: i64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub deleted: bool,
    /// The device that changed it last.
    #[serde(default)]
    pub device: String,
    #[serde(default)]
    pub device_id: String,
}

fn default_color() -> String {
    "yellow".into()
}

fn default_style() -> String {
    "lighten".into()
}

/// Highlights sent by a device.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Batch {
    #[serde(default)]
    pub annotations: Vec<Annotation>,
    /// The sending device, used for highlights that do not name one.
    #[serde(default)]
    pub device: String,
    #[serde(default)]
    pub device_id: String,
    /// The device's clock when sending (Unix seconds), to correct a wrong clock.
    #[serde(default)]
    pub sent_at: Option<i64>,
}

/// Highlights of a book that changed, sent to devices listening for them.
#[derive(Clone, Debug, Serialize)]
pub struct Change {
    pub document: String,
    pub annotations: Vec<Annotation>,
}

#[derive(Clone)]
pub struct Annotations {
    file: PathBuf,
    books: Arc<Mutex<HashMap<String, Vec<Annotation>>>>,
    changes: broadcast::Sender<Change>,
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn clip(mut value: String, max: usize) -> String {
    if value.len() > max {
        let mut end = max;
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        value.truncate(end);
    }
    value
}

impl Annotation {
    /// Cleans up what a device sent, with its times corrected by `skew` and never in the future.
    fn tidy(mut self, batch: &Batch, skew: i64, now: i64) -> Option<Self> {
        if !valid_id(&self.id) {
            return None;
        }
        let fix = |t: i64| if t <= 0 { now } else { t.saturating_add(skew).min(now) };
        self.updated = fix(self.updated);
        self.created = if self.created <= 0 {
            self.updated
        } else {
            fix(self.created).min(self.updated)
        };
        self.text = clip(self.text, MAX_TEXT);
        self.note = clip(self.note, MAX_NOTE);
        self.chapter = clip(self.chapter, MAX_FIELD);
        self.pos0 = clip(self.pos0, MAX_FIELD);
        self.pos1 = clip(self.pos1, MAX_FIELD);
        self.cfi = self.cfi.map(|c| clip(c, MAX_FIELD)).filter(|c| !c.is_empty());
        self.context = self
            .context
            .map(|c| Context {
                before: clip(c.before, MAX_FIELD),
                after: clip(c.after, MAX_FIELD),
                selected: String::new(),
            })
            .filter(|c| !c.before.is_empty() || !c.after.is_empty());
        self.color = self.color.to_ascii_lowercase();
        if !COLORS.contains(&self.color.as_str()) {
            self.color = default_color();
        }
        if !STYLES.contains(&self.style.as_str()) {
            self.style = default_style();
        }
        self.percentage = if self.percentage.is_finite() {
            self.percentage.clamp(0.0, 1.0)
        } else {
            0.0
        };
        if self.device_id.trim().is_empty() {
            self.device_id = batch.device_id.clone();
            self.device = batch.device.clone();
        }
        self.device = clip(self.device, 120);
        self.device_id = clip(self.device_id, 120);
        if self.deleted {
            // Only what is needed to delete it elsewhere
            self.text.clear();
            self.note.clear();
            self.context = None;
        }
        Some(self)
    }
}

impl Annotations {
    pub fn open(dir: &Path) -> Self {
        let file = dir.join("annotations.json");
        let books = match std::fs::read(&file) {
            Ok(json) => serde_json::from_slice(&json).unwrap_or_else(|err| {
                tracing::error!("ignoring unreadable {}: {err}", file.display());
                HashMap::new()
            }),
            Err(_) => HashMap::new(),
        };
        Self {
            file,
            books: Arc::new(Mutex::new(books)),
            changes: broadcast::channel(256).0,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Change> {
        self.changes.subscribe()
    }

    /// Every highlight of a book in reading order, deleted ones included.
    pub fn list(&self, document: &str) -> Vec<Annotation> {
        self.books.lock().unwrap().get(document).cloned().unwrap_or_default()
    }

    /// Highlights of every book: (document, highlights not deleted).
    pub fn all(&self) -> Vec<(String, Vec<Annotation>)> {
        self.books
            .lock()
            .unwrap()
            .iter()
            .map(|(doc, list)| {
                (
                    doc.clone(),
                    list.iter().filter(|a| !a.deleted).cloned().collect::<Vec<_>>(),
                )
            })
            .filter(|(_, list)| !list.is_empty())
            .collect()
    }

    /// Merges what a device sent. Returns the highlights that changed.
    pub fn merge(&self, document: &str, batch: Batch, now: i64) -> std::io::Result<Vec<Annotation>> {
        let skew = batch.sent_at.map_or(0, |sent| now - sent);
        let incoming: Vec<Annotation> = batch
            .annotations
            .clone()
            .into_iter()
            .filter_map(|a| a.tidy(&batch, skew, now))
            .collect();
        let mut changed = Vec::new();
        {
            let mut books = self.books.lock().unwrap();
            let list = books.entry(document.to_string()).or_default();
            for annotation in incoming {
                match list.iter_mut().find(|a| a.id == annotation.id) {
                    Some(kept) => {
                        // The newest change wins. Of two made in the same second, a
                        // deletion, or else the one sent last by the same device
                        let same_time = annotation.updated == kept.updated;
                        let newer = annotation.updated > kept.updated
                            || (same_time && annotation.deleted && !kept.deleted)
                            || (same_time && annotation.device_id == kept.device_id && !kept.deleted);
                        if newer && *kept != annotation {
                            let created = kept.created.min(annotation.created);
                            *kept = Annotation { created, ..annotation };
                            changed.push(kept.clone());
                        }
                    }
                    None => {
                        list.push(annotation.clone());
                        changed.push(annotation);
                    }
                }
            }
            tidy_list(list, now);
            books.retain(|_, l| !l.is_empty());
        }
        if !changed.is_empty() {
            self.save()?;
            let _ = self.changes.send(Change {
                document: document.to_string(),
                annotations: changed.clone(),
            });
        }
        Ok(changed)
    }

    /// Drops the highlights of documents `keep` rejects (books no longer in the
    /// library), or only counts them with `dry_run`. Returns how many.
    pub fn retain_documents(&self, keep: impl Fn(&str) -> bool, dry_run: bool) -> std::io::Result<usize> {
        let removed = {
            let mut all = self.books.lock().unwrap();
            let gone: Vec<String> = all.keys().filter(|d| !keep(d)).cloned().collect();
            if !dry_run {
                for document in &gone {
                    all.remove(document);
                }
            }
            gone.len()
        };
        if removed > 0 && !dry_run {
            self.save()?;
        }
        Ok(removed)
    }

    fn save(&self) -> std::io::Result<()> {
        let json = serde_json::to_vec(&*self.books.lock().unwrap()).map_err(std::io::Error::other)?;
        if let Some(dir) = self.file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.file.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(tmp, &self.file)
    }
}

/// Reading order, old tombstones dropped and at most [`MAX_PER_BOOK`] kept.
fn tidy_list(list: &mut Vec<Annotation>, now: i64) {
    list.retain(|a| !a.deleted || now - a.updated < TOMBSTONE_SECONDS);
    list.sort_by(|a, b| {
        a.percentage
            .partial_cmp(&b.percentage)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.created.cmp(&b.created))
            .then(a.id.cmp(&b.id))
    });
    if list.len() > MAX_PER_BOOK {
        // Drop tombstones first, then the oldest highlights
        let mut excess = list.len() - MAX_PER_BOOK;
        list.retain(|a| {
            if excess > 0 && a.deleted {
                excess -= 1;
                false
            } else {
                true
            }
        });
        if list.len() > MAX_PER_BOOK {
            let mut by_age: Vec<i64> = list.iter().map(|a| a.updated).collect();
            by_age.sort_unstable();
            let cut = by_age[list.len() - MAX_PER_BOOK - 1];
            let mut excess = list.len() - MAX_PER_BOOK;
            list.retain(|a| {
                if excess > 0 && a.updated <= cut {
                    excess -= 1;
                    false
                } else {
                    true
                }
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Export

/// What an export says about the book.
pub struct BookInfo {
    pub title: String,
    pub authors: String,
}

const COLOR_NAMES_ES: &[(&str, &str)] = &[
    ("yellow", "amarillo"),
    ("orange", "naranja"),
    ("red", "rojo"),
    ("purple", "morado"),
    ("blue", "azul"),
    ("cyan", "cian"),
    ("green", "verde"),
    ("olive", "oliva"),
    ("gray", "gris"),
];

fn quote_lines(text: &str) -> String {
    text.lines()
        .map(|l| {
            if l.trim().is_empty() {
                ">".to_string()
            } else {
                format!("> {}", l.trim_end())
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn date(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

/// The highlights of a book as Markdown, by chapter, in reading order.
pub fn markdown(book: &BookInfo, list: &[Annotation], spanish: bool) -> String {
    let mut out = format!("# {}\n", book.title.trim());
    if !book.authors.trim().is_empty() {
        out.push_str(&format!("\n*{}*\n", book.authors.trim()));
    }
    let live: Vec<&Annotation> = list.iter().filter(|a| !a.deleted).collect();
    let notes = live.iter().filter(|a| !a.note.trim().is_empty()).count();
    out.push_str(&if spanish {
        format!("\n{} subrayados, {} con nota.\n", live.len(), notes)
    } else {
        format!("\n{} highlights, {} with a note.\n", live.len(), notes)
    });
    let mut chapter: Option<&str> = None;
    for a in live {
        if chapter != Some(a.chapter.as_str()) {
            chapter = Some(a.chapter.as_str());
            let title = if a.chapter.trim().is_empty() {
                if spanish {
                    "Sin capítulo"
                } else {
                    "No chapter"
                }
            } else {
                a.chapter.trim()
            };
            out.push_str(&format!("\n## {title}\n"));
        }
        out.push('\n');
        if !a.text.trim().is_empty() {
            out.push_str(&quote_lines(a.text.trim()));
            out.push('\n');
        }
        if !a.note.trim().is_empty() {
            out.push('\n');
            out.push_str(a.note.trim());
            out.push('\n');
        }
        let color = if spanish {
            COLOR_NAMES_ES
                .iter()
                .find(|(en, _)| *en == a.color)
                .map_or(a.color.as_str(), |(_, es)| es)
        } else {
            a.color.as_str()
        };
        let mut meta = vec![format!("{}%", (a.percentage * 100.0).round() as i64)];
        if let Some(page) = a.page {
            meta.insert(
                0,
                if spanish {
                    format!("pág. {page}")
                } else {
                    format!("p. {page}")
                },
            );
        }
        meta.push(color.to_string());
        meta.push(date(a.created));
        out.push_str(&format!("\n<sub>{}</sub>\n", meta.join(" · ")));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(id: &str, updated: i64) -> Annotation {
        Annotation {
            id: id.into(),
            text: "It was a dark night".into(),
            note: String::new(),
            color: "yellow".into(),
            style: "lighten".into(),
            pos0: "/body/DocFragment[2]/body/p[1]/text()[1].0".into(),
            pos1: "/body/DocFragment[2]/body/p[1]/text()[1].19".into(),
            cfi: None,
            context: None,
            chapter: "One".into(),
            page: None,
            percentage: 0.1,
            created: updated,
            updated,
            deleted: false,
            device: "Kobo".into(),
            device_id: "kobo".into(),
        }
    }

    fn batch(list: Vec<Annotation>) -> Batch {
        Batch {
            annotations: list,
            ..Default::default()
        }
    }

    #[test]
    fn the_newest_change_wins() {
        let dir = tempfile::tempdir().unwrap();
        let a = Annotations::open(dir.path());
        a.merge("doc", batch(vec![note("h1", 100)]), 1000).unwrap();
        let mut edited = note("h1", 200);
        edited.note = "Gloomy".into();
        edited.color = "BLUE".into();
        assert_eq!(a.merge("doc", batch(vec![edited]), 1000).unwrap().len(), 1);
        // An older change arriving late does not undo it
        let mut stale = note("h1", 150);
        stale.color = "red".into();
        assert!(a.merge("doc", batch(vec![stale]), 1000).unwrap().is_empty());
        let list = a.list("doc");
        assert_eq!((list[0].note.as_str(), list[0].color.as_str()), ("Gloomy", "blue"));
        assert_eq!(list[0].created, 100);
    }

    #[test]
    fn deletions_are_kept_as_tombstones() {
        let dir = tempfile::tempdir().unwrap();
        let a = Annotations::open(dir.path());
        a.merge("doc", batch(vec![note("h1", 100), note("h2", 100)]), 1000)
            .unwrap();
        let mut gone = note("h1", 200);
        gone.deleted = true;
        a.merge("doc", batch(vec![gone]), 1000).unwrap();
        let list = a.list("doc");
        let tomb = list.iter().find(|x| x.id == "h1").unwrap();
        assert!(tomb.deleted && tomb.text.is_empty());
        assert_eq!(a.all()[0].1.len(), 1);
        // Forgotten after a while
        a.merge(
            "doc",
            batch(vec![note("h3", 1000 + TOMBSTONE_SECONDS)]),
            1000 + TOMBSTONE_SECONDS,
        )
        .unwrap();
        assert!(a.list("doc").iter().all(|x| x.id != "h1"));
    }

    #[test]
    fn wrong_clocks_and_bad_values_are_fixed() {
        let dir = tempfile::tempdir().unwrap();
        let a = Annotations::open(dir.path());
        let mut late = note("h1", 20000 - 3600 - 5);
        late.created = 0;
        late.color = "chartreuse".into();
        late.device_id = String::new();
        let changed = a
            .merge(
                "doc",
                Batch {
                    annotations: vec![late, note("bad id!", 10)],
                    device: "Web".into(),
                    device_id: "web".into(),
                    sent_at: Some(20000 - 3600),
                },
                20000,
            )
            .unwrap();
        assert_eq!(changed.len(), 1);
        assert_eq!((changed[0].updated, changed[0].created), (19995, 19995));
        assert_eq!(
            (changed[0].color.as_str(), changed[0].device_id.as_str()),
            ("yellow", "web")
        );
    }

    #[test]
    fn changes_are_announced_and_persist() {
        let dir = tempfile::tempdir().unwrap();
        let a = Annotations::open(dir.path());
        let mut rx = a.subscribe();
        a.merge("doc", batch(vec![note("h1", 100)]), 1000).unwrap();
        assert_eq!(rx.try_recv().unwrap().annotations[0].id, "h1");
        // Sending the same again changes nothing
        a.merge("doc", batch(vec![note("h1", 100)]), 1000).unwrap();
        assert!(rx.try_recv().is_err());
        assert_eq!(Annotations::open(dir.path()).list("doc").len(), 1);
    }

    #[test]
    fn markdown_groups_by_chapter() {
        let mut second = note("h2", 300);
        second.chapter = "Two".into();
        second.percentage = 0.5;
        second.note = "Remember this".into();
        let md = markdown(
            &BookInfo {
                title: "Book".into(),
                authors: "Ann".into(),
            },
            &[note("h1", 100), second],
            false,
        );
        assert!(md.starts_with("# Book\n\n*Ann*\n\n2 highlights, 1 with a note.\n\n## One\n\n> It was a dark night\n"));
        assert!(md.contains("## Two\n\n> It was a dark night\n\nRemember this\n\n<sub>50% · yellow · 1970-01-01</sub>"));
    }
}
