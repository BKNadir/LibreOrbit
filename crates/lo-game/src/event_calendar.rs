//! Event calendar (OpenOrbit's `event_calendar.py`, data `data/events.json` with the World Editor's patches).
//!
//! An event is active either on certain days of the year (`periods`: `[[start, end]]`, end day included; a period may
//! span the new year) or on certain days of the week and at certain hours. Hours are in **local time**, and every
//! computation is done like Python's naive `datetime`: a timestamp becomes local wall-clock fields
//! ([`lo_core::pytime`]), the calendar works on those fields, and the results go back to timestamps with Python's rules.
//!
//! A period end is:
//!
//! * `"MM-DD"`: every year;
//! * `"YYYY-MM-DD"` (or `"YYYY-MM-DD HH:MM"` for a start): that year only (runs of the real game). After the last known
//!   day (`known_calendar`) the event's real seasons take turns, the last one first, each run moved by whole years to
//!   the same weekday (at most 3 days away). Only mature seasons take part (at least 60 % of the last season's runs);
//!   an event that did not run in the last season does not come back. Seasons before `history_start` are not used. A
//!   period whose third element is `"single"` does not repeat;
//! * `"easter+N"`, `"chinese_new_year-N"`, `"thanksgiving_day+N"`: N days after / before that year's holiday.
//!
//! A period's third element is the hours of those days (`[[start, end]]`; an end beyond 24 spills into the next day).
//! Weekly events: `days` (0 = Monday), `hours`, `nth_of_month`, `week_cycle` `[n, i]` (counted from [`week_start`]),
//! `block_cycle` `[n, i]` (half-week blocks Monday-Wednesday / Thursday-Sunday), `week_pattern`
//! `{start, pattern, letter, span}`. `together`: only while one of these events runs. `counts_as`: while this event
//! runs, that one also counts as active. Map sections may have their own `periods` and spawn hours (`hours`,
//! `hours_weekend`).
//!
//! The player's choice comes first: the panel's Events page (`settings.json`, `events`: on / off / by the calendar),
//! then the world's `world_status`. The game server's `--event` option opens events regardless of date
//! (`all`, `none` or names). Events that share a feature (Battle Pass, map decoration, cargo box look) conflict: the one
//! opened by hand wins, then the one kept alive by the Battle Pass, then the one that started first.
//!
//! Deliberate differences with OpenOrbit, all on invalid data: a period end that cannot be read is refused when the
//! data is loaded (Python fails when the event is asked about); a yearly day that does not exist that year (29
//! February) gives no period that year (Python fails); a `counts_as` / `together` loop stops instead of overflowing the
//! stack.

use crate::event_appearances::Appearances;
use chrono::{Datelike, NaiveDate, NaiveDateTime, NaiveTime, TimeDelta};
use lo_core::pytime::{self, weekday};
use lo_core::settings::{EventState, Settings, truthy};
use lo_protocol::Struct;
use serde_json::{Map, Value};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex};

type Dt = NaiveDateTime;

/// Hour windows of a day: `(start, end)` from midnight (`[[a, b]]` in the data).
pub type Hours = Vec<(TimeDelta, TimeDelta)>;

/// A period, a map section or a weekly event open interval: `(start, end)`, end excluded.
pub type Interval = (Dt, Dt);

const MATURE: f64 = 0.6;
/// The time left announced for the permanent feature flags (210,030 hours).
const ALWAYS_LEFT: i64 = 210_030 * 3600;
/// Recursion limit for `together` / `counts_as` chains.
const DEPTH: u32 = 32;

fn day() -> TimeDelta {
    TimeDelta::days(1)
}

/// Week cycles (`week_cycle`, `block_cycle`) are counted from this Monday.
pub fn week_start() -> NaiveDate {
    NaiveDate::from_ymd_opt(2024, 1, 8).expect("date")
}

fn midnight(t: Dt) -> Dt {
    t.date().and_time(NaiveTime::MIN)
}

/// Python's `a // b`.
fn floordiv(a: i64, b: i64) -> i64 {
    let q = a / b;
    if (a % b != 0) && ((a < 0) != (b < 0)) {
        q - 1
    } else {
        q
    }
}

/// Python's `a % b`.
fn pymod(a: i64, b: i64) -> i64 {
    a - b * floordiv(a, b)
}

/// `d.replace(year=d.year + n)`, 29 February becoming the 28th.
fn add_years(d: Dt, n: i64) -> Dt {
    let y = d.year() + n as i32;
    let date = NaiveDate::from_ymd_opt(y, d.month(), d.day())
        .or_else(|| NaiveDate::from_ymd_opt(y, d.month(), 28))
        .unwrap_or(d.date());
    date.and_time(d.time())
}

// ------------------------------------------------------------------ holidays

/// Easter (Gregorian; anonymous algorithm).
pub fn easter(y: i32) -> Option<NaiveDate> {
    let y = i64::from(y);
    let (a, b, c) = (y % 19, y / 100, y % 100);
    let h = pymod(19 * a + b - b / 4 - (b - (b + 8) / 25 + 1) / 3 + 15, 30);
    let l = pymod(32 + 2 * (b % 4) + 2 * (c / 4) - h - c % 4, 7);
    let m = (a + 11 * h + 22 * l) / 451;
    let x = h + l - 7 * m + 114;
    NaiveDate::from_ymd_opt(y as i32, (x / 31) as u32, (x % 31 + 1) as u32)
}

/// Thanksgiving: the 4th Thursday of November.
pub fn thanksgiving(y: i32) -> Option<NaiveDate> {
    let first = NaiveDate::from_ymd_opt(y, 11, 1)?;
    Some(first + TimeDelta::days(pymod(3 - weekday(first), 7) + 21))
}

const CNY: [(i32, u32, u32); 21] = [
    (2020, 1, 25),
    (2021, 2, 12),
    (2022, 2, 1),
    (2023, 1, 22),
    (2024, 2, 10),
    (2025, 1, 29),
    (2026, 2, 17),
    (2027, 2, 6),
    (2028, 1, 26),
    (2029, 2, 13),
    (2030, 2, 3),
    (2031, 1, 23),
    (2032, 2, 11),
    (2033, 1, 31),
    (2034, 2, 19),
    (2035, 2, 8),
    (2036, 1, 28),
    (2037, 2, 15),
    (2038, 2, 4),
    (2039, 1, 24),
    (2040, 2, 12),
];

/// `timedelta(days=x)` (whole microseconds).
fn float_days(x: f64) -> TimeDelta {
    TimeDelta::microseconds((x * 86_400_000_000.0).round_ties_even() as i64)
}

