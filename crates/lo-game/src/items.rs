//! The item data the equipment systems read: `items.json` (as it is, like OpenOrbit's modules), `items_o6.json`, the
//! event gates' alien CPUs, `ship_abilities.json`, the Galaxy Gate maps (`gates.json` with the world's patches), the
//! Echoes maps and the client's `flashres` keys (for the old text protocol's messages).

use lo_core::GameData;
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

/// The loaded item data.
pub struct ItemData {
    /// `items.json`.
    pub e: Value,
    /// `items_o6.json`.
    pub o6: Value,
    /// `combat_items.alien_cpu` with the event gates' CPUs (`ALIEN_CPUS`).
    pub alien_cpus: Map<String, Value>,
    /// `ship_abilities.json`.
    pub abilities: Value,
    /// Galaxy Gate map -> gate key (`galaxy_gates.BY_MAP`).
    pub gate_maps: HashMap<i64, String>,
    /// Echoes in the Depths of Space maps and hall (`echoes.D`).
    pub echoes_maps: HashSet<i64>,
    /// The client's flashres keys (`legacy_protocol.flashres`), `None` without the file.
    pub flashres: Option<HashSet<String>>,
    /// Booster kinds with the Ö6 ones (`booster.B['kinds']`).
    pub booster_kinds: Map<String, Value>,
}

fn read(data: &GameData, f: &str) -> Result<Value, String> {
    data.raw(f).map_err(|e| format!("{f}: {e}"))
}

impl ItemData {
    pub fn load(data: &GameData) -> Result<ItemData, String> {
        let e = read(data, "items.json")?;
        let o6 = read(data, "items_o6.json")?;
        let mut alien_cpus = e["combat_items"]["alien_cpu"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        if let Ok(eg) = read(data, "event_gates.json")
            && let Some(c) = eg.get("cpu").and_then(Value::as_object)
        {
            alien_cpus.extend(c.clone());
        }
        let gates = data
            .load("gates.json")
            .map_err(|e| format!("gates.json: {e}"))?;
        let mut gate_maps = HashMap::new();
        for (k, v) in gates["gates"].as_object().into_iter().flatten() {
            for m in v["maps"].as_array().into_iter().flatten() {
                if let Some(m) = m.as_i64() {
                    gate_maps.insert(m, k.clone());
                }
            }
        }
        let pve = read(data, "pve_modes.json")?;
        let mut echoes_maps: HashSet<i64> = pve["echoes"]["maps"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_i64)
            .collect();
        echoes_maps.extend(pve["echoes"]["hall"].as_i64());
        let flashres = data.raw("flashres_keys.json").ok().map(|f| {
            f["keys"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        });
        let mut booster_kinds = e["boosters"]["kinds"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        for (k, v) in o6["boosters"]["kinds"].as_object().into_iter().flatten() {
            booster_kinds.entry(k.clone()).or_insert_with(|| v.clone());
        }
        Ok(ItemData {
            abilities: read(data, "ship_abilities.json")?,
            e,
            o6,
            alien_cpus,
            gate_maps,
            echoes_maps,
            flashres,
            booster_kinds,
        })
    }

    /// `items.json` `combat_items` (`S` in `combat_items.py`).
    pub fn s(&self) -> &Value {
        &self.e["combat_items"]
    }

    /// `items.json` `extras` (`X` in `extras.py`).
    pub fn extras(&self) -> &Value {
        &self.e["extras"]
    }

    /// An old text protocol message (`legacy`): a parameterless `0|A|STM|key` whose key the client does not know becomes
    /// `0|A|CSTM|key|-1`; a credit or uridium gain over 2^31-1 is capped.
    pub fn legacy(&self, text: &str) -> (&'static str, Value) {
        let mut text = text.to_string();
        if text.starts_with("0|A|STM|") && text.matches('|').count() == 3 {
            if let Some(keys) = &self.flashres
                && !keys.contains(&text[8..])
            {
                text = format!("0|A|CSTM|{}|-1", &text[8..]);
            }
        } else if text.starts_with("0|LM|ST|CRE|") || text.starts_with("0|LM|ST|URI|") {
            let mut p: Vec<String> = text.split('|').map(str::to_string).collect();
            if p.get(4)
                .and_then(|v| v.parse::<i128>().ok())
                .is_some_and(|v| v > (1 << 31) - 1)
            {
                p[4] = ((1i64 << 31) - 1).to_string();
                text = p.join("|");
            }
        }
        ("LegacyModule", serde_json::json!({"message": text}))
    }
}
