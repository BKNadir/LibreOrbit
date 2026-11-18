//! Decoding and encoding of message payloads (`decode_message` / `encode_message` in `protocol.py`).

use crate::spec::{Counter, FieldType, Prim, Spec, Step};
use crate::value::{Struct, Value};

/// Deepest submodule nesting accepted while decoding (`protocol.py` stops at Python's recursion limit).
pub const MAX_DEPTH: usize = 64;
/// Longest list accepted while decoding.
const MAX_LIST: i64 = 100_000;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum DecodeError {
    #[error("data ended ({want} bytes requested, {left} left)")]
    Short { want: usize, left: usize },
    #[error("impossible list length: {n} ({field})")]
    ListLength { n: i64, field: String },
    #[error("unknown submodule id {0}")]
    UnknownModule(u16),
    #[error("superclass unknown: {0}")]
    UnknownSuper(String),
    #[error("nested module depth too large")]
    Depth,
    #[error("cannot convert float {0} to integer")]
    NonFinite(f64),
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum EncodeError {
    #[error("unknown message {0:?}")]
    UnknownMessage(String),
    #[error("superclass id unknown ({0})")]
    UnknownParent(String),
    #[error("field {field}: {msg}")]
    Value { field: String, msg: String },
    #[error("list {field} has {len} items; its counter ({counter:?}) allows at most {max}")]
    ListTooLong {
        field: String,
        len: usize,
        counter: Counter,
        max: u64,
    },
}

/// The result of decoding one payload. Like `decode_message`, a decoding error keeps the fields
/// read before it.
#[derive(Debug, Clone, PartialEq)]
pub struct Decoded {
    /// `None` when the payload is shorter than an id.
    pub id: Option<u16>,
    /// Class name (`?<id>` for an unknown id, `?` without an id).
    pub name: String,
    pub msg: Struct,
    /// Bytes left after decoding (or at the error).
    pub remaining: usize,
    pub error: Option<String>,
}

impl Decoded {
    pub fn is_ok(&self) -> bool {
        self.error.is_none()
    }
}

struct Reader<'a> {
    b: &'a [u8],
    o: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
        let left = self.b.len() - self.o;
        if n > left {
            return Err(DecodeError::Short { want: n, left });
        }
        let r = &self.b[self.o..self.o + n];
        self.o += n;
        Ok(r)
    }
    fn arr<const N: usize>(&mut self) -> Result<[u8; N], DecodeError> {
        Ok(self.take(N)?.try_into().expect("length checked"))
    }
    fn u8(&mut self) -> Result<u8, DecodeError> {
        Ok(self.take(1)?[0])
    }
    fn i16(&mut self) -> Result<i16, DecodeError> {
        Ok(i16::from_be_bytes(self.arr()?))
    }
    fn u16(&mut self) -> Result<u16, DecodeError> {
        Ok(u16::from_be_bytes(self.arr()?))
    }
    fn u32(&mut self) -> Result<u32, DecodeError> {
        Ok(u32::from_be_bytes(self.arr()?))
    }
    fn f32(&mut self) -> Result<f32, DecodeError> {
        Ok(f32::from_be_bytes(self.arr()?))
    }
    fn f64(&mut self) -> Result<f64, DecodeError> {
        Ok(f64::from_be_bytes(self.arr()?))
    }
    fn left(&self) -> usize {
        self.b.len() - self.o
    }
}

/// An integral double as a value: `Int` when it fits in an i64.
fn integral(d: f64) -> Value {
    const LIMIT: f64 = 9_223_372_036_854_775_808.0; // 2^63
    if (-LIMIT..LIMIT).contains(&d) {
        Value::Int(d as i64)
    } else {
        Value::BigInt(d)
    }
}

