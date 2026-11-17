//! The message description: `protocol_read.json`, compiled for decoding and encoding.

use crate::expr::Expr;
use serde_json::Value as Json;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, OnceLock};

/// Errors while loading the spec.
#[derive(Debug, thiserror::Error)]
pub enum SpecError {
    #[error("cannot read {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("message {id}: {msg}")]
    Invalid { id: String, msg: String },
}

/// Primitive field types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Prim {
    Int,
    Uint,
    Short,
    Ushort,
    Byte,
    Sbyte,
    Bool,
    Float,
    Double,
    /// Sent as a double, read as an integer.
    Long,
    Str,
    /// A nested message: u16 id (0 = none) and its fields.
    Module,
}

impl Prim {
    fn parse(s: &str) -> Option<Prim> {
        Some(match s {
            "int" => Prim::Int,
            "uint" => Prim::Uint,
            "short" => Prim::Short,
            "ushort" => Prim::Ushort,
            "byte" => Prim::Byte,
            "sbyte" => Prim::Sbyte,
            "bool" => Prim::Bool,
            "float" => Prim::Float,
            "double" => Prim::Double,
            "long" => Prim::Long,
            "string" => Prim::Str,
            "module" => Prim::Module,
            _ => return None,
        })
    }

    /// Short, ushort, byte and sbyte: encoded through an inverse table.
    pub fn is_small(self) -> bool {
        matches!(self, Prim::Short | Prim::Ushort | Prim::Byte | Prim::Sbyte)
    }

    /// (number of raw bits, smallest decoded value) of a small type.
    fn small_range(self) -> (u32, i64) {
        match self {
            Prim::Short => (16, -32768),
            Prim::Ushort => (16, 0),
            Prim::Byte => (8, 0),
            Prim::Sbyte => (8, -128),
            _ => unreachable!(),
        }
    }
}

/// List length counter types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Counter {
    Byte,
    Short,
    Ushort,
    Int,
}

impl Counter {
    fn parse(s: &str) -> Option<Counter> {
        Some(match s {
            "byte" => Counter::Byte,
            "short" => Counter::Short,
            "ushort" => Counter::Ushort,
            "int" => Counter::Int,
            _ => return None,
        })
    }

    /// The largest list length the counter can hold.
    pub fn max(self) -> u64 {
        match self {
            Counter::Byte => 255,
            Counter::Short => 32767,
            Counter::Ushort => 65535,
            Counter::Int => u32::MAX as u64,
        }
    }
}

const NO_RAW: u32 = u32::MAX;

/// A field's type and conversion. Shared between fields with the same type and expression.
#[derive(Debug)]
pub struct FieldType {
    pub prim: Prim,
    /// Conversion applied after reading (`None`: the raw value as is).
    pub x: Option<Expr>,
    /// Ints: the rotation encoding undoes (`Err`: the conversion has another shape).
    pub(crate) rot: Result<u32, ()>,
    /// Small types: decoded value -> first raw value that decodes to it (built on first use).
    table: OnceLock<Box<[u32]>>,
}

impl FieldType {
    fn new(prim: Prim, x: Option<Expr>) -> Self {
        let rot = match &x {
            None => Ok(0),
            Some(e) => e.rotation().ok_or(()),
        };
        FieldType {
            prim,
            x,
            rot,
            table: OnceLock::new(),
        }
    }

    fn conv(&self, raw: u32) -> u32 {
        self.x.as_ref().map_or(raw, |x| x.eval(raw))
    }

    /// Decodes the raw bits of a small type (u16 for short types, u8 for byte types).
    pub(crate) fn small_value(&self, bits: u32) -> i64 {
        match self.prim {
            // read as a signed 16-bit value, then masked to 32 bits
            Prim::Short => self.conv(bits as u16 as i16 as i32 as u32) as u16 as i16 as i64,
            Prim::Ushort => (self.conv(bits as u16 as i16 as i32 as u32) & 0xFFFF) as i64,
            Prim::Byte => (self.conv(bits & 0xFF) & 0xFF) as i64,
            Prim::Sbyte => self.conv(bits as u8 as i8 as i32 as u32) as u8 as i8 as i64,
            _ => unreachable!(),
        }
    }

