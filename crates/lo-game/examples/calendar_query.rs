//! Answers event calendar queries, for `tools/calendar_diff` (the same queries go to OpenOrbit's `event_calendar.py`).
//!
//! `calendar_query <data dir> <scenario.json> <queries.json>`: prints one JSON list of answers. The scenario gives the
//! settings file, the `--event` arguments and the Battle Pass runs; the World Editor world comes from `OO_WORLD`.

use chrono::NaiveDateTime;
use lo_core::settings::Settings;
use lo_core::{GameData, pyjson};
use lo_game::event_calendar::{EventCalendar, Interval, chinese_new_year, easter, thanksgiving};
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;
use std::sync::Arc;

fn iso(t: NaiveDateTime) -> String {
    let base = t.format("%Y-%m-%dT%H:%M:%S").to_string();
    match t.and_utc().timestamp_subsec_micros() {
        0 => base,
        us => format!("{base}.{us:06}"),
    }
}

fn dt(v: &Value) -> NaiveDateTime {
    let s = v.as_str().expect("datetime");
    NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f").expect("datetime")
}

fn ivs(v: Vec<Interval>) -> Value {
    Value::Array(
        v.into_iter()
            .map(|(a, b)| json!([iso(a), iso(b)]))
            .collect(),
    )
}

fn set(v: BTreeSet<String>) -> Value {
    json!(v.into_iter().collect::<Vec<_>>())
}

fn keys(v: &Value) -> BTreeSet<String> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [data_dir, scenario, queries] = args.as_slice() else {
        eprintln!("usage: calendar_query <data dir> <scenario.json> <queries.json>");
        std::process::exit(2);
    };
    let read = |p: &str| -> Value {
        serde_json::from_str(&std::fs::read_to_string(p).expect(p)).expect(p)
    };
    let sc = read(scenario);
    let qs = read(queries);
    let settings = Arc::new(match sc.get("settings").and_then(Value::as_str) {
        Some(p) => Settings::bind(p),
        None => Settings::unbound(),
    });
    let data = GameData::from_env(data_dir);
    let mut cal = EventCalendar::load(&data, settings).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1)
    });
    let argv: Vec<String> = sc
        .get("argv")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect();
    cal.configure(&argv);
    for (k, r) in sc
        .get("pass_runs")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
    {
        cal.set_pass_run(k, Some((r[0].as_f64().unwrap(), r[1].as_f64().unwrap())));
    }

    let mut out = Vec::new();
    for q in qs.as_array().expect("queries") {
        let op = q["op"].as_str().unwrap_or_default();
        let key = q.get("key").and_then(Value::as_str).unwrap_or_default();
        let now = q.get("now").and_then(Value::as_f64).unwrap_or(0.0);
        let section = || {
            let i = q["idx"].as_u64().unwrap() as usize;
            &cal.event(key).expect("event").sections[i]
        };
        let ans = match op {
            "events" => {
                let m: Map<String, Value> = cal
                    .keys()
                    .map(|k| {
                        (
                            k.to_string(),
                            Value::Object(cal.event(k).unwrap().data.clone()),
                        )
                    })
                    .collect();
                Value::Object(m)
            }
            "conflicts" => {
                let m: Map<String, Value> = cal
                    .keys()
                    .map(|k| {
                        (
                            k.to_string(),
                            set(cal.conflicts(k).cloned().unwrap_or_default()),
                        )
                    })
                    .collect();
                Value::Object(m)
            }
            "known" => json!([iso(cal.known()), cal.always()]),
            "holidays" => {
                let y = q["year"].as_i64().unwrap() as i32;
                let f = |d: Option<chrono::NaiveDate>| d.map(|d| d.to_string());
                json!([f(easter(y)), f(thanksgiving(y)), f(chinese_new_year(y))])
            }
            "season" => json!(cal.season(dt(&q["t"]))),
            "instant" => {
                // everything about one moment
                let mut m = Map::new();
                let active = cal.active(now);
                for k in cal.keys() {
                    m.insert(
                        k.to_string(),
                        json!([
                            cal.period(k, now).map(|p| vec![p.0, p.1]),
                            cal.is_on(k, now),
                            cal.is_active(k, now),
                            cal.forced(k),
                            cal.time_left(k, now),
                            cal.news(k, now),
                        ]),
                    );
                    let e = cal.event(k).unwrap();
                    let w: Vec<bool> = e.sections.iter().map(|s| cal.window(s, now)).collect();
                    m.insert(format!("{k}/window"), json!(w));
                }
                m.insert("_active".into(), set(active.clone()));
                m.insert("_cmd".into(), cal.event_cmd(now).to_json(false));
                m.insert(
                    "_moods".into(),
                    Value::Array(
                        cal.mood_cmds(&active, None)
                            .iter()
                            .map(|s| s.to_json(false))
                            .collect(),
                    ),
                );
                m.insert("_achievements".into(), set(cal.achievement_groups(now)));
                let types: Vec<bool> = q["types"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|t| cal.has_type(t.as_str().unwrap(), now))
                    .collect();
                m.insert("_types".into(), json!(types));
                let mut g = Map::new();
                for gate in q["gates"].as_array().unwrap() {
                    let gate = gate.as_str().unwrap();
                    g.insert(
                        gate.to_string(),
                        json!([
                            cal.gate_rewards(gate, now),
                            cal.gate_choices(gate, now),
                            cal.gate_bundles(gate, now)
                        ]),
                    );
                }
                m.insert("_gates".into(), Value::Object(g));
                Value::Object(m)
            }
            "mood_change" => {
                let a = keys(&q["keys"]);
                let b = keys(&q["before"]);
                Value::Array(
                    cal.mood_cmds(&a, Some(&b))
                        .iter()
                        .map(|s| s.to_json(false))
                        .collect(),
                )
            }
            "upcoming" => {
                let days = q["days"].as_i64().unwrap();
                let m: Map<String, Value> = cal
                    .keys()
                    .map(|k| (k.to_string(), ivs(cal.upcoming(k, now, days))))
                    .collect();
                Value::Object(m)
            }
            "intervals" => {
                let (a, b) = (dt(&q["a"]), dt(&q["b"]));
                let mut m = Map::new();
                for k in cal.keys() {
                    m.insert(k.to_string(), ivs(cal.intervals(k, a, b)));
                    let e = cal.event(k).unwrap();
                    let s: Vec<Value> = e
                        .sections
                        .iter()
                        .map(|s| {
                            json!([
                                ivs(cal.section_intervals(s, a, b, false)),
                                ivs(cal.section_intervals(s, a, b, true))
                            ])
                        })
                        .collect();
                    m.insert(format!("{k}/sections"), json!(s));
                }
                Value::Object(m)
            }
            "section_window" => json!(cal.window(section(), now)),
            other => panic!("unknown op {other}"),
        };
        out.push(ans);
    }
    println!("{}", pyjson::dumps(&Value::Array(out)));
}
