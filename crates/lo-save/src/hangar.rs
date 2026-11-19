//! Hangar and inventory reads of the profile (`Profile.ship`, `config`, `loadout`, `drone_config`, `drone_loadout`, `ammo`,
//! `ore_count`, `resource`). Like OpenOrbit, reading a ship's config creates an empty record for it when it is missing.

use crate::Profile;
use serde_json::{Map, Value, json};

/// The ship slots of a config (`loadout`).
pub const SLOTS: [&str; 5] = ["laser", "generators", "extra", "rocket", "upgrades"];

/// `str(v)` of an id kept in the profile.
pub fn py_str(v: &Value) -> String {
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

/// A number as Python compares it (`None` for other values).
pub fn num(v: &Value) -> Option<f64> {
    match v {
        Value::Bool(b) => Some(f64::from(u8::from(*b))),
        Value::Number(n) => n.as_f64(),
        _ => None,
    }
}

impl Profile {
    /// The ship id (text): the given one, or the active ship when it is missing or falsy (`sid or active_ship`).
    pub fn ship_id(&self, sid: Option<&str>) -> String {
        match sid.filter(|s| !s.is_empty()) {
            Some(s) => s.to_string(),
            None => py_str(&self.d["active_ship"]),
        }
    }

    /// The ship record (`ship`).
    pub fn ship(&self, sid: Option<&str>) -> &Value {
        &self.d["ships"][self.ship_id(sid).as_str()]
    }

    fn ship_mut(&mut self, sid: Option<&str>) -> &mut Map<String, Value> {
        let id = self.ship_id(sid);
        self.d["ships"][id.as_str()]
            .as_object_mut()
            .expect("ship record")
    }

    /// The config id (text): the given one or the ship's active config.
    pub fn config_id(&self, cid: Option<&str>, sid: Option<&str>) -> String {
        match cid.filter(|c| !c.is_empty()) {
            Some(c) => c.to_string(),
            None => py_str(&self.ship(sid)["active_config"]),
        }
    }

    fn section(
        &mut self,
        section: &str,
        cid: Option<&str>,
        sid: Option<&str>,
    ) -> Map<String, Value> {
        let c = self.config_id(cid, sid);
        let s = self.ship_mut(sid);
        let sec = s
            .entry(section.to_string())
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .expect("config section");
        sec.entry(c)
            .or_insert_with(|| json!({}))
            .as_object()
            .cloned()
            .unwrap_or_default()
    }

    /// A ship's config: slot -> fitted item ids (`config`).
    pub fn config(&mut self, cid: Option<&str>, sid: Option<&str>) -> Map<String, Value> {
        self.section("configs", cid, sid)
    }

    /// In a ship's config, drone id (text) -> fitted item ids (`drone_config`).
    pub fn drone_config(&mut self, cid: Option<&str>, sid: Option<&str>) -> Map<String, Value> {
        self.section("drone_config", cid, sid)
    }

    fn loots(&self, ids: Option<&Value>) -> Vec<String> {
        let items = &self.d["items"];
        ids.and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|i| {
                items
                    .get(py_str(i))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .collect()
    }

    /// The equipment in a ship's config: slot -> lootIds (`loadout`).
    pub fn loadout(
        &mut self,
        sid: Option<&str>,
        cid: Option<&str>,
    ) -> Vec<(&'static str, Vec<String>)> {
        let cfg = self.config(cid, sid);
        SLOTS
            .iter()
            .map(|s| (*s, self.loots(cfg.get(*s))))
            .collect()
    }

    /// In drone order: (drone record, its fitted lootIds) (`drone_loadout`).
    pub fn drone_loadout(
        &mut self,
        sid: Option<&str>,
        cid: Option<&str>,
    ) -> Vec<(Value, Vec<String>)> {
        let cfg = self.drone_config(cid, sid);
        let drones = self.d["drones"].as_array().cloned().unwrap_or_default();
        drones
            .into_iter()
            .map(|x| {
                let l = self.loots(cfg.get(&py_str(&x["uid"])));
                (x, l)
            })
            .collect()
    }

    /// Ammo count (`ammo`).
    pub fn ammo(&self, loot: &str) -> f64 {
        self.d["ammunition"].get(loot).and_then(num).unwrap_or(0.0)
    }

    /// The count of an ore type (`ore_count`).
    pub fn ore_count(&self, t: i64) -> f64 {
        self.d["ores"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|m| num(&m["oreType"]["typeValue"]) == Some(t as f64))
            .and_then(|m| num(&m["count"]))
            .unwrap_or(0.0)
    }

    /// A resource in storage (`resource`).
    pub fn resource(&self, loot: &str) -> f64 {
        self.d
            .get("other_items")
            .and_then(|o| o.get(loot))
            .and_then(num)
            .unwrap_or(0.0)
    }

    /// The lootIds of every owned item instance (`d['items'].values()`).
    pub fn item_loots(&self) -> Vec<&str> {
        self.d["items"]
            .as_object()
            .into_iter()
            .flat_map(|m| m.values())
            .filter_map(Value::as_str)
            .collect()
    }
}
