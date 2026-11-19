//! Boosters (OpenOrbit's `booster.py`, profile `boosters`, data `items.json` `boosters` + `items_o6.json`): timed
//! percentage bonuses that run only while online, summed per type up to a cap, scaled by the panel's booster power.
//!
//! Messages are returned as JSON dicts in OpenOrbit's form (`{"_": class, …}`).

use crate::items::ItemData;
use lo_core::settings::Settings;
use lo_save::Profile;
use lo_save::hangar::num;
use serde_json::{Map, Value, json};

/// The client's booster types, in the recording's order (`ATTRS`).
pub const ATTRS: [&str; 10] = [
    "ABILITY_COOLDOWN_TIME",
    "COLLECT_RESOURCES",
    "COLLECT_RESOURCES_NEWBIE",
    "EXPERIENCE_POINTS",
    "HITPOINTS",
    "HONOUR_POINTS",
    "DAMAGE",
    "REPAIR",
    "SHIELD",
    "SHIELD_REGENERATION",
];

/// Client type -> the server's bonus keys (`MOD_KEYS`).
pub const MOD_KEYS: [(&str, &[&str]); 9] = [
    ("DAMAGE", &["laser", "rocket"]),
    ("HITPOINTS", &["hp"]),
    ("SHIELD", &["shield"]),
    ("SHIELD_REGENERATION", &["shield_regen"]),
    ("REPAIR", &["repair"]),
    ("EXPERIENCE_POINTS", &["ep"]),
    ("HONOUR_POINTS", &["honor"]),
    ("COLLECT_RESOURCES", &["cargo"]),
    ("ABILITY_COOLDOWN_TIME", &["cooldown"]),
];

pub fn is_booster(items: &ItemData, loot: &str) -> bool {
    items.booster_kinds.contains_key(loot) || items.o6["boosters"]["alias"].get(loot).is_some()
}

/// The profile's boosters, an old save's wall-clock end converted to online time (`state`).
pub fn state(p: &mut Profile, now: f64) -> &mut Map<String, Value> {
    let d = p.d.as_object_mut().expect("profile");
    let mut dirty = false;
    let st = d.entry("boosters").or_insert_with(|| json!({}));
    for e in st.as_object_mut().into_iter().flat_map(|m| m.values_mut()) {
        let end = e.get("end_time").and_then(num).unwrap_or(0.0);
        if end != 0.0 {
            let rem = e.get("remaining").and_then(num).unwrap_or(0.0) + (end - now).max(0.0);
            e["remaining"] = json!(rem);
            e["end_time"] = json!(0);
            dirty = true;
        }
    }
    if dirty {
        p.mark_dirty();
    }
    p.d["boosters"].as_object_mut().expect("boosters")
}

/// Seconds left (`left`).
pub fn left(p: &mut Profile, loot: &str, now: f64) -> f64 {
    let Some(e) = state(p, now).get(loot).cloned() else {
        return 0.0;
    };
    if !lo_core::settings::truthy(&e) {
        return 0.0;
    }
    (e.get("end_time").and_then(num).unwrap_or(0.0) - now).max(0.0)
        + e.get("remaining").and_then(num).unwrap_or(0.0)
}

/// The running boosters: (lootId, seconds left), in the profile's order (`active`).
pub fn active(p: &mut Profile, now: f64) -> Vec<(String, f64)> {
    let keys: Vec<String> = state(p, now).keys().cloned().collect();
    keys.into_iter()
        .filter_map(|l| {
            let s = left(p, &l, now);
            (s > 0.0).then_some((l, s))
        })
        .collect()
}

/// Type -> percentage (`values`).
pub fn values(
    items: &ItemData,
    p: &mut Profile,
    settings: &Settings,
    now: f64,
) -> Vec<(String, f64)> {
    let k = settings.multiplier("booster_power", None);
    let mut out: Vec<(String, f64)> = Vec::new();
    for (loot, _) in active(p, now) {
        for (a, v) in items
            .booster_kinds
            .get(&loot)
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
        {
            if ATTRS.contains(&a.as_str()) {
                let add = num(v).unwrap_or(0.0) * k;
                match out.iter_mut().find(|(x, _)| x == a) {
                    Some(slot) => slot.1 += add,
                    None => out.push((a.clone(), 0.0 + add)),
                }
            }
        }
    }
    let cap = &items.e["boosters"]["cap"];
    out.into_iter()
        .map(|(a, v)| match cap.get(&a).and_then(num) {
            Some(c) => {
                let c = c * k;
                (a, if c < v { c } else { v })
            }
            None => (a, v),
        })
        .collect()
}

