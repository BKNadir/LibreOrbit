//! Python's naive local time (`datetime.fromtimestamp`, `datetime.timestamp`, `time.timezone`).
//!
//! OpenOrbit works with naive `datetime` objects in the machine's local time zone: a timestamp becomes local wall-clock
//! fields, calendar arithmetic is done on those fields, and the result goes back to a timestamp with Python's rules
//! (the earlier of two candidates in the autumn overlap, the gap handled like CPython's `_mktime`). This module gives
//! the same answers for the same time zone.
//!
//! On Unix the C library's `localtime_r` is used, exactly as CPython does. On Windows the system time zone is read
//! through `chrono` (CPython uses the C runtime there); both follow the same Windows settings, but corner cases in
//! past years with other rules have not been compared.

use chrono::{NaiveDate, NaiveDateTime, TimeDelta, Timelike};
use std::sync::OnceLock;

/// The UNIX epoch as a naive datetime.
pub fn epoch() -> NaiveDateTime {
    NaiveDate::from_ymd_opt(1970, 1, 1)
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .expect("epoch")
}

/// Local fields of a UNIX second, whether daylight saving time is in effect and the UTC offset in seconds (east > 0).
#[derive(Debug, Clone, Copy)]
pub struct LocalTm {
    pub at: NaiveDateTime,
    pub isdst: bool,
    pub gmtoff: i64,
}

#[cfg(unix)]
mod sys {
    use super::LocalTm;
    use chrono::NaiveDate;
    use std::sync::Once;

    unsafe extern "C" {
        fn tzset();
    }

    static TZSET: Once = Once::new();

    pub fn localtime(u: i64) -> Option<LocalTm> {
        // SAFETY: tzset only (re)reads the TZ environment; called once, before any localtime_r of ours.
        TZSET.call_once(|| unsafe { tzset() });
        #[allow(clippy::useless_conversion)] // time_t is 32 bits on some targets
        let t: libc::time_t = u.try_into().ok()?;
        // SAFETY: `tm` is plain data; localtime_r writes it and returns null on failure.
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        let r = unsafe { libc::localtime_r(&t, &mut tm) };
        if r.is_null() {
            return None;
        }
        let d = NaiveDate::from_ymd_opt(
            tm.tm_year.checked_add(1900)?,
            u32::try_from(tm.tm_mon + 1).ok()?,
            u32::try_from(tm.tm_mday).ok()?,
        )?;
        // A leap second (60) becomes 59, as in CPython.
        let at = d.and_hms_opt(
            u32::try_from(tm.tm_hour).ok()?,
            u32::try_from(tm.tm_min).ok()?,
            u32::try_from(tm.tm_sec.min(59)).ok()?,
        )?;
        Some(LocalTm {
            at,
            isdst: tm.tm_isdst > 0,
            gmtoff: tm.tm_gmtoff as i64,
        })
    }
}

#[cfg(not(unix))]
mod sys {
    use super::LocalTm;
    use chrono::{Local, Offset, TimeZone};

    pub fn localtime(u: i64) -> Option<LocalTm> {
        let dt = Local.timestamp_opt(u, 0).single()?;
        let gmtoff = dt.offset().fix().local_minus_utc() as i64;
        Some(LocalTm {
            at: dt.naive_local(),
            isdst: -gmtoff != super::zones().0,
            gmtoff,
        })
    }
}

/// C `localtime` of a UNIX second.
pub fn localtime(u: i64) -> Option<LocalTm> {
    sys::localtime(u)
}

/// `time.timezone` and `time.altzone` (seconds west of UTC), worked out like CPython on Unix: the offsets at the start
/// and the middle of the current year, the larger one being the standard time. Fixed for the process, like Python's.
pub fn zones() -> (i64, i64) {
    static Z: OnceLock<(i64, i64)> = OnceLock::new();
    *Z.get_or_init(|| {
        const YEAR: i64 = (365 * 24 + 6) * 3600;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let t = now / YEAR * YEAR;
        let jan = -sys::localtime(t).map_or(0, |x| x.gmtoff);
        let jul = -sys::localtime(t + YEAR / 2).map_or(0, |x| x.gmtoff);
        if jan < jul { (jul, jan) } else { (jan, jul) }
    })
}

/// Seconds east of UTC as OpenOrbit reports them to the client:
/// `-time.timezone if not time.localtime(now).tm_isdst else -time.altzone`.
pub fn utc_offset(now: f64) -> i64 {
    let (tz, alt) = zones();
    let dst = localtime(now.floor() as i64).is_some_and(|x| x.isdst);
    if dst { -alt } else { -tz }
}

/// The current time as a Python `time.time()` float.
pub fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
}

/// `datetime.fromtimestamp(t)` (naive, local): rounded to the microsecond, half to even.
pub fn from_timestamp(t: f64) -> Option<NaiveDateTime> {
    let mut secs = t.trunc();
    let mut us = ((t - secs) * 1e6).round_ties_even();
    if us >= 1e6 {
        secs += 1.0;
        us -= 1e6;
    } else if us < 0.0 {
        secs -= 1.0;
        us += 1e6;
    }
    let tm = localtime(secs as i64)?;
    Some(tm.at + TimeDelta::microseconds(us as i64))
}

/// `datetime.date.fromtimestamp(t)` (local): the second is rounded down.
pub fn date_from_timestamp(t: f64) -> Option<NaiveDate> {
    localtime(t.floor() as i64).map(|x| x.at.date())
}

/// Whole seconds from the epoch, rounded down (`(dt - epoch) // timedelta(seconds=1)`).
fn floor_secs(dt: NaiveDateTime) -> i64 {
    let d = dt - epoch();
    d.num_seconds() - i64::from(d.subsec_nanos() < 0)
}

/// `local(u)` of CPython's `_mktime`: the local fields of `u`, as seconds from the epoch.
fn local(u: i64) -> i64 {
    localtime(u).map_or(u, |x| floor_secs(x.at))
}

/// `datetime.timestamp()` of a naive local datetime with `fold=0` (CPython's `local_to_seconds`).
pub fn timestamp(dt: NaiveDateTime) -> f64 {
    const MAX_FOLD: i64 = 24 * 3600;
    let t = floor_secs(dt);
    let a = local(t) - t;
    let u1 = t - a;
    let t1 = local(u1);
    let b;
    if t1 == t {
        // fold=0: look for an earlier solution
        let u2 = u1 - MAX_FOLD;
        b = local(u2) - u2;
        if a == b {
            return with_us(u1, dt);
        }
    } else {
        b = t1 - u1;
    }
    let u2 = t - b;
    let t2 = local(u2);
    let s = if t2 == t {
        u2
    } else if t1 == t {
        u1
    } else {
        // in the gap: fold=0 takes the larger
        u1.max(u2)
    };
    with_us(s, dt)
}

fn with_us(s: i64, dt: NaiveDateTime) -> f64 {
    s as f64 + f64::from(dt.nanosecond() / 1000) / 1e6
}

/// `timedelta(hours=h)` for a number of hours from the data (whole microseconds).
pub fn hours(h: f64) -> TimeDelta {
    TimeDelta::microseconds((h * 3_600_000_000.0).round_ties_even() as i64)
}

/// Python's `date.weekday()` (Monday = 0).
pub fn weekday(d: NaiveDate) -> i64 {
    use chrono::Datelike;
    i64::from(d.weekday().num_days_from_monday())
}