/// The k-th new moon after January 2000 (Meeus, ch. 49, main terms): UTC.
fn new_moon(k: i64) -> Dt {
    let kf = k as f64;
    let t = kf / 1236.85;
    let mut jde = 2451550.09766 + 29.530588861 * kf + 0.00015437 * t * t;
    let e = 1.0 - 0.002516 * t;
    let rad = f64::to_radians;
    let m = rad(2.5534 + 29.10535670 * kf);
    let mp = rad(201.5643 + 385.81693528 * kf + 0.0107582 * t * t);
    let f = rad(160.7108 + 390.67050284 * kf - 0.0016118 * t * t);
    let om = rad(124.7746 - 1.56375588 * kf);
    jde += -0.40720 * mp.sin()
        + 0.17241 * e * m.sin()
        + 0.01608 * (2.0 * mp).sin()
        + 0.01039 * (2.0 * f).sin()
        + 0.00739 * e * (mp - m).sin()
        - 0.00514 * e * (mp + m).sin()
        + 0.00208 * e * e * (2.0 * m).sin()
        - 0.00111 * (mp - 2.0 * f).sin()
        - 0.00057 * (mp + 2.0 * f).sin()
        + 0.00056 * e * (2.0 * mp + m).sin()
        - 0.00042 * (3.0 * mp).sin()
        + 0.00042 * e * (m + 2.0 * f).sin()
        + 0.00038 * e * (m - 2.0 * f).sin()
        - 0.00024 * e * (2.0 * mp - m).sin()
        - 0.00017 * om.sin();
    let base = NaiveDate::from_ymd_opt(2000, 1, 1)
        .and_then(|d| d.and_hms_opt(12, 0, 0))
        .expect("date");
    base + float_days(jde - 2451545.0 - 69.0 / 86400.0)
}

