//! Completions of older profiles (`Profile._migrate`) and the hangar helpers they use (`sync_cargo`, `add_ores`,
//! `_add_item`, `sync_equipment`, `add_windows`).
//!
//! Every step follows OpenOrbit's order, including where a new key lands in a dict (a key added to a JSON object goes
//! last, a replaced one keeps its place), so a migrated profile is written byte for byte like OpenOrbit's.

use crate::{Profile, truthy};
use lo_core::settings::Settings;
use serde_json::{Map, Value, json};

/// Ores that take cargo space (`Profile.CARGO_ORES`): Prometium … Palladium and Osmium; Xenomit (3) does not.
pub const CARGO_ORES: [i64; 9] = [0, 1, 2, 4, 5, 6, 7, 8, 28];
/// The windows added next to the Return of Hero item (`NEW_WINDOWS`): item id, title, width, height.
pub const NEW_WINDOWS: [(&str, &str, i64, i64); 2] = [
    ("apotheosis", "title_kristallon_apotheosis", 728, 460),
    ("recallBattlePass", "return_pass_name", 800, 440),
];
/// The drone's design slots (`DESIGN_KEYS` values).
const DESIGN_KEYS: [&str; 2] = ["design", "appearance"];
const SLOTS: [&str; 5] = ["laser", "generators", "extra", "rocket", "upgrades"];

/// A Python number (int or float) with Python's arithmetic.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Num {
    I(i64),
    F(f64),
}

impl Num {
    fn of(v: &Value) -> Option<Num> {
        match v {
            Value::Bool(b) => Some(Num::I(i64::from(*b))),
            Value::Number(n) => n.as_i64().map(Num::I).or_else(|| n.as_f64().map(Num::F)),
            _ => None,
        }
    }
    fn f(self) -> f64 {
        match self {
            Num::I(i) => i as f64,
            Num::F(f) => f,
        }
    }
    fn add(self, o: Num) -> Num {
        match (self, o) {
            (Num::I(a), Num::I(b)) => Num::I(a + b),
            _ => Num::F(self.f() + o.f()),
        }
    }
    fn sub(self, o: Num) -> Num {
        match (self, o) {
            (Num::I(a), Num::I(b)) => Num::I(a - b),
            _ => Num::F(self.f() - o.f()),
        }
    }
    fn lt(self, o: Num) -> bool {
        match (self, o) {
            (Num::I(a), Num::I(b)) => a < b,
            _ => self.f() < o.f(),
        }
    }
    /// `min(self, o)`: the first one unless the second is smaller.
    fn min(self, o: Num) -> Num {
        if o.lt(self) { o } else { self }
    }
    /// `max(self, o)`.
    fn max(self, o: Num) -> Num {
        if self.lt(o) { o } else { self }
    }
    fn json(self) -> Value {
        match self {
            Num::I(i) => json!(i),
            Num::F(f) => json!(f),
        }
    }
}

fn num(v: Option<&Value>) -> Num {
    v.and_then(Num::of).unwrap_or(Num::I(0))
}

/// `str(v)` for ids kept in the profile.
fn py_str(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => match n.as_i64() {
            Some(i) => i.to_string(),
            None => lo_core::pyjson::float_repr(n.as_f64().unwrap_or(0.0)),
        },
        Value::Bool(b) => if *b { "True" } else { "False" }.into(),
        Value::Null => "None".into(),
        other => other.to_string(),
    }
}

/// `int(v)` of a level kept in the profile.
fn py_int(v: &Value) -> i64 {
    match v {
        Value::Bool(b) => i64::from(*b),
        Value::Number(n) => n
            .as_i64()
            .unwrap_or_else(|| n.as_f64().unwrap_or(0.0).trunc() as i64),
        Value::String(s) => s.trim().parse().unwrap_or(0),
        _ => 0,
    }
}

fn obj_mut<'a>(v: &'a mut Value, what: &str) -> &'a mut Map<String, Value> {
    if !v.is_object() {
        panic!("profile: {what} is not an object");
    }
    v.as_object_mut().unwrap()
}

/// `d.setdefault(k, default)` on an object.
fn setdefault<'a>(m: &'a mut Map<String, Value>, k: &str, default: Value) -> &'a mut Value {
    m.entry(k.to_string()).or_insert(default)
}

