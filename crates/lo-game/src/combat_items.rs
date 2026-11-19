//! Combat items and active CPUs (OpenOrbit's `combat_items.py`, data `items.json` `combat_items`, `rockets`, `extras`).
//! Ported so far: the profile side used by the login and the shortcut bar (item classes, cooldown groups and timers, the
//! fitted CPUs, the launcher's load, the shortcut bar state of mines, Smart Bomb, Insta-Shield, jump, advanced jump,
//! cloak, trade drone, repair robot, on/off CPUs and the rocket launcher). The items in the world come with the map
//! simulation.

use crate::extras;
use crate::items::ItemData;
use crate::map_links::{MapLinks, SPECIAL};
use crate::technology::Timer;
use lo_core::settings::truthy;
use lo_save::Profile;
use lo_save::hangar::{num, py_str};
use serde_json::{Map, Value, json};

pub const EMOTE_PREFIX: &str = "ammunition_specialammo_emote_";
pub const SPRAY_PREFIX: &str = "ammunition_specialammo_spray_";
pub const SPRAY_GROUP: i64 = 57;
pub const SPRAY_CD: i64 = 45;
pub const XENOMIT: i64 = 3;
pub const REPBOT_MENU: &str = "equipment_extra_repbot_rep";
pub const HM7: &str = "equipment_extra_hmd-07";
/// The profile key of the cooldown groups: `{group: [end, max ms]}`.
pub const TIMERS: &str = "cooldown_groups";
/// Curcubitor's Trick mine: born when the event alien dies, never placed by the player.
pub const EVENT_MINES: [&str; 1] = ["curcubitor_mayini"];
/// Dot colour of the launcher item by loaded rocket (`LAUNCHER_COLOR`).
pub const LAUNCHER_COLOR: [(&str, i64); 9] = [
    ("ammunition_rocketlauncher_eco-10", 4),
    ("ammunition_rocketlauncher_hstrm-01", 3),
    ("ammunition_rocketlauncher_ubr-100", 1),
    ("ammunition_rocketlauncher_bdr1212", 5),
    ("ammunition_rocketlauncher_sar-01", 5),
    ("ammunition_rocketlauncher_sar-02", 5),
    ("ammunition_rocketlauncher_pir-100", 5),
    ("ammunition_rocketlauncher_cbr", 4),
    ("ammunition_rocketlauncher_obr-100", 0),
];
/// Toggles off by default (`TOGGLE_DEFAULT`): the aiming CPU spends Xenomit.
const TOGGLE_DEFAULT: [(&str, bool); 1] = [("aim", false)];

/// The type of the selected ammo: laser, launcher, rocket, slug, gsp or `None` (`ammo_kind`).
pub fn ammo_kind(items: &ItemData, loot: &str) -> Option<&'static str> {
    if loot.starts_with("ammunition_laser_") {
        Some("laser")
    } else if loot.starts_with("ammunition_rocketlauncher_") {
        Some("launcher")
    } else if loot.starts_with("ammunition_rocket_") || items.e["rockets"].get(loot).is_some() {
        Some("rocket")
    } else if loot.starts_with("ammunition_slug_") {
        Some("slug")
    } else if loot.starts_with("ammunition_spool_") {
        Some("gsp")
    } else {
        None
    }
}

fn mines(items: &ItemData) -> &Map<String, Value> {
    items.s()["mines"].as_object().expect("mines")
}

fn player_mine(items: &ItemData, loot: &str) -> bool {
    mines(items).contains_key(loot) && !EVENT_MINES.contains(&loot)
}

fn pair(v: &Value, g: &str, c: &str) -> (Value, Value) {
    (
        v.get(g).cloned().unwrap_or(Value::Null),
        v.get(c).cloned().unwrap_or(Value::Null),
    )
}

