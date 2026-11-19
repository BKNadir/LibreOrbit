//! Answers profile cases, for `tools/profile_diff` (the same cases go to OpenOrbit's `Profile.patch_template`).
//!
//! `profile_query <data dir> <cases.json> <prepared.json>`: `prepared.json` holds each case's profile as OpenOrbit holds it
//! after loading. Prints one JSON list of answers (see `tools/profile_diff/oracle.py`).

use lo_core::settings::Settings;
use lo_core::{GameData, pyjson};
use lo_game::event_calendar::EventCalendar;
use lo_game::items::ItemData;
use lo_game::login_markers::LoginMarkers;
use lo_game::login_markers::Player;
use lo_game::map_links::MapLinks;
use lo_game::ranks::Ranks;
use lo_game::technology::Timer;
use lo_game::templates::{Ctx, MenuState, menu_status, patch_template};
use lo_game::{booster, combat_items, extras, ship_ability, technology};
use lo_save::Profile;
use serde_json::{Map, Value, json};
use std::sync::Arc;

fn timer(t: Option<Timer>) -> Value {
    t.map_or(Value::Null, |t| {
        json!({"activatable": t.activatable, "timerState": t.state, "maxTime": t.max_time, "time": t.time})
    })
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [data_dir, cases, prepared] = args.as_slice() else {
        eprintln!("usage: profile_query <data dir> <cases.json> <prepared.json>");
        std::process::exit(2);
    };
    let read = |p: &str| -> Value {
        serde_json::from_str(&std::fs::read_to_string(p).expect(p)).expect(p)
    };
    let cases = read(cases);
    let prepared = read(prepared);
    let data = GameData::from_env(data_dir);
    let items = ItemData::load(&data).expect("items");
    let maps = MapLinks::load(&data).expect("maps");
    let markers = LoginMarkers::load(&data).expect("markers");
    let tpl = data.raw("defaults/login.json").expect("login.json");
    let ranks = Ranks::load(&data).expect("ranks");
    let mut out = Vec::new();
    for (c, doc) in cases
        .as_array()
        .unwrap()
        .iter()
        .zip(prepared.as_array().unwrap())
    {
        let settings = Arc::new(match c.get("settings_path").and_then(Value::as_str) {
            Some(p) => Settings::bind(p),
            None => Settings::unbound(),
        });
        let cal = EventCalendar::load(&data, settings.clone()).expect("calendar");
        let hooks: Map<String, Value> = c
            .get("hooks")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let hook = |id: &str| hooks.get(id).and_then(Value::as_bool);
        let now = c["now"].as_f64().unwrap();
        let ctx = Ctx {
            items: &items,
            maps: &maps,
            markers: &markers,
            cal: &cal,
            settings: &settings,
            now,
            hook: &hook,
            daily: c
                .get("daily")
                .and_then(Value::as_object)
                .cloned()
                .filter(|d| !d.is_empty()),
        };
        let mut p = Profile::from_value("x.json", doc.clone());
        let mut state = MenuState::default();
        let force = c
            .get("force_rank")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let r = ranks.daily_rank(
            &items,
            &mut p,
            &settings,
            Player::LOCAL,
            now,
            force,
            &mut |_| {},
        );
        let rank = json!([r, ranks.breakdown(&mut p, now)]);
        let mut msgs = Vec::new();
        for kind in ["entry", "ready"] {
            for m in tpl[kind].as_array().unwrap() {
                let name = m["_m"].as_str().unwrap();
                if name.starts_with('@') {
                    continue;
                }
                msgs.push(patch_template(&ctx, &mut p, &mut state, name, m));
            }
        }
        let mut keys: Vec<String> = state.status_cache.keys().cloned().collect();
        keys.sort();
        let cache: Map<String, Value> = keys
            .iter()
            .map(|k| {
                (
                    k.clone(),
                    menu_status(&ctx, &mut p, &state, k).map_or(Value::Null, Value::Object),
                )
            })
            .collect();
        let sent: Map<String, Value> = state
            .sent
            .iter()
            .map(|(k, v)| (k.clone(), json!(v)))
            .collect();
        let removed: Vec<String> = c
            .get("removed")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
        let strs = |k: &str| -> Vec<String> {
            c[k].as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_string())
                .collect()
        };
        let b = booster::update_cmd(&items, &mut p, &settings, &removed, now);
        let uses: Vec<Value> = strs("loots")
            .iter()
            .map(|l| {
                json!([
                    extras::uses(&items, &mut p, l),
                    extras::charges(&items, &mut p, l),
                    extras::owned(&mut p, l),
                    extras::unlimited(&items, &p, l)
                ])
            })
            .collect();
        let cit: Vec<Value> = strs("timer_loots")
            .iter()
            .map(|l| timer(combat_items::timer_fields(&items, &p, l, now)))
            .collect();
        let tech: Vec<Value> = strs("techs")
            .iter()
            .map(|t| timer(Some(technology::timer_fields(&mut p, t, now))))
            .collect();
        let sa: Vec<Value> = strs("abilities")
            .iter()
            .map(|s| {
                let t = timer(Some(ship_ability::timer_fields(&items, &mut p, s, now)));
                let d = ship_ability::ship_items(&items, &p)
                    .contains(s)
                    .then(|| ship_ability::duration(&items, &p, s));
                json!([t, d])
            })
            .collect();
        let mut sent_sorted: Vec<(&String, &Value)> = sent.iter().collect();
        sent_sorted.sort_by(|a, b| a.0.cmp(b.0));
        out.push(json!({
            "rank": rank, "msgs": msgs, "cache": cache,
            "sent": Value::Object(sent_sorted.into_iter().map(|(k, v)| (k.clone(), v.clone())).collect()),
            "cpu": state.cpu_init_sent, "booster": b, "uses": uses, "cit": cit, "tech": tech, "sa": sa,
            "doc": p.d, "dirty": p.is_dirty()
        }));
    }
    println!("{}", pyjson::dumps(&Value::Array(out)));
}