fn deep_get<'a>(v: &'a Value, path: &[&str]) -> Option<&'a Value> {
    path.iter().try_fold(v, |v, k| v.get(*k))
}

/// `x.get(k) or {}` style lookups: the value when truthy.
fn truthy_get<'a>(v: &'a Value, k: &str) -> Option<&'a Value> {
    v.get(k).filter(|x| truthy(x))
}

impl Profile {
    /// Adds the sections that newer versions keep to an older profile (`_migrate`). `seed`: the login template's seed.
    pub fn migrate(&mut self, seed: &Value, settings: &Settings) {
        let infinite = settings.switch("infinite_cargo");
        let mut dirty = false;
        let d = obj_mut(&mut self.d, "profile");

        if !d.contains_key("equipment") {
            let eq = truthy_get(seed, "equipment").cloned().unwrap_or_else(|| {
                json!({"laser": ["equipment_weapon_laser_lf-1"], "generators": [], "extra": [], "rocket": []})
            });
            d.insert("equipment".into(), eq);
            dirty = true;
        }
        if !d.get("ammo_seeded").is_some_and(truthy) {
            let mut base = truthy_get(seed, "ammunition")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_else(|| {
                    json!({"ammunition_laser_lcb-10": 1000, "ammunition_rocket_r-310": 20})
                        .as_object()
                        .cloned()
                        .unwrap()
                });
            if let Some(got) = d.get("ammunition").and_then(Value::as_object) {
                for (k, v) in got {
                    let n = num(base.get(k)).add(num(Some(v)));
                    base.insert(k.clone(), n.json());
                }
            }
            d.insert("ammunition".into(), Value::Object(base));
            d.insert("ammo_seeded".into(), json!(true));
            dirty = true;
        }
        let resources: Vec<String> = d
            .get("ammunition")
            .and_then(Value::as_object)
            .map(|a| {
                a.keys()
                    .filter(|k| k.starts_with("resource_"))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        for k in resources {
            // the World Editor's bonus box resources had been written to ammo: to storage
            let n = d
                .get_mut("ammunition")
                .and_then(Value::as_object_mut)
                .and_then(|a| a.shift_remove(&k))
                .unwrap_or(Value::Null);
            let n = num(Some(&n));
            if Num::I(0).lt(n) {
                let other = obj_mut(setdefault(d, "other_items", json!({})), "other_items");
                let total = num(other.get(&k)).add(n);
                other.insert(k, total.json());
            }
            dirty = true;
        }
        for (k, default) in [
            (
                "selected",
                json!({"laser": "ammunition_laser_lcb-10", "rocket": "ammunition_rocket_r-310"}),
            ),
            ("item_id", json!({})),
        ] {
            if !d.contains_key(k) {
                d.insert(k.into(), truthy_get(seed, k).cloned().unwrap_or(default));
                dirty = true;
            }
        }
        if !d.contains_key("configs") && !d.contains_key("ships") {
            if truthy_get(seed, "configs").is_some() && truthy_get(seed, "items").is_some() {
                d.insert("items".into(), seed["items"].clone());
                d.insert("configs".into(), seed["configs"].clone());
                d.insert(
                    "active_config".into(),
                    seed.get("active_config").cloned().unwrap_or(json!(1)),
                );
            } else {
                d.insert("items".into(), json!({}));
                let equipment = d
                    .get("equipment")
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default();
                let mut cfg = Map::new();
                for (slot, loots) in &equipment {
                    let ids: Vec<Value> = loots
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|loot| json!(add_item(d, loot.clone())))
                        .collect();
                    cfg.insert(slot.clone(), Value::Array(ids));
                }
                d.insert("configs".into(), json!({"1": cfg.clone(), "2": cfg}));
                d.insert("active_config".into(), json!(1));
            }
            dirty = true;
        }
        if !d.contains_key("next_id") {
            let next = max_item_id(d) + 1;
            d.insert("next_id".into(), json!(next));
            dirty = true;
        }
        if !d.contains_key("ships") {
            let k = truthy_get(seed, "ship_ids")
                .cloned()
                .unwrap_or(json!({"shipId": 1, "hangarId": 1}));
            let loot = d
                .get("ship")
                .and_then(|s| s.get("typeId"))
                .cloned()
                .unwrap_or(json!("ship_liberator"));
            let configs = d.shift_remove("configs").unwrap_or(Value::Null);
            let active = d.shift_remove("active_config").unwrap_or(json!(1));
            let sid = py_str(&k["shipId"]);
            let mut ships = Map::new();
            ships.insert(
                sid.clone(),
                json!({"lootId": loot, "hangarId": k["hangarId"].clone(), "configs": configs,
                       "active_config": active, "favorite": false}),
            );
            d.insert("ships".into(), Value::Object(ships));
            d.insert("active_ship".into(), json!(sid));
            dirty = true;
        }
        if !d.contains_key("drones") {
            d.insert("drones".into(), json!([]));
            d.insert("formations".into(), json!([]));
            d.insert("next_drone".into(), json!(1));
            dirty = true;
        }
        sync_cargo(d, infinite);
        if !d.contains_key("upgrades") {
            d.insert("upgrades".into(), json!({}));
            dirty = true;
        }
        if !d.contains_key("quests") {
            d.insert(
                "quests".into(),
                json!({"active": {"20000": {}}, "done": []}),
            );
            dirty = true;
        }
        for s in ships_mut(d) {
            let s = obj_mut(s, "ship");
            if !s.contains_key("drone_config") {
                let cfg: Map<String, Value> = s
                    .get("configs")
                    .and_then(Value::as_object)
                    .map(|c| c.keys().map(|k| (k.clone(), json!({}))).collect())
                    .unwrap_or_default();
                s.insert("drone_config".into(), Value::Object(cfg));
                dirty = true;
            }
        }

        // drone designs and P.E.T. equipment are tied to the ship (hangar): the old ones go to the active ship (the player's own
        // ship during an Astral run)
        let active = py_str(d.get("active_ship").unwrap_or(&Value::Null));
        let run = deep_get(&Value::Object(d.clone()), &["pve", "astral", "run"])
            .filter(|r| truthy(r))
            .cloned()
            .unwrap_or(json!({}));
        let inside = run.get("inside").is_some_and(truthy);
        let backup_active = truthy_get(&run, "backup")
            .and_then(|b| b.get("active"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|a| {
                d.get("ships")
                    .and_then(Value::as_object)
                    .is_some_and(|s| s.contains_key(a))
            });
        let mut own: Vec<(String, Vec<&str>)> = vec![(active.clone(), vec!["drones"])];
        if let (true, Some(b)) = (inside, backup_active) {
            own = vec![
                (b, vec!["pve", "astral", "run", "backup", "drones"]),
                (active.clone(), vec!["drones"]),
            ];
        }
        for (sid, path) in &own {
            let Some(mut drones) = deep_get(&Value::Object(d.clone()), path)
                .and_then(Value::as_array)
                .cloned()
            else {
                continue;
            };
            let mut changed = false;
            let mut sets: Vec<(String, String, &str, Value)> = Vec::new();
            for x in drones.iter_mut() {
                let Some(xm) = x.as_object_mut() else {
                    continue;
                };
                for k in DESIGN_KEYS {
                    let Some(v) = xm.shift_remove(k) else {
                        continue;
                    };
                    changed = true;
                    let per: Vec<(String, Value)> = if !truthy(&v) {
                        Vec::new()
                    } else if let Some(m) = v.as_object() {
                        m.iter().map(|(c, i)| (c.clone(), i.clone())).collect()
                    } else {
                        vec![("1".into(), v.clone()), ("2".into(), v.clone())]
                    };
                    let uid = py_str(xm.get("uid").unwrap_or(&Value::Null));
                    for (c, iid) in per {
                        if truthy(&iid) {
                            sets.push((c, uid.clone(), k, iid));
                        }
                    }
                }
            }
            if changed {
                dirty = true;
                let mut target = &mut *d;
                for k in &path[..path.len() - 1] {
                    target = obj_mut(target.get_mut(*k).unwrap(), k);
                }
                target.insert(path[path.len() - 1].to_string(), Value::Array(drones));
            }
            for (c, uid, k, iid) in sets {
                let ship = obj_mut(d["ships"].get_mut(sid.as_str()).expect("ship"), "ship");
                let dd = obj_mut(setdefault(ship, "drone_design", json!({})), "drone_design");
                let per_c = obj_mut(setdefault(dd, &c, json!({})), "drone_design config");
                let per_d = obj_mut(setdefault(per_c, &uid, json!({})), "drone_design drone");
                per_d.insert(k.into(), iid);
            }
        }
        let pet_cfg = d
            .get_mut("pet")
            .filter(|p| truthy(p))
            .and_then(Value::as_object_mut)
            .and_then(|p| p.shift_remove("config"));
        if let Some(cfg) = pet_cfg {
            let ship = obj_mut(d["ships"].get_mut(own[0].0.as_str()).expect("ship"), "ship");
            let pc = obj_mut(setdefault(ship, "pet_config", json!({})), "pet_config");
            for (k, v) in cfg.as_object().cloned().unwrap_or_default() {
                pc.insert(k, v);
            }
            dirty = true;
        }
        for s in ships_mut(d) {
            let s = obj_mut(s, "ship");
            let old = truthy_get(&Value::Object(s.clone()), "saved_drone_design")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let has_old = old.values().any(|per| {
                per.as_object()
                    .is_some_and(|p| p.keys().any(|k| DESIGN_KEYS.contains(&k.as_str())))
            });
            if has_old {
                let mut new = Map::new();
                for (dr, per) in &old {
                    for (k, cs) in per.as_object().into_iter().flatten() {
                        for (c, iid) in cs.as_object().into_iter().flatten() {
                            if truthy(iid) {
                                let pc = obj_mut(setdefault(&mut new, c, json!({})), "c");
                                let pd = obj_mut(setdefault(pc, dr, json!({})), "d");
                                pd.insert(k.clone(), iid.clone());
                            }
                        }
                    }
                }
                s.insert("saved_drone_design".into(), Value::Object(new));
                dirty = true;
            }
        }
        let osmium = d
            .get_mut("other_items")
            .and_then(Value::as_object_mut)
            .and_then(|o| o.shift_remove("resource_ore_osmium"))
            .unwrap_or(json!(0));
        if truthy(&osmium) {
            add_ores_in(d, &[(28, num(Some(&osmium)))], infinite);
            dirty = true;
        }
        if let Some(q) = d
            .get_mut("settings")
            .and_then(|s| s.get_mut("qualitySettingsModule"))
            .filter(|q| truthy(q))
            .and_then(Value::as_object_mut)
        {
            if Num::I(255).lt(num(q.get("qualityExplosion"))) {
                // the client's byte-swapped value (3 saved as 768)
                let v = py_int(&q["qualityExplosion"]);
                q.insert(
                    "qualityExplosion".into(),
                    json!((v >> 8 & 0xFF) | (v << 8 & 0xFF00)),
                );
                dirty = true;
            }
            let four = [
                "qualityShip",
                "qualityEngine",
                "qualityExplosion",
                "qualityCollectables",
            ];
            if !q.get("notSet").is_some_and(truthy)
                && q.get("qualityPresetting")
                    .and_then(Num::of)
                    .is_some_and(|n| n.f() == 3.0)
                && !four.iter().any(|k| q.get(*k).is_some_and(truthy))
            {
                // the client's own default (preset 3, the four at 0) saved by pressing Save: repaired
                for k in four {
                    q.insert(k.into(), json!(3));
                }
                dirty = true;
            }
        }
        if let Some(ship) = d.get_mut("ship").and_then(Value::as_object_mut)
            && ship
                .shift_remove("galaxyGatesDone")
                .is_some_and(|v| !v.is_null())
        {
            dirty = true;
        }
        dirty |= add_windows(d);
        sync_equipment(d);
        if dirty {
            self.mark_dirty();
        }
    }

    /// Adds `{ore type: count}` (negative removes) -> the ore list (`add_ores`).
    pub fn add_ores(&mut self, ores: &[(i64, i64)], settings: &Settings) -> Value {
        let ores: Vec<(i64, Num)> = ores.iter().map(|&(t, n)| (t, Num::I(n))).collect();
        let d = obj_mut(&mut self.d, "profile");
        add_ores_in(d, &ores, settings.switch("infinite_cargo"));
        self.mark_dirty();
        self.d["ores"].clone()
    }

    /// Ores that take cargo space (`cargo_used`).
    pub fn cargo_used(&self) -> i64 {
        cargo_used(self.d.as_object().expect("profile")).f() as i64
    }

    /// `ShipInitializationCommand.cargoSpace` = free cargo space (`sync_cargo`).
    pub fn sync_cargo(&mut self, settings: &Settings) {
        sync_cargo(
            obj_mut(&mut self.d, "profile"),
            settings.switch("infinite_cargo"),
        );
    }

    /// The active ship's equipment lists from its active config (`sync_equipment`).
    pub fn sync_equipment(&mut self) {
        sync_equipment(obj_mut(&mut self.d, "profile"));
    }
}

fn ships_mut(d: &mut Map<String, Value>) -> impl Iterator<Item = &mut Value> {
    d.get_mut("ships")
        .and_then(Value::as_object_mut)
        .into_iter()
        .flat_map(|s| s.values_mut())
}

fn max_item_id(d: &Map<String, Value>) -> i64 {
    d.get("items")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter_map(|(k, _)| k.trim().parse::<i64>().ok())
        .chain([99_000_000])
        .max()
        .unwrap_or(99_000_000)
}

/// A new equipment instance -> its item id (`_add_item`).
fn add_item(d: &mut Map<String, Value>, loot: Value) -> i64 {
    let iid = match d.get("next_id").filter(|v| truthy(v)).map(py_int) {
        Some(i) => i,
        None => max_item_id(d) + 1,
    };
    d.insert("next_id".into(), json!(iid + 1));
    obj_mut(d.get_mut("items").expect("items"), "items").insert(iid.to_string(), loot);
    iid
}

fn ore_type(m: &Value) -> Option<&Value> {
    m.get("oreType").and_then(|o| o.get("typeValue"))
}

fn cargo_used(d: &Map<String, Value>) -> Num {
    d.get("ores")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|m| {
            ore_type(m)
                .and_then(Num::of)
                .is_some_and(|t| CARGO_ORES.iter().any(|c| (*c as f64) == t.f()))
        })
        .fold(Num::I(0), |a, m| a.add(num(m.get("count"))))
}

fn sync_cargo(d: &mut Map<String, Value>, infinite: bool) {
    let max = num(d.get("ship").and_then(|s| s.get("cargoSpaceMax")));
    let free = if infinite {
        Num::I(1_000_000)
    } else {
        Num::I(0).max(max.sub(cargo_used(d)))
    };
    let v = free.min(max).json();
    obj_mut(d.get_mut("ship").expect("ship"), "ship").insert("cargoSpace".into(), v);
}

fn add_ores_in(d: &mut Map<String, Value>, ores: &[(i64, Num)], infinite: bool) {
    // {type: count} in the list's order, a later duplicate replacing the count
    let mut cur: Vec<(Num, Num)> = Vec::new();
    for m in d
        .get("ores")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let t = ore_type(m).and_then(Num::of).unwrap_or(Num::I(0));
        let n = num(m.get("count"));
        match cur.iter_mut().find(|(x, _)| x.f() == t.f()) {
            Some(slot) => slot.1 = n,
            None => cur.push((t, n)),
        }
    }
    for &(t, n) in ores {
        match cur.iter_mut().find(|(x, _)| x.f() == t as f64) {
            Some(slot) => slot.1 = slot.1.add(n),
            None => cur.push((Num::I(t), n)),
        }
    }
    cur.sort_by(|a, b| a.0.f().total_cmp(&b.0.f()));
    let list: Vec<Value> = cur
        .into_iter()
        .filter(|(_, n)| Num::I(0).lt(*n))
        .map(|(t, n)| {
            json!({"_": "OreCountModule", "oreType": {"_": "OreTypeModule", "typeValue": t.json()}, "count": n.json()})
        })
        .collect();
    d.insert("ores".into(), Value::Array(list));
    sync_cargo(d, infinite);
}

