//! `@gemi`, `@arayuz` and `@isaret` against what OpenOrbit's `Profile.generate` gives (`tests/fixtures/login_markers.json`,
//! written by `tools/save_diff/gen_login_markers.py`): varied profiles, panel settings, guests, stations, session hooks
//! and moments (only moments that give the same layout in every time zone of the fixture are kept).

use lo_core::GameData;
use lo_core::settings::{Settings, Values};
use lo_game::event_calendar::EventCalendar;
use lo_game::login_markers::{BeaconState, LoginMarkers, Player, Station};
use lo_save::Profile;
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

fn settings(v: &Value) -> Arc<Settings> {
    Arc::new(Settings::fixed(Values::clean(v)))
}

fn summary(ui: &Value) -> Value {
    let bars = ui["featureBars"].as_array().unwrap();
    Value::Array(
        bars.iter()
            .map(|b| {
                let items: Vec<Value> = b["featureBarItems"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|it| json!([it.get("itemId"), it.get("visible"), it.get("maximized")]))
                    .collect();
                json!([
                    b.get("featureBarType"),
                    b.get("featureBarPosition"),
                    b.get("featureBarOrder"),
                    items
                ])
            })
            .collect(),
    )
}

#[test]
fn same_as_openorbit() {
    let fx: Value = serde_json::from_str(
        &std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/login_markers.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let data = GameData::new(root().join("data"), None);
    let seed = data
        .raw("defaults/login.json")
        .unwrap()
        .get("seed")
        .cloned()
        .unwrap_or(Value::Null);
    let markers = LoginMarkers::load(&data).unwrap();
    let cal = EventCalendar::load(&data, Arc::new(Settings::unbound())).unwrap();
    let (mut n_gemi, mut n_isaret, mut n_arayuz) = (0, 0, 0);
    for case in fx["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let doc = &fx["profiles"][case["profile"].as_str().unwrap()];
        let profile = || Profile::from_value("x.json", doc.clone());
        let set = settings(&case["settings"]);
        let who = Player {
            guest: case["guest"].as_bool().unwrap(),
            host: true,
        };
        let gemi = markers.ship_init(&profile(), &seed, &set, who, None);
        assert_eq!(Value::Object(gemi.clone()), case["gemi"], "{name} @gemi");
        assert_eq!(
            serde_json::to_string(&gemi).unwrap(),
            serde_json::to_string(&case["gemi"]).unwrap(),
            "{name} @gemi order"
        );
        n_gemi += 1;
        for x in case["isaret"].as_array().unwrap() {
            let mut p = profile();
            let st = Station::from_value(&x["station"]);
            let b = markers.login_beacon(&mut p, &seed, st);
            assert_eq!(
                serde_json::to_string(&b).unwrap(),
                serde_json::to_string(&x["beacon"]).unwrap(),
                "{name} @isaret {}",
                x["station"]
            );
            assert_eq!(
                p.d.get("skills").cloned().unwrap_or(Value::Null),
                x["skills_after"],
                "{name} skills"
            );
            n_isaret += 1;
        }
        for x in case["beacon_moves"].as_array().unwrap() {
            let a = &x["args"];
            let flag = |k: &str| a.get(k).and_then(Value::as_bool).unwrap_or(false);
            let pos = a
                .get("pos")
                .map(|p| (p[0].as_f64().unwrap(), p[1].as_f64().unwrap()));
            let mut p = profile();
            let b = markers.beacon(
                &mut p,
                &seed,
                BeaconState {
                    station: a.get("station").and_then(Station::from_value),
                    near_gate: flag("near_gate"),
                    pos,
                    repairing: flag("repairing"),
                    gate_peace: flag("gate_peace"),
                    radiation: flag("radiation"),
                    void: flag("void"),
                },
            );
            assert_eq!(
                serde_json::to_string(&b).unwrap(),
                serde_json::to_string(&x["beacon"]).unwrap(),
                "{name} beacon {a}"
            );
        }
        for x in case
            .get("arayuz")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let now = x["now"].as_f64().unwrap();
            let hooks = x["hooks"].as_object().unwrap().clone();
            let hook = |id: &str| hooks.get(id).and_then(Value::as_bool);
            let ui = markers.ui_layout(&profile(), &cal, now, &hook);
            let want = &fx["uis"][x["ui"].as_u64().unwrap() as usize];
            match want.get("summary") {
                Some(s) => assert_eq!(&summary(&ui), s, "{name} @arayuz at {now}"),
                None => assert_eq!(
                    serde_json::to_string(&ui).unwrap(),
                    serde_json::to_string(want).unwrap(),
                    "{name} @arayuz at {now}"
                ),
            }
            n_arayuz += 1;
        }
    }
    assert!(
        n_gemi > 100 && n_isaret > 400 && n_arayuz > 50,
        "{n_gemi} {n_isaret} {n_arayuz}"
    );
}
