//! Ship abilities (OpenOrbit's `ship_ability.py`, data `ship_abilities.json`). Ported so far: the profile side (the
//! ship's ability row, durations, timers, the shortcut bar state); the effects in the world come with the map simulation.

use crate::items::ItemData;
use crate::technology::{ACTIVE, COOLING, READY, Timer};
use lo_core::settings::{fmt_g, truthy};
use lo_save::Profile;
use lo_save::hangar::num;
use serde_json::{Map, Value, json};

fn abilities(items: &ItemData) -> &Value {
    &items.abilities["skills"]
}

/// Whether the menu item is a ship ability (`item in ABILITIES`).
pub fn is_ability(items: &ItemData, item: &str) -> bool {
    abilities(items).get(item).is_some()
}

/// The ship's ability row: the worn design's own row, otherwise the ship's (`row`).
pub fn row<'a>(items: &'a ItemData, p: &Profile) -> &'a Value {
    let ships = &items.abilities["ships"];
    let s = p.ship(None);
    let design = s
        .get("design")
        .and_then(Value::as_str)
        .filter(|d| !d.is_empty())
        .unwrap_or("");
    match ships.get(design).filter(|r| truthy(r)) {
        Some(r) => r,
        None => {
            static EMPTY: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
            s.get("lootId")
                .and_then(Value::as_str)
                .and_then(|l| ships.get(l))
                .unwrap_or_else(|| EMPTY.get_or_init(|| json!({})))
        }
    }
}

/// The active ship's abilities (`ship_items`).
pub fn ship_items(items: &ItemData, p: &Profile) -> Vec<String> {
    row(items, p)["skills"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

fn factor(items: &ItemData, p: &Profile, item: &str, key: &str) -> f64 {
    row(items, p)
        .get("multipliers")
        .and_then(|m| m.get(item))
        .and_then(|m| m.get(key))
        .and_then(num)
        .unwrap_or(1.0)
}

fn special<'a>(items: &'a ItemData, p: &Profile, item: &str) -> Option<&'a Value> {
    row(items, p).get("special").and_then(|s| s.get(item))
}

/// The ability's duration in seconds, with the ship's own value (`duration`).
pub fn duration(items: &ItemData, p: &Profile, item: &str) -> f64 {
    num(&duration_value(items, p, item)).unwrap_or(0.0)
}

/// [`duration`] with Python's number type (the ship's own value is kept as written: `0` stays an integer).
pub fn duration_value(items: &ItemData, p: &Profile, item: &str) -> Value {
    if let Some(d) = special(items, p, item)
        .and_then(|o| o.get("duration_sec"))
        .filter(|d| num(d).is_some())
    {
        return d.clone();
    }
    json!(
        abilities(items)[item]["duration_sec"]
            .as_f64()
            .unwrap_or(0.0)
            * factor(items, p, item, "duration")
    )
}

fn state(p: &mut Profile) -> &mut Map<String, Value> {
    p.d.as_object_mut()
        .expect("profile")
        .entry("ship_abilities")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .expect("ship_abilities")
}

pub fn is_active(items: &ItemData, p: &mut Profile, item: &str, now: f64) -> bool {
    let Some(def) = abilities(items).get(item) else {
        return false;
    };
    let s = state(p).get(item).cloned().unwrap_or(json!({}));
    if def.get("toggle").is_some_and(truthy) {
        return s.get("on").is_some_and(truthy);
    }
    now < s.get("end_time").and_then(num).unwrap_or(0.0)
}

/// The ability's timer (`timer_fields`).
pub fn timer_fields(items: &ItemData, p: &mut Profile, item: &str, now: f64) -> Timer {
    let s = state(p).get(item).cloned().unwrap_or(json!({}));
    let g = |k: &str| s.get(k).and_then(num).unwrap_or(0.0);
    if is_active(items, p, item, now) {
        if abilities(items)[item].get("toggle").is_some_and(truthy) {
            return Timer {
                activatable: now >= g("ready"),
                state: ACTIVE,
                max_time: json!(0),
                time: 0,
            };
        }
        return Timer {
            activatable: false,
            state: ACTIVE,
            max_time: s.get("duration_ms").cloned().unwrap_or(json!(0)),
            time: 0i64.max(((g("end_time") - now) * 1000.0) as i64),
        };
    }
    if now < g("ready") {
        return Timer {
            activatable: false,
            state: COOLING,
            max_time: s.get("cooldown_ms").cloned().unwrap_or(json!(0)),
            time: ((g("ready") - now) * 1000.0) as i64,
        };
    }
    Timer {
        activatable: true,
        state: READY,
        max_time: json!(0),
        time: 0,
    }
}

/// The ability on the shortcut bar: only the active ship's are visible; %DURATION% filled in (`menu_status`).
pub fn menu_status(
    items: &ItemData,
    p: &mut Profile,
    item: &str,
    mut st: Map<String, Value>,
    now: f64,
) -> Map<String, Value> {
    let owned = ship_items(items, p).iter().any(|i| i == item);
    let act = owned && timer_fields(items, p, item, now).activatable;
    st.insert("visible".into(), json!(owned));
    st.insert("available".into(), json!(owned));
    st.insert("activatable".into(), json!(act));
    if owned {
        let rep = json!({"_": "MessageWildcardReplacementModule", "wildcard": "%DURATION%",
                         "replacement": fmt_g(duration(items, p, item)),
                         "replacementType": {"_": "ReplacementTypeModule", "typeValue": 0}});
        for k in ["toolTipSlotBar", "toolTipItemBar"] {
            for line in st
                .get_mut(k)
                .and_then(|t| t.get_mut("commandList"))
                .and_then(Value::as_array_mut)
                .into_iter()
                .flatten()
            {
                line["wildCardReplacements"] = json!([rep.clone()]);
            }
        }
    }
    if owned
        && abilities(items)[item]
            .get("effect")
            .and_then(|e| e.get("last_stand"))
            .is_some_and(truthy)
    {
        let built = state(p)
            .get(item)
            .and_then(|s| s.get("built"))
            .is_some_and(truthy);
        st.insert("selected".into(), json!(built));
    }
    st
}