/// The window items added next to the Return of Hero item (`add_windows`) -> whether something was added.
fn add_windows(d: &mut Map<String, Value>) -> bool {
    let mut added = false;
    let n_bars = d
        .get("ui")
        .and_then(|u| u.get("featureBars"))
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    for b in 0..n_bars {
        let ui = &d["ui"]["featureBars"];
        let Some(items) = ui[b].get("featureBarItems").and_then(Value::as_array) else {
            continue;
        };
        let Some(pos) = items
            .iter()
            .position(|it| it.get("itemId").and_then(Value::as_str) == Some("return_of_hero"))
        else {
            continue;
        };
        let reference = items[pos].clone();
        let have: Vec<Value> = ui
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|bar| {
                bar.get("featureBarItems")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .map(|it| it.get("itemId").cloned().unwrap_or(Value::Null))
            })
            .collect();
        for (id, title, w, h) in NEW_WINDOWS {
            if have.iter().any(|x| x == id) {
                continue;
            }
            let mut it = reference.clone();
            if let Some(m) = it.as_object_mut() {
                m.insert("itemId".into(), json!(id));
                m.insert("windowTitle".into(), json!(title));
                m.insert("width".into(), json!(w));
                m.insert("height".into(), json!(h));
                m.insert("maximized".into(), json!(false));
                m.insert("visible".into(), json!(false));
            }
            for m in it
                .get_mut("toolTip")
                .and_then(|t| t.get_mut("commandList"))
                .and_then(Value::as_array_mut)
                .into_iter()
                .flatten()
            {
                if let Some(m) = m.as_object_mut() {
                    m.insert("baseKey".into(), json!(title));
                }
            }
            d["ui"]["featureBars"][b]["featureBarItems"]
                .as_array_mut()
                .unwrap()
                .insert(pos + 1, it);
            added = true;
        }
    }
    added
}