/// Chinese New Year: the table, otherwise the first new moon (China time) on or after 21 January.
pub fn chinese_new_year(y: i32) -> Option<NaiveDate> {
    if let Some(&(_, m, d)) = CNY.iter().find(|x| x.0 == y) {
        return NaiveDate::from_ymd_opt(y, m, d);
    }
    let floor = NaiveDate::from_ymd_opt(y, 1, 21)?;
    let mut k = ((f64::from(y) - 2000.0) * 12.3685).round_ties_even() as i64 - 1;
    loop {
        let d = (new_moon(k) + TimeDelta::hours(8)).date();
        if d >= floor {
            return Some(d);
        }
        k += 1;
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Holiday {
    Easter,
    Thanksgiving,
    ChineseNewYear,
}

impl Holiday {
    fn of(name: &str) -> Option<Holiday> {
        Some(match name {
            "easter" => Holiday::Easter,
            "thanksgiving_day" => Holiday::Thanksgiving,
            "chinese_new_year" => Holiday::ChineseNewYear,
            _ => return None,
        })
    }

    fn date(self, y: i32) -> Option<NaiveDate> {
        match self {
            Holiday::Easter => easter(y),
            Holiday::Thanksgiving => thanksgiving(y),
            Holiday::ChineseNewYear => chinese_new_year(y),
        }
    }
}

/// A yearly period end: `"MM-DD"` or `"holiday±N"`.
#[derive(Debug, Clone, Copy, PartialEq)]
enum YearDay {
    MonthDay(u32, u32),
    Holiday(Holiday, i64),
}

/// Python's `int(text)` (surrounding spaces, a sign, underscores between digits).
fn py_int(s: &str) -> Option<i64> {
    let t = s.trim();
    if t.starts_with('_') || t.ends_with('_') || t.contains("__") {
        return None;
    }
    t.replace('_', "").parse().ok()
}

impl YearDay {
    fn parse(s: &str) -> Result<YearDay, String> {
        let bad = || format!("invalid period end {s:?}");
        let first = s.chars().next().ok_or_else(bad)?;
        let (name, sign, n) = if let Some((a, b)) = s.split_once('+') {
            (a, "+", b)
        } else if first.is_alphabetic() {
            s.split_once('-').map_or((s, "", ""), |(a, b)| (a, "-", b))
        } else {
            (s, "", "")
        };
        if let Some(h) = Holiday::of(name) {
            let n = if n.is_empty() {
                0
            } else {
                py_int(n).ok_or_else(bad)?
            };
            return Ok(YearDay::Holiday(h, if sign == "-" { -n } else { n }));
        }
        let parts: Vec<&str> = s.split('-').collect();
        let [m, d] = parts.as_slice() else {
            return Err(bad());
        };
        let (m, d) = (py_int(m).ok_or_else(bad)?, py_int(d).ok_or_else(bad)?);
        // must be a day of some year (a leap one)
        NaiveDate::from_ymd_opt(
            2000,
            u32::try_from(m).map_err(|_| bad())?,
            u32::try_from(d).map_err(|_| bad())?,
        )
        .ok_or_else(bad)?;
        Ok(YearDay::MonthDay(m as u32, d as u32))
    }

    /// `_day(s, y)`.
    fn at(self, y: i32) -> Option<Dt> {
        match self {
            YearDay::MonthDay(m, d) => {
                NaiveDate::from_ymd_opt(y, m, d).map(|d| d.and_time(NaiveTime::MIN))
            }
            YearDay::Holiday(h, n) => {
                Some(h.date(y)?.and_time(NaiveTime::MIN) + TimeDelta::days(n))
            }
        }
    }
}

// ------------------------------------------------------------------ periods

/// The hours of a period (`h` in OpenOrbit: present when the period has a third element).
type PeriodHours = Option<Arc<Hours>>;

/// A period instance: `(start, end, hours)`.
#[derive(Debug, Clone, PartialEq)]
struct Span {
    s: Dt,
    e: Dt,
    h: PeriodHours,
}

#[derive(Debug, Clone)]
enum Entry {
    Yearly {
        a: YearDay,
        b: YearDay,
        h: PeriodHours,
    },
    Fixed,
}

fn number(v: &Value) -> Result<f64, String> {
    v.as_f64().ok_or_else(|| format!("not a number: {v}"))
}

/// `[[a, b], …]` -> hour windows.
pub fn parse_hours(v: &Value) -> Result<Hours, String> {
    let list = v.as_array().ok_or_else(|| format!("invalid hours {v}"))?;
    list.iter()
        .map(|x| match x.as_array().map(Vec::as_slice) {
            Some([a, b]) => Ok((pytime::hours(number(a)?), pytime::hours(number(b)?))),
            _ => Err(format!("invalid hours {v}")),
        })
        .collect()
}

/// The hours of a period from the elements after its ends (`h`): the first one, if any.
fn period_hours(rest: &[Value]) -> Result<PeriodHours, String> {
    match rest.first() {
        None => Ok(None),
        Some(h) => Ok(Some(Arc::new(parse_hours(h)?))),
    }
}

fn parse_dated(s: &str, end: bool) -> Result<Dt, String> {
    let n = s.chars().count();
    if n == 16 {
        return Dt::parse_from_str(s, "%Y-%m-%d %H:%M")
            .map_err(|e| format!("invalid date {s:?}: {e}"));
    }
    let d =
        NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|e| format!("invalid date {s:?}: {e}"))?;
    let d = d.and_time(NaiveTime::MIN);
    Ok(if end { d + day() } else { d })
}

/// The calendar's fixed points.
#[derive(Debug, Clone, Copy)]
struct Clock {
    /// End of the last known day.
    known: Dt,
    /// The dated runs are complete from here.
    history_start: Dt,
}

impl Clock {
    /// 1 = the first year after `known_calendar`, 0 = the last known year, -1 the one before …
    fn season(&self, x: Dt) -> i64 {
        let mut n = i64::from(x.year() - self.known.year());
        if x < add_years(self.known, n) {
            n -= 1;
        }
        n + 1
    }
}

/// A past run moved by whole years: the same date, then to the same weekday (at most 3 days).
fn project(r: &Span, years: i64) -> Span {
    let base = add_years(r.s, years);
    let mut delta = pymod(weekday(r.s.date()) - weekday(base.date()), 7);
    if delta > 3 {
        delta -= 7;
    }
    let start = base + TimeDelta::days(delta);
    Span {
        s: start,
        e: start + (r.e - r.s),
        h: r.h.clone(),
    }
}

type SpanKey = (i32, Option<i64>);

/// A list of periods (an event's or a map section's `periods`).
#[derive(Debug)]
pub struct Periods {
    entries: Vec<Entry>,
    dated: Vec<Span>,
    once: Vec<Span>,
    /// The real seasons -> indexes in `dated`, and the rotation order.
    by: BTreeMap<i64, Vec<usize>>,
    order: Vec<i64>,
    clock: Clock,
    cache: Mutex<Option<(SpanKey, Arc<Vec<Span>>)>>,
}

impl Periods {
    fn parse(v: &Value, clock: Clock) -> Result<Periods, String> {
        let list = v.as_array().ok_or_else(|| format!("invalid periods {v}"))?;
        let (mut entries, mut dated, mut once) = (Vec::new(), Vec::new(), Vec::new());
        for p in list {
            let p = p.as_array().map(Vec::as_slice).unwrap_or_default();
            let (Some(a), Some(b)) = (
                p.first().and_then(Value::as_str),
                p.get(1).and_then(Value::as_str),
            ) else {
                return Err(format!("invalid period {p:?}"));
            };
            let rest = &p[2..];
            let n = a.chars().count();
            if a.chars().take(4).count() == 4
                && a.chars().take(4).all(|c| c.is_ascii_digit())
                && (n == 10 || n == 16)
            {
                let s = parse_dated(a, false)?;
                let e = if b.chars().count() == 16 {
                    parse_dated(b, false)?
                } else {
                    parse_dated(b, true)?
                };
                if rest.first().and_then(Value::as_str) == Some("single") {
                    once.push(Span {
                        s,
                        e,
                        h: period_hours(&rest[1..])?,
                    });
                } else {
                    dated.push(Span {
                        s,
                        e,
                        h: period_hours(rest)?,
                    });
                }
                entries.push(Entry::Fixed);
                continue;
            }
            entries.push(Entry::Yearly {
                a: YearDay::parse(a)?,
                b: YearDay::parse(b)?,
                h: period_hours(rest)?,
            });
        }
        let mut by: BTreeMap<i64, Vec<usize>> = BTreeMap::new();
        for (i, r) in dated.iter().enumerate() {
            let h = clock.season(r.s);
            if h <= 0 && add_years(clock.known, h - 1) >= clock.history_start {
                by.entry(h).or_default().push(i);
            }
        }
        let order = match by.get(&0) {
            None => Vec::new(),
            Some(last) => {
                let need = MATURE * last.len() as f64;
                by.iter()
                    .rev()
                    .filter(|(_, v)| v.len() as f64 >= need)
                    .map(|(h, _)| *h)
                    .collect()
            }
        };
        Ok(Periods {
            entries,
            dated,
            once,
            by,
            order,
            clock,
            cache: Mutex::new(None),
        })
    }

    /// Periods that may contain `t` (`_spans`): depends only on `t`'s year and season.
    fn spans(&self, t: Dt) -> Arc<Vec<Span>> {
        let key = (
            t.year(),
            (t >= self.clock.known).then(|| self.clock.season(t)),
        );
        let mut c = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((k, v)) = c.as_ref()
            && *k == key
        {
            return v.clone();
        }
        let v = Arc::new(self.compute_spans(t));
        *c = Some((key, v.clone()));
        v
    }

    fn compute_spans(&self, t: Dt) -> Vec<Span> {
        let mut out = Vec::new();
        for entry in &self.entries {
            let Entry::Yearly { a, b, h } = entry else {
                continue;
            };
            for y in [t.year() - 1, t.year()] {
                let (Some(s), Some(e)) = (a.at(y), b.at(y)) else {
                    continue;
                };
                let end = if e >= s { Some(e) } else { b.at(y + 1) };
                if let Some(end) = end {
                    out.push(Span {
                        s,
                        e: end + day(),
                        h: h.clone(),
                    });
                }
            }
        }
        if !self.dated.is_empty() && t >= self.clock.known {
            let st = self.clock.season(t);
            let mut proj = Vec::new();
            for s in [st - 1, st] {
                if s >= 1 && !self.order.is_empty() {
                    let src = self.order[((s - 1) as usize) % self.order.len()];
                    for &i in &self.by[&src] {
                        let p = project(&self.dated[i], s - src);
                        if p.e > self.clock.known {
                            proj.push(p);
                        }
                    }
                }
            }
            // a run already announced for those days replaces the projected one; a long projected run goes on after
            // the announced one when at least half of it is left
            for p in proj {
                let hit = self
                    .dated
                    .iter()
                    .filter(|d| p.s < d.e && d.s < p.e)
                    .map(|d| d.e)
                    .max();
                match hit {
                    None => out.push(p),
                    Some(m) if (p.e - m) * 2 >= p.e - p.s => out.push(Span {
                        s: m,
                        e: p.e,
                        h: p.h,
                    }),
                    Some(_) => {}
                }
            }
        }
        out.extend(self.dated.iter().cloned());
        out.extend(self.once.iter().cloned());
        out
    }

    /// The open window of the first period that contains `t` (`_span_at`).
    fn at(&self, t: Dt) -> Option<Interval> {
        for sp in self.spans(t).iter() {
            if sp.s <= t && t < sp.e {
                return match &sp.h {
                    Some(h) => hours_at(t, |_| h.as_slice(), Some((sp.s, sp.e))),
                    None => Some((sp.s, sp.e)),
                };
            }
        }
        None
    }

    /// The open intervals within `[a, b)` (`_span_windows`).
    fn windows(&self, a: Dt, b: Dt) -> Vec<Interval> {
        let mut found: Vec<Span> = Vec::new();
        let mut t = a - day() * 7;
        loop {
            for sp in self.spans(t).iter() {
                if !found.contains(sp) {
                    found.push(sp.clone());
                }
            }
            if t >= b {
                break;
            }
            t = (t + day() * 7).min(b);
        }
        let mut out = Vec::new();
        for sp in found {
            if sp.e <= a || sp.s >= b {
                continue;
            }
            match &sp.h {
                Some(h) => out.extend(daily(sp.s.max(a - day()), sp.e.min(b), |d| {
                    if sp.s <= d && d < sp.e {
                        h.as_slice()
                    } else {
                        &[]
                    }
                })),
                None => out.push((sp.s, sp.e)),
            }
        }
        merge(clip(out, a, b))
    }
}

/// The hour window that covers `t`; the previous day's window running past 24 also counts. `range`: the day the
/// window started on must be in it (`_hours`).
fn hours_at<'a>(
    t: Dt,
    hours: impl Fn(Dt) -> &'a [(TimeDelta, TimeDelta)],
    range: Option<Interval>,
) -> Option<Interval> {
    let today = midnight(t);
    for d in [today - day(), today] {
        if let Some((first, last)) = range
            && !(first <= d && d < last)
        {
            continue;
        }
        for &(a, b) in hours(d) {
            let (s, e) = (d + a, d + b);
            if s <= t && t < e {
                return Some((s, e));
            }
        }
    }
    None
}

