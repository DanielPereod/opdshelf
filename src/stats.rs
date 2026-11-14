//! Reading statistics, worked out from the reading positions that devices send.
//!
//! Every client sends its position a few seconds after a page turn, so the
//! time between two positions of the same device in the same book is time spent
//! reading, unless it is longer than [`GAP`]: then the reader stopped and a new
//! session starts. Time is added to the day it was read on (server time zone)
//! and kept in DATA_DIR/stats.json: a small summary per day, not every position.

use axum::extract::State;
use axum::response::{IntoResponse, Json, Response};
use chrono::{Datelike, Local, NaiveDate, TimeZone};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::web::AppState;

/// A longer pause between two positions ends a reading session.
pub const GAP: i64 = 10 * 60;
/// Read this far, a book counts as finished.
const FINISHED: f64 = 0.99;
/// A day with at least this much reading keeps a streak going.
const STREAK_SECONDS: i64 = 60;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Day {
    pub seconds: i64,
    pub sessions: u32,
    /// Seconds read in each book (by document id).
    #[serde(default)]
    pub books: BTreeMap<String, i64>,
}

/// The last position seen from a device in a book.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Last {
    at: i64,
    progress: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Data {
    /// By local date, "2026-11-14".
    #[serde(default)]
    days: BTreeMap<String, Day>,
    /// When each book was first finished (by document id).
    #[serde(default)]
    finished: BTreeMap<String, i64>,
    /// By "device id/document id".
    #[serde(default)]
    last: HashMap<String, Last>,
}

#[derive(Clone)]
pub struct Stats {
    file: PathBuf,
    data: Arc<Mutex<Data>>,
}

/// The local date of a time, as used for the days.
pub fn day_of(at: i64) -> String {
    Local
        .timestamp_opt(at, 0)
        .single()
        .map_or_else(String::new, |t| t.format("%Y-%m-%d").to_string())
}

impl Stats {
    /// Opens DATA_DIR/stats.json. When there is none yet, `seed` gives the
    /// positions already saved (device id, document, progress, percentage, time)
    /// so the first statistics are not empty.
    pub fn open(dir: &Path, seed: impl FnOnce() -> Vec<(String, String, String, f64, i64)>) -> Self {
        let file = dir.join("stats.json");
        let (data, seeded) = match std::fs::read(&file) {
            Ok(json) => (
                serde_json::from_slice(&json).unwrap_or_else(|err| {
                    tracing::error!("ignoring unreadable {}: {err}", file.display());
                    Data::default()
                }),
                false,
            ),
            Err(_) => (Data::default(), true),
        };
        let stats = Self {
            file,
            data: Arc::new(Mutex::new(data)),
        };
        if seeded {
            let mut positions = seed();
            if !positions.is_empty() {
                positions.sort_by_key(|p| p.4);
                {
                    let mut data = stats.data.lock().unwrap();
                    for (device_id, document, progress, percentage, at) in positions {
                        data.add(&device_id, &document, &progress, percentage, at);
                    }
                }
                if let Err(err) = stats.save() {
                    tracing::error!("cannot save reading statistics: {err}");
                }
            }
        }
        stats
    }

    /// Counts a position a device reached at `at`.
    pub fn record(&self, device_id: &str, document: &str, progress: &str, percentage: f64, at: i64) {
        let changed = self
            .data
            .lock()
            .unwrap()
            .add(device_id, document, progress, percentage, at);
        if changed {
            if let Err(err) = self.save() {
                tracing::error!("cannot save reading statistics: {err}");
            }
        }
    }

    /// Seconds read on a local date.
    pub fn day(&self, date: &str) -> Day {
        self.data.lock().unwrap().days.get(date).cloned().unwrap_or_default()
    }

    pub fn days(&self) -> BTreeMap<String, Day> {
        self.data.lock().unwrap().days.clone()
    }

    pub fn finished(&self) -> BTreeMap<String, i64> {
        self.data.lock().unwrap().finished.clone()
    }

    fn save(&self) -> std::io::Result<()> {
        let json = serde_json::to_vec(&*self.data.lock().unwrap()).map_err(std::io::Error::other)?;
        if let Some(dir) = self.file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.file.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(tmp, &self.file)
    }
}

