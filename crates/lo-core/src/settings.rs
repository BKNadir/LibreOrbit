//! Game settings (OpenOrbit's `settings.py`): multipliers, on/off switches, choices and numbers, the event choices.
//!
//! The single definition is OpenOrbit's `SCHEMA`, exported to `settings_schema.json` (embedded here; see
//! `tools/settings_diff/export_schema.py`). Values are in `settings.json` (`{key: value, "events": {event: "on" | "off"}}`):
//! a key missing from the file is the default, unknown ones are ignored, numbers are clamped to their limits, a corrupt
//! file falls back to the defaults. A bound file is re-read when it changes, at most once per [`REFRESH`].
//!
//! Sub-settings: a multiplier listed in OpenOrbit's `SUBS` has one sub-multiplier per source (`uridium_npc`, …); the
//! effective multiplier is parent × sub ([`Settings::multiplier`]).

use crate::pyrandom::Random;
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

/// How long a read of the file is trusted before its modification time is checked again (`REFRESH_SEC`).
pub const REFRESH: Duration = Duration::from_secs(1);
/// The largest value sent to the client in a 32-bit field.
pub const I32: i64 = (1 << 31) - 1;

const SCHEMA_JSON: &str = include_str!("settings_schema.json");

/// The type of a setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Multiplier,
    Bool,
    Level,
    Number,
}

/// One setting of the schema.
#[derive(Debug, Clone)]
pub struct Setting {
    pub key: String,
    pub name: String,
    pub kind: Kind,
    pub default: Value,
    pub min: f64,
    pub cap: f64,
    /// Number of options of a choice.
    pub options: usize,
    pub unit: String,
    pub parent: Option<String>,
    pub source: Option<String>,
    /// The whole entry, as OpenOrbit's panel receives it.
    pub raw: Value,
}

/// The schema, in OpenOrbit's order (sub-settings right after their parent).
pub struct Schema {
    pub settings: Vec<Setting>,
    by_key: BTreeMap<String, usize>,
    sub: BTreeMap<(String, String), String>,
    /// The panel's groups, presets, auction rival levels and the hand-typed limit.
    pub raw: Value,
}

impl Schema {
    pub fn get(&self, key: &str) -> Option<&Setting> {
        self.by_key.get(key).map(|&i| &self.settings[i])
    }

    /// The sub-setting of a multiplier for a source (`SUB`).
    pub fn sub(&self, key: &str, source: &str) -> Option<&str> {
        self.sub
            .get(&(key.to_string(), source.to_string()))
            .map(String::as_str)
    }
}

/// The embedded schema.
pub fn schema() -> &'static Schema {
    static S: OnceLock<Schema> = OnceLock::new();
    S.get_or_init(|| {
        let raw: Value = serde_json::from_str(SCHEMA_JSON).expect("settings schema");
        let mut settings = Vec::new();
        for s in raw["schema"].as_array().expect("schema") {
            let st = |k: &str| s.get(k).and_then(Value::as_str).map(str::to_string);
            let kind = match s["type"].as_str() {
                Some("multiplier") => Kind::Multiplier,
                Some("bool") => Kind::Bool,
                Some("level") => Kind::Level,
                _ => Kind::Number,
            };
            settings.push(Setting {
                key: st("key").unwrap_or_default(),
                name: st("name").unwrap_or_default(),
                kind,
                default: s["default"].clone(),
                min: s.get("min").and_then(Value::as_f64).unwrap_or(0.0),
                cap: s.get("cap").and_then(Value::as_f64).unwrap_or(0.0),
                options: s
                    .get("options")
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len),
                unit: st("unit").unwrap_or_default(),
                parent: st("parent"),
                source: st("source"),
                raw: s.clone(),
            });
        }
        let by_key = settings
            .iter()
            .enumerate()
            .map(|(i, s)| (s.key.clone(), i))
            .collect();
        let sub = settings
            .iter()
            .filter_map(|s| Some(((s.parent.clone()?, s.source.clone()?), s.key.clone())))
            .collect();
        Schema {
            settings,
            by_key,
            sub,
            raw,
        }
    })
}

/// The panel's choice for an event (`EVENT_STATES`); no entry means "by the calendar".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EventState {
    On,
    Off,
}

