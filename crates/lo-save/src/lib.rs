//! The player profile (OpenOrbit's `player_profile.py`): the save slot's `profile.json`.
//!
//! The profile stays a JSON document (`d`), like the Python dict: its field names are the protocol's, so its sections turn
//! straight into messages, and a save written by LibreOrbit is byte-for-byte what OpenOrbit writes
//! (`json.dumps(d, ensure_ascii=False, indent=1)`), so both can open the same slots.
//!
//! Port status: reading, the new-profile seed, saving (atomic, flushed to disk, retried while another program holds the
//! file on Windows) and the login markers that depend on the save alone (`@ayarlar`, `@tuslar`, `@madenler`,
//! `@formasyon`, `@giris_yaniti`) and the completions of older profiles (`Profile::migrate`; [`Profile::load`] reads
//! and migrates like OpenOrbit's `Profile(...)`, [`Profile::open`] only reads). `@gemi`, `@arayuz` and `@isaret` need
//! other systems: they are in `lo_game::login_markers`. Saves of the old v1/v2 formats are converted by OpenOrbit's panel
//! (`save_migration.py`) and are refused here.

pub mod hangar;
mod migrate;

pub use migrate::{CARGO_ORES, NEW_WINDOWS};

use lo_core::settings::Settings;
use serde_json::{Map, Value, json};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// `PILOT_FIELDS`: the `ShipInitializationCommand` fields kept in the profile's `pilot` section.
pub const PILOT_FIELDS: [&str; 18] = [
    "userId",
    "userName",
    "factionId",
    "clanId",
    "clanTag",
    "level",
    "ep",
    "honourPoints",
    "credits",
    "uridium",
    "premium",
    "dailyRank",
    "galaxyGatesDone",
    "expansionStage",
    "jackpot",
    "useSystemFont",
    "captainEnergy",
    "maxCaptainEnergy",
];
/// `SHIP_FIELDS`: those kept in the `ship` section.
pub const SHIP_FIELDS: [&str; 12] = [
    "typeId",
    "speed",
    "shield",
    "shieldMax",
    "hitPoints",
    "hitMax",
    "nanoHull",
    "maxNanoHull",
    "cargoSpace",
    "cargoSpaceMax",
    "cloaked",
    "modifier",
];

/// `DEFAULT_SHIP_INIT`: the ship and pilot of a profile without a seed.
pub fn default_ship_init() -> Map<String, Value> {
    let v = json!({
        "userId": 1, "userName": "Pilot", "typeId": "ship_goliath", "speed": 300, "shield": 100000, "shieldMax": 100000,
        "hitPoints": 356000, "hitMax": 356000, "cargoSpace": 0, "cargoSpaceMax": 1500, "x": 2000, "y": 2000, "mapId": 1,
        "factionId": 1, "expansionStage": 3, "premium": true, "ep": 100000, "honourPoints": 1000, "level": 12, "credits": 1000000,
        "uridium": 50000, "dailyRank": 1, "refreshMap": true, "modifier": []
    });
    match v {
        Value::Object(m) => m,
        _ => unreachable!(),
    }
}

#[derive(Debug)]
pub enum SaveError {
    Io(std::io::Error),
    Json(serde_json::Error),
    /// A save of OpenOrbit's old formats (v1/v2): open it once in OpenOrbit's panel to convert it.
    OldFormat,
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaveError::Io(e) => write!(f, "{e}"),
            SaveError::Json(e) => write!(f, "invalid profile: {e}"),
            SaveError::OldFormat => f.write_str(
                "old save format (v1/v2): open it once in OpenOrbit's panel to convert it",
            ),
        }
    }
}

impl std::error::Error for SaveError {}

impl From<std::io::Error> for SaveError {
    fn from(e: std::io::Error) -> Self {
        SaveError::Io(e)
    }
}

/// A player's profile.
#[derive(Debug)]
pub struct Profile {
    pub path: PathBuf,
    pub d: Value,
    dirty: bool,
    write_lock: Mutex<()>,
}