impl Data {
    /// Returns whether anything changed.
    fn add(&mut self, device_id: &str, document: &str, progress: &str, percentage: f64, at: i64) -> bool {
        let mut changed = false;
        if percentage >= FINISHED && !self.finished.contains_key(document) {
            self.finished.insert(document.to_string(), at);
            changed = true;
        }
        let key = format!("{device_id}/{document}");
        let last = self.last.get(&key);
        // The same position sent again (on closing or sleeping) is not reading,
        // and one older than the last is from a device that was offline
        if last.is_some_and(|l| l.progress == progress || at < l.at) {
            return changed;
        }
        let elapsed = last.map(|l| at - l.at);
        let day = self.days.entry(day_of(at)).or_default();
        match elapsed {
            Some(seconds) if seconds <= GAP => {
                day.seconds += seconds;
                *day.books.entry(document.to_string()).or_default() += seconds;
            }
            _ => {
                day.sessions += 1;
                day.books.entry(document.to_string()).or_default();
            }
        }
        self.last.insert(
            key,
            Last {
                at,
                progress: progress.to_string(),
            },
        );
        true
    }
}

/// Days in a row with some reading, up to `today` (a streak is still alive
/// while today has nothing yet), and the longest ever.
pub fn streaks(days: &BTreeMap<String, Day>, today: NaiveDate) -> (u32, u32) {
    let read: Vec<NaiveDate> = days
        .iter()
        .filter(|(_, d)| d.seconds >= STREAK_SECONDS)
        .filter_map(|(date, _)| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok())
        .collect();
    let mut longest = 0;
    let mut run = 0;
    let mut previous: Option<NaiveDate> = None;
    for date in &read {
        run = match previous {
            Some(p) if p.succ_opt() == Some(*date) => run + 1,
            _ => 1,
        };
        longest = longest.max(run);
        previous = Some(*date);
    }
    let current = match previous {
        Some(last) if last == today || last.succ_opt() == Some(today) => run,
        _ => 0,
    };
    (current, longest)
}

// ---------------------------------------------------------------------------
// HTTP

/// Everything the panel's statistics page shows.
pub async fn summary(State(state): State<AppState>) -> Response {
    let stats = state.progress.stats();
    let today = Local::now().date_naive();
    let days = stats.days();
    let (current, longest) = streaks(&days, today);
    let books = crate::sync::books_by_document(&state).await;
    let title = |document: &str| books.get(document).map(|b| b.1.clone()).filter(|t| !t.is_empty());

    // The last 30 days, with the ones without reading
    let recent: Vec<_> = (0..30)
        .rev()
        .filter_map(|i| today.checked_sub_days(chrono::Days::new(i)))
        .map(|date| {
            let key = date.format("%Y-%m-%d").to_string();
            json!({ "date": key, "seconds": days.get(&key).map_or(0, |d| d.seconds) })
        })
        .collect();
    // Time per book in those 30 days
    let since = today
        .checked_sub_days(chrono::Days::new(29))
        .unwrap_or(today)
        .format("%Y-%m-%d")
        .to_string();
    let mut per_book: HashMap<&str, i64> = HashMap::new();
    for (_, day) in days.range(since..) {
        for (document, seconds) in &day.books {
            *per_book.entry(document).or_default() += seconds;
        }
    }
    let mut top: Vec<_> = per_book.into_iter().filter(|b| b.1 > 0).collect();
    top.sort_by_key(|b| std::cmp::Reverse(b.1));
    let top: Vec<_> = top
        .into_iter()
        .take(5)
        .map(|(document, seconds)| json!({ "document": document, "title": title(document), "seconds": seconds }))
        .collect();

    // Books finished in each of the last 12 months
    let mut months = Vec::new();
    let (mut year, mut month) = (today.year(), today.month());
    for _ in 0..12 {
        months.push(format!("{year:04}-{month:02}"));
        (year, month) = if month == 1 { (year - 1, 12) } else { (year, month - 1) };
    }
    months.reverse();
    let mut finished: BTreeMap<String, Vec<serde_json::Value>> =
        months.iter().map(|m| (m.clone(), Vec::new())).collect();
    for (document, at) in stats.finished() {
        let month = day_of(at).get(..7).unwrap_or_default().to_string();
        if let Some(list) = finished.get_mut(&month) {
            list.push(json!({ "document": document, "title": title(&document), "at": at }));
        }
    }
    let finished: Vec<_> = finished
        .into_iter()
        .map(|(month, books)| json!({ "month": month, "count": books.len(), "books": books }))
        .collect();

    let today_stats = stats.day(&today.format("%Y-%m-%d").to_string());
    Json(json!({
        "today": { "seconds": today_stats.seconds, "sessions": today_stats.sessions },
        "total_seconds": days.values().map(|d| d.seconds).sum::<i64>(),
        "reading_days": days.values().filter(|d| d.seconds >= STREAK_SECONDS).count(),
        "streak": { "current": current, "longest": longest },
        "days": recent,
        "top_books": top,
        "finished": finished,
    }))
    .into_response()
}