/// The shortcut item's cooldown group and duration in seconds (`item_cooldown`).
pub fn item_cooldown(items: &ItemData, loot: &str) -> Option<(Value, Value)> {
    let s = items.s();
    if loot.starts_with(EMOTE_PREFIX) {
        return Some(pair(&items.o6["phrases"], "cooldown_group", "cooldown_sec"));
    }
    if loot.starts_with(SPRAY_PREFIX) {
        return Some((json!(SPRAY_GROUP), json!(SPRAY_CD)));
    }
    if let Some(m) = mines(items).get(loot) {
        let g = m
            .get("cooldown_group")
            .cloned()
            .unwrap_or_else(|| s["mine"]["cooldown_group"].clone());
        let c = m
            .get("cooldown_sec")
            .cloned()
            .unwrap_or_else(|| s["mine"]["cooldown_sec"].clone());
        return Some((g, c));
    }
    if let Some(c) = items.alien_cpus.get(loot) {
        return Some(pair(c, "cooldown_group", "cooldown_sec"));
    }
    if items.e["formations"].get(loot).is_some() {
        return Some(pair(&s["formation"], "cooldown_group", "cooldown_sec"));
    }
    for key in ["smart_bomb", "insta_shield", "emp", "ghost"] {
        if s[key]["menu"].as_str() == Some(loot) {
            return Some(pair(&s[key], "cooldown_group", "cooldown_sec"));
        }
    }
    let r = &items.e["rockets"][loot];
    if r.get("special").is_some_and(truthy) {
        return Some(pair(r, "cooldown_group", "cooldown_sec"));
    }
    None
}

/// The end of a cooldown group (`ready_at`).
pub fn ready_at(p: &Profile, group: &str) -> f64 {
    p.d.get(TIMERS)
        .and_then(|t| t.get(group))
        .filter(|v| truthy(v))
        .and_then(|v| num(&v[0]))
        .unwrap_or(0.0)
}

/// The shortcut item's ongoing cooldown at login and on map change (`timer_fields`).
pub fn timer_fields(items: &ItemData, p: &Profile, loot: &str, now: f64) -> Option<Timer> {
    let (group, _) = item_cooldown(items, loot)?;
    let t =
        p.d.get(TIMERS)
            .and_then(|t| t.get(py_str(&group)))
            .filter(|v| truthy(v))
            .cloned()
            .unwrap_or(json!([0, 0]));
    let until = num(&t[0]).unwrap_or(0.0);
    if until <= now {
        return None;
    }
    Some(Timer {
        activatable: false,
        state: 2,
        max_time: t[1].clone(),
        time: ((until - now) * 1000.0) as i64,
    })
}

fn first(items: &ItemData, p: &mut Profile, key: &str) -> Option<String> {
    extras::props(items, p, key, None)
        .into_iter()
        .next()
        .map(|(l, _)| l)
}

/// The fitted jump CPU: the first with uses left, else the first (`jump_loot`).
pub fn jump_loot(items: &ItemData, p: &mut Profile) -> Option<String> {
    let x = items.extras();
    let cpus: Vec<String> = extras::props(items, p, "jump", None)
        .into_iter()
        .map(|(l, _)| l)
        .filter(|l| x[l.as_str()].get("right").is_some())
        .collect();
    for l in &cpus {
        if extras::uses(items, p, l) > 0 {
            return Some(l.clone());
        }
    }
    cpus.first().cloned()
}

pub fn has_ajp(items: &ItemData, p: &mut Profile) -> bool {
    extras::has(items, p, "advanced_jump", None)
}

/// The fitted cloak CPU: the first with uses left, else the first (`cloak_loot`).
pub fn cloak_loot(items: &ItemData, p: &mut Profile) -> Option<String> {
    let cpus: Vec<String> = extras::props(items, p, "cloak", None)
        .into_iter()
        .map(|(l, _)| l)
        .collect();
    for l in &cpus {
        if extras::uses(items, p, l) > 0 {
            return Some(l.clone());
        }
    }
    cpus.first().cloned()
}

/// Whether an on/off CPU is on (`toggled`).
pub fn toggled(items: &ItemData, p: &mut Profile, item: &str) -> Value {
    let Some(key) = items.s()["on_off"].get(item).and_then(Value::as_str) else {
        return json!(false);
    };
    if !extras::has(items, p, key, None) {
        return json!(false);
    }
    let default = TOGGLE_DEFAULT
        .iter()
        .find(|(k, _)| *k == key)
        .is_none_or(|(_, v)| *v);
    p.d.get("cpu_on")
        .and_then(|c| c.get(item))
        .cloned()
        .unwrap_or(json!(default))
}

