//! Reading statistics, worked out from the reading positions that devices send.
//!
//! Every client sends its position a few seconds after a page turn, so the
//! time between two positions of the same device in the same book is time spent
//! reading, unless it is longer than [`GAP`]: then the reader stopped and a new
//! session starts. Time is added to the day it was read on (server time zone)
//! and kept in DATA_DIR/stats.json: a small summary per day, not every position.

use axum::extract::{Query, State};
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
/// A day with at least this much reading keeps a streak going and counts as
/// a reading day. The panel and the apps write durations in whole minutes read
/// (33 s are "0 min"), so a day shown with "1 min" always counts.
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

/// The stretch of time the statistics page looks at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Range {
    Week,
    Month,
    Year,
    All,
}

impl Range {
    pub fn parse(value: Option<&str>) -> Self {
        match value {
            Some("7d") => Self::Week,
            Some("12m") => Self::Year,
            Some("all") => Self::All,
            _ => Self::Month,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::Week => "7d",
            Self::Month => "30d",
            Self::Year => "12m",
            Self::All => "all",
        }
    }
}

/// Time read in each part of a range, oldest first: days for a week or a
/// month, months for a year, and months or years for all time. Answers the
/// kind of part (`day`, `month` or `year`) and (key, seconds) for each.
pub fn buckets(days: &BTreeMap<String, Day>, today: NaiveDate, range: Range) -> (&'static str, Vec<(String, i64)>) {
    let by_day = |n: u64| -> Vec<(String, i64)> {
        (0..n)
            .rev()
            .filter_map(|i| today.checked_sub_days(chrono::Days::new(i)))
            .map(|date| {
                let key = date.format("%Y-%m-%d").to_string();
                let seconds = days.get(&key).map_or(0, |d| d.seconds);
                (key, seconds)
            })
            .collect()
    };
    let sum_by = |len: usize, keys: Vec<String>| -> Vec<(String, i64)> {
        let mut out: BTreeMap<String, i64> = keys.into_iter().map(|k| (k, 0)).collect();
        for (date, day) in days {
            if let Some(total) = date.get(..len).and_then(|k| out.get_mut(k)) {
                *total += day.seconds;
            }
        }
        out.into_iter().collect()
    };
    let months_back = |n: u32| -> Vec<String> {
        let (mut year, mut month) = (today.year(), today.month());
        let mut keys = Vec::new();
        for _ in 0..n {
            keys.push(format!("{year:04}-{month:02}"));
            (year, month) = if month == 1 { (year - 1, 12) } else { (year, month - 1) };
        }
        keys
    };
    match range {
        Range::Week => ("day", by_day(7)),
        Range::Month => ("day", by_day(30)),
        Range::Year => ("month", sum_by(7, months_back(12))),
        Range::All => {
            let first = days
                .iter()
                .find(|(_, d)| d.seconds > 0)
                .and_then(|(date, _)| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok())
                .unwrap_or(today);
            let months = (today.year() - first.year()) * 12 + today.month() as i32 - first.month() as i32 + 1;
            if months <= 24 {
                // At least a year, so a short history still reads as months
                ("month", sum_by(7, months_back(months.max(12) as u32)))
            } else {
                let years = (first.year()..=today.year()).map(|y| format!("{y:04}")).collect();
                ("year", sum_by(4, years))
            }
        }
    }
}

#[derive(Deserialize, Default)]
pub struct SummaryQuery {
    /// `7d`, `30d` (the default), `12m` or `all`.
    range: Option<String>,
}

