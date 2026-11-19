//! Answers map queries, for `tools/map_diff` (the same queries go to OpenOrbit's `map_links.py` and `map_layout.py`).
//!
//! `map_query <data dir> <scenario.json> <queries.json>`: prints one JSON list of answers. The World Editor world comes
//! from `OO_WORLD`.

use lo_core::pyrandom::Random;
use lo_core::settings::Settings;
use lo_core::{GameData, pyjson};
use lo_game::map_links::{MapLinks, asset_available};
use serde_json::{Map, Value, json};

const FACTIONS: [Option<i64>; 4] = [None, Some(1), Some(2), Some(3)];

fn pair<A: Into<Value>, B: Into<Value>>(p: Option<(A, B)>) -> Value {
    p.map_or(Value::Null, |(a, b)| json!([a.into(), b.into()]))
}

fn tip(h: &MapLinks, g: &Map<String, Value>) -> Value {
    let (k, vals) = h.gate_tip(g);
    let vals: Vec<Value> = vals.into_iter().map(|(v, t)| json!([v, t])).collect();
    json!([k, vals])
}

fn rect4(v: &Value) -> (f64, f64, f64, f64) {
    let f = |i: usize| v[i].as_f64().unwrap();
    (f(0), f(1), f(2), f(3))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [data_dir, scenario, queries] = args.as_slice() else {
        eprintln!("usage: map_query <data dir> <scenario.json> <queries.json>");
        std::process::exit(2);
    };
    let read = |p: &str| -> Value {
        serde_json::from_str(&std::fs::read_to_string(p).expect(p)).expect(p)
    };
    let sc = read(scenario);
    let qs = read(queries);
    let settings = match sc.get("settings").and_then(Value::as_str) {
        Some(p) => Settings::bind(p),
        None => Settings::unbound(),
    };
    let data = GameData::from_env(data_dir);
    let h = MapLinks::load(&data).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1)
    });
    let ml = &h.layout;
    let tpl = data
        .raw("defaults/maps.json")
        .ok()
        .and_then(|m| m.get("5").cloned());
    let mut out = Vec::new();
    for q in qs.as_array().unwrap() {
        let op = q["op"].as_str().unwrap();
        let m = q.get("map").and_then(Value::as_i64).unwrap_or(0);
        let ans = match op {
            "tables" => {
                let names: Map<String, Value> = h
                    .names
                    .iter()
                    .map(|(k, v)| (k.to_string(), json!(v)))
                    .collect();
                let mut lv: Vec<_> = h.levels.iter().collect();
                lv.sort();
                let levels: Map<String, Value> = lv
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), json!([v.0, v.1])))
                    .collect();
                let mut sc: Vec<_> = h.scale.iter().collect();
                sc.sort_by_key(|x| *x.0);
                let scale: Map<String, Value> = sc
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), json!(v)))
                    .collect();
                let mut gs: Vec<_> = h.gates.iter().collect();
                gs.sort_by_key(|x| *x.0);
                let gates: Map<String, Value> = gs
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), json!(v)))
                    .collect();
                json!({"names": names, "levels": levels, "scale": scale, "gates": gates})
            }
            "map" => {
                let gates = h.gates.get(&m).cloned().unwrap_or_default();
                let keys: Vec<i64> = q["keys"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|k| k.as_i64().unwrap())
                    .collect();
                let (w, hh) = h.map_size(m);
                let c = h.center(m);
                let (asset, radius) = ml.structure_asset(m);
                json!({
                    "name": h.name(m), "company": h.company(m), "size": [w, hh], "class": h.map_class(m),
                    "parked": h.parked(m), "center": [c.0, c.1], "client": h.client_map_name(m), "dmz": h.giver_dmz(m),
                    "levels": FACTIONS.iter().map(|f| h.level_required(m, *f, &settings)).collect::<Vec<_>>(),
                    "x8": ([1, 2, 3].iter().flat_map(|f| [11, 12].map(|lv| h.x8_home(*f, lv, m))).collect::<Vec<_>>()),
                    "gates": FACTIONS.iter().map(|f| json!(h.gates(m, *f))).collect::<Vec<_>>(),
                    "jumpgates": gates.iter().map(|g| FACTIONS.iter().map(|f| h.jumpgate_create(g, *f).1).collect::<Vec<_>>()).collect::<Vec<_>>(),
                    "tips": gates.iter().map(|g| json!([tip(&h, g), h.gate_available(g, true).1, h.gate_available(g, false).1])).collect::<Vec<_>>(),
                    "layout": {
                        "of": ml.of(m), "walls": ml.walls(m).iter().map(|r| json!([r.0, r.1, r.2, r.3])).collect::<Vec<_>>(),
                        "poi": ml.poi_msgs(m), "one_way": ml.one_way_gates(m),
                        "structure": ml.structure_pos(m).map(|p| json!([p.0, p.1])),
                        "asset": [pair(asset), radius], "lairs": ml.lairs(m),
                        "safe": ml.safe_zones(m).iter().map(|s| json!([s.0, s.1, s.2])).collect::<Vec<_>>(),
                        "peace": ml.gate_peace_radius(m, 1000.0), "navigates": ml.navigates(m),
                        "gate_pos": keys.iter().map(|k| pair(ml.gate_pos(m, *k))).collect::<Vec<_>>(),
                        "arrival": keys.iter().map(|k| ml.arrival(m, *k)).collect::<Vec<_>>(),
                        "design_pos": (0..120).map(|d| pair(ml.design_pos(m, d))).collect::<Vec<_>>(),
                    }
                })
            }
            "world" => json!(
                FACTIONS
                    .iter()
                    .map(|f| json!([h.map_world(m, tpl.as_ref(), *f), h.map_world(m, None, *f)]))
                    .collect::<Vec<_>>()
            ),
            "arrival" => {
                let src = q["src"].as_i64().unwrap();
                json!(
                    FACTIONS
                        .iter()
                        .map(|f| {
                            let a = h.arrival(src, m, *f);
                            json!([a.0, a.1])
                        })
                        .collect::<Vec<_>>()
                )
            }
            "tip" => {
                let g = q["gate"].as_object().unwrap();
                json!([tip(&h, g), h.gate_available(g, true).1])
            }
            "asset" => {
                let wild: Vec<(String, Value)> = q["wild"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|p| (p[0].as_str().unwrap().to_string(), p[1].clone()))
                    .collect();
                let tip = q["tip"].as_str().unwrap();
                json!([true, false].map(|near| asset_available(&q["id"], tip, near, &wild).1))
            }
            "points" => json!(
                q["points"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|p| {
                        let f = |i: usize| p[i].as_f64().unwrap();
                        json!([
                            ml.in_safe(m, f(0), f(1), f(2)),
                            ml.blocked(m, f(0), f(1), f(3))
                        ])
                    })
                    .collect::<Vec<_>>()
            ),
            "segments" => {
                let (w, hh) = h.map_size(m);
                json!(
                    q["segments"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|s| {
                            let f = |i: usize| s[i].as_f64().unwrap();
                            let wp = ml.waypoint(m, w, hh, (f(0), f(1)), (f(2), f(3)));
                            json!([ml.clear(m, (f(0), f(1)), (f(2), f(3)), f(4)), [wp.0, wp.1]])
                        })
                        .collect::<Vec<_>>()
                )
            }
            "random" => {
                let mut rng = Random::new(q["seed"].as_i64().unwrap());
                let (w, hh) = h.map_size(m);
                let gates: Vec<(f64, f64)> = h
                    .gates(m, None)
                    .iter()
                    .map(|g| (g["x"].as_f64().unwrap(), g["y"].as_f64().unwrap()))
                    .collect();
                let a = ml.free_spot(
                    m,
                    (0.0, 0.0, w as f64, hh as f64),
                    lo_game::map_layout::SHIP_PAD,
                    40,
                    &mut rng,
                );
                let b = ml.free_spot(
                    m,
                    rect4(&q["rect"]),
                    q["pad"].as_f64().unwrap(),
                    q["tries"].as_u64().unwrap() as u32,
                    &mut rng,
                );
                let c = h.random_arrival(m, &mut rng);
                let d = ml.mine_spots(m, &gates, None, (w, hh), &mut rng);
                let e = ml.mine_spots(m, &gates, Some(3), (w, hh), &mut rng);
                let probe: Vec<Value> = q["probe"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|p| {
                        ml.mine_field_of(m, p[0].as_f64().unwrap(), p[1].as_f64().unwrap(), (w, hh))
                            .map_or(Value::Null, |r| json!([r.0, r.1, r.2, r.3]))
                    })
                    .collect();
                json!([
                    pair(a),
                    pair(b),
                    [c.0, c.1],
                    d.iter().map(|p| json!([p.0, p.1])).collect::<Vec<_>>(),
                    e.iter().map(|p| json!([p.0, p.1])).collect::<Vec<_>>(),
                    probe,
                    rng.random()
                ])
            }
            other => panic!("unknown op {other}"),
        };
        out.push(ans);
    }
    println!("{}", pyjson::dumps(&Value::Array(out)));
}
