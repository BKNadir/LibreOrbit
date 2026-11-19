//! Daily login calendar (OpenOrbit's `login_calendar.py`, profile `daily_login`): one reward per real day, starting over
//! every month (25 days); from 2027 the client's 12 lists change months every year (`login_set`).
//!
//! Ported so far: the state and the `DailyLoginRewardDayCommand` fields; claiming the reward comes with the shop.

use chrono::{Datelike, NaiveDate, NaiveTime};
use lo_core::pytime;
use lo_save::Profile;
use serde_json::{Map, Value, json};

pub const DAYS: i64 = 25;
/// The year of the client's `rewards.xml` lists (`reward_renewal.BASE_YEAR`).
pub const BASE_YEAR: i32 = 2026;

/// Which of the 12 real lists the calendar shows in that month (`reward_renewal.login_set`).
pub fn login_set(year: i32, month: u32) -> i64 {
    let k = i64::from(year - BASE_YEAR);
    let month = i64::from(month);
    if k <= 0 {
        return month;
    }
    if month == 1 || month == 12 {
        return if k % 2 == 0 { month } else { 13 - month };
    }
    2 + (month - 2 + 3 * k).rem_euclid(10)
}

fn today(now: f64) -> NaiveDate {
    pytime::from_timestamp(now).map_or(NaiveDate::MIN, |t| t.date())
}

/// The calendar in the profile, started over when the month changes (`state`).
pub fn state(p: &mut Profile, now: f64) -> Map<String, Value> {
    let ym = today(now).format("%Y-%m").to_string();
    let d =
        p.d.as_object_mut()
            .expect("profile")
            .entry("daily_login")
            .or_insert_with(|| json!({"ay": "", "day": 0, "last": ""}));
    if d.get("ay").and_then(Value::as_str) != Some(ym.as_str()) {
        d["ay"] = json!(ym);
        d["day"] = json!(0);
        p.mark_dirty();
    }
    p.d["daily_login"].as_object().cloned().unwrap_or_default()
}

pub fn claimed_today(p: &mut Profile, now: f64) -> bool {
    state(p, now).get("last").and_then(Value::as_str)
        == Some(today(now).format("%Y-%m-%d").to_string().as_str())
}

/// Seconds until the next month starts (local time) (`next_month_seconds`).
pub fn next_month_seconds(now: f64) -> i64 {
    let t = today(now);
    let next = if t.month() == 12 {
        NaiveDate::from_ymd_opt(t.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(t.year(), t.month() + 1, 1)
    };
    let (Some(next), Some(cur)) = (next, pytime::from_timestamp(now)) else {
        return 0;
    };
    let d = next.and_time(NaiveTime::MIN) - cur;
    0i64.max((d.num_microseconds().unwrap_or(0) as f64 / 1e6) as i64)
}

fn day(st: &Map<String, Value>) -> i64 {
    st.get("day").and_then(Value::as_i64).unwrap_or(0)
}

/// Today's box (the client's `day`, from 0) (`today_index`).
pub fn today_index(p: &mut Profile, now: f64) -> i64 {
    let d = day(&state(p, now));
    if claimed_today(p, now) && d > 0 {
        d - 1
    } else {
        d
    }
}

/// `DailyLoginRewardDayCommand` fields (`fields`).
pub fn fields(p: &mut Profile, now: f64) -> Map<String, Value> {
    let st = state(p, now);
    let t = today(now);
    let mut m = Map::new();
    m.insert("claimed".into(), json!(claimed_today(p, now)));
    m.insert("month".into(), json!(login_set(t.year(), t.month())));
    m.insert("end".into(), json!(day(&st) >= DAYS));
    m.insert("nextMonthSeconds".into(), json!(next_month_seconds(now)));
    m.insert("day".into(), json!(today_index(p, now)));
    m
}