impl EventState {
    pub fn as_str(self) -> &'static str {
        match self {
            EventState::On => "on",
            EventState::Off => "off",
        }
    }
}

/// Python's `float(v)` for the values a JSON file can hold (`None` where Python raises).
pub fn py_float(v: &Value) -> Option<f64> {
    match v {
        Value::Bool(b) => Some(f64::from(u8::from(*b))),
        Value::Number(n) => n.as_f64(),
        Value::String(s) => {
            let t = s.trim();
            if t.starts_with('_') || t.ends_with('_') || t.contains("__") {
                return None;
            }
            let t = t.replace('_', "");
            let lower = t.to_ascii_lowercase();
            let bare = lower.trim_start_matches(['+', '-']);
            let numeric = bare
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_digit() || c == '.');
            if !numeric && !["inf", "infinity", "nan"].contains(&bare) {
                return None;
            }
            t.parse().ok()
        }
        _ => None,
    }
}

/// Python's `int(v)` (`None` where Python raises).
pub fn py_int(v: &Value) -> Option<i64> {
    match v {
        Value::Bool(b) => Some(i64::from(*b)),
        Value::Number(n) => n.as_i64().or_else(|| {
            n.as_f64()
                .filter(|f| f.is_finite())
                .map(|f| f.trunc() as i64)
        }),
        Value::String(s) => {
            let t = s.trim();
            if t.starts_with('_') || t.ends_with('_') || t.contains("__") {
                return None;
            }
            t.replace('_', "").parse().ok()
        }
        _ => None,
    }
}

/// A float as JSON (always a float, like a Python float).
fn jf(f: f64) -> Value {
    serde_json::Number::from_f64(f).map_or(Value::Null, Value::Number)
}

/// Cleaned values of the file (`clean`), in the file's order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Values {
    pub values: Map<String, Value>,
    pub events: BTreeMap<String, EventState>,
    /// The events in the file's order; `None` when the file has no `events` dict.
    event_order: Option<Vec<String>>,
}

impl Values {
    /// A dict read from the file -> the valid values.
    pub fn clean(raw: &Value) -> Values {
        let mut out = Values::default();
        let Some(m) = raw.as_object() else {
            return out;
        };
        let sch = schema();
        for (key, v) in m {
            let Some(s) = sch.get(key) else { continue };
            let cleaned = match s.kind {
                Kind::Multiplier => py_float(v)
                    .filter(|f| !f.is_nan())
                    .map(|f| jf(s.cap.min(s.min.max(f)))),
                Kind::Bool => Some(json!(truthy(v))),
                Kind::Level => py_int(v).map(|i| json!((s.options as i64 - 1).min(0i64.max(i)))),
                Kind::Number => py_float(v)
                    .filter(|f| !f.is_nan())
                    .map(|f| json!(s.cap.min(s.min.max(f)).round_ties_even() as i64)),
            };
            if let Some(c) = cleaned {
                out.values.insert(key.clone(), c);
            }
        }
        if let Some(ev) = m.get("events").and_then(Value::as_object) {
            let mut order = Vec::new();
            for (k, v) in ev {
                let st = match v.as_str() {
                    Some("on") => EventState::On,
                    Some("off") => EventState::Off,
                    _ => continue,
                };
                out.events.insert(k.clone(), st);
                order.push(k.clone());
            }
            out.event_order = Some(order);
        }
        out
    }

