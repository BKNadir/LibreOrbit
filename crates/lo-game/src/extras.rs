//! Extras and CPUs (OpenOrbit's `extras.py`, data `items.json` `extras`): only those fitted in the active config take
//! effect; the passive effects are queried here and applied by the systems; limited-use extras keep one pool of uses per
//! lootId in the profile (`extra_uses`, `extra_copies`).

use crate::items::ItemData;
use lo_save::Profile;
use lo_save::hangar::{num, py_str};
use serde_json::{Value, json};

/// The uses of an extra that has no limit.
pub const UNLIMITED: i64 = 1_000_000;

/// The lootIds of the extras fitted on the active (or given) ship's active config, those fitted to drones included
/// (`equipped`).
pub fn equipped(p: &mut Profile, sid: Option<&str>) -> Vec<String> {
    let active = py_str(&p.d["active_ship"]);
    let strs = |v: &Value| -> Vec<String> {
        v.as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect()
    };
    let (mut own, drones): (Vec<String>, Vec<Vec<String>>) = match sid {
        None => (
            strs(
                p.d.get("equipment")
                    .and_then(|e| e.get("extra"))
                    .unwrap_or(&Value::Null),
            ),
            p.d.get("drone_equipment")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|pair| strs(&pair[1]))
                .collect(),
        ),
        Some(s) if s == active => (
            strs(
                p.d.get("equipment")
                    .and_then(|e| e.get("extra"))
                    .unwrap_or(&Value::Null),
            ),
            p.d.get("drone_equipment")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|pair| strs(&pair[1]))
                .collect(),
        ),
        Some(s) => {
            let own = p
                .loadout(Some(s), None)
                .into_iter()
                .find(|(k, _)| *k == "extra")
                .map(|(_, v)| v)
                .unwrap_or_default();
            let drones = p
                .drone_loadout(Some(s), None)
                .into_iter()
                .map(|(_, l)| l)
                .collect();
            (own, drones)
        }
    };
    own.extend(
        drones
            .into_iter()
            .flatten()
            .filter(|l| l.starts_with("equipment_extra_")),
    );
    own
}

/// A property of the fitted extras: `(lootId, value)` (`props`).
pub fn props(
    items: &ItemData,
    p: &mut Profile,
    key: &str,
    sid: Option<&str>,
) -> Vec<(String, Value)> {
    let x = items.extras();
    equipped(p, sid)
        .into_iter()
        .filter_map(|l| x.get(&l).and_then(|e| e.get(key)).cloned().map(|v| (l, v)))
        .collect()
}

pub fn has(items: &ItemData, p: &mut Profile, key: &str, sid: Option<&str>) -> bool {
    !props(items, p, key, sid).is_empty()
}

/// Whether the extra is in the inventory or fitted (`owned`).
pub fn owned(p: &mut Profile, loot: &str) -> bool {
    p.item_loots().contains(&loot) || equipped(p, None).iter().any(|l| l == loot)
}

/// The extra slot of the slot CPU fitted among these extras (`slot_bonus_of`).
pub fn slot_bonus_of(items: &ItemData, loots: &[String]) -> f64 {
    loots
        .iter()
        .filter_map(|l| {
            items
                .extras()
                .get(l)
                .and_then(|e| e.get("extra_slot"))
                .and_then(num)
        })
        .fold(0.0, f64::max)
}

/// The cargo space multiplier (`cargo_factor`).
pub fn cargo_factor(items: &ItemData, p: &mut Profile) -> f64 {
    props(items, p, "cargo_multiplier", None)
        .iter()
        .filter_map(|(_, v)| num(v))
        .fold(1.0, f64::max)
}

/// The rocket cooldown multiplier (`rocket_factor`).
pub fn rocket_factor(items: &ItemData, p: &mut Profile) -> f64 {
    props(items, p, "rocket_cooldown", None)
        .iter()
        .filter_map(|(_, v)| num(v))
        .fold(None, |m: Option<f64>, v| Some(m.map_or(v, |m| m.min(v))))
        .unwrap_or(1.0)
}

