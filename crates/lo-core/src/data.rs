//! Game data and worlds (OpenOrbit's `game_data.py`).
//!
//! A world is the change layer made in the panel's World Editor (`save/worlds/<id>.json`), chosen with the `OO_WORLD`
//! environment variable; without it the standard world (`data/` as it is). Format:
//!
//! ```json
//! {"name": "…", "patches": {"npc.json": {"/types/84": {}, "/maps/5/npc": []}, "shop": {"/products/<lootId>": {}}}}
//! ```
//!
//! A patch is a JSON Pointer to a full value (replaced as a whole); `{"$delete": true}` deletes. A path whose parent is
//! missing is skipped. Virtual files (`shop`) exist only in worlds.

use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

const DELETE: &str = "$delete";
/// Files applied again while the game runs when the world file changes (shop prices and sales, auction lots).
pub const LIVE: [&str; 2] = ["shop", "social.json"];

fn virtual_file(file: &str) -> Option<Value> {
    (file == "shop").then(|| json!({"products": {}}))
}

/// A World Editor world (empty: the standard world).
#[derive(Debug, Clone, Default)]
pub struct World {
    pub name: Option<String>,
    pub patches: Map<String, Value>,
}

impl World {
    /// Reads a world file; the standard world if it cannot be read.
    pub fn read(path: &Path) -> World {
        let v: Value = std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or(Value::Null);
        let Value::Object(m) = v else {
            return World::default();
        };
        World {
            name: m.get("name").and_then(Value::as_str).map(str::to_string),
            patches: m
                .get("patches")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default(),
        }
    }

    pub fn is_standard(&self) -> bool {
        self.patches.is_empty() && self.name.is_none()
    }

    /// The world's patches to one file.
    pub fn patches_for(&self, file: &str) -> Option<&Map<String, Value>> {
        self.patches.get(file).and_then(Value::as_object)
    }
}

/// The game data folder with the selected world.
#[derive(Debug)]
pub struct GameData {
    dir: PathBuf,
    world_path: Option<PathBuf>,
    state: Mutex<(World, Option<SystemTime>)>,
    skipped: Mutex<Vec<(String, String)>>,
}