fn merge(mut iv: Vec<Interval>) -> Vec<Interval> {
    iv.sort();
    let mut out: Vec<Interval> = Vec::new();
    for (s, e) in iv {
        match out.last_mut() {
            Some(last) if s <= last.1 => last.1 = last.1.max(e),
            _ => out.push((s, e)),
        }
    }
    out
}

fn clip(iv: Vec<Interval>, a: Dt, b: Dt) -> Vec<Interval> {
    iv.into_iter()
        .filter(|&(s, e)| e > a && s < b)
        .map(|(s, e)| (s.max(a), e.min(b)))
        .collect()
}

/// Intersection of two merged interval lists.
fn intersect(x: &[Interval], y: &[Interval]) -> Vec<Interval> {
    let (mut out, mut i, mut j) = (Vec::new(), 0, 0);
    while i < x.len() && j < y.len() {
        let (s, e) = (x[i].0.max(y[j].0), x[i].1.min(y[j].1));
        if s < e {
            out.push((s, e));
        }
        if x[i].1 < y[j].1 {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

/// Daily hour windows within `[a, b)` (the day before `a` included: hours beyond 24).
fn daily<'a>(a: Dt, b: Dt, hours: impl Fn(Dt) -> &'a [(TimeDelta, TimeDelta)]) -> Vec<Interval> {
    let mut out = Vec::new();
    let mut d = midnight(a) - day();
    while d < b {
        for &(x, y) in hours(d) {
            out.push((d + x, d + y));
        }
        d += day();
    }
    merge(clip(out, a, b))
}

// ------------------------------------------------------------------ weekly events and map sections

#[derive(Debug, Clone)]
struct WeekPattern {
    start: NaiveDate,
    pattern: Vec<char>,
    letter: String,
    span: i64,
}

#[derive(Debug, Clone)]
struct Weekly {
    days: Option<Vec<f64>>,
    nth_of_month: Option<Vec<f64>>,
    week_pattern: Option<WeekPattern>,
    block_cycle: Option<(i64, i64)>,
    week_cycle: (i64, i64),
    hours: Hours,
}

fn int_pair(v: &Value, what: &str) -> Result<(i64, i64), String> {
    match v.as_array().map(Vec::as_slice) {
        Some([n, i]) => {
            let (n, i) = (n.as_i64(), i.as_i64());
            match (n, i) {
                (Some(n), Some(i)) if n != 0 => Ok((n, i)),
                _ => Err(format!("invalid {what} {v}")),
            }
        }
        _ => Err(format!("invalid {what} {v}")),
    }
}

fn numbers(v: &Value, what: &str) -> Result<Vec<f64>, String> {
    v.as_array()
        .ok_or_else(|| format!("invalid {what} {v}"))?
        .iter()
        .map(number)
        .collect()
}

impl Weekly {
    fn parse(e: &Map<String, Value>) -> Result<Weekly, String> {
        let week_pattern = match e.get("week_pattern").filter(|v| truthy(v)) {
            None => None,
            Some(w) => {
                let start = w
                    .get("start")
                    .and_then(Value::as_str)
                    .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
                    .ok_or("invalid week_pattern start")?;
                let pattern: Vec<char> = w
                    .get("pattern")
                    .and_then(Value::as_str)
                    .ok_or("invalid week_pattern pattern")?
                    .chars()
                    .collect();
                if pattern.is_empty() {
                    return Err("empty week_pattern".into());
                }
                Some(WeekPattern {
                    start,
                    pattern,
                    letter: w
                        .get("letter")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    span: match w.get("span") {
                        None => 5,
                        Some(s) => s.as_i64().ok_or("invalid week_pattern span")?,
                    },
                })
            }
        };
        Ok(Weekly {
            days: e.get("days").map(|v| numbers(v, "days")).transpose()?,
            nth_of_month: e
                .get("nth_of_month")
                .filter(|v| truthy(v))
                .map(|v| numbers(v, "nth_of_month"))
                .transpose()?,
            week_pattern,
            block_cycle: e
                .get("block_cycle")
                .filter(|v| truthy(v))
                .map(|v| int_pair(v, "block_cycle"))
                .transpose()?,
            week_cycle: e
                .get("week_cycle")
                .map(|v| int_pair(v, "week_cycle"))
                .transpose()?
                .unwrap_or((1, 0)),
            hours: match e.get("hours") {
                Some(h) => parse_hours(h)?,
                None => vec![(TimeDelta::zero(), TimeDelta::hours(24))],
            },
        })
    }

    /// Whether the event is on this day (`_week_day`).
    fn on_day(&self, d: Dt) -> bool {
        let date = d.date();
        let wd = weekday(date);
        if self
            .days
            .as_ref()
            .is_some_and(|days| !days.contains(&(wd as f64)))
        {
            return false;
        }
        if let Some(nth) = &self.nth_of_month
            && !nth.contains(&(f64::from(date.day() - 1).div_euclid(7.0) + 1.0))
        {
            return false;
        }
        if let Some(w) = &self.week_pattern {
            let th = date - TimeDelta::days(pymod(wd - 3, 7));
            if (date - th).num_days() >= w.span {
                return false;
            }
            let k = pymod(
                floordiv((th - w.start).num_days(), 7),
                w.pattern.len() as i64,
            );
            let mut buf = [0u8; 4];
            return *w.pattern[k as usize].encode_utf8(&mut buf) == *w.letter;
        }
        let weeks = floordiv((date - week_start()).num_days(), 7);
        if let Some((n, i)) = self.block_cycle
            && pymod(2 * weeks + i64::from(wd >= 3), n) != i
        {
            return false;
        }
        let (n, i) = self.week_cycle;
        pymod(weeks, n) == i
    }

    fn hours_of(&self, d: Dt) -> &[(TimeDelta, TimeDelta)] {
        if self.on_day(d) { &self.hours } else { &[] }
    }
}

/// A map section's own calendar: `periods` and spawn hours (`hours`, weekend `hours_weekend`).
#[derive(Debug)]
pub struct Section {
    periods: Option<Periods>,
    hours: Option<Hours>,
    hours_weekend: Option<Hours>,
}

impl Section {
    fn parse(v: &Value, clock: Clock) -> Result<Section, String> {
        Ok(Section {
            periods: v
                .get("periods")
                .map(|p| Periods::parse(p, clock))
                .transpose()?,
            hours: v.get("hours").map(parse_hours).transpose()?,
            hours_weekend: v.get("hours_weekend").map(parse_hours).transpose()?,
        })
    }

    /// The spawn hours of a day: the weekend ones on Saturday and Sunday.
    fn hours_of(&self, d: Dt) -> &[(TimeDelta, TimeDelta)] {
        let normal = self.hours.as_deref().unwrap_or_default();
        if weekday(d.date()) >= 5 {
            self.hours_weekend.as_deref().unwrap_or(normal)
        } else {
            normal
        }
    }
}

// ------------------------------------------------------------------ events

#[derive(Debug)]
enum Rule {
    Periods(Periods),
    Weekly(Weekly),
}

/// One event of `events.json`.
#[derive(Debug)]
pub struct Event {
    pub key: String,
    /// The event's data (with the event appearances added).
    pub data: Map<String, Value>,
    rule: Rule,
    together: Vec<String>,
    world_status: Option<bool>,
    kinds: Vec<String>,
    news: Vec<String>,
    clan_news: Option<Vec<String>>,
    news_default: Option<String>,
    news_extra: Vec<String>,
    /// The map sections (`map` when it is a dict, then `map_extra`).
    pub sections: Vec<Section>,
}

fn strings(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// The map section values of an event: `map` when it is a dict, then `map_extra`.
fn section_values(e: &Map<String, Value>) -> Vec<&Value> {
    let mut out: Vec<&Value> = e.get("map").filter(|m| m.is_object()).into_iter().collect();
    out.extend(
        e.get("map_extra")
            .and_then(Value::as_array)
            .into_iter()
            .flatten(),
    );
    out
}

/// `str(v)` for the values the data uses as names.
fn py_str(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => match n.as_i64() {
            Some(i) => i.to_string(),
            None => lo_core::pyjson::float_repr(n.as_f64().unwrap_or(0.0)),
        },
        other => lo_core::pyjson::dumps(other),
    }
}

/// The news text fields of a run.
fn format_news(pattern: &str, d: NaiveDate) -> Result<String, String> {
    const MON: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    const MONTH: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let m = d.month0() as usize;
    let mut out = String::new();
    let mut it = pattern.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '{' if it.peek() == Some(&'{') => {
                it.next();
                out.push('{');
            }
            '}' if it.peek() == Some(&'}') => {
                it.next();
                out.push('}');
            }
            '{' => {
                let mut name = String::new();
                loop {
                    match it.next() {
                        Some('}') => break,
                        Some(c) => name.push(c),
                        None => return Err(format!("invalid news pattern {pattern:?}")),
                    }
                }
                match name.as_str() {
                    "Y" => out.push_str(&d.year().to_string()),
                    "m" => out.push_str(&format!("{:02}", d.month())),
                    "Mon" => out.push_str(MON[m]),
                    "mon" => out.push_str(&MON[m].to_lowercase()),
                    "Month" => out.push_str(MONTH[m]),
                    _ => {
                        return Err(format!(
                            "unknown field {name:?} in news pattern {pattern:?}"
                        ));
                    }
                }
            }
            '}' => return Err(format!("invalid news pattern {pattern:?}")),
            c => out.push(c),
        }
    }
    Ok(out)
}