    /// The raw bits that decode to `v` (small types), the first in raw order, like `protocol.py`.
    pub(crate) fn small_raw(&self, v: i64) -> Option<u32> {
        let (bits, min) = self.prim.small_range();
        let table = self.table.get_or_init(|| {
            let n = 1usize << bits;
            let mut t = vec![NO_RAW; n].into_boxed_slice();
            for raw in 0..n as u32 {
                let idx = (self.small_value(raw) - min) as usize;
                if t[idx] == NO_RAW {
                    t[idx] = raw;
                }
            }
            t
        });
        let idx = v.checked_sub(min)?;
        let raw = *table.get(usize::try_from(idx).ok()?)?;
        (raw != NO_RAW).then_some(raw)
    }
}

/// One decoding step of a message.
#[derive(Debug, Clone)]
pub enum Step {
    /// A u16 parent id written before the fields (`pid`).
    ParentId {
        class: String,
        expected: Option<u16>,
    },
    /// The fields of the superclass, preceded by its id when `read_id` (`super`).
    Super {
        class: String,
        read_id: bool,
        expected: Option<u16>,
    },
    Field {
        name: String,
        ty: Arc<FieldType>,
    },
    List {
        name: String,
        counter: Counter,
        elem: Arc<FieldType>,
    },
}

/// A message (or submodule) class.
#[derive(Debug)]
pub struct MessageSpec {
    pub id: u16,
    /// Short class name, e.g. `ShipMoveRequest`.
    pub name: String,
    /// Full client class name, e.g. `net.bigpoint.darkorbit.com.module.ShipMoveRequest`.
    pub full_name: String,
    pub steps: Vec<Step>,
}

