//! Event appearances (OpenOrbit's `event_appearances.py`, data `data/event_appearances.json`): the designs that have no
//! source in the shop and are won through an event in the real game, and the ways of winning them in single player.
//!
//! The panel's `event_looks_in_shop` setting (default on): when on they are sold in the shop; when off they are not.
//! The ways are always open regardless of the setting:
//!
//! * `exchange`: in the key exchange while the event's Battle Pass lasts ([`Appearances::exchange_rows`]);
//! * `box`: into the box's local design rows with their weight ([`Appearances::box_rows`]);
//! * `gate`: an optional row in the gate rewards of the GG Special Rewards Weekends ([`Appearances::apply`]);
//! * `map`: into the contents of the event's map box ([`Appearances::apply`]);
//! * `season`: the season designs in the reward tracks, in the season's order ([`Appearances::season_track`]).
//!
//! The file is read as it is (no World Editor patches), like OpenOrbit.

use lo_core::settings::Settings;
use lo_protocol::Struct;
use serde_json::{Map, Value, json};

/// The event appearances data.
#[derive(Debug, Clone)]
pub struct Appearances {
    setting: String,
    looks: Map<String, Value>,
    seasons: Vec<Value>,
    box_extra: Map<String, Value>,
    recorded: Value,
    first: (i64, i64),
}

fn year_month(s: &str) -> Option<(i64, i64)> {
    let (y, m) = s.split_once('-')?;
    Some((y.trim().parse().ok()?, m.trim().parse().ok()?))
}

impl Appearances {
    /// From the content of `event_appearances.json`.
    pub fn from_value(d: &Value) -> Result<Appearances, String> {
        let seasons = d
            .get("seasons")
            .and_then(Value::as_array)
            .filter(|s| !s.is_empty())
            .ok_or("event_appearances: no seasons")?
            .clone();
        let season_of = |s: &Value| s.get("season").and_then(Value::as_str).map(str::to_string);
        let recorded_id = match d.get("recorded_season").and_then(Value::as_str) {
            Some(r) => r.to_string(),
            None => season_of(seasons.last().unwrap_or(&Value::Null)).unwrap_or_default(),
        };
        let recorded = seasons
            .iter()
            .find(|s| season_of(s).as_deref() == Some(recorded_id.as_str()))
            .ok_or("event_appearances: recorded season missing")?
            .clone();
        let first = season_of(&seasons[0])
            .as_deref()
            .and_then(year_month)
            .ok_or("event_appearances: bad first season")?;
        Ok(Appearances {
            setting: d
                .get("setting")
                .and_then(Value::as_str)
                .unwrap_or("event_looks_in_shop")
                .to_string(),
            looks: d
                .get("looks")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default(),
            seasons,
            box_extra: d
                .get("box_extra")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default(),
            recorded,
            first,
        })
    }

    /// Reads `data/event_appearances.json`.
    pub fn load(data: &lo_core::GameData) -> Result<Appearances, String> {
        let d = data
            .raw("event_appearances.json")
            .map_err(|e| format!("event_appearances.json: {e}"))?;
        Appearances::from_value(&d)
    }

    /// Whether the design is an event appearance.
    pub fn is_look(&self, loot: &str) -> bool {
        self.looks.contains_key(loot)
    }

    /// Whether it is sold in the shop: it is not an event appearance, or the panel's setting is on.
    pub fn in_shop(&self, loot: &str, settings: &Settings) -> bool {
        !self.looks.contains_key(loot) || settings.switch(&self.setting)
    }