impl Event {
    fn parse(key: &str, v: &Value, clock: Clock) -> Result<Event, String> {
        let e = v
            .as_object()
            .ok_or_else(|| format!("event {key}: not an object"))?;
        let ctx = |err: String| format!("event {key}: {err}");
        let rule = match e.get("periods") {
            Some(p) => Rule::Periods(Periods::parse(p, clock).map_err(ctx)?),
            None => Rule::Weekly(Weekly::parse(e).map_err(ctx)?),
        };
        let sections = section_values(e)
            .into_iter()
            .map(|s| Section::parse(s, clock))
            .collect::<Result<_, _>>()
            .map_err(ctx)?;
        let news = strings(e.get("news"));
        for p in &news {
            format_news(p, NaiveDate::MIN).map_err(ctx)?;
        }
        Ok(Event {
            key: key.to_string(),
            data: e.clone(),
            rule,
            together: if e.get("together").is_some_and(truthy) {
                strings(e.get("together"))
            } else {
                Vec::new()
            },
            world_status: match e.get("world_status").and_then(Value::as_str) {
                Some("on") => Some(true),
                Some("off") => Some(false),
                _ => None,
            },
            kinds: strings(e.get("kinds")),
            news,
            clan_news: e.get("clan_news").map(|v| strings(Some(v))),
            news_default: e
                .get("news_default")
                .and_then(Value::as_str)
                .map(str::to_string),
            news_extra: strings(e.get("news_extra")),
            sections,
        })
    }

    fn yearly(&self) -> bool {
        matches!(self.rule, Rule::Periods(_))
    }

    /// Features an event can have on its own: Battle Pass, map decoration, the look of the alien cargo box.
    fn tags(&self) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        if self.kinds.iter().any(|k| k == "BattlePass") {
            out.insert("pass".to_string());
        }
        if let Some(d) = self.data.get("decor") {
            out.insert(format!(
                "sus:{}",
                py_str(d.get("type").unwrap_or(&Value::Null))
            ));
        }
        if self.kinds.iter().any(|k| k == "CandyCargo")
            || section_values(&self.data)
                .iter()
                .any(|s| s.get("cargo_kind").is_some_and(truthy))
        {
            out.insert("cargo".to_string());
        }
        out
    }
}

/// The `--event` mode for yearly events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Calendar,
    All,
    None,
}

/// What one query reads from the outside: the panel's choices and the Battle Pass runs.
struct Snap {
    states: BTreeMap<String, EventState>,
    passes: BTreeMap<String, (f64, f64)>,
}

impl Snap {
    fn pass_run(&self, key: &str, t: f64) -> bool {
        self.passes.get(key).is_some_and(|r| r.0 <= t && t < r.1)
    }
}

/// The event calendar.
pub struct EventCalendar {
    events: Vec<Event>,
    index: HashMap<String, usize>,
    always: Vec<String>,
    counted_as: HashMap<String, Vec<String>>,
    conflicts: HashMap<String, BTreeSet<String>>,
    clock: Clock,
    forced: BTreeSet<String>,
    mode: Mode,
    pass_runs: Mutex<BTreeMap<String, (f64, f64)>>,
    settings: Arc<Settings>,
    ids: Mutex<Vec<String>>,
}

fn parse_day(s: &str) -> Result<Dt, String> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map(|d| d.and_time(NaiveTime::MIN))
        .map_err(|e| format!("invalid date {s:?}: {e}"))
}