impl Profile {
    /// Reads the profile; a missing file gives a new profile from the seed (the `seed` of the login template), saved at once
    /// (the web server reads it at login).
    pub fn open(path: impl Into<PathBuf>, seed: Option<&Value>) -> Result<Profile, SaveError> {
        let path = path.into();
        if path.exists() {
            let text = std::fs::read_to_string(&path)?;
            let d: Value = serde_json::from_str(&text).map_err(SaveError::Json)?;
            if !is_current(&d) {
                return Err(SaveError::OldFormat);
            }
            return Ok(Profile {
                path,
                d,
                dirty: false,
                write_lock: Mutex::new(()),
            });
        }
        let mut p = Profile {
            path,
            d: from_seed(seed.unwrap_or(&Value::Null)),
            dirty: true,
            write_lock: Mutex::new(()),
        };
        p.save(false)?;
        Ok(p)
    }

    /// The profile as OpenOrbit's game holds it (`Profile(seed, path)`): read (or made from the seed when the file does
    /// not exist), completed by [`Profile::migrate`], and a new profile written at once (the web server reads it at login).
    /// A migrated existing profile is only marked as changed: it is written with the game's usual save.
    pub fn load(
        path: impl Into<PathBuf>,
        seed: &Value,
        settings: &Settings,
    ) -> Result<Profile, SaveError> {
        let path = path.into();
        let created = !path.exists();
        let mut p = if created {
            Profile {
                path,
                d: from_seed(seed),
                dirty: true,
                write_lock: Mutex::new(()),
            }
        } else {
            Profile::open(path, None)?
        };
        p.migrate(seed, settings);
        if created && p.dirty {
            p.save(false)?;
        }
        Ok(p)
    }

    /// A profile from a document (a guest's save on a friend server).
    pub fn from_value(path: impl Into<PathBuf>, d: Value) -> Profile {
        Profile {
            path: path.into(),
            d,
            dirty: false,
            write_lock: Mutex::new(()),
        }
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn user_id(&self) -> Option<i64> {
        self.d.get("pilot")?.get("userId")?.as_i64()
    }

    /// A login marker of the template (`@…`) -> (message name, fields), for the markers ported so far (`Profile.generate`).
    pub fn generate(&self, marker: &str) -> Option<(&'static str, Value)> {
        let d = &self.d;
        let field = |k: &str| d.get(k).cloned().unwrap_or(Value::Null);
        Some(match marker {
            "@ayarlar" => ("UserSettingsCommand", settings_command(&field("settings"))),
            "@tuslar" => (
                "UserKeyBindingsUpdate",
                json!({"changedKeyBindings": field("key_bindings")}),
            ),
            "@madenler" => (
                "AttributeOreCountUpdateCommand",
                json!({"oreCountList": field("ores")}),
            ),
            "@formasyon" => (
                "DroneFormationChangeCommand",
                json!({"uid": d.get("pilot").and_then(|p| p.get("userId")).cloned().unwrap_or(Value::Null), "selectedFormationId": field("formation")}),
            ),
            "@giris_yaniti" => ("LoginResponse", json!({"status": 0})),
            _ => return None,
        })
    }

    /// The text written to the file: `json.dumps(d, ensure_ascii=False, indent=1)`.
    pub fn text(&self) -> String {
        lo_core::pyjson::dumps_with(&self.d, false, Some(1))
    }

    /// Writes the profile if it changed (or `force`): to `<path>.tmp`, flushed to disk, then renamed over the file (retried a few times
    /// while another program, such as an antivirus scanner, holds the file on Windows). On failure it stays marked as changed.
    pub fn save(&mut self, force: bool) -> Result<(), SaveError> {
        if !(self.dirty || force) {
            return Ok(());
        }
        self.dirty = false;
        let r = self.write();
        if r.is_err() {
            self.dirty = true;
        }
        r
    }

    fn write(&self) -> Result<(), SaveError> {
        let _guard = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(dir) = self.path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = tmp_path(&self.path);
        {
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(self.text().as_bytes())?;
            f.flush()?;
            f.sync_all()?; // no half-written profile after a power cut
        }
        let mut k = 0;
        loop {
            match std::fs::rename(&tmp, &self.path) {
                Ok(()) => return Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied && k < 5 => {
                    k += 1;
                    std::thread::sleep(std::time::Duration::from_millis(50 * k));
                }
                Err(e) => return Err(e.into()),
            }
        }
    }
}

/// `@ayarlar`: the saved settings. A quality module never saved (`notSet`, a new profile) goes out as the high preset:
/// the client skips a `notSet` module and keeps its own default 0, which turns the engine trail off on every ship.
fn settings_command(a: &Value) -> Value {
    let mut a = a.clone();
    let not_set = a
        .get("qualitySettingsModule")
        .and_then(|q| q.get("notSet"))
        .is_some_and(truthy);
    if not_set {
        if let Some(Value::Object(q)) = a.get_mut("qualitySettingsModule") {
            let high: Vec<String> = q
                .keys()
                .filter(|k| k.starts_with("quality") && *k != "qualityCustomized")
                .cloned()
                .collect();
            q.insert("notSet".into(), json!(false));
            q.insert("qualityCustomized".into(), json!(false));
            for k in high {
                q.insert(k, json!(3));
            }
        }
    }
    a
}

/// Python truthiness of a JSON value.
pub fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64() != Some(0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(m) => !m.is_empty(),
    }
}