impl Spec {
    /// Decodes a message payload (u16 id + fields).
    pub fn decode(&self, payload: &[u8]) -> Decoded {
        if payload.len() < 2 {
            return Decoded {
                id: None,
                name: "?".into(),
                msg: Struct::default(),
                remaining: payload.len(),
                error: Some("short payload".into()),
            };
        }
        let id = u16::from_be_bytes([payload[0], payload[1]]);
        let Some(m) = self.by_id(id) else {
            return Decoded {
                id: Some(id),
                name: format!("?{id}"),
                msg: Struct::default(),
                remaining: payload.len() - 2,
                error: Some("unknown message id".into()),
            };
        };
        let mut r = Reader { b: payload, o: 2 };
        let mut msg = Struct::new(m.name.clone());
        let error = self
            .steps(&mut r, &m.steps, &mut msg, &mut 0, 0)
            .err()
            .map(|e| e.to_string());
        Decoded {
            id: Some(id),
            name: m.name.clone(),
            msg,
            remaining: r.left(),
            error,
        }
    }

    fn steps(
        &self,
        r: &mut Reader,
        steps: &[Step],
        out: &mut Struct,
        n: &mut u32,
        depth: usize,
    ) -> Result<(), DecodeError> {
        for s in steps {
            match s {
                Step::ParentId { expected, .. }
                | Step::Super {
                    expected,
                    read_id: true,
                    ..
                } => {
                    // a superclass id that differs from the expected one is kept (for re-encoding)
                    let v = r.u16()?;
                    if Some(v) != *expected {
                        out.supers.insert(*n, v);
                    }
                    *n += 1;
                }
                _ => {}
            }
            match s {
                Step::ParentId { .. } => {}
                Step::Super { class, .. } => {
                    let parent = self
                        .by_class(class)
                        .ok_or_else(|| DecodeError::UnknownSuper(class.clone()))?;
                    self.steps(r, &parent.steps, out, n, depth)?;
                }
                Step::Field { name, ty } => {
                    let v = self.prim(r, ty, depth)?;
                    out.fields.insert(name.clone(), v);
                }
                Step::List {
                    name,
                    counter,
                    elem,
                } => {
                    let len = match counter {
                        Counter::Byte => r.u8()? as i64,
                        Counter::Short => r.i16()? as i64,
                        Counter::Ushort => r.u16()? as i64,
                        Counter::Int => r.u32()? as i64,
                    };
                    if !(0..=MAX_LIST).contains(&len) {
                        return Err(DecodeError::ListLength {
                            n: len,
                            field: name.clone(),
                        });
                    }
                    let mut items = Vec::with_capacity(len.min(1024) as usize);
                    for _ in 0..len {
                        items.push(self.prim(r, elem, depth)?);
                    }
                    out.fields.insert(name.clone(), Value::List(items));
                }
            }
        }
        Ok(())
    }

    fn prim(&self, r: &mut Reader, ty: &FieldType, depth: usize) -> Result<Value, DecodeError> {
        Ok(match ty.prim {
            Prim::Int | Prim::Uint => {
                let raw = r.u32()?;
                let v = ty.x.as_ref().map_or(raw, |x| x.eval(raw));
                Value::Int(if ty.prim == Prim::Int {
                    v as i32 as i64
                } else {
                    v as i64
                })
            }
            Prim::Short | Prim::Ushort => Value::Int(ty.small_value(r.u16()? as u32)),
            Prim::Byte | Prim::Sbyte => Value::Int(ty.small_value(r.u8()? as u32)),
            Prim::Bool => Value::Bool(r.u8()? != 0),
            Prim::Float => Value::Float(r.f32()? as f64),
            Prim::Double => {
                let d = r.f64()?;
                if !d.is_finite() {
                    return Err(DecodeError::NonFinite(d));
                }
                if d.fract() == 0.0 {
                    integral(d)
                } else {
                    Value::Float(d)
                }
            }
            Prim::Long => {
                let d = r.f64()?;
                if !d.is_finite() {
                    return Err(DecodeError::NonFinite(d));
                }
                integral(d.trunc())
            }
            Prim::Str => {
                let n = r.u16()? as usize;
                Value::Str(String::from_utf8_lossy(r.take(n)?).into_owned())
            }
            Prim::Module => self.module(r, depth + 1)?,
        })
    }