impl EventCalendar {
    /// From the content of `events.json` (World Editor patches already applied).
    pub fn from_value(
        d: &Value,
        appearances: &Appearances,
        settings: Arc<Settings>,
    ) -> Result<EventCalendar, String> {
        let mut raw = d
            .get("events")
            .and_then(Value::as_object)
            .cloned()
            .ok_or("events.json: no events")?;
        appearances.apply(&mut raw);
        let known = parse_day(
            d.get("known_calendar")
                .and_then(Value::as_str)
                .ok_or("events.json: no known_calendar")?,
        )? + day();
        let history_start = parse_day(
            d.get("history_start")
                .and_then(Value::as_str)
                .unwrap_or("2023-01-01"),
        )?;
        let clock = Clock {
            known,
            history_start,
        };
        let events: Vec<Event> = raw
            .iter()
            .map(|(k, v)| Event::parse(k, v, clock))
            .collect::<Result<_, _>>()?;
        let index = events
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key.clone(), i))
            .collect();
        let mut counted_as: HashMap<String, Vec<String>> = HashMap::new();
        for e in &events {
            if let Some(c) = e.data.get("counts_as").filter(|v| truthy(v)) {
                counted_as.entry(py_str(c)).or_default().push(e.key.clone());
            }
        }
        let tags: Vec<BTreeSet<String>> = events.iter().map(Event::tags).collect();
        let conflicts = events
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let others = events
                    .iter()
                    .enumerate()
                    .filter(|&(j, _)| j != i && !tags[i].is_disjoint(&tags[j]))
                    .map(|(_, f)| f.key.clone())
                    .collect();
                (e.key.clone(), others)
            })
            .collect();
        Ok(EventCalendar {
            events,
            index,
            always: strings(d.get("continuous").and_then(|c| c.get("kinds"))),
            counted_as,
            conflicts,
            clock,
            forced: BTreeSet::new(),
            mode: Mode::Calendar,
            pass_runs: Mutex::new(BTreeMap::new()),
            settings,
            ids: Mutex::new(Vec::new()),
        })
    }

    /// `data/events.json` with the world's patches, and the event appearances.
    pub fn load(
        data: &lo_core::GameData,
        settings: Arc<Settings>,
    ) -> Result<EventCalendar, String> {
        let d = data
            .load("events.json")
            .map_err(|e| format!("events.json: {e}"))?;
        let appearances = Appearances::load(data)?;
        EventCalendar::from_value(&d, &appearances, settings)
    }

    /// The `--event` option: `all`, `none` or comma-separated event names (`configure`).
    pub fn configure(&mut self, args: &[String]) {
        self.forced.clear();
        self.mode = Mode::Calendar;
        for (i, a) in args.iter().enumerate() {
            let val = if let Some(v) = a.strip_prefix("--event=") {
                v
            } else if a == "--event" && i + 1 < args.len() {
                args[i + 1].as_str()
            } else {
                continue;
            };
            for x in val.split(',') {
                let x = x.trim().to_lowercase();
                match x.as_str() {
                    "all" => self.mode = Mode::All,
                    "none" => self.mode = Mode::None,
                    _ if self.index.contains_key(&x) => {
                        self.forced.insert(x);
                    }
                    _ => {}
                }
            }
        }
    }

    /// The season of a moment: 1 = the first year after `known_calendar`, 0 = the last known year …
    pub fn season(&self, x: Dt) -> i64 {
        self.clock.season(x)
    }

    /// End of the last known day (`known_calendar` + 1 day).
    pub fn known(&self) -> Dt {
        self.clock.known
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn forced_events(&self) -> &BTreeSet<String> {
        &self.forced
    }

    /// An event kept alive by the save's Battle Pass for `[start, end)` (`PASS_RUNS`); `None` removes it.
    pub fn set_pass_run(&self, key: &str, run: Option<(f64, f64)>) {
        let mut p = self.pass_runs.lock().unwrap_or_else(|e| e.into_inner());
        match run {
            Some(r) => {
                p.insert(key.to_string(), r);
            }
            None => {
                p.remove(key);
            }
        }
    }

    pub fn event(&self, key: &str) -> Option<&Event> {
        self.index.get(key).map(|&i| &self.events[i])
    }

    /// The event names, in the data's order.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.events.iter().map(|e| e.key.as_str())
    }

    /// The permanent feature flags (`continuous.kinds`).
    pub fn always(&self) -> &[String] {
        &self.always
    }

    /// The events that conflict with this one.
    pub fn conflicts(&self, key: &str) -> Option<&BTreeSet<String>> {
        self.conflicts.get(key)
    }

    fn snap(&self) -> Snap {
        Snap {
            states: self.settings.events(),
            passes: self
                .pass_runs
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone(),
        }
    }

    // -------------------------------------------------------------- periods

    /// The event's current period (`period`): `(start, end)` timestamps, end excluded.
    pub fn period(&self, key: &str, now: f64) -> Option<(f64, f64)> {
        self.period_in(&self.snap(), key, now, 0)
    }

    fn period_in(&self, sn: &Snap, key: &str, now: f64, depth: u32) -> Option<(f64, f64)> {
        if depth > DEPTH {
            return None;
        }
        let e = self.event(key)?;
        if !e.together.is_empty() && !e.together.iter().any(|k| self.on_in(sn, k, now, depth + 1)) {
            return None;
        }
        let t = pytime::from_timestamp(now)?;
        let p = match &e.rule {
            Rule::Periods(p) => p.at(t),
            Rule::Weekly(w) => hours_at(t, |d| w.hours_of(d), None),
        };
        if let Some((s, e)) = p {
            return Some((pytime::timestamp(s), pytime::timestamp(e)));
        }
        self.counted_as
            .get(key)
            .into_iter()
            .flatten()
            .find_map(|k| self.period_in(sn, k, now, depth + 1))
    }

    /// Whether the map section is open now (`window`): its own periods and spawn hours; with the panel's
    /// `session_always_open` setting, only the periods.
    pub fn window(&self, s: &Section, now: f64) -> bool {
        let Some(t) = pytime::from_timestamp(now) else {
            return false;
        };
        if let Some(p) = &s.periods
            && p.at(t).is_none()
        {
            return false;
        }
        if s.hours.is_none() || self.settings.switch("session_always_open") {
            return true;
        }
        hours_at(t, |d| s.hours_of(d), None).is_some()
    }

    /// The event's periods within the next `days` days (`upcoming`; daily hours left out).
    pub fn upcoming(&self, key: &str, now: f64, days: i64) -> Vec<Interval> {
        let (Some(e), Some(t)) = (self.event(key), pytime::from_timestamp(now)) else {
            return Vec::new();
        };
        let Rule::Periods(p) = &e.rule else {
            return Vec::new();
        };
        let end = t + day() * days as i32;
        let mut found = BTreeSet::new();
        for x in [t, t + day() * floordiv(days, 2) as i32, end] {
            for sp in p.spans(x).iter() {
                if sp.e > t && sp.s < end {
                    found.insert((sp.s, sp.e));
                }
            }
        }
        found.into_iter().collect()
    }

    /// The event's open intervals within `[a, b)` by the calendar (`intervals`; merged and clipped). The panel's
    /// choices and `--event` do not count.
    pub fn intervals(&self, key: &str, a: Dt, b: Dt) -> Vec<Interval> {
        self.intervals_in(key, a, b, 0)
    }

    fn intervals_in(&self, key: &str, a: Dt, b: Dt, depth: u32) -> Vec<Interval> {
        let Some(e) = self.event(key).filter(|_| depth <= DEPTH) else {
            return Vec::new();
        };
        let mut iv = match &e.rule {
            Rule::Periods(p) => p.windows(a, b),
            Rule::Weekly(w) => daily(a, b, |d| w.hours_of(d)),
        };
        for k in self.counted_as.get(key).into_iter().flatten() {
            iv.extend(self.intervals_in(k, a, b, depth + 1));
        }
        let mut iv = merge(iv);
        if !e.together.is_empty() {
            let other: Vec<Interval> = e
                .together
                .iter()
                .flat_map(|k| self.intervals_in(k, a, b, depth + 1))
                .collect();
            iv = intersect(&iv, &merge(other));
        }
        iv
    }

    /// The map section's open intervals within `[a, b)` (`section_intervals`).
    pub fn section_intervals(&self, s: &Section, a: Dt, b: Dt, always_open: bool) -> Vec<Interval> {
        let iv = match &s.periods {
            Some(p) => p.windows(a, b),
            None => vec![(a, b)],
        };
        if s.hours.is_some() && !always_open {
            return intersect(&iv, &daily(a, b, |d| s.hours_of(d)));
        }
        iv
    }

    // -------------------------------------------------------------- which events are open

    /// Priority on conflict (smaller wins).
    fn rank(&self, sn: &Snap, key: &str, now: f64) -> (u8, f64) {
        let top = if sn.states.get(key) == Some(&EventState::On) || self.forced.contains(key) {
            0
        } else if sn.pass_run(key, now) {
            1
        } else {
            2
        };
        (top, self.period_in(sn, key, now, 0).map_or(0.0, |p| p.0))
    }

    /// Non-conflicting ones among the open events, in priority order (`resolve`).
    fn resolve(&self, sn: &Snap, on: &BTreeSet<String>, now: f64) -> BTreeSet<String> {
        let mut ranked: Vec<((u8, f64), &String)> =
            on.iter().map(|k| (self.rank(sn, k, now), k)).collect();
        ranked.sort_by(|x, y| {
            x.0.0
                .cmp(&y.0.0)
                .then(x.0.1.partial_cmp(&y.0.1).unwrap_or(Ordering::Equal))
                .then(x.1.cmp(y.1))
        });
        let mut keep = on.clone();
        for (_, k) in ranked {
            if keep.contains(k)
                && let Some(c) = self.conflicts.get(k)
            {
                keep.retain(|x| !c.contains(x));
            }
        }
        keep
    }

    fn on_in(&self, sn: &Snap, key: &str, now: f64, depth: u32) -> bool {
        let Some(e) = self.event(key) else {
            return false;
        };
        if let Some(st) = sn.states.get(key) {
            return *st == EventState::On;
        }
        if let Some(ws) = e.world_status {
            return ws;
        }
        let yearly = e.yearly();
        if self.forced.contains(key) || (yearly && self.mode == Mode::All) {
            return true;
        }
        if yearly && self.mode == Mode::None {
            return false;
        }
        if sn.pass_run(key, now) {
            return true;
        }
        self.period_in(sn, key, now, depth).is_some()
    }

    /// Whether the event is open by its own rule, conflicts left aside (`_on`).
    pub fn is_on(&self, key: &str, now: f64) -> bool {
        self.on_in(&self.snap(), key, now, 0)
    }

    /// Whether the event is open: its own rule and the conflicts (`is_active`).
    pub fn is_active(&self, key: &str, now: f64) -> bool {
        let sn = self.snap();
        if !self.on_in(&sn, key, now, 0) {
            return false;
        }
        if self.forced.contains(key) {
            return true;
        }
        let mut comp = BTreeSet::from([key.to_string()]);
        let mut todo = vec![key.to_string()];
        while let Some(k) = todo.pop() {
            for j in self.conflicts.get(&k).into_iter().flatten() {
                if !comp.contains(j) && self.on_in(&sn, j, now, 0) {
                    comp.insert(j.clone());
                    todo.push(j.clone());
                }
            }
        }
        comp.len() == 1 || self.resolve(&sn, &comp, now).contains(key)
    }

    /// Whether the event is open regardless of date (`forced`): map sections' windows do not count.
    pub fn forced(&self, key: &str) -> bool {
        let Some(e) = self.event(key) else {
            return false;
        };
        let st = self.settings.event_state(key);
        st == Some(EventState::On)
            || self.forced.contains(key)
            || (self.mode == Mode::All && e.yearly())
            || (st.is_none() && e.world_status == Some(true))
    }

    /// The open events (`active`).
    pub fn active(&self, now: f64) -> BTreeSet<String> {
        let sn = self.snap();
        let on: BTreeSet<String> = self
            .events
            .iter()
            .filter(|e| self.on_in(&sn, &e.key, now, 0))
            .map(|e| e.key.clone())
            .collect();
        let mut r = self.resolve(&sn, &on, now);
        r.extend(on.intersection(&self.forced).cloned());
        r
    }

    /// Whether the event type (`EventModule.type`) is active now.
    pub fn has_type(&self, t: &str, now: f64) -> bool {
        self.always.iter().any(|x| x == t)
            || self.active(now).iter().any(|k| {
                self.event(k)
                    .is_some_and(|e| e.kinds.iter().any(|x| x == t))
            })
    }

    /// Remaining seconds (30 days for an event open without a period).
    pub fn time_left(&self, key: &str, now: f64) -> i64 {
        self.time_left_in(&self.snap(), key, now)
    }

    fn time_left_in(&self, sn: &Snap, key: &str, now: f64) -> i64 {
        let p = self.period_in(sn, key, now, 0);
        if let Some(r) = sn.passes.get(key).filter(|_| sn.pass_run(key, now)) {
            return ((r.1 - now) as i64).max(p.map_or(0, |p| (p.1 - now) as i64));
        }
        p.map_or(30 * 86400, |p| ((p.1 - now) as i64).max(0))
    }

    fn gate_rows(
        &self,
        gate: &str,
        now: f64,
        pick: impl Fn(&Map<String, Value>) -> bool,
    ) -> Vec<&Map<String, Value>> {
        let mut out = Vec::new();
        for k in self.active(now) {
            let Some(e) = self.event(&k) else { continue };
            for s in e
                .data
                .get("gate_reward")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let Some(s) = s.as_object() else { continue };
                let at_gate = s
                    .get("gates")
                    .and_then(Value::as_array)
                    .is_some_and(|g| g.iter().any(|x| x.as_str() == Some(gate)));
                if at_gate && pick(s) {
                    out.push(s);
                }
            }
        }
        out
    }

    /// Galaxy Gate rewards of the active events (`gate_reward` rows without a package): `[lootId, count, probability]`.
    pub fn gate_rewards(&self, gate: &str, now: f64) -> Vec<Value> {
        self.gate_rows(gate, now, |s| !s.get("package").is_some_and(truthy))
            .into_iter()
            .flat_map(|s| {
                s.get("items")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default()
            })
            .collect()
    }

    /// The active events' choice rewards at this gate (`gate_reward` rows with a `choice`).
    pub fn gate_choices(&self, gate: &str, now: f64) -> Vec<Map<String, Value>> {
        self.gate_rows(gate, now, |s| s.get("choice").is_some_and(truthy))
            .into_iter()
            .cloned()
            .collect()
    }

    /// The active events' reward bundles at this gate (`gate_reward` rows with a `package` name).
    pub fn gate_bundles(&self, gate: &str, now: f64) -> Vec<Map<String, Value>> {
        self.gate_rows(gate, now, |s| s.get("package").is_some_and(truthy))
            .into_iter()
            .cloned()
            .collect()
    }

    /// The achievement groups of the active events.
    pub fn achievement_groups(&self, now: f64) -> BTreeSet<String> {
        self.active(now)
            .iter()
            .filter_map(|k| self.event(k))
            .flat_map(|e| strings(e.data.get("achievement")))
            .collect()
    }

    /// The Command Center news of this run of the event (`news`).
    pub fn news(&self, key: &str, now: f64) -> Vec<String> {
        self.news_in(&self.snap(), key, now)
    }

    fn news_in(&self, sn: &Snap, key: &str, now: f64) -> Vec<String> {
        let Some(e) = self.event(key) else {
            return Vec::new();
        };
        let Some(clan) = &e.clan_news else {
            return Vec::new();
        };
        let p = self.period_in(sn, key, now, 0);
        let Some(d) = pytime::date_from_timestamp(p.map_or(now, |p| p.0)) else {
            return Vec::new();
        };
        let ids: Vec<String> = e
            .news
            .iter()
            .filter_map(|x| format_news(x, d).ok())
            .collect();
        let main = ids
            .into_iter()
            .find(|x| clan.contains(x))
            .or_else(|| e.news_default.clone())
            .filter(|m| !m.is_empty());
        main.into_iter()
            .chain(e.news_extra.iter().cloned())
            .collect()
    }

    /// `EventModule.id`: fixed per type name, numbered in order of first use.
    fn id(&self, t: &str) -> i64 {
        let mut ids = self.ids.lock().unwrap_or_else(|e| e.into_inner());
        let i = match ids.iter().position(|x| x == t) {
            Some(i) => i,
            None => {
                ids.push(t.to_string());
                ids.len() - 1
            }
        };
        13000 + i as i64
    }

    fn module(&self, t: &str, left: i64) -> Struct {
        Struct::new("EventModule")
            .with("type", t)
            .with("timeLeft", left)
            .with("id", self.id(t))
            .with("lang", "all")
    }

    /// `EventUpdateCommand` (at login, in place of the command in the save).
    pub fn event_cmd(&self, now: f64) -> Struct {
        let sn = self.snap();
        let mut mods = Vec::new();
        let mut seen = BTreeSet::new();
        for t in &self.always {
            mods.push(self.module(t, ALWAYS_LEFT));
            seen.insert(t.clone());
        }
        for k in self.active(now) {
            let left = self.time_left_in(&sn, &k, now);
            let Some(e) = self.event(&k) else { continue };
            let news = self
                .news_in(&sn, &k, now)
                .into_iter()
                .map(|n| format!("BE_{n}"));
            for t in e.kinds.iter().cloned().chain(news) {
                if seen.insert(t.clone()) {
                    mods.push(self.module(&t, left));
                }
            }
        }
        Struct::new("EventUpdateCommand")
            .with("timestamp", (now * 1000.0) as i64)
            .with("activeCommands", mods)
            .with("utcTimeOffset", pytime::utc_offset(now))
    }

    /// The events' map decorations by type (`moods`): a later event replaces an earlier one of the same type.
    fn moods<'a>(&'a self, keys: &BTreeSet<String>) -> Vec<(&'a Value, &'a Value)> {
        let mut out: Vec<(&Value, &Value)> = Vec::new();
        for k in keys {
            let Some(d) = self.event(k).and_then(|e| e.data.get("decor")) else {
                continue;
            };
            let t = d.get("type").unwrap_or(&Value::Null);
            match out.iter_mut().find(|(x, _)| py_eq(x, t)) {
                Some(slot) => slot.1 = d,
                None => out.push((t, d)),
            }
        }
        out.sort_by(|a, b| py_cmp(a.0, b.0));
        out
    }

    /// The decorations' `EventActivationStateCommand`s: those open (all of them, or with `before`, only those that open
    /// and close since).
    pub fn mood_cmds(
        &self,
        keys: &BTreeSet<String>,
        before: Option<&BTreeSet<String>>,
    ) -> Vec<Struct> {
        let on = self.moods(keys);
        let old = before.map(|b| self.moods(b)).unwrap_or_default();
        let cmd = |s: &Value, a: bool| {
            let typ = lo_protocol::Value::from_json(s.get("type").unwrap_or(&Value::Null))
                .unwrap_or(lo_protocol::Value::Null);
            let attrs: Vec<Struct> = s
                .get("value")
                .map(|v| {
                    vec![Struct::new("EventActivationIntegerAttributeModule").with(
                        "value",
                        lo_protocol::Value::from_json(v).unwrap_or(lo_protocol::Value::Null),
                    )]
                })
                .unwrap_or_default();
            Struct::new("EventActivationStateCommand")
                .with("type", typ)
                .with("active", a)
                .with("attributes", attrs)
        };
        let has = |list: &[(&Value, &Value)], t: &Value| list.iter().any(|(x, _)| py_eq(x, t));
        let mut out: Vec<Struct> = on
            .iter()
            .filter(|(t, _)| before.is_none() || !has(&old, t))
            .map(|(_, s)| cmd(s, true))
            .collect();
        out.extend(
            old.iter()
                .filter(|(t, _)| !has(&on, t))
                .map(|(_, s)| cmd(s, false)),
        );
        out
    }
}

