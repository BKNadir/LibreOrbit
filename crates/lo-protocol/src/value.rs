//! Dynamic message values.
//!
//! A decoded message is a [`Struct`]: its class name, its fields in wire order and the
//! superclass ids that differed from the expected ones (kept so re-encoding gives the same bytes).
//! The JSON form ([`Struct::to_json`]) is the same as the dicts of OpenOrbit's `protocol.py`:
//! `{"_": class, "_super": {"0": id}, field: value, ...}` (no `"_"` at the top level).

use indexmap::IndexMap;
use serde_json::{Map, Number, Value as Json};
use std::collections::BTreeMap;
use std::fmt;

/// A field value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// An empty submodule (id 0) or a missing value.
    Null,
    Bool(bool),
    Int(i64),
    /// An integral number beyond the i64 range (a double the client sent as an integer; a Python int in `protocol.py`).
    BigInt(f64),
    Float(f64),
    Str(String),
    List(Vec<Value>),
    Struct(Box<Struct>),
}

/// A message or a submodule.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Struct {
    /// Short class name, e.g. `ShipMoveRequest`.
    pub class: String,
    /// Fields in wire order.
    pub fields: IndexMap<String, Value>,
    /// Superclass ids read from the wire that differed from the expected ones, by position.
    pub supers: BTreeMap<u32, u16>,
}

impl Struct {
    pub fn new(class: impl Into<String>) -> Self {
        Struct {
            class: class.into(),
            ..Default::default()
        }
    }

    /// Builder: sets a field.
    pub fn with(mut self, name: impl Into<String>, v: impl Into<Value>) -> Self {
        self.fields.insert(name.into(), v.into());
        self
    }

    pub fn set(&mut self, name: impl Into<String>, v: impl Into<Value>) {
        self.fields.insert(name.into(), v.into());
    }

    /// A field by name; a spec name ending in `?` (name not recovered from the client) also
    /// matches the name without it, and the other way round.
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.fields
            .get(name)
            .or_else(|| match name.strip_suffix('?') {
                Some(base) => self.fields.get(base),
                None => self.fields.get(&format!("{name}?")),
            })
    }

    pub fn int(&self, name: &str) -> Option<i64> {
        self.get(name).and_then(Value::as_i64)
    }

    pub fn float(&self, name: &str) -> Option<f64> {
        self.get(name).and_then(Value::as_f64)
    }

    pub fn str(&self, name: &str) -> Option<&str> {
        self.get(name).and_then(Value::as_str)
    }

    pub fn bool(&self, name: &str) -> Option<bool> {
        self.get(name).and_then(Value::as_bool)
    }

    pub fn module(&self, name: &str) -> Option<&Struct> {
        self.get(name).and_then(Value::as_struct)
    }

    pub fn list(&self, name: &str) -> Option<&[Value]> {
        self.get(name).and_then(Value::as_list)
    }

    /// The JSON form of `protocol.py` (`top`: a message, without the `"_"` class key).
    pub fn to_json(&self, top: bool) -> Json {
        let mut m = Map::new();
        if !top {
            m.insert("_".into(), Json::String(self.class.clone()));
        }
        if !self.supers.is_empty() {
            let s = self
                .supers
                .iter()
                .map(|(k, v)| (k.to_string(), Json::from(*v)))
                .collect();
            m.insert("_super".into(), Json::Object(s));
        }
        for (k, v) in &self.fields {
            m.insert(k.clone(), v.to_json());
        }
        Json::Object(m)
    }

    /// Reads the JSON form. `class` is used when the object has no `"_"` key (a message).
    pub fn from_json(j: &Json, class: Option<&str>) -> Result<Struct, String> {
        let m = j.as_object().ok_or("a struct must be a JSON object")?;
        let class = match m.get("_") {
            Some(Json::String(c)) => c.clone(),
            Some(other) => return Err(format!("invalid class {other}")),
            None => class.ok_or("struct without class (\"_\")")?.to_string(),
        };
        let mut s = Struct::new(class);
        for (k, v) in m {
            match k.as_str() {
                "_" => {}
                "_super" => {
                    for (n, id) in v.as_object().ok_or("_super must be an object")? {
                        let n: u32 = n.parse().map_err(|_| format!("_super key {n:?}"))?;
                        let id = id
                            .as_u64()
                            .filter(|i| *i <= 0xFFFF)
                            .ok_or("_super id must be a u16")?;
                        s.supers.insert(n, id as u16);
                    }
                }
                _ => {
                    s.fields.insert(k.clone(), Value::from_json(v)?);
                }
            }
        }
        Ok(s)
    }
}