/// The bonuses by the server's keys, as ratios (`mods`).
pub fn mods(
    items: &ItemData,
    p: &mut Profile,
    settings: &Settings,
    now: f64,
) -> Vec<(String, f64)> {
    let mut out: Vec<(String, f64)> = Vec::new();
    let mut add = |k: &str, v: f64| match out.iter_mut().find(|(x, _)| x == k) {
        Some(slot) => slot.1 += v,
        None => out.push((k.to_string(), 0.0 + v)),
    };
    for (a, v) in values(items, p, settings, now) {
        for (t, keys) in MOD_KEYS {
            if t == a {
                for k in keys {
                    add(k, v / 100.0);
                }
            }
        }
    }
    for (l, _) in active(p, now) {
        for (k, v) in items
            .booster_kinds
            .get(&l)
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
        {
            if !ATTRS.contains(&k.as_str()) {
                add(k, num(v).unwrap_or(0.0) / 100.0);
            }
        }
    }
    out
}

/// How a booster is obtained (`add`'s `n`, `hours`, `seconds`).
#[derive(Debug, Clone, Copy)]
pub enum Gain {
    /// `n` from the shop; `hours`: their own duration (otherwise the item's).
    Shop { n: i64, hours: Option<f64> },
    /// One dropped by an alien, lasting this long.
    Dropped { seconds: f64 },
}

/// Adds boosters; both kinds run online only. An alias (a booster the client does not know) gives its equivalents
/// (`add`).
pub fn add(
    items: &ItemData,
    p: &mut Profile,
    settings: &Settings,
    loot: &str,
    gain: Gain,
    now: f64,
) {
    if let Some(alias) = items.o6["boosters"]["alias"]
        .get(loot)
        .and_then(Value::as_array)
    {
        let n = match gain {
            Gain::Shop { n, .. } => n,
            Gain::Dropped { .. } => 1,
        };
        for pair in alias.clone() {
            let other = pair[0].as_str().unwrap_or_default().to_string();
            add(
                items,
                p,
                settings,
                &other,
                Gain::Shop {
                    n,
                    hours: num(&pair[1]),
                },
                now,
            );
        }
        return;
    }
    let b = &items.e["boosters"];
    let secs = match gain {
        Gain::Dropped { seconds } => seconds,
        Gain::Shop { n, hours } => {
            let h = hours
                .filter(|h| *h != 0.0)
                .or_else(|| b["hour"].get(loot).and_then(num));
            match h.filter(|h| *h != 0.0) {
                Some(h) => n as f64 * h * 3600.0,
                None => n as f64 * num(&b["duration_sec"]).unwrap_or(0.0),
            }
        }
    };
    let mult = settings.multiplier("booster_duration", None);
    let st = state(p, now);
    let e = st
        .entry(loot.to_string())
        .or_insert_with(|| json!({"end_time": 0, "remaining": 0}));
    let rem = e.get("remaining").and_then(num).unwrap_or(0.0) + secs * mult;
    e["remaining"] = json!(rem);
    p.mark_dirty();
}

/// Time spent online -> the boosters whose time ran out (`tick`).
pub fn tick(p: &mut Profile, now: f64, dt: f64) -> Vec<String> {
    let mut gone = Vec::new();
    let keys: Vec<String> = state(p, now).keys().cloned().collect();
    for loot in keys {
        let st = state(p, now);
        let e = st.get_mut(&loot).expect("booster");
        let rem = e.get("remaining").and_then(num).unwrap_or(0.0);
        if rem > 0.0 && e.get("end_time").and_then(num).unwrap_or(0.0) <= now {
            // max(0, remaining - dt): Python keeps the int 0 when nothing is left
            e["remaining"] = if rem - dt > 0.0 {
                json!(rem - dt)
            } else {
                json!(0)
            };
            p.mark_dirty();
        }
        if left(p, &loot, now) <= 0.0 {
            state(p, now).shift_remove(&loot);
            gone.push(loot);
            p.mark_dirty();
        }
    }
    gone
}

/// `AttributeBoosterUpdateCommand` fields (`update_cmd`).
pub fn update_cmd(
    items: &ItemData,
    p: &mut Profile,
    settings: &Settings,
    removed: &[String],
    now: f64,
) -> Value {
    let v = values(items, p, settings, now);
    let attrs: Vec<Value> = ATTRS
        .iter()
        .map(|a| {
            let x = v.iter().find(|(k, _)| k == a).map_or(0.0, |(_, x)| *x);
            json!({"attributeType": a, "value": x, "_": "BoosterUpdateModule"})
        })
        .collect();
    let mut act = active(p, now);
    act.sort_by(|a, b| a.0.cmp(&b.0));
    let details: Vec<Value> = act
        .into_iter()
        .map(|(l, s)| json!({"lootId": l, "secondsLeft": s.trunc() as i64, "_": "BoosterPlayerDetailModule"}))
        .collect();
    let rem: Vec<Value> = removed
        .iter()
        .map(|l| json!({"lootId": l, "secondsLeft": 0, "_": "BoosterPlayerDetailModule"}))
        .collect();
    json!({"boostedAttributes": attrs, "boostedDetailsUpdate": details, "boostedDetailsRemove": rem})
}
