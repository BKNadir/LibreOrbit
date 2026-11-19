//! Technology items (OpenOrbit's `technology.py`, data `items.json` `technology`): counts and timers in the profile and
//! their shortcut bar state. Ported so far: the profile side (counts, timers, menu state); the in-world effects come with
//! the map simulation.

use crate::items::ItemData;
use lo_save::Profile;
use lo_save::hangar::{num, py_str};
use serde_json::{Map, Value, json};

/// The client's `TimerState`.
pub const READY: i64 = 0;
pub const ACTIVE: i64 = 1;
pub const COOLING: i64 = 2;

/// A shortcut item's timer: activatable, state, max time and time left (ms) (`timer_fields`).
#[derive(Debug, Clone, PartialEq)]
pub struct Timer {
    pub activatable: bool,
    pub state: i64,
    pub max_time: Value,
    pub time: i64,
}

pub fn is_item(items: &ItemData, item: &str) -> bool {
    items.e["technology"]["items"].get(item).is_some()
}

/// The count owned (`count`).
pub fn count(p: &Profile, item: &str) -> f64 {
    p.d.get("technology")
        .and_then(|t| t.get(item))
        .and_then(num)
        .unwrap_or(0.0)
}

fn state(p: &mut Profile) -> &mut Map<String, Value> {
    p.d.as_object_mut()
        .expect("profile")
        .entry("technology_state")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .expect("technology_state")
}

pub fn is_active(p: &mut Profile, item: &str, now: f64) -> bool {
    now < state(p)
        .get(item)
        .and_then(|s| s.get("end_time"))
        .and_then(num)
        .unwrap_or(0.0)
}

pub fn timer_fields(p: &mut Profile, item: &str, now: f64) -> Timer {
    let s = state(p).get(item).cloned().unwrap_or(json!({}));
    let g = |k: &str| s.get(k).and_then(num).unwrap_or(0.0);
    if now < g("end_time") {
        return Timer {
            activatable: false,
            state: ACTIVE,
            max_time: s.get("duration_ms").cloned().unwrap_or(json!(0)),
            time: ((g("end_time") - now) * 1000.0) as i64,
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

/// Echoes in the Depths of Space: no technology there (`echoes_map`).
pub fn echoes_map(items: &ItemData, map_id: i64) -> bool {
    items.echoes_maps.contains(&map_id)
}

/// The technology item on the shortcut bar: visible if owned, the count on the counter (`menu_status`); `None` for
/// another item.
pub fn menu_status(
    items: &ItemData,
    p: &mut Profile,
    item: &str,
    mut st: Map<String, Value>,
    now: f64,
) -> Option<Map<String, Value>> {
    let def = items.e["technology"]["items"].get(item)?;
    let n = count(p, item);
    if n <= 0.0 {
        st.insert("visible".into(), json!(false));
        st.insert("available".into(), json!(false));
        st.insert("activatable".into(), json!(false));
        st.insert("counterValue".into(), json!(0));
        return Some(st);
    }
    let map_id = p.d["position"].get("mapId").cloned().unwrap_or(json!(0));
    let usable = match def.get("map_only").filter(|m| lo_core::settings::truthy(m)) {
        Some(maps) => maps
            .as_array()
            .is_some_and(|ms| ms.iter().any(|m| py_str(m) == py_str(&map_id))),
        None => true,
    };
    let usable = usable && !echoes_map(items, map_id.as_f64().map_or(0, |f| f as i64));
    let count_v = p.d["technology"][item].clone();
    let lines = json!([
        {"_": "MessageKeyListWildcardModule", "baseKey": item, "baseKeyType": {"_": "ReplacementTypeModule", "typeValue": 3},
         "textStyle": 0, "wildCardReplacements": []},
        {"_": "MessageKeyListWildcardModule", "baseKey": "ttip_count", "baseKeyType": {"_": "ReplacementTypeModule", "typeValue": 5},
         "textStyle": 0, "wildCardReplacements": [
            {"_": "MessageWildcardReplacementModule", "wildcard": "%COUNT%", "replacement": py_str(&count_v),
             "replacementType": {"_": "ReplacementTypeModule", "typeValue": 0}}]}
    ]);
    for k in ["toolTipSlotBar", "toolTipItemBar"] {
        st.insert(
            k.into(),
            json!({"_": "MessageKeyListWildcardCommand", "commandList": lines.clone()}),
        );
    }
    let act = usable && timer_fields(p, item, now).activatable;
    st.insert("visible".into(), json!(true));
    st.insert("available".into(), json!(usable));
    st.insert("activatable".into(), json!(act));
    st.insert("counterValue".into(), count_v);
    st.insert("counterStyle".into(), json!(0));
    Some(st)
}