impl Value {
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            Value::Float(f) if f.fract() == 0.0 => Some(*f as i64),
            Value::Bool(b) => Some(*b as i64),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Int(i) => Some(*i as f64),
            Value::Float(f) | Value::BigInt(f) => Some(*f),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_struct(&self) -> Option<&Struct> {
        match self {
            Value::Struct(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&[Value]> {
        match self {
            Value::List(l) => Some(l),
            _ => None,
        }
    }

    /// Python truthiness (used for bool fields).
    pub fn truthy(&self) -> bool {
        match self {
            Value::Null => false,
            Value::Bool(b) => *b,
            Value::Int(i) => *i != 0,
            Value::Float(f) | Value::BigInt(f) => *f != 0.0,
            Value::Str(s) => !s.is_empty(),
            Value::List(l) => !l.is_empty(),
            Value::Struct(_) => true,
        }
    }

    /// JSON form; non-finite floats (which JSON cannot hold) become `{"__f": "nan" | "inf" | "-inf"}`.
    pub fn to_json(&self) -> Json {
        match self {
            Value::Null => Json::Null,
            Value::Bool(b) => Json::Bool(*b),
            Value::Int(i) => Json::from(*i),
            Value::Float(f) | Value::BigInt(f) => match Number::from_f64(*f) {
                Some(n) => Json::Number(n),
                None => {
                    let tag = if f.is_nan() {
                        "nan"
                    } else if *f > 0.0 {
                        "inf"
                    } else {
                        "-inf"
                    };
                    serde_json::json!({ "__f": tag })
                }
            },
            Value::Str(s) => Json::String(s.clone()),
            Value::List(l) => Json::Array(l.iter().map(Value::to_json).collect()),
            Value::Struct(s) => s.to_json(false),
        }
    }

    pub fn from_json(j: &Json) -> Result<Value, String> {
        Ok(match j {
            Json::Null => Value::Null,
            Json::Bool(b) => Value::Bool(*b),
            Json::Number(n) => match n.as_i64() {
                Some(i) => Value::Int(i),
                None => Value::Float(n.as_f64().ok_or("number out of range")?),
            },
            Json::String(s) => Value::Str(s.clone()),
            Json::Array(a) => {
                Value::List(a.iter().map(Value::from_json).collect::<Result<_, _>>()?)
            }
            Json::Object(m) => match m.get("__f").and_then(Json::as_str) {
                Some("nan") => Value::Float(f64::NAN),
                Some("inf") => Value::Float(f64::INFINITY),
                Some("-inf") => Value::Float(f64::NEG_INFINITY),
                _ => Value::Struct(Box::new(Struct::from_json(j, None)?)),
            },
        })
    }
}

impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Value::Bool(v)
    }
}
impl From<i32> for Value {
    fn from(v: i32) -> Self {
        Value::Int(v as i64)
    }
}
impl From<u32> for Value {
    fn from(v: u32) -> Self {
        Value::Int(v as i64)
    }
}
impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Value::Int(v)
    }
}
impl From<f32> for Value {
    fn from(v: f32) -> Self {
        Value::Float(v as f64)
    }
}
impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Value::Float(v)
    }
}
impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::Str(v.to_string())
    }
}
impl From<String> for Value {
    fn from(v: String) -> Self {
        Value::Str(v)
    }
}
impl From<Struct> for Value {
    fn from(v: Struct) -> Self {
        Value::Struct(Box::new(v))
    }
}
impl From<Option<Struct>> for Value {
    fn from(v: Option<Struct>) -> Self {
        v.map_or(Value::Null, Value::from)
    }
}
impl<T: Into<Value>> From<Vec<T>> for Value {
    fn from(v: Vec<T>) -> Self {
        Value::List(v.into_iter().map(Into::into).collect())
    }
}

/// A short, readable single line (for logs), like `fmt` in `protocol.py`.
impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Null => f.write_str("None"),
            Value::Bool(b) => f.write_str(if *b { "True" } else { "False" }),
            Value::Int(i) => write!(f, "{i}"),
            Value::Float(x) => write!(f, "{x:.6}"),
            Value::BigInt(x) => write!(f, "{x:.0}"),
            Value::Str(s) => write!(f, "{}", Json::String(s.clone())),
            Value::List(l) => {
                f.write_str("[")?;
                for (i, v) in l.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{v}")?;
                }
                f.write_str("]")
            }
            Value::Struct(s) => write!(f, "{s}"),
        }
    }
}

impl fmt::Display for Struct {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{{", self.class)?;
        for (i, (k, v)) in self.fields.iter().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            write!(f, "{k}={v}")?;
        }
        f.write_str("}")
    }
}