    /// The ways of one kind: `(lootId, path)`.
    pub fn paths(&self, kind: &str) -> Vec<(&str, &Map<String, Value>)> {
        let mut out = Vec::new();
        for (loot, x) in &self.looks {
            for y in x
                .get("paths")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if let Some(y) = y.as_object()
                    && y.get("route").and_then(Value::as_str) == Some(kind)
                {
                    out.push((loot.as_str(), y));
                }
            }
        }
        out
    }

    /// Design rows added to an event's key exchange (`BattlePassKeyExchangeRewardModule`).
    pub fn exchange_rows(&self, event: &str) -> Vec<Struct> {
        self.paths("exchange")
            .into_iter()
            .filter(|(_, y)| y.get("event").and_then(Value::as_str) == Some(event))
            .map(|(loot, y)| {
                let num = |k: &str| {
                    y.get(k)
                        .map(lo_protocol::Value::from_json)
                        .and_then(Result::ok)
                        .unwrap_or(lo_protocol::Value::Null)
                };
                Struct::new("BattlePassKeyExchangeRewardModule")
                    .with("cost", num("charge"))
                    .with("limit", num("cap"))
                    .with("exchangeReward", exchange_id(loot))
                    .with(
                        "listOfloot",
                        vec![
                            Struct::new("BattlePassKeyExchangeLootModule")
                                .with("lootId", loot)
                                .with("amount", 1),
                        ],
                    )
                    .with("priority", 2)
                    .with("isSpecial", true)
                    .with("claimCount", 0)
            })
            .collect()
    }

    /// The box's local design rows `[lootId, 1, weight]`: appearances not in the wiki table and the box's old pool.
    pub fn box_rows(&self, box_: &str) -> Vec<Value> {
        let mut out: Vec<Value> = self
            .paths("box")
            .into_iter()
            .filter(|(_, y)| {
                y.get("box").and_then(Value::as_str) == Some(box_)
                    && !y.get("table").is_some_and(lo_core::settings::truthy)
            })
            .map(|(loot, y)| json!([loot, 1, y.get("weight").cloned().unwrap_or(Value::Null)]))
            .collect();
        if let Some(rows) = self.box_extra.get(box_).and_then(Value::as_array) {
            out.extend(rows.iter().cloned());
        }
        out
    }

    /// Into the calendar's events (in place): the gates' optional reward and the contents of the map boxes.
    pub fn apply(&self, events: &mut Map<String, Value>) {
        // grouped by (event, gates, probability), in order of first appearance
        let mut gates: Vec<((String, Value, Value), Vec<Value>)> = Vec::new();
        for (loot, y) in self.paths("gate") {
            let key = (
                y.get("event")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                y.get("gates").cloned().unwrap_or(Value::Null),
                y.get("probability").cloned().unwrap_or(Value::Null),
            );
            let row = json!([loot, 1]);
            match gates.iter_mut().find(|(k, _)| same_key(k, &key)) {
                Some((_, rows)) => rows.push(row),
                None => gates.push((key, vec![row])),
            }
        }
        for ((ev, keys, chance), rows) in gates {
            for (k, e) in events.iter_mut() {
                if k.starts_with(&ev)
                    && let Some(e) = e.as_object_mut()
                {
                    let list = e
                        .entry("gate_reward")
                        .or_insert_with(|| Value::Array(Vec::new()));
                    if let Some(list) = list.as_array_mut() {
                        list.push(json!({
                            "gates": keys, "probability": chance, "choice": rows, "non_repeating": true
                        }));
                    }
                }
            }
        }
        for (loot, y) in self.paths("map") {
            let Some(e) = y
                .get("event")
                .and_then(Value::as_str)
                .and_then(|ev| events.get_mut(ev))
                .and_then(Value::as_object_mut)
            else {
                continue;
            };
            let box_ = y.get("box").cloned().unwrap_or(Value::Null);
            let weight = y.get("weight").cloned().unwrap_or(Value::Null);
            let mut sections: Vec<&mut Value> = Vec::new();
            let (map, extra) = split_map(e);
            if let Some(m) = map {
                sections.push(m);
            }
            if let Some(Value::Array(x)) = extra {
                sections.extend(x.iter_mut());
            }
            for s in sections {
                for b in s
                    .get_mut("boxes")
                    .and_then(Value::as_array_mut)
                    .into_iter()
                    .flatten()
                {
                    if b.get("type") == Some(&box_)
                        && let Some(c) = b.get_mut("contents").and_then(Value::as_array_mut)
                    {
                        c.push(json!([loot, 1, weight]));
                    }
                }
            }
        }
    }

    /// The season's designs (like `"2026-11"`): a cycle of the seasons counted from the first one, three months each.
    pub fn season(&self, sid: &str) -> &Value {
        let (y, m) = year_month(sid).unwrap_or(self.first);
        let n = ((y - self.first.0) * 12 + m - self.first.1).div_euclid(3);
        &self.seasons[n.rem_euclid(self.seasons.len() as i64) as usize]
    }

    /// A reward track (`ucretsiz` | `gold` | `ext`: `[[level, lootId, count]]`) with the season's designs.
    pub fn season_track(&self, key: &str, rows: &[Value], sid: &str) -> Vec<Value> {
        let s = self.season(sid);
        let swap = |l: &Value| -> Value {
            for k in ["pet", "drone"] {
                if self.recorded.get(k).is_some_and(|r| r == l) {
                    return s.get(k).cloned().unwrap_or(Value::Null);
                }
            }
            l.clone()
        };
        let mut out: Vec<Value> = rows
            .iter()
            .map(|r| {
                let r = r.as_array().map(Vec::as_slice).unwrap_or_default();
                let g = |i: usize| r.get(i).cloned().unwrap_or(Value::Null);
                json!([g(0), swap(&g(1)), g(2)])
            })
            .collect();
        if key == "gold"
            && let Some(ship) = s.get("ship").filter(|v| lo_core::settings::truthy(v))
            && let Some(last) = out.last_mut()
        {
            *last = json!([last[0].clone(), ship.clone(), 1]);
        }
        out
    }
}

