//! Where things are (OpenOrbit's `paths.py`).
//!
//! ```text
//! <root>/data/                          game data (a copy of OpenOrbit's server/data)
//! <root>/cdn/                           the client's CDN files; game images are added on first request
//! <save>/slots/<id>/profile.json        a save slot's profile
//! <save>/slots/<id>/web_settings.json   the login screen's language
//! <save>/slots/<id>/settings.json       the slot's game settings
//! <save>/panel.json                     panel settings (active slot …)
//! <save>/settings.json                  game settings when the slot has none
//! ```
//!
//! `<root>`: `LO_ROOT`, otherwise the first folder holding `data/protocol_read.json` above the executable (or the current
//! folder). `<save>`: `OO_SAVE_DIR` (the same variable as OpenOrbit, so both can share saves), otherwise `<root>/save`.

use serde_json::Value;
use std::path::{Path, PathBuf};

/// Login templates: your own record in `data/`, otherwise the shareable copy in `data/defaults/`.
pub const TEMPLATES: [&str; 3] = ["login.json", "maps.json", "responses.json"];

#[derive(Debug, Clone)]
pub struct Layout {
    pub root: PathBuf,
    pub data: PathBuf,
    pub cdn: PathBuf,
    pub save: PathBuf,
}

fn is_root(p: &Path) -> bool {
    p.join("data").join("protocol_read.json").is_file()
}

impl Layout {
    /// The layout under a root folder (`OO_SAVE_DIR` still chooses the save folder).
    pub fn at(root: impl Into<PathBuf>) -> Layout {
        let root = root.into();
        let save = std::env::var_os("OO_SAVE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("save"));
        Layout {
            data: root.join("data"),
            cdn: root.join("cdn"),
            save,
            root,
        }
    }

    /// The layout of this installation (see the module documentation).
    pub fn from_env() -> Layout {
        if let Some(r) = std::env::var_os("LO_ROOT") {
            return Layout::at(r);
        }
        let exe = std::env::current_exe().ok();
        let found = exe.as_deref().and_then(|e| {
            e.ancestors()
                .skip(1)
                .find(|p| is_root(p))
                .map(Path::to_path_buf)
        });
        let root = found
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));
        Layout::at(root)
    }

    pub fn slots(&self) -> PathBuf {
        self.save.join("slots")
    }

    pub fn slot_dir(&self, slot: &str) -> PathBuf {
        self.slots().join(slot)
    }

    pub fn panel_file(&self) -> PathBuf {
        self.save.join("panel.json")
    }

    /// `panel.json` as an object (empty if missing or broken).
    pub fn panel_config(&self) -> serde_json::Map<String, Value> {
        std::fs::read_to_string(self.panel_file())
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .and_then(|v| match v {
                Value::Object(m) => Some(m),
                _ => None,
            })
            .unwrap_or_default()
    }

    /// The active slot (`panel.json` `active_save`) if its folder exists.
    pub fn active_slot(&self) -> Option<String> {
        let s = self
            .panel_config()
            .get("active_save")?
            .as_str()?
            .to_string();
        (!s.is_empty() && self.slot_dir(&s).is_dir()).then_some(s)
    }

    /// The slot's profile (the active slot's if not given); the old `save/profile.json` without a slot.
    pub fn profile_path(&self, slot: Option<&str>) -> PathBuf {
        match slot.map(str::to_string).or_else(|| self.active_slot()) {
            Some(s) => self.slot_dir(&s).join("profile.json"),
            None => self.save.join("profile.json"),
        }
    }

    /// The slot's game settings if it has some, otherwise `save/settings.json`.
    pub fn settings_path(&self, slot: Option<&str>) -> PathBuf {
        if let Some(s) = slot.map(str::to_string).or_else(|| self.active_slot()) {
            let p = self.slot_dir(&s).join("settings.json");
            if p.is_file() {
                return p;
            }
        }
        self.save.join("settings.json")
    }

    /// A login template: `data/<name>` (your own record) or `data/defaults/<name>`.
    pub fn template(&self, name: &str) -> Option<PathBuf> {
        [self.data.join(name), self.data.join("defaults").join(name)]
            .into_iter()
            .find(|p| p.is_file())
    }
}
