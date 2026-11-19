//! Answers settings queries, for `tools/settings_diff` (the same queries go to OpenOrbit's `settings.py`).
//!
//! `settings_query <queries.json>`: each query is a settings document; prints, for each, the cleaned file text, every
//! multiplier with every source, the scaling helpers on sample numbers and the summary.

use lo_core::pyrandom::Random;
use lo_core::settings::{Settings, Values, file_text, schema};
use serde_json::{Value, json};

fn main() {
    let path = std::env::args().nth(1).expect("queries.json");
    let qs: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let sch = schema();
    let sources: Vec<Option<String>> = std::iter::once(None)
        .chain(
            sch.settings
                .iter()
                .filter_map(|s| s.source.clone())
                .map(Some),
        )
        .chain([Some("nope".to_string())])
        .collect();
    let mut out = Vec::new();
    for q in qs.as_array().unwrap() {
        let doc = &q["doc"];
        let s = Settings::fixed(Values::clean(doc));
        let mut mult = Vec::new();
        for st in sch
            .settings
            .iter()
            .filter(|x| x.parent.is_none() && x.kind == lo_core::settings::Kind::Multiplier)
        {
            for src in &sources {
                mult.push(json!(s.multiplier(&st.key, src.as_deref())));
            }
        }
        let mut rng = Random::new(q["seed"].as_i64().unwrap());
        let mut helpers = Vec::new();
        for n in [0i64, 1, 3, 7, 100, 12345, -5] {
            helpers.push(json!([
                s.price("shop_price", n, Some("uridium")),
                s.count("npc_count", n, Some("map")),
                s.duration(n, Some("skylab")),
                s.amount("uridium", n, Some("npc"), &mut rng),
                s.chance("drop", n as f64 / 50.0, Some("booster")),
            ]));
        }
        let summary: Vec<Value> = s
            .summary()
            .into_iter()
            .map(|(a, b)| json!([a, b]))
            .collect();
        out.push(json!({"text": file_text(doc), "mult": mult, "helpers": helpers, "rng": rng.random(), "summary": summary}));
    }
    println!("{}", lo_core::pyjson::dumps(&Value::Array(out)));
}
