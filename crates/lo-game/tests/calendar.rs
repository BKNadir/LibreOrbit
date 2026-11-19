//! The parts of the event calendar that do not depend on the time zone, against what OpenOrbit's `event_calendar.py`
//! gives (`tests/fixtures/calendar.json`, written by `tools/calendar_diff/gen_fixture.py`). The moments in local time
//! are compared by `tools/calendar_diff/compare.py`.

use chrono::{NaiveDate, NaiveDateTime};
use lo_core::GameData;
use lo_core::settings::Settings;
use lo_game::event_calendar::{EventCalendar, Interval, chinese_new_year, easter, thanksgiving};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Arc;

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
}

fn dt(s: &str) -> NaiveDateTime {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").unwrap()
}

fn ivs(v: Vec<Interval>) -> Value {
    Value::Array(
        v.into_iter()
            .map(|(a, b)| {
                json!([
                    a.format("%Y-%m-%dT%H:%M:%S").to_string(),
                    b.format("%Y-%m-%dT%H:%M:%S").to_string()
                ])
            })
            .collect(),
    )
}

#[test]
fn same_as_openorbit() {
    let fx: Value = serde_json::from_str(
        &std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/calendar.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let data = GameData::new(root().join("data"), None);
    let cal = EventCalendar::load(&data, Arc::new(Settings::unbound())).unwrap();
    let mut n = 0;
    for w in fx["windows"].as_array().unwrap() {
        let (a, b) = (dt(w["a"].as_str().unwrap()), dt(w["b"].as_str().unwrap()));
        for (k, expected) in w["events"].as_object().unwrap() {
            assert_eq!(&ivs(cal.intervals(k, a, b)), expected, "intervals {k}");
            let e = cal.event(k).unwrap();
            let secs: Vec<Value> = e
                .sections
                .iter()
                .map(|s| {
                    json!([
                        ivs(cal.section_intervals(s, a, b, false)),
                        ivs(cal.section_intervals(s, a, b, true))
                    ])
                })
                .collect();
            assert_eq!(
                &Value::Array(secs),
                &w["sections"][k],
                "section intervals {k}"
            );
            n += 1;
        }
    }
    assert!(n > 100);
    for (y, expected) in fx["holidays"].as_object().unwrap() {
        let y: i32 = y.parse().unwrap();
        let f = |d: Option<NaiveDate>| d.map(|d| d.to_string());
        assert_eq!(
            &json!([f(easter(y)), f(thanksgiving(y)), f(chinese_new_year(y))]),
            expected,
            "holidays {y}"
        );
    }
    for (t, expected) in fx["seasons"].as_object().unwrap() {
        assert_eq!(cal.season(dt(t)), expected.as_i64().unwrap(), "season {t}");
    }
}