fn py_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        _ => a == b,
    }
}

/// Sorting of decoration types: numbers by value, then strings.
fn py_cmp(a: &Value, b: &Value) -> Ordering {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x
            .as_f64()
            .partial_cmp(&y.as_f64())
            .unwrap_or(Ordering::Equal),
        (Value::Number(_), _) => Ordering::Less,
        (_, Value::Number(_)) => Ordering::Greater,
        (Value::String(x), Value::String(y)) => x.cmp(y),
        _ => Ordering::Equal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holidays() {
        assert_eq!(easter(2026), NaiveDate::from_ymd_opt(2026, 4, 5));
        assert_eq!(easter(2025), NaiveDate::from_ymd_opt(2025, 4, 20));
        assert_eq!(thanksgiving(2026), NaiveDate::from_ymd_opt(2026, 11, 26));
        assert_eq!(chinese_new_year(2026), NaiveDate::from_ymd_opt(2026, 2, 17));
        // beyond the table: the new moon rule agrees with the table's last years
        for &(y, m, d) in &CNY {
            let floor = NaiveDate::from_ymd_opt(y, 1, 21).unwrap();
            let mut k = ((f64::from(y) - 2000.0) * 12.3685).round_ties_even() as i64 - 1;
            let got = loop {
                let x = (new_moon(k) + TimeDelta::hours(8)).date();
                if x >= floor {
                    break x;
                }
                k += 1;
            };
            assert_eq!(got, NaiveDate::from_ymd_opt(y, m, d).unwrap(), "{y}");
        }
    }

    #[test]
    fn year_days() {
        assert_eq!(YearDay::parse("12-31").unwrap(), YearDay::MonthDay(12, 31));
        assert_eq!(
            YearDay::parse("chinese_new_year-1").unwrap(),
            YearDay::Holiday(Holiday::ChineseNewYear, -1)
        );
        assert_eq!(
            YearDay::parse("easter+3").unwrap(),
            YearDay::Holiday(Holiday::Easter, 3)
        );
        assert_eq!(
            YearDay::parse("thanksgiving_day").unwrap(),
            YearDay::Holiday(Holiday::Thanksgiving, 0)
        );
        assert!(YearDay::parse("13-01").is_err());
        assert!(YearDay::parse("foo-3").is_err());
    }

    #[test]
    fn python_division() {
        assert_eq!(floordiv(-1, 7), -1);
        assert_eq!(pymod(-1, 7), 6);
        assert_eq!(pymod(5, -3), -1);
    }

    #[test]
    fn news_patterns() {
        let d = NaiveDate::from_ymd_opt(2026, 3, 9).unwrap();
        assert_eq!(
            format_news("agatusBreach-{Y}{m}", d).unwrap(),
            "agatusBreach-202603"
        );
        assert_eq!(
            format_news("chrominrush{mon}{Y}", d).unwrap(),
            "chrominrushmar2026"
        );
        assert_eq!(
            format_news("apoc{Month}{Y}{{x}}", d).unwrap(),
            "apocMarch2026{x}"
        );
        assert!(format_news("x{nope}", d).is_err());
    }
}