fn tmp_path(p: &Path) -> PathBuf {
    let mut s = p.as_os_str().to_owned();
    s.push(".tmp");
    PathBuf::from(s)
}

/// A v3 profile has the English section names (`pilot`, `ship`, `position`); the old formats do not.
fn is_current(d: &Value) -> bool {
    ["pilot", "ship", "position"]
        .iter()
        .all(|k| d.get(*k).is_some_and(Value::is_object))
}

/// `Profile._from_seed`: a new profile from the login template's seed (the recorded login commands).
pub fn from_seed(seed: &Value) -> Value {
    let mut si = default_ship_init();
    if let Some(Value::Object(m)) = seed.get("ShipInitializationCommand") {
        for (k, v) in m {
            si.insert(k.clone(), v.clone());
        }
    }
    let pick = |fields: &[&str]| -> Value {
        Value::Object(
            fields
                .iter()
                .filter_map(|k| si.get(*k).map(|v| (k.to_string(), v.clone())))
                .collect(),
        )
    };
    let get = |cmd: &str, field: &str, default: Value| -> Value {
        seed.get(cmd)
            .filter(|v| v.is_object() && !v.as_object().is_some_and(Map::is_empty))
            .and_then(|c| c.get(field))
            .cloned()
            .unwrap_or(default)
    };
    let truthy_obj = |cmd: &str| {
        seed.get(cmd)
            .filter(|v| v.as_object().is_some_and(|m| !m.is_empty()))
            .cloned()
    };
    json!({
        "version": 1,
        "created": chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        "pilot": pick(&PILOT_FIELDS),
        "ship": pick(&SHIP_FIELDS),
        "position": {"mapId": si["mapId"], "x": si["x"], "y": si["y"]},
        "ores": get("AttributeOreCountUpdateCommand", "oreCountList", json!([])),
        "formation": get("DroneFormationChangeCommand", "selectedFormationId", json!(0)),
        "settings": truthy_obj("UserSettingsCommand").unwrap_or(json!({})),
        "key_bindings": get("UserKeyBindingsUpdate", "changedKeyBindings", json!([])),
        "ui": truthy_obj("UILayoutCommand").unwrap_or(json!({"featureBars": []})),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seedless_profile() {
        let d = from_seed(&Value::Null);
        assert_eq!(d["pilot"]["userName"], "Pilot");
        assert_eq!(d["ship"]["typeId"], "ship_goliath");
        assert_eq!(d["position"], json!({"mapId": 1, "x": 2000, "y": 2000}));
        assert!(d["pilot"].get("x").is_none() && d["ship"].get("level").is_none());
    }
}