/// `e['map']` and `e['map_extra']`, borrowed mutably together.
fn split_map(e: &mut Map<String, Value>) -> (Option<&mut Value>, Option<&mut Value>) {
    let mut map = None;
    let mut extra = None;
    for (k, v) in e.iter_mut() {
        match k.as_str() {
            "map" => map = Some(v),
            "map_extra" => extra = Some(v),
            _ => {}
        }
    }
    (map, extra)
}

/// Python equality of the grouping keys (a number compares by value: 1 == 1.0).
fn same_key(a: &(String, Value, Value), b: &(String, Value, Value)) -> bool {
    a.0 == b.0 && a.1 == b.1 && py_eq(&a.2, &b.2)
}

fn py_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        _ => a == b,
    }
}

/// Name of the key exchange row, in the client's format (`<theme>-<ship>-ship-design`, `nobilis-a-elite-pet-design`).
pub fn exchange_id(loot: &str) -> String {
    if let Some(rest) = loot.strip_prefix("ship_") {
        // re.match(r'ship_(.+?)_design_(.+)$'): the shortest base followed by "_design_" and something
        let (base, d) = (1..rest.len())
            .filter(|&i| rest.is_char_boundary(i))
            .find_map(|i| {
                let tail = rest[i..].strip_prefix("_design_")?;
                (!tail.is_empty()).then(|| (&rest[..i], tail))
            })
            .unwrap_or(("", ""));
        let d = d
            .strip_prefix(&format!("ship-{base}-"))
            .or_else(|| d.strip_prefix(&format!("{base}-")))
            .unwrap_or(d);
        return format!("{d}-{base}-ship-design");
    }
    let kind = if loot.starts_with("drone_") {
        "drone"
    } else {
        "pet"
    };
    let after = loot.split_once("_designs_").map_or("", |x| x.1);
    let after = after.strip_prefix(&format!("{kind}-")).unwrap_or(after);
    match after.rsplit_once('-') {
        Some((a, b)) => format!("{b}-{a}-{kind}-design"),
        None => format!("{after}-{kind}-design"),
    }
}

#[cfg(test)]
mod tests {
    use super::exchange_id;

    #[test]
    fn exchange_ids() {
        assert_eq!(
            exchange_id("ship_goliath_design_goliath-ignite"),
            "ignite-goliath-ship-design"
        );
        assert_eq!(
            exchange_id("ship_sentinel_design_ship-sentinel-neikos"),
            "neikos-sentinel-ship-design"
        );
        assert_eq!(
            exchange_id("ship_g-champion_design_lava"),
            "lava-g-champion-ship-design"
        );
        assert_eq!(
            exchange_id("pet_designs_pet-nobilis-elite"),
            "elite-nobilis-pet-design"
        );
        assert_eq!(exchange_id("drone_designs_havoc"), "havoc-drone-design");
    }
}