/// The company's X-1 (`home_map`).
pub fn home_map(items: &ItemData, p: &Profile) -> Value {
    let f = py_str(p.d["pilot"].get("factionId").unwrap_or(&Value::Null));
    items.s()["jump_cpu"]["main_map"]
        .get(&f)
        .cloned()
        .unwrap_or(json!(1))
}

/// Whether the jump CPU works from this map (`jump_allowed`).
pub fn jump_allowed(items: &ItemData, maps: &MapLinks, loot: &str, map_id: i64) -> bool {
    if map_id == 42 {
        return true;
    }
    if !maps.names.contains_key(&map_id) || SPECIAL.contains(&map_id) {
        return false;
    }
    let cfg = items
        .extras()
        .get(loot)
        .and_then(|e| e.get("jump"))
        .cloned()
        .unwrap_or(json!({}));
    if let Some(ms) = cfg.get("maps") {
        let c = maps.map_class(map_id);
        return ms
            .as_array()
            .is_some_and(|a| a.iter().any(|m| m.as_str() == Some(c.as_str())));
    }
    let n = maps.name(map_id);
    !cfg.get("ban")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .any(|s| n.starts_with(s))
}

/// A Galaxy Gate map (`gate_map`).
pub fn gate_map(items: &ItemData, map_id: i64) -> bool {
    items.gate_maps.contains_key(&map_id)
}

fn launcher_kinds(items: &ItemData) -> &Map<String, Value> {
    items.s()["launcher"]["kinds"]
        .as_object()
        .expect("launcher kinds")
}