/// The mine cooldown factor (`mine_factor`).
pub fn mine_factor(items: &ItemData, p: &mut Profile) -> f64 {
    1.0 + props(items, p, "mine_cooldown", None)
        .iter()
        .filter_map(|(_, v)| num(v))
        .fold(None, |m: Option<f64>, v| Some(m.map_or(v, |m| m.min(v))))
        .unwrap_or(0.0)
}

/// Whether the uses are unlimited (HM-7 with Goliath's Amber, Jade or Sapphire design) (`unlimited`).
pub fn unlimited(items: &ItemData, p: &Profile, loot: &str) -> bool {
    let Some(designs) = items
        .extras()
        .get(loot)
        .and_then(|e| e.get("unlimited_design"))
        .and_then(Value::as_array)
        .filter(|d| !d.is_empty())
    else {
        return false;
    };
    p.d["ships"]
        .as_object()
        .into_iter()
        .flat_map(|s| s.values())
        .any(|s| {
            let owned = s.get("designs").and_then(Value::as_array);
            designs.iter().any(|d| owned.is_some_and(|o| o.contains(d)))
        })
}

fn right(items: &ItemData, loot: &str) -> i64 {
    items
        .extras()
        .get(loot)
        .and_then(|e| e.get("right"))
        .and_then(num)
        .map_or(0, |f| f as i64)
}

/// The remaining uses of a limited-use extra: one pool per lootId, each owned copy adding its uses when first counted,
/// selling copies capping the pool (`uses`).
pub fn uses(items: &ItemData, p: &mut Profile, loot: &str) -> i64 {
    if unlimited(items, p, loot) {
        return UNLIMITED;
    }
    let right = right(items, loot);
    let n = (p.item_loots().iter().filter(|l| **l == loot).count() as i64).max(1);
    let d = p.d.as_object_mut().expect("profile");
    d.entry("extra_uses").or_insert_with(|| json!({}));
    d.entry("extra_copies").or_insert_with(|| json!({}));
    let st_before = d["extra_uses"].get(loot).cloned();
    let seen_before = d["extra_copies"].get(loot).cloned();
    let prev = seen_before.as_ref().and_then(num).map_or(1, |f| f as i64);
    let cur = st_before.as_ref().and_then(num).map(|f| f as i64);
    let new = match cur {
        None => right * n,
        Some(c) if n > prev => c + right * (n - prev),
        Some(c) if n < prev => c.min(right * n),
        Some(c) => c,
    };
    if cur != Some(new) || st_before.is_none() {
        d["extra_uses"][loot] = json!(new);
    }
    d["extra_copies"][loot] = json!(n);
    let changed = st_before.as_ref().and_then(num).map(|f| f as i64) != Some(new)
        || seen_before.as_ref().and_then(num).map(|f| f as i64) != Some(n);
    if changed {
        p.mark_dirty();
    }
    new
}

/// The counter on the bar: the uses left on the copy in use (`charges`).
pub fn charges(items: &ItemData, p: &mut Profile, loot: &str) -> i64 {
    let n = uses(items, p, loot);
    let r = right(items, loot);
    if n != UNLIMITED && n > 0 && r != 0 {
        (n - 1).rem_euclid(r) + 1
    } else {
        n
    }
}

/// Spends uses -> what is left (`use`).
pub fn use_extra(items: &ItemData, p: &mut Profile, loot: &str, n: i64) -> i64 {
    if unlimited(items, p, loot) {
        return UNLIMITED;
    }
    let left = uses(items, p, loot) - n;
    p.d["extra_uses"][loot] = json!(left.max(0));
    p.mark_dirty();
    left.max(0)
}

/// The ammo the Auto-Buy CPU has selected in the hangar (`attached`).
pub fn attached(p: &Profile, loot: &str) -> Option<Value> {
    p.d.get("auto_buy_ammo").and_then(|a| a.get(loot)).cloned()
}