/// Everything the panel's statistics page shows. `days` and `top_books` without
/// a range are the last 30 days, as the Android app reads them; `chart`,
/// `period` and `top_books` follow the range asked for.
pub async fn summary(State(state): State<AppState>, Query(query): Query<SummaryQuery>) -> Response {
    let range = Range::parse(query.range.as_deref());
    let stats = state.progress.stats();
    let today = Local::now().date_naive();
    let days = stats.days();
    let (current, longest) = streaks(&days, today);
    let books = crate::sync::books_by_document(&state).await;
    let snapshot = state.library.snapshot().await;
    let title = |document: &str| books.get(document).map(|b| b.1.clone()).filter(|t| !t.is_empty());

    // The last 30 days, with the ones without reading
    let recent: Vec<_> = buckets(&days, today, Range::Month)
        .1
        .into_iter()
        .map(|(date, seconds)| json!({ "date": date, "seconds": seconds }))
        .collect();
    let (unit, chart) = buckets(&days, today, range);
    let chart: Vec<_> = chart
        .into_iter()
        .map(|(key, seconds)| json!({ "key": key, "seconds": seconds }))
        .collect();

    // Time per book, days read and sessions in the range
    let since = match range {
        Range::Week => today.checked_sub_days(chrono::Days::new(6)),
        Range::Month => today.checked_sub_days(chrono::Days::new(29)),
        Range::Year => today
            .checked_sub_months(chrono::Months::new(11))
            .and_then(|d| d.with_day(1)),
        Range::All => None,
    }
    .map(|d| d.format("%Y-%m-%d").to_string())
    .unwrap_or_default();
    let mut per_book: HashMap<&str, i64> = HashMap::new();
    let (mut seconds, mut sessions, mut reading_days) = (0, 0, 0);
    for (_, day) in days.range(since.clone()..) {
        seconds += day.seconds;
        sessions += day.sessions;
        if day.seconds >= STREAK_SECONDS {
            reading_days += 1;
        }
        for (document, seconds) in &day.books {
            *per_book.entry(document).or_default() += seconds;
        }
    }
    let mut top: Vec<_> = per_book.into_iter().filter(|b| b.1 > 0).collect();
    top.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    let top: Vec<_> = top
        .into_iter()
        .take(8)
        .map(|(document, seconds)| {
            let path = books.get(document).map(|b| b.0.as_str());
            let book = path.and_then(|p| snapshot.books.iter().find(|b| b.path == p));
            json!({
                "document": document,
                "title": title(document),
                "seconds": seconds,
                "path": path,
                "authors": book.map(|b| b.author_line()),
                "format": book.map(|b| b.format.clone()),
                "readable": book.is_some_and(|b| b.readable),
                "cover": book.filter(|b| b.has_cover).map(|b| format!(
                    "/thumb/{}?v={}",
                    crate::paths::url_encode(&b.path),
                    crate::covers::version(b)
                )),
            })
        })
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
    let all_finished = finished_books(stats.finished(), state.marks.all());
    let finished_in_range = all_finished.values().filter(|at| day_of(**at) >= since).count();
    for (document, at) in all_finished {
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
        "range": range.code(),
        "chart": { "unit": unit, "parts": chart },
        "period": {
            "seconds": seconds,
            "sessions": sessions,
            "reading_days": reading_days,
            "finished": finished_in_range,
        },
        "top_books": top,
        "finished": finished,
    }))
    .into_response()
}

/// When each book was finished: read to the end, or marked as read in the
/// panel or the app. A book marked as unread after it was finished no longer
/// counts.
fn finished_books(read: BTreeMap<String, i64>, marks: HashMap<String, crate::marks::Mark>) -> BTreeMap<String, i64> {
    use crate::marks::State as Mark;
    let mut out = read;
    for (document, mark) in marks {
        match mark.state {
            Mark::Read => {
                out.entry(document).or_insert(mark.at);
            }
            Mark::Unread => {
                if out.get(&document).is_some_and(|at| *at <= mark.at) {
                    out.remove(&document);
                }
            }
        }
    }
    out
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
    fn marks_count_as_finished_or_not() {
        use crate::marks::{Mark, State};
        let read: BTreeMap<String, i64> = [("a".to_string(), 100), ("b".to_string(), 100)].into();
        let marks: HashMap<String, Mark> = [
            // Finished before, so it keeps that date
            (
                "a".to_string(),
                Mark {
                    state: State::Read,
                    at: 500,
                },
            ),
            // Marked as not finished after finishing it
            (
                "b".to_string(),
                Mark {
                    state: State::Unread,
                    at: 200,
                },
            ),
            // Never read to the end here, but marked as finished
            (
                "c".to_string(),
                Mark {
                    state: State::Read,
                    at: 300,
                },
            ),
        ]
        .into();
        let finished = finished_books(read, marks);
        assert_eq!(finished, [("a".to_string(), 100), ("c".to_string(), 300)].into());
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

    #[test]
    fn ranges_add_up_by_day_month_or_year() {
        let day = |seconds| Day {
            seconds,
            ..Default::default()
        };
        let days: BTreeMap<String, Day> = [
            ("2024-03-05", day(100)),
            ("2026-10-31", day(60)),
            ("2026-11-01", day(30)),
            ("2026-11-14", day(300)),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
        let today = NaiveDate::from_ymd_opt(2026, 11, 14).unwrap();
        let (unit, week) = buckets(&days, today, Range::Week);
        assert_eq!((unit, week.len()), ("day", 7));
        assert_eq!(week.last().unwrap(), &("2026-11-14".to_string(), 300));
        let (unit, year) = buckets(&days, today, Range::Year);
        assert_eq!((unit, year.len()), ("month", 12));
        assert_eq!(year[10], ("2026-10".to_string(), 60));
        assert_eq!(year[11], ("2026-11".to_string(), 330));
        let (unit, all) = buckets(&days, today, Range::All);
        assert_eq!(unit, "year");
        assert_eq!(all, [("2024".into(), 100), ("2025".into(), 0), ("2026".into(), 390)]);
        assert_eq!(Range::parse(Some("bogus")), Range::Month);
    }
}