/// The fitted rocket launchers (`launchers`).
pub fn launchers(items: &ItemData, p: &Profile) -> Vec<String> {
    p.d.get("equipment")
        .and_then(|e| e.get("rocket"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|l| launcher_kinds(items).contains_key(*l))
        .map(str::to_string)
        .collect()
}

/// The launchers' total capacity (`capacity`).
pub fn capacity(items: &ItemData, p: &Profile) -> f64 {
    launchers(items, p)
        .iter()
        .map(|l| num(&launcher_kinds(items)[l.as_str()]["capacity"]).unwrap_or(0.0))
        .sum()
}

/// The rockets in the launcher: (ammo, count) (`loaded`).
pub fn loaded(p: &Profile) -> (Value, f64) {
    match p.d.get("launcher_loaded").filter(|v| truthy(v)) {
        Some(v) => (v[0].clone(), num(&v[1]).unwrap_or(0.0)),
        None => (Value::Null, 0.0),
    }
}

/// Smart Bomb / Insta-Shield uses with the mines and Xenomit at hand (`mine_uses`).
pub fn mine_uses(p: &Profile, cfg: &Value) -> f64 {
    let a = (p.ammo(cfg["mine"].as_str().unwrap_or_default())
        / num(&cfg["mine_count"]).unwrap_or(1.0))
    .floor();
    let b = (p.ore_count(XENOMIT) / num(&cfg["xenomit"]).unwrap_or(1.0)).floor();
    a.min(b)
}

/// A tooltip line (`_line`).
pub fn line(key: &str, reps: &[(&str, Value, i64)], ktype: i64) -> Value {
    let w: Vec<Value> = reps
        .iter()
        .map(|(w, r, t)| {
            json!({"_": "MessageWildcardReplacementModule", "wildcard": w, "replacement": py_str(r),
                   "replacementType": {"_": "ReplacementTypeModule", "typeValue": t}})
        })
        .collect();
    json!({"_": "MessageKeyListWildcardModule", "baseKey": key, "baseKeyType": {"_": "ReplacementTypeModule", "typeValue": ktype},
           "textStyle": 0, "wildCardReplacements": w})
}

/// Both tooltips set to these lines (`_tips`).
pub fn tips(st: &mut Map<String, Value>, lines: &[Value]) {
    for k in ["toolTipSlotBar", "toolTipItemBar"] {
        st.insert(
            k.into(),
            json!({"_": "MessageKeyListWildcardCommand", "commandList": lines}),
        );
    }
}

fn explosive(st: &mut Map<String, Value>, loot: &str, n: Value) {
    tips(
        st,
        &[
            line("ttip_explosive", &[("%TYPE%", json!(loot), 1)], 5),
            line("ttip_count", &[("%COUNT%", n, 0)], 5),
            line(loot, &[], 7),
        ],
    );
}

fn drop_not_equipped(st: &mut Map<String, Value>) {
    for k in ["toolTipSlotBar", "toolTipItemBar"] {
        if let Some(lines) = st
            .get_mut(k)
            .filter(|t| truthy(t))
            .and_then(|t| t.get_mut("commandList"))
            .and_then(Value::as_array_mut)
            .filter(|l| !l.is_empty())
        {
            lines.retain(|l| {
                l.get("baseKey").and_then(Value::as_str) != Some("ttip_cpu_not_equipped")
            });
        }
    }
}

fn set(st: &mut Map<String, Value>, kv: &[(&str, Value)]) {
    for (k, v) in kv {
        st.insert(k.to_string(), v.clone());
    }
}

/// Python's `==` of two JSON values (numbers by value).
fn py_eq(a: &Value, b: &Value) -> bool {
    match (num(a), num(b)) {
        (Some(x), Some(y)) => x == y,
        _ => a == b,
    }
}

/// A count as Python keeps it: an integer when whole.
fn n_json(n: f64) -> Value {
    if n.fract() == 0.0 && n.abs() < 9e15 {
        json!(n as i64)
    } else {
        json!(n)
    }
}

/// The state of this module's shortcut items (`menu_status`); `None` for another item.
pub fn menu_status(
    items: &ItemData,
    maps: &MapLinks,
    p: &mut Profile,
    menu_id: &str,
    mut st: Map<String, Value>,
) -> Option<Map<String, Value>> {
    let s = items.s().clone();
    let here = p.d["position"]
        .get("mapId")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let start_map = maps.map_class(here) == "X-1";
    if player_mine(items, menu_id) {
        let n = p.ammo(menu_id);
        let vis = st.get("visible").is_some_and(truthy) || n > 0.0;
        let vis = if st.get("visible").is_some_and(truthy) {
            st["visible"].clone()
        } else {
            json!(vis)
        };
        set(
            &mut st,
            &[
                (
                    "counterValue",
                    p.d["ammunition"].get(menu_id).cloned().unwrap_or(json!(0)),
                ),
                ("counterStyle", json!(if n > 0.0 { 4 } else { 1 })),
                ("visible", vis),
                ("available", json!(n > 0.0 && !start_map)),
                ("activatable", json!(n > 0.0 && !start_map)),
            ],
        );
        if !start_map {
            let nv = st["counterValue"].clone();
            explosive(&mut st, menu_id, nv);
        }
        return Some(st);
    }
    let smb_menu = s["smart_bomb"]["menu"].as_str().unwrap_or_default();
    let ish_menu = s["insta_shield"]["menu"].as_str().unwrap_or_default();
    if menu_id == smb_menu || menu_id == ish_menu {
        let smb = menu_id == smb_menu;
        let key = if smb { "smart_bomb" } else { "insta_shield" };
        let cfg = &s[key];
        let owned = extras::has(items, p, key, None);
        let n = if owned { mine_uses(p, cfg) } else { 0.0 };
        let in_gate = smb && gate_map(items, here);
        let blocked = smb && (start_map || in_gate);
        let ok = owned && n > 0.0 && !blocked;
        set(
            &mut st,
            &[
                ("visible", json!(owned)),
                ("counterValue", n_json(n)),
                ("available", json!(ok)),
                ("activatable", json!(ok)),
            ],
        );
        if owned && !blocked {
            explosive(&mut st, menu_id, n_json(n));
        } else if owned && in_gate {
            tips(&mut st, &[line("ttip_inactive_smb_galaxy_gate", &[], 5)]);
        }
        return Some(st);
    }
    if Some(menu_id) == s["jump_cpu"]["menu"].as_str() {
        let Some(loot) = jump_loot(items, p) else {
            set(
                &mut st,
                &[
                    ("visible", json!(false)),
                    ("available", json!(false)),
                    ("activatable", json!(false)),
                    ("counterValue", json!(0)),
                ],
            );
            return Some(st);
        };
        let n = extras::charges(items, p, &loot);
        let here_v = p.d["position"].get("mapId").cloned().unwrap_or(json!(0));
        let (lines, ok) = if py_eq(&here_v, &home_map(items, p)) {
            (
                vec![line("ttip_inactive_jump_already_on_map", &[], 5)],
                false,
            )
        } else if !jump_allowed(items, maps, &loot, here) {
            (
                vec![line("ttip_inactive_jump_map_range_exceeded", &[], 5)],
                false,
            )
        } else {
            (
                vec![
                    line("ttip_jump_cpu", &[("%TYPE%", json!(loot), 1)], 5),
                    line("ttip_cpu_jump_count", &[("%COUNT%", json!(n), 0)], 5),
                    line("ttip_cpu_jump_desc", &[], 5),
                ],
                n > 0,
            )
        };
        tips(&mut st, &lines);
        set(
            &mut st,
            &[
                ("visible", json!(true)),
                ("available", json!(true)),
                ("activatable", json!(ok)),
                ("counterValue", json!(n)),
                ("iconLootId", json!(loot)),
            ],
        );
        return Some(st);
    }
    if Some(menu_id) == s["advanced_jump"]["menu"].as_str() {
        let owned = has_ajp(items, p);
        set(
            &mut st,
            &[
                ("visible", json!(owned)),
                ("available", json!(owned)),
                ("activatable", json!(owned)),
            ],
        );
        if owned {
            drop_not_equipped(&mut st);
        }
        return Some(st);
    }
    if Some(menu_id) == s["cloak_cpu"]["menu"].as_str() {
        let Some(loot) = cloak_loot(items, p) else {
            set(
                &mut st,
                &[
                    ("visible", json!(false)),
                    ("available", json!(false)),
                    ("activatable", json!(false)),
                    ("selected", json!(false)),
                    ("counterValue", json!(0)),
                ],
            );
            return Some(st);
        };
        let n = extras::charges(items, p, &loot);
        let on = p.d.get("cpu_hidden").is_some_and(truthy);
        tips(
            &mut st,
            &[
                line("ttip_cloak_cpu", &[("%TYPE%", json!(loot), 1)], 5),
                line("ttip_cpu_cloak_count", &[("%COUNT%", json!(n), 0)], 5),
            ],
        );
        set(
            &mut st,
            &[
                ("visible", json!(true)),
                ("available", json!(true)),
                ("activatable", json!(n > 0 && !on)),
                ("selected", json!(on)),
                ("counterValue", json!(n)),
                ("iconLootId", json!(loot)),
            ],
        );
        return Some(st);
    }
    if menu_id == HM7 {
        let owned = extras::owned(p, HM7);
        let ok = owned && extras::uses(items, p, HM7) > 0;
        set(
            &mut st,
            &[
                ("visible", json!(owned)),
                ("available", json!(ok)),
                ("activatable", json!(ok)),
            ],
        );
        if owned && !extras::unlimited(items, p, HM7) {
            let n = extras::charges(items, p, HM7);
            set(
                &mut st,
                &[
                    ("counterValue", json!(n)),
                    ("counterStyle", json!(if n > 0 { 4 } else { 1 })),
                ],
            );
        }
        if owned {
            drop_not_equipped(&mut st);
        }
        return Some(st);
    }
    if menu_id == REPBOT_MENU {
        let robots = items.e["repair_robots"].clone();
        let is_robot = |e: &str| match &robots {
            Value::Object(m) => m.contains_key(e),
            Value::Array(a) => a.iter().any(|x| x.as_str() == Some(e)),
            _ => false,
        };
        let loot = extras::equipped(p, None).into_iter().find(|e| is_robot(e));
        match loot {
            Some(loot) => {
                tips(
                    &mut st,
                    &[line("ttip_robot_cpu", &[("%TYPE%", json!(loot), 1)], 5)],
                );
                set(
                    &mut st,
                    &[
                        ("visible", json!(true)),
                        ("available", json!(true)),
                        ("activatable", json!(true)),
                        ("iconLootId", json!(loot)),
                    ],
                );
            }
            None => {
                tips(&mut st, &[line("ttip_cpu_not_equipped", &[], 5)]);
                set(
                    &mut st,
                    &[
                        ("visible", json!(true)),
                        ("available", json!(false)),
                        ("activatable", json!(false)),
                        ("selected", json!(false)),
                    ],
                );
            }
        }
        return Some(st);
    }
    if let Some(key) = s["on_off"].get(menu_id).and_then(Value::as_str) {
        let loot = first(items, p, key);
        let has = loot.is_some();
        let sel = toggled(items, p, menu_id);
        set(
            &mut st,
            &[
                ("visible", json!(has)),
                ("available", json!(has)),
                ("activatable", json!(has)),
                ("selected", sel),
            ],
        );
        if let Some(loot) = loot {
            if Some(menu_id) == s["aim_cpu"]["menu"].as_str() {
                tips(
                    &mut st,
                    &[line("ttip_aim_cpu_category", &[], 5), line(&loot, &[], 7)],
                );
                let n =
                    (p.ore_count(XENOMIT) / num(&s["aim_cpu"]["xenomit"]).unwrap_or(1.0)).floor();
                set(
                    &mut st,
                    &[
                        ("iconLootId", json!(loot)),
                        ("counterValue", n_json(n)),
                        ("counterStyle", json!(if n > 0.0 { 4 } else { 1 })),
                    ],
                );
            }
            drop_not_equipped(&mut st);
        }
        return Some(st);
    }
    if Some(menu_id) == s["launcher"]["menu"].as_str() {
        let lns = launchers(items, p);
        if lns.is_empty() {
            return Some(st);
        }
        let (ammo, n) = loaded(p);
        let cap = capacity(items, p);
        let sel =
            p.d.get("selected")
                .and_then(|s| s.get("launcher"))
                .cloned()
                .unwrap_or(Value::Null);
        let mut lines = vec![line(
            "ttip_rocketlauncher",
            &[("%TYPE%", json!(menu_id), 5)],
            5,
        )];
        let nv =
            p.d.get("launcher_loaded")
                .filter(|v| truthy(v))
                .map_or(json!(0), |v| v[1].clone());
        let rocket = if n != 0.0 { ammo } else { sel.clone() };
        if truthy(&rocket) {
            lines.push(line(
                "ttip_rocketlauncher_loadcount",
                &[("%COUNT%", nv.clone(), 0), ("%TYPE%", rocket.clone(), 1)],
                5,
            ));
        }
        tips(&mut st, &lines);
        let icon = if truthy(&rocket) {
            rocket.clone()
        } else {
            json!(lns[0])
        };
        let style = rocket
            .as_str()
            .and_then(|r| LAUNCHER_COLOR.iter().find(|(k, _)| *k == r))
            .map_or(1, |(_, c)| *c);
        set(
            &mut st,
            &[
                ("visible", json!(true)),
                ("available", json!(n > 0.0)),
                ("activatable", json!(n > 0.0)),
                ("counterValue", nv),
                ("maxCounterValue", n_json(cap)),
                ("iconLootId", icon),
                ("counterStyle", json!(style)),
                ("selected", json!(false)),
            ],
        );
        if sel.is_null() {
            st.insert("available".into(), json!(false));
            st.insert("activatable".into(), json!(false));
        }
        return Some(st);
    }
    None
}

/// `0|A|JV|n`: the advanced jump coupons (`vouchers_msg`).
pub fn vouchers_msg(items: &ItemData, p: &Profile) -> (&'static str, Value) {
    let v = p.resource(
        items.s()["advanced_jump"]["voucher"]
            .as_str()
            .unwrap_or_default(),
    );
    items.legacy(&format!("0|A|JV|{}", v as i64))
}