/// Field conversions OpenOrbit fixed by hand (read_spec.py could not recover them from the client;
/// `FIXES` in `protocol.py`): (class, field) -> (name, conversion).
/// (class, spec field) -> (field name, conversion).
type Fix = ((&'static str, &'static str), (&'static str, Json));

fn fixes() -> Vec<Fix> {
    use serde_json::json;
    let rot16 = || {
        json!([
            "or",
            ["and", ["shr", ["zx16", "v"], 8], 255],
            ["shl", ["zx16", "v"], 8]
        ])
    };
    vec![
        (
            ("SkylabModuleProductivityCommand", "bonus?"),
            ("bonus", json!(["or", ["shr", "v", 8], ["shl", "v", 24]])),
        ),
        (
            ("SkylabModuleProductivityCommand", "maxActive?"),
            (
                "maxActive",
                json!(["or", ["shr", "v", 22], ["shl", "v", 10]]),
            ),
        ),
        (
            ("SkylabModuleProductivityCommand", "inStorage?"),
            (
                "inStorage",
                json!(["or", ["shr", "v", 17], ["shl", "v", 15]]),
            ),
        ),
        // read(): CONCAT11((char)v, (char)((ushort)v >> 8)) - a rotation by 8 in 16 bits
        (("PetActivationCommand", "petLevel"), ("petLevel", rot16())),
        (
            ("QualitySettingsRequest", "qualityExplosion"),
            ("qualityExplosion", rot16()),
        ),
    ]
}

/// The compiled protocol.
#[derive(Debug)]
pub struct Spec {
    msgs: Vec<MessageSpec>,
    by_id: HashMap<u16, usize>,
    by_class: HashMap<String, usize>,
}

impl Spec {
    /// Loads `protocol_read.json`.
    pub fn load(path: impl AsRef<Path>) -> Result<Spec, SpecError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|source| SpecError::Io {
            path: path.display().to_string(),
            source,
        })?;
        Spec::parse(&text)
    }

    pub fn parse(text: &str) -> Result<Spec, SpecError> {
        let raw: serde_json::Map<String, Json> = serde_json::from_str(text)?;
        let fixes = fixes();
        let mut types: HashMap<(Prim, String), Arc<FieldType>> = HashMap::new();
        let mut intern = |prim: Prim, x: Option<&Json>| -> Result<Arc<FieldType>, String> {
            let x = match x {
                None | Some(Json::Null) => None,
                Some(Json::String(s)) if s == "v" => None,
                Some(j) => Some(j),
            };
            let key = (prim, x.map(|j| j.to_string()).unwrap_or_default());
            if let Some(t) = types.get(&key) {
                return Ok(t.clone());
            }
            let t = Arc::new(FieldType::new(prim, x.map(Expr::parse).transpose()?));
            types.insert(key, t.clone());
            Ok(t)
        };

        let mut msgs = Vec::with_capacity(raw.len());
        for (id, v) in &raw {
            let bad = |msg: String| SpecError::Invalid {
                id: id.clone(),
                msg,
            };
            let num: u16 = id.parse().map_err(|_| bad("id is not a u16".into()))?;
            let full_name = v["name"]
                .as_str()
                .ok_or_else(|| bad("no name".into()))?
                .to_string();
            let name = full_name
                .rsplit('.')
                .next()
                .unwrap_or(&full_name)
                .to_string();
            let mut steps = Vec::new();
            for s in v["steps"]
                .as_array()
                .ok_or_else(|| bad("no steps".into()))?
            {
                let t = s["t"]
                    .as_str()
                    .ok_or_else(|| bad("step without type".into()))?;
                let mut f = s["f"].as_str().map(str::to_string);
                let mut x = s.get("x").cloned();
                if let Some(fname) = &f {
                    if let Some((_, (nf, nx))) =
                        fixes.iter().find(|((c, ff), _)| *c == name && ff == fname)
                    {
                        f = Some(nf.to_string());
                        x = Some(nx.clone());
                    }
                }
                let class = || s["c"].as_str().unwrap_or_default().to_string();
                steps.push(match t {
                    "pid" => Step::ParentId {
                        class: class(),
                        expected: None,
                    },
                    "super" => Step::Super {
                        class: class(),
                        read_id: s["id"].as_bool().unwrap_or(true),
                        expected: None,
                    },
                    _ => {
                        let fname = f.ok_or_else(|| bad(format!("unnamed {t} field")))?;
                        if t == "list" {
                            let counter = s["n"]
                                .as_str()
                                .and_then(Counter::parse)
                                .ok_or_else(|| bad(format!("{fname}: list counter")))?;
                            let (et, ex) = match s.get("e") {
                                Some(e) if e.is_object() => {
                                    (e["t"].as_str().unwrap_or("module"), e.get("x"))
                                }
                                _ => ("module", None),
                            };
                            let prim = Prim::parse(et)
                                .ok_or_else(|| bad(format!("{fname}: element type {et}")))?;
                            Step::List {
                                name: fname,
                                counter,
                                elem: intern(prim, ex).map_err(bad)?,
                            }
                        } else {
                            let prim =
                                Prim::parse(t).ok_or_else(|| bad(format!("{fname}: type {t}")))?;
                            Step::Field {
                                name: fname,
                                ty: intern(prim, x.as_ref()).map_err(bad)?,
                            }
                        }
                    }
                });
            }
            msgs.push(MessageSpec {
                id: num,
                name,
                full_name,
                steps,
            });
        }

        let by_id: HashMap<u16, usize> = msgs.iter().enumerate().map(|(i, m)| (m.id, i)).collect();
        let by_class: HashMap<String, usize> = msgs
            .iter()
            .enumerate()
            .map(|(i, m)| (m.name.clone(), i))
            .collect();
        let id_of = |c: &str| by_class.get(c).map(|&i| msgs[i].id);
        let resolved: Vec<Vec<Option<u16>>> = msgs
            .iter()
            .map(|m| {
                m.steps
                    .iter()
                    .map(|s| match s {
                        Step::ParentId { class, .. } | Step::Super { class, .. }
                            if !class.is_empty() =>
                        {
                            id_of(class)
                        }
                        _ => None,
                    })
                    .collect()
            })
            .collect();
        for (m, ids) in msgs.iter_mut().zip(resolved) {
            for (s, id) in m.steps.iter_mut().zip(ids) {
                if let Step::ParentId { expected, .. } | Step::Super { expected, .. } = s {
                    *expected = id;
                }
            }
        }
        Ok(Spec {
            msgs,
            by_id,
            by_class,
        })
    }

    pub fn by_id(&self, id: u16) -> Option<&MessageSpec> {
        self.by_id.get(&id).map(|&i| &self.msgs[i])
    }

    pub fn by_class(&self, name: &str) -> Option<&MessageSpec> {
        self.by_class.get(name).map(|&i| &self.msgs[i])
    }

    pub fn id_of(&self, name: &str) -> Option<u16> {
        self.by_class(name).map(|m| m.id)
    }

    /// Class name of an id (`?<id>` when unknown).
    pub fn name(&self, id: u16) -> String {
        self.by_id(id)
            .map_or_else(|| format!("?{id}"), |m| m.name.clone())
    }

    pub fn messages(&self) -> impl Iterator<Item = &MessageSpec> {
        self.msgs.iter()
    }

    pub fn len(&self) -> usize {
        self.msgs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.msgs.is_empty()
    }
}