    fn module(&self, r: &mut Reader, depth: usize) -> Result<Value, DecodeError> {
        let id = r.u16()?;
        if id == 0 {
            return Ok(Value::Null);
        }
        let m = self.by_id(id).ok_or(DecodeError::UnknownModule(id))?;
        if depth > MAX_DEPTH {
            return Err(DecodeError::Depth);
        }
        let mut s = Struct::new(m.name.clone());
        self.steps(r, &m.steps, &mut s, &mut 0, depth)?;
        Ok(Value::Struct(Box::new(s)))
    }

    /// Encodes a message (its class gives the id); missing fields take their type's default.
    pub fn encode(&self, msg: &Struct) -> Result<Vec<u8>, EncodeError> {
        let m = self
            .by_class(&msg.class)
            .ok_or_else(|| EncodeError::UnknownMessage(msg.class.clone()))?;
        let mut out = Vec::with_capacity(64);
        out.extend_from_slice(&m.id.to_be_bytes());
        self.enc_steps(&mut out, &m.steps, msg, &mut 0)?;
        Ok(out)
    }

    fn enc_steps(
        &self,
        out: &mut Vec<u8>,
        steps: &[Step],
        f: &Struct,
        n: &mut u32,
    ) -> Result<(), EncodeError> {
        for s in steps {
            match s {
                Step::ParentId { class, expected }
                | Step::Super {
                    class,
                    expected,
                    read_id: true,
                } => {
                    let v = f
                        .supers
                        .get(n)
                        .copied()
                        .or(*expected)
                        .ok_or_else(|| EncodeError::UnknownParent(class.clone()))?;
                    out.extend_from_slice(&v.to_be_bytes());
                    *n += 1;
                }
                _ => {}
            }
            match s {
                Step::ParentId { .. } => {}
                Step::Super { class, .. } => {
                    let parent = self
                        .by_class(class)
                        .ok_or_else(|| EncodeError::UnknownParent(class.clone()))?;
                    self.enc_steps(out, &parent.steps, f, n)?;
                }
                Step::Field { name, ty } => {
                    let v = lookup(f, name).cloned().unwrap_or_else(|| default(ty.prim));
                    self.enc_prim(out, ty, &v, name)?;
                }
                Step::List {
                    name,
                    counter,
                    elem,
                } => {
                    let empty = Vec::new();
                    let items = match lookup(f, name) {
                        None | Some(Value::Null) => &empty,
                        Some(Value::List(l)) => l,
                        Some(other) => {
                            return Err(bad(name, format!("a list was expected, not {other}")));
                        }
                    };
                    let len = items.len();
                    if len as u64 > counter.max() {
                        return Err(EncodeError::ListTooLong {
                            field: name.clone(),
                            len,
                            counter: *counter,
                            max: counter.max(),
                        });
                    }
                    match counter {
                        Counter::Byte => out.push(len as u8),
                        Counter::Short => out.extend_from_slice(&(len as i16).to_be_bytes()),
                        Counter::Ushort => out.extend_from_slice(&(len as u16).to_be_bytes()),
                        Counter::Int => out.extend_from_slice(&(len as u32).to_be_bytes()),
                    }
                    for item in items {
                        self.enc_prim(out, elem, item, name)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn enc_prim(
        &self,
        out: &mut Vec<u8>,
        ty: &FieldType,
        v: &Value,
        name: &str,
    ) -> Result<(), EncodeError> {
        match ty.prim {
            Prim::Int | Prim::Uint => {
                let i = to_int(v).map_err(|m| bad(name, m))?;
                let hi: i64 = if ty.prim == Prim::Int {
                    i32::MAX as i64
                } else {
                    u32::MAX as i64
                };
                let lo: i64 = i32::MIN as i64;
                let c = i.clamp(lo, hi);
                if c != i {
                    // it would silently wrap to negative (earnings, points): clamped to the limit
                    tracing::warn!("field {name} did not fit in 32 bits: {i} -> {c}");
                }
                let mut raw = c as u32;
                if ty.x.is_some() {
                    // read(): value = (raw >>> a) | (raw << b)  ->  raw = the value rotated left by a bits
                    let a = ty.rot.map_err(|_| {
                        bad(
                            name,
                            format!("int conversion cannot be inverted {:?}", ty.x),
                        )
                    })?;
                    raw = raw.rotate_left(a);
                }
                out.extend_from_slice(&raw.to_be_bytes());
            }
            Prim::Short | Prim::Ushort | Prim::Byte | Prim::Sbyte => {
                let i = to_int(v).map_err(|m| bad(name, m))?;
                let raw = ty
                    .small_raw(i)
                    .ok_or_else(|| bad(name, format!("{:?} cannot hold value {i}", ty.prim)))?;
                if matches!(ty.prim, Prim::Short | Prim::Ushort) {
                    out.extend_from_slice(&(raw as u16).to_be_bytes());
                } else {
                    out.push(raw as u8);
                }
            }
            Prim::Bool => out.push(v.truthy() as u8),
            Prim::Float => {
                let f = to_float(v).map_err(|m| bad(name, m))?;
                let s = f as f32;
                if f.is_finite() && s.is_infinite() {
                    return Err(bad(name, "float too large to pack with f format".into()));
                }
                out.extend_from_slice(&s.to_be_bytes());
            }
            Prim::Double | Prim::Long => {
                let f = to_float(v).map_err(|m| bad(name, m))?;
                out.extend_from_slice(&f.to_be_bytes());
            }
            Prim::Str => {
                let s = match v {
                    Value::Str(s) => s.as_str(),
                    v if !v.truthy() => "",
                    other => return Err(bad(name, format!("a string was expected, not {other}"))),
                };
                let n = u16::try_from(s.len())
                    .map_err(|_| bad(name, format!("string too long ({} bytes)", s.len())))?;
                out.extend_from_slice(&n.to_be_bytes());
                out.extend_from_slice(s.as_bytes());
            }
            Prim::Module => match v {
                Value::Null => out.extend_from_slice(&[0, 0]),
                Value::Struct(s) => {
                    let m = self
                        .by_class(&s.class)
                        .ok_or_else(|| bad(name, format!("unknown module {:?}", s.class)))?;
                    out.extend_from_slice(&m.id.to_be_bytes());
                    self.enc_steps(out, &m.steps, s, &mut 0)?;
                }
                other => return Err(bad(name, format!("a module was expected, not {other}"))),
            },
        }
        Ok(())
    }
}

fn bad(field: &str, msg: String) -> EncodeError {
    EncodeError::Value {
        field: field.to_string(),
        msg,
    }
}

/// The field a spec name refers to: a name ending in `?` also matches the name without it.
fn lookup<'a>(f: &'a Struct, name: &str) -> Option<&'a Value> {
    f.fields
        .get(name)
        .or_else(|| name.strip_suffix('?').and_then(|base| f.fields.get(base)))
}

fn default(p: Prim) -> Value {
    match p {
        Prim::Str => Value::Str(String::new()),
        Prim::Module => Value::Null,
        Prim::Bool => Value::Bool(false),
        Prim::Float | Prim::Double => Value::Float(0.0),
        _ => Value::Int(0),
    }
}

/// Python's `int(v)`.
fn to_int(v: &Value) -> Result<i64, String> {
    match v {
        Value::Int(i) => Ok(*i),
        Value::Bool(b) => Ok(*b as i64),
        Value::Float(f) | Value::BigInt(f) if f.is_finite() => Ok(f.trunc() as i64),
        Value::Str(s) => s
            .trim()
            .parse()
            .map_err(|_| format!("invalid literal for int(): {s:?}")),
        other => Err(format!("an integer was expected, not {other}")),
    }
}

/// Python's `float(v)`.
fn to_float(v: &Value) -> Result<f64, String> {
    match v {
        Value::Int(i) => Ok(*i as f64),
        Value::Float(f) | Value::BigInt(f) => Ok(*f),
        Value::Bool(b) => Ok(*b as i64 as f64),
        Value::Str(s) => s
            .trim()
            .parse()
            .map_err(|_| format!("could not convert string to float: {s:?}")),
        other => Err(format!("a number was expected, not {other}")),
    }
}