    /// The JSON form (`clean`'s dict), as `write` puts it in the file.
    pub fn to_json(&self) -> Value {
        let mut m = self.values.clone();
        if let Some(order) = &self.event_order {
            let ev: Map<String, Value> = order
                .iter()
                .map(|k| (k.clone(), json!(self.events[k].as_str())))
                .collect();
            m.insert("events".into(), Value::Object(ev));
        }
        Value::Object(m)
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

    pub fn path(&self) -> Option<&Path> {
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

    /// A setting's value (`get`): the file's, otherwise the default.
    pub fn get(&self, key: &str) -> Value {
        let s = schema()
            .get(key)
            .unwrap_or_else(|| panic!("unknown setting {key}"));
        self.current()
            .values
            .get(key)
            .cloned()
            .unwrap_or_else(|| s.default.clone())
    }

    /// An on/off setting.
    pub fn switch(&self, key: &str) -> bool {
        truthy(&self.get(key))
    }

    /// A choice among options (its index) or a number setting.
    pub fn level(&self, key: &str) -> i64 {
        py_int(&self.get(key)).unwrap_or(0)
    }

    /// A multiplier; with a source, times its sub-setting (`multiplier`).
    pub fn multiplier(&self, key: &str, source: Option<&str>) -> f64 {
        let sub = source.and_then(|s| schema().sub(key, s));
        let v = self.current();
        let get = |k: &str| {
            v.values
                .get(k)
                .cloned()
                .unwrap_or_else(|| schema().get(k).map_or(Value::Null, |s| s.default.clone()))
        };
        py_float(&get(key)).unwrap_or(0.0) * sub.map_or(1.0, |s| py_float(&get(s)).unwrap_or(0.0))
    }

    /// The file's modification time (`version`).
    pub fn version(&self) -> Option<SystemTime> {
        self.current();
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .mtime
            .flatten()
    }

    /// A price scaled by the multiplier: integer, at least 0, fits a 32-bit field (`price`).
    pub fn price(&self, key: &str, n: i64, source: Option<&str>) -> i64 {
        let m = self.multiplier(key, source);
        if m == 1.0 || n == 0 {
            return n;
        }
        I32.min(0i64.max((n as f64 * m).round_ties_even() as i64))
    }

    /// A count scaled by the multiplier: a positive count stays at least 1, 0 if the multiplier is 0 (`count`).
    pub fn count(&self, key: &str, n: i64, source: Option<&str>) -> i64 {
        let m = self.multiplier(key, source);
        if m == 1.0 || n <= 0 {
            return n;
        }
        if m <= 0.0 {
            0
        } else {
            1i64.max((n as f64 * m).round_ties_even() as i64)
        }
    }

    /// An amount scaled by the multiplier, the fraction rounded at random (`amount`).
    pub fn amount(&self, key: &str, n: i64, source: Option<&str>, rng: &mut Random) -> i64 {
        let m = self.multiplier(key, source);
        if m == 1.0 || n <= 0 {
            return n;
        }
        let v = n as f64 * m;
        let whole = v.trunc();
        whole as i64 + i64::from(rng.random() < v - whole)
    }

    /// A production or cooldown time in seconds, at least 1 (`duration`).
    pub fn duration(&self, sec: i64, source: Option<&str>) -> i64 {
        let m = self.multiplier("cooldown", source);
        if m == 1.0 {
            sec
        } else {
            1i64.max((sec as f64 * m).round_ties_even() as i64)
        }
    }

    /// A probability scaled by the multiplier, within 0..1 (`chance`).
    pub fn chance(&self, key: &str, p: f64, source: Option<&str>) -> f64 {
        let m = self.multiplier(key, source);
        if m == 1.0 {
            p
        } else {
            1f64.min(0f64.max(p * m))
        }
    }

    /// The panel's choice for an event (`event_state`).
    pub fn event_state(&self, key: &str) -> Option<EventState> {
        self.current().events.get(key).copied()
    }

    /// All the event choices.
    pub fn events(&self) -> BTreeMap<String, EventState> {
        self.current().events
    }

    /// The settings that differ from the default: (name, value text) (`summary`).
    pub fn summary(&self) -> Vec<(String, String)> {
        summary(&self.current())
    }
}

/// The setting's name in summaries: a sub-setting with its parent (`full_name`).
pub fn full_name(s: &Setting) -> String {
    match &s.parent {
        Some(p) => format!(
            "{} › {}",
            schema().get(p).map_or("", |x| x.name.as_str()),
            s.name
        ),
        None => s.name.clone(),
    }
}

/// `'%g' % f`.
pub fn fmt_g(f: f64) -> String {
    if f == 0.0 {
        return if f.is_sign_negative() { "-0" } else { "0" }.into();
    }
    if !f.is_finite() {
        return if f.is_nan() {
            "nan"
        } else if f > 0.0 {
            "inf"
        } else {
            "-inf"
        }
        .into();
    }
    let s = format!("{f:.5e}");
    let (mant, e) = s.split_once('e').unwrap_or((&s, "0"));
    let e: i32 = e.parse().unwrap_or(0);
    if (-4..6).contains(&e) {
        let t = format!("{:.*}", (5 - e) as usize, f);
        if t.contains('.') {
            t.trim_end_matches('0').trim_end_matches('.').to_string()
        } else {
            t
        }
    } else {
        let m = if mant.contains('.') {
            mant.trim_end_matches('0').trim_end_matches('.')
        } else {
            mant
        };
        format!("{m}e{}{:02}", if e < 0 { '-' } else { '+' }, e.abs())
    }
}

/// Settings that differ from the default (`summary`).
pub fn summary(v: &Values) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for s in &schema().settings {
        let val = v
            .values
            .get(&s.key)
            .cloned()
            .unwrap_or_else(|| s.default.clone());
        if py_eq(&val, &s.default) {
            continue;
        }
        let (name, text) = match s.kind {
            Kind::Multiplier => (
                full_name(s),
                format!("x{}", fmt_g(py_float(&val).unwrap_or(0.0))),
            ),
            Kind::Bool => (
                s.name.clone(),
                if truthy(&val) { "on" } else { "off" }.into(),
            ),
            Kind::Number => (
                s.name.clone(),
                format!("{} {}", py_int(&val).unwrap_or(0), s.unit)
                    .trim()
                    .to_string(),
            ),
            Kind::Level => (
                s.name.clone(),
                s.raw["options"][py_int(&val).unwrap_or(0).max(0) as usize]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            ),
        };
        out.push((name, text));
    }
    out
}

fn py_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(_) | Value::Bool(_), Value::Number(_) | Value::Bool(_)) => {
            py_float(a) == py_float(b)
        }
        _ => a == b,
    }
}