impl GameData {
    /// `world_path`: the world file (`OO_WORLD`), `None` for the standard world.
    pub fn new(dir: impl Into<PathBuf>, world_path: Option<PathBuf>) -> GameData {
        let mtime = world_path
            .as_deref()
            .and_then(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
        let world = world_path.as_deref().map(World::read).unwrap_or_default();
        GameData {
            dir: dir.into(),
            world_path,
            state: Mutex::new((world, mtime)),
            skipped: Mutex::new(Vec::new()),
        }
    }

    /// With the world named by the `OO_WORLD` environment variable.
    pub fn from_env(dir: impl Into<PathBuf>) -> GameData {
        GameData::new(
            dir,
            std::env::var_os("OO_WORLD")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from),
        )
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn world(&self) -> World {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .0
            .clone()
    }

    /// The file in the standard world (no patches). Virtual files start empty.
    pub fn raw(&self, file: &str) -> std::io::Result<Value> {
        if let Some(v) = virtual_file(file) {
            return Ok(v);
        }
        let text = std::fs::read_to_string(self.dir.join(file))?;
        serde_json::from_str(&text)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// `data/<file>` with the world's patches to it.
    pub fn load(&self, file: &str) -> std::io::Result<Value> {
        let mut d = self.raw(file)?;
        let world = self.world();
        if let Some(p) = world.patches_for(file) {
            let skipped = apply(&mut d, p);
            self.skipped
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .extend(skipped.into_iter().map(|r| (file.to_string(), r)));
        }
        Ok(d)
    }

    /// Re-reads the world file if it changed -> the [`LIVE`] files whose patches changed.
    pub fn refresh(&self) -> Vec<&'static str> {
        let Some(p) = &self.world_path else {
            return Vec::new();
        };
        let Ok(t) = std::fs::metadata(p).and_then(|m| m.modified()) else {
            return Vec::new();
        };
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if st.1 == Some(t) {
            return Vec::new();
        }
        let new = World::read(p);
        let changed = LIVE
            .into_iter()
            .filter(|f| st.0.patches.get(*f) != new.patches.get(*f))
            .collect();
        *st = (new, Some(t));
        changed
    }

    /// Log line: the world's name, the number of patches and the skipped ones.
    pub fn summary(&self) -> String {
        let w = self.world();
        if w.is_standard() {
            return "world: standard".into();
        }
        let n: usize = w
            .patches
            .values()
            .map(|v| v.as_object().map_or(0, Map::len))
            .sum();
        let skipped = self.skipped.lock().unwrap_or_else(|e| e.into_inner());
        let s = if skipped.is_empty() {
            String::new()
        } else {
            format!(", skipped: {skipped:?}")
        };
        format!(
            "world: {} ({n} changes{s})",
            w.name.as_deref().unwrap_or("?")
        )
    }
}

/// `'/a/b~1c'` -> `["a", "b/c"]`.
pub fn path_parts(route: &str) -> Vec<String> {
    route
        .split('/')
        .skip(1)
        .map(|p| p.replace("~1", "/").replace("~0", "~"))
        .collect()
}

/// Python's `int(text)` for list indexes (negative counts from the end).
fn index(part: &str, len: usize) -> Option<(i64, usize)> {
    let i: i64 = part.trim().parse().ok()?;
    let idx = if i < 0 { len as i64 + i } else { i };
    Some((i, usize::try_from(idx).ok()?))
}

/// Applies patches in place -> the routes that could not be applied.
pub fn apply(data: &mut Value, patches: &Map<String, Value>) -> Vec<String> {
    let mut skipped = Vec::new();
    for (route, value) in patches {
        let parts = path_parts(route);
        let Some((last, parents)) = parts.split_last() else {
            continue;
        };
        if apply_one(data, parents, last, value).is_none() {
            skipped.push(route.clone());
        }
    }
    skipped
}

fn apply_one(data: &mut Value, parents: &[String], last: &str, value: &Value) -> Option<()> {
    let mut parent = data;
    for p in parents {
        parent = match parent {
            Value::Array(a) => {
                let (_, i) = index(p, a.len())?;
                a.get_mut(i)?
            }
            Value::Object(m) => m.get_mut(p.as_str())?,
            _ => return None,
        };
    }
    let remove = value
        .as_object()
        .and_then(|m| m.get(DELETE))
        .is_some_and(truthy);
    match parent {
        Value::Array(a) => {
            let (raw, i) = index(last, a.len())?;
            if remove {
                (i < a.len()).then(|| a.remove(i))?;
            } else if raw >= 0 && i == a.len() {
                a.push(value.clone());
            } else {
                *a.get_mut(i)? = value.clone();
            }
        }
        Value::Object(m) => {
            if remove {
                m.shift_remove(last);
            } else {
                m.insert(last.to_string(), value.clone());
            }
        }
        _ => return None,
    }
    Some(())
}

/// Python truthiness of a JSON value.
fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64() != Some(0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(m) => !m.is_empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patches() {
        let mut d =
            json!({"types": {"84": {"hp": 1}}, "maps": {"5": {"npc": [1, 2, 3]}}, "list": [0, 1]});
        let p = json!({
            "/types/84": {"hp": 9},
            "/maps/5/npc/-1": 30,
            "/list/2": 2,
            "/types/85/hp": 1,
            "/maps/5/gone": {"$delete": true},
            "/list/0": {"$delete": true}
        });
        let skipped = apply(&mut d, p.as_object().unwrap());
        assert_eq!(skipped, vec!["/types/85/hp".to_string()]);
        assert_eq!(
            d,
            json!({"types": {"84": {"hp": 9}}, "maps": {"5": {"npc": [1, 2, 30]}}, "list": [1, 2]})
        );
    }
}