/// Just today, for the e-reader's home screen.
pub async fn today(State(state): State<AppState>) -> Response {
    let today = Local::now().date_naive();
    let stats = state.progress.stats();
    let day = stats.day(&today.format("%Y-%m-%d").to_string());
    let (current, _) = streaks(&stats.days(), today);
    Json(json!({ "seconds": day.seconds, "sessions": day.sessions, "streak": current })).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noon() -> i64 {
        Local.with_ymd_and_hms(2026, 11, 14, 12, 0, 0).unwrap().timestamp()
    }

    #[test]
    fn time_between_page_turns_is_reading() {
        let dir = tempfile::tempdir().unwrap();
        let s = Stats::open(dir.path(), Vec::new);
        let t = noon();
        s.record("kobo", "doc", "p1", 0.1, t);
        s.record("kobo", "doc", "p2", 0.2, t + 60);
        s.record("kobo", "doc", "p3", 0.3, t + 150);
        // Sent again when the device sleeps: not reading
        s.record("kobo", "doc", "p3", 0.3, t + 500);
        // Back after a long pause: a new session, the pause is not counted
        s.record("kobo", "doc", "p4", 0.4, t + 3600);
        s.record("kobo", "doc", "p5", 0.5, t + 3630);
        // Another device reading at the same time counts on its own
        s.record("phone", "other", "a", 0.1, t + 3600);
        s.record("phone", "other", "b", 0.1, t + 3700);
        let day = s.day(&day_of(t));
        assert_eq!(day.seconds, 60 + 90 + 30 + 100);
        assert_eq!(day.sessions, 3);
        assert_eq!(day.books["doc"], 180);
        // Kept on disk
        let reopened = Stats::open(dir.path(), || panic!("not seeded twice"));
        assert_eq!(reopened.day(&day_of(t)), day);
    }

    #[test]
    fn finished_once_and_seeded_from_saved_positions() {
        let dir = tempfile::tempdir().unwrap();
        let t = noon();
        let s = Stats::open(dir.path(), || {
            vec![
                ("kobo".into(), "doc".into(), "b".into(), 1.0, t + 120),
                ("kobo".into(), "doc".into(), "a".into(), 0.9, t),
            ]
        });
        assert_eq!(s.day(&day_of(t)).seconds, 120);
        s.record("web", "doc", "c", 1.0, t + 9000);
        assert_eq!(s.finished()["doc"], t + 120);
    }

    #[test]
    fn streaks_count_days_in_a_row() {
        let day = |seconds| Day {
            seconds,
            ..Default::default()
        };
        let days: BTreeMap<String, Day> = [
            ("2026-11-01", day(600)),
            ("2026-11-02", day(600)),
            ("2026-11-03", day(600)),
            ("2026-11-05", day(10)),
            ("2026-11-12", day(300)),
            ("2026-11-13", day(300)),
        ]
        .into_iter()
        .map(|(d, v)| (d.to_string(), v))
        .collect();
        let date = |d| NaiveDate::parse_from_str(d, "%Y-%m-%d").unwrap();
        // Nothing read today yet: the streak from yesterday is still alive
        assert_eq!(streaks(&days, date("2026-11-14")), (2, 3));
        assert_eq!(streaks(&days, date("2026-11-15")), (0, 3));
    }
}
