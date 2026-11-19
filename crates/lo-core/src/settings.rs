//! Game settings file (OpenOrbit's `settings.py`): the part the ported systems use so far.
//!
//! The file is `{key: value, "events": {event: "on" | "off"}}`, written by the panel. A key missing from the file is
//! the default, unknown ones are ignored, a corrupt file falls back to the defaults. A bound file is re-read when it
//! changes, at most once per [`REFRESH`] (the panel saves it while the game runs).
//!
//! Ported so far: the event choices (`event_state`) and the switches the event calendar and the event appearances
//! read. The multipliers and the rest of the schema come with the systems that use them.

use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

/// How long a read of the file is trusted before its modification time is checked again (`REFRESH_SEC`).
pub const REFRESH: Duration = Duration::from_secs(1);

/// On/off switches ported so far, with their defaults (`_b` entries of `SCHEMA`).
pub const SWITCHES: [(&str, bool); 4] = [
    ("session_always_open", false),
    ("event_looks_in_shop", true),
    ("admin_mode", false),
    ("gm_console", false),
];

/// Choices among options ported so far (`type='level'` entries): key, default, number of options.
pub const LEVELS: [(&str, i64, i64); 1] = [("admin_ship", 0, 5)];

/// The panel's choice for an event (`EVENT_STATES`); no entry means "by the calendar".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EventState {
    On,
    Off,
}

/// Cleaned values of the file (`clean`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Values {
    pub switches: BTreeMap<String, bool>,
    pub levels: BTreeMap<String, i64>,
    pub events: BTreeMap<String, EventState>,
}

impl Values {
    /// A dict read from the file -> the valid values.
    pub fn clean(raw: &Value) -> Values {
        let mut out = Values::default();
        let Some(m) = raw.as_object() else {
            return out;
        };
        for (k, _) in SWITCHES {
            if let Some(v) = m.get(k) {
                out.switches.insert(k.to_string(), truthy(v));
            }
        }
        for (k, _, n) in LEVELS {
            // min(len(options) - 1, max(0, int(v))); int() of a bool, a number or a text holding an integer
            let v = match m.get(k) {
                Some(Value::Bool(b)) => Some(i64::from(*b)),
                Some(Value::Number(x)) => x.as_i64().or_else(|| {
                    x.as_f64()
                        .filter(|f| f.is_finite())
                        .map(|f| f.trunc() as i64)
                }),
                Some(Value::String(s)) => s.trim().parse::<i64>().ok(),
                _ => None,
            };
            if let Some(v) = v {
                out.levels.insert(k.to_string(), v.max(0).min(n - 1));
            }
        }
        if let Some(ev) = m.get("events").and_then(Value::as_object) {
            for (k, v) in ev {
                let st = match v.as_str() {
                    Some("on") => EventState::On,
                    Some("off") => EventState::Off,
                    _ => continue,
                };
                out.events.insert(k.clone(), st);
            }
        }
        out
    }
}

struct State {
    mtime: Option<Option<SystemTime>>,
    read_at: Option<Instant>,
    values: Values,
}

/// The settings, bound to a file or unbound (all defaults: tests and tools).
pub struct Settings {
    path: Option<PathBuf>,
    state: Mutex<State>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings::unbound()
    }
}

impl Settings {
    pub fn unbound() -> Settings {
        Settings {
            path: None,
            state: Mutex::new(State {
                mtime: None,
                read_at: None,
                values: Values::default(),
            }),
        }
    }

    /// Bound to a settings file (`configure`).
    pub fn bind(path: impl Into<PathBuf>) -> Settings {
        Settings {
            path: Some(path.into()),
            ..Settings::unbound()
        }
    }

    /// Fixed values (tests).
    pub fn fixed(values: Values) -> Settings {
        let s = Settings::unbound();
        s.state.lock().unwrap_or_else(|e| e.into_inner()).values = values;
        s
    }

    pub fn path(&self) -> Option<&std::path::Path> {
        self.path.as_deref()
    }

    /// The current values (`_current`).
    pub fn current(&self) -> Values {
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let Some(path) = &self.path else {
            return st.values.clone();
        };
        let now = Instant::now();
        if st.mtime.is_some() && st.read_at.is_some_and(|r| now - r < REFRESH) {
            return st.values.clone();
        }
        st.read_at = Some(now);
        let m = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        if st.mtime != Some(m) {
            st.mtime = Some(m);
            st.values = if m.is_some() {
                read(path)
            } else {
                Values::default()
            };
        }
        st.values.clone()
    }

    /// An on/off switch from [`SWITCHES`].
    pub fn switch(&self, key: &str) -> bool {
        let default = SWITCHES
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, d)| *d)
            .unwrap_or_else(|| panic!("unknown switch {key}"));
        self.current().switches.get(key).copied().unwrap_or(default)
    }

    /// A choice among options from [`LEVELS`] (its index).
    pub fn level(&self, key: &str) -> i64 {
        let default = LEVELS
            .iter()
            .find(|(k, _, _)| *k == key)
            .map(|(_, d, _)| *d)
            .unwrap_or_else(|| panic!("unknown level setting {key}"));
        self.current().levels.get(key).copied().unwrap_or(default)
    }

    /// The panel's choice for an event (`event_state`).
    pub fn event_state(&self, key: &str) -> Option<EventState> {
        self.current().events.get(key).copied()
    }

    /// All the event choices.
    pub fn events(&self) -> BTreeMap<String, EventState> {
        self.current().events
    }
}

/// The file's values; the defaults if it is missing or corrupt.
pub fn read(path: &std::path::Path) -> Values {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .map(|v| Values::clean(&v))
        .unwrap_or_default()
}

/// Python truthiness of a JSON value (`bool(v)`).
pub fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(m) => !m.is_empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn clean() {
        let v = Values::clean(&json!({
            "session_always_open": 1, "event_looks_in_shop": "", "unknown": true, "admin_ship": 9,
            "events": {"halloween": "on", "winter": "off", "agatus": true, "spring": null}
        }));
        assert!(v.switches["session_always_open"]);
        assert!(!v.switches["event_looks_in_shop"]);
        assert_eq!(v.events.len(), 2);
        assert_eq!(v.events["halloween"], EventState::On);
        let s = Settings::fixed(v);
        assert!(s.switch("session_always_open"));
        assert_eq!(s.event_state("winter"), Some(EventState::Off));
        assert_eq!(s.event_state("agatus"), None);
        assert!(Settings::unbound().switch("event_looks_in_shop"));
        assert_eq!(s.level("admin_ship"), 4);
        assert_eq!(Settings::unbound().level("admin_ship"), 0);
    }
}