/// The file's values; the defaults if it is missing or corrupt (`read`).
pub fn read(path: &Path) -> Values {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .map(|v| Values::clean(&v))
        .unwrap_or_default()
}

/// The text `write` puts in the file: `json.dump(clean(values), ensure_ascii=False, indent=1)`.
pub fn file_text(values: &Value) -> String {
    crate::pyjson::dumps_with(&Values::clean(values).to_json(), false, Some(1))
}

/// Writes the settings atomically (the panel) -> the cleaned values. Values equal to the default are written too.
pub fn write(path: &Path, values: &Value) -> std::io::Result<Values> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".{}.tmp", std::process::id()));
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, file_text(values))?;
    let mut k = 0;
    loop {
        match std::fs::rename(&tmp, path) {
            Ok(()) => break,
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied && k < 5 => {
                k += 1;
                std::thread::sleep(Duration::from_millis(50 * k));
            }
            Err(e) => return Err(e),
        }
    }
    Ok(Values::clean(values))
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

    #[test]
    fn clean() {
        let v = Values::clean(&json!({
            "session_always_open": 1, "event_looks_in_shop": "", "unknown": true, "admin_ship": 9,
            "uridium": "2.5", "uridium_npc": 0.5, "ship_hp": 1e9, "credits": "nan",
            "events": {"halloween": "on", "winter": "off", "agatus": true, "spring": null}
        }));
        assert_eq!(v.values["session_always_open"], json!(true));
        assert_eq!(v.values["event_looks_in_shop"], json!(false));
        assert!(!v.values.contains_key("unknown") && !v.values.contains_key("credits"));
        assert_eq!(v.events.len(), 2);
        assert_eq!(v.events["halloween"], EventState::On);
        let s = Settings::fixed(v);
        assert!(s.switch("session_always_open"));
        assert_eq!(s.event_state("winter"), Some(EventState::Off));
        assert_eq!(s.event_state("agatus"), None);
        assert_eq!(s.level("admin_ship"), 4);
        assert_eq!(s.multiplier("uridium", Some("npc")), 1.25);
        assert_eq!(s.multiplier("uridium", Some("nope")), 2.5);
        assert_eq!(s.multiplier("ship_hp", None), 1000.0);
        assert!(Settings::unbound().switch("event_looks_in_shop"));
        assert_eq!(Settings::unbound().level("admin_ship"), 0);
    }

    #[test]
    fn g_format() {
        for (f, s) in [
            (2.0, "2"),
            (0.5, "0.5"),
            (1.25, "1.25"),
            (1000.0, "1000"),
            (1e7, "1e+07"),
            (0.0001, "0.0001"),
            (1.5e-5, "1.5e-05"),
            (123456.7, "123457"),
        ] {
            assert_eq!(fmt_g(f), s);
        }
    }
}