/// `config()` / `drone_config()`: the active ship's record for its active config, created if missing.
fn active_cfg(d: &mut Map<String, Value>, section: &str) -> Map<String, Value> {
    let sid = py_str(d.get("active_ship").unwrap_or(&Value::Null));
    let ship = obj_mut(
        d.get_mut("ships")
            .and_then(|s| s.get_mut(sid.as_str()))
            .expect("active ship"),
        "ship",
    );
    let cid = py_str(ship.get("active_config").unwrap_or(&Value::Null));
    let sec = obj_mut(setdefault(ship, section, json!({})), section);
    obj_mut(setdefault(sec, &cid, json!({})), "config").clone()
}

fn items_of(cfg: &Map<String, Value>, key: &str, items: &Map<String, Value>) -> Vec<String> {
    cfg.get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(py_str)
        .filter(|i| items.contains_key(i))
        .collect()
}

fn sync_equipment(d: &mut Map<String, Value>) {
    let cfg = active_cfg(d, "configs");
    let items = d
        .get("items")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let loot = |ids: Vec<String>| -> Value {
        Value::Array(ids.iter().map(|i| items[i].clone()).collect())
    };
    let equipment: Map<String, Value> = SLOTS
        .iter()
        .map(|s| (s.to_string(), loot(items_of(&cfg, s, &items))))
        .collect();
    let dcfg = active_cfg(d, "drone_config");
    let drones = d
        .get("drones")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let uid = |x: &Value| py_str(x.get("uid").unwrap_or(&Value::Null));
    let drone_equipment: Vec<Value> = drones
        .iter()
        .map(|x| {
            json!([
                x.get("level").cloned().unwrap_or(Value::Null),
                loot(items_of(&dcfg, &uid(x), &items))
            ])
        })
        .collect();
    let lv = d
        .get("upgrade_level")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let levels = |ids: Vec<String>| -> Value {
        Value::Array(
            ids.iter()
                .map(|i| json!(lv.get(i).map_or(0, py_int)))
                .collect(),
        )
    };
    let equipment_level: Map<String, Value> = SLOTS[..4]
        .iter()
        .map(|s| (s.to_string(), levels(items_of(&cfg, s, &items))))
        .collect();
    let drone_levels: Vec<Value> = drones
        .iter()
        .map(|x| {
            json!([
                x.get("lootId").cloned().unwrap_or(Value::Null),
                x.get("upgrade").map_or(0, py_int),
                levels(items_of(&dcfg, &uid(x), &items))
            ])
        })
        .collect();
    d.insert("equipment".into(), Value::Object(equipment));
    d.insert("drone_equipment".into(), Value::Array(drone_equipment));
    d.insert("equipment_level".into(), Value::Object(equipment_level));
    d.insert("drone_equipment_level".into(), Value::Array(drone_levels));
}
