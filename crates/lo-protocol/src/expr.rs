//! Field conversions: the bit operations the client's `read()` applies to a raw value.
//!
//! The spec stores them as nested JSON arrays, e.g. `["or", ["shr", "v", 29], ["shl", "v", 3]]`
//! (a rotation), with `"v"` standing for the raw value. Evaluation uses 32-bit unsigned
//! arithmetic, like `eval_x` in OpenOrbit's `protocol.py`.

use serde_json::Value as Json;

/// One-operand operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    /// Sign-extend the low 16 bits.
    Sx16,
    /// Keep the low 16 bits.
    Zx16,
    /// Sign-extend the low 8 bits.
    Sx8,
    /// Keep the low 8 bits.
    Zx8,
    Not,
    Neg,
}

/// Two-operand operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Or,
    And,
    Xor,
    Add,
    Sub,
    Shl,
    Shr,
    Sar,
    Rol,
    Ror,
}

/// A conversion expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    /// The raw value.
    V,
    Const(u32),
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
}

impl Expr {
    /// Parses the JSON form used by `protocol_read.json`.
    pub fn parse(j: &Json) -> Result<Expr, String> {
        match j {
            Json::String(s) if s == "v" => Ok(Expr::V),
            Json::Number(n) => {
                let v = n
                    .as_i64()
                    .ok_or_else(|| format!("constant is not an integer: {n}"))?;
                Ok(Expr::Const(v as u32)) // `e & M32`
            }
            Json::Array(a) if !a.is_empty() => {
                let op = a[0].as_str().ok_or("operation name is not a string")?;
                let arg = |i: usize| -> Result<Box<Expr>, String> {
                    a.get(i)
                        .ok_or_else(|| format!("{op}: operand {i} missing"))
                        .and_then(Expr::parse)
                        .map(Box::new)
                };
                let un = match op {
                    "sx16" => Some(UnOp::Sx16),
                    "zx16" => Some(UnOp::Zx16),
                    "sx8" => Some(UnOp::Sx8),
                    "zx8" => Some(UnOp::Zx8),
                    "not" => Some(UnOp::Not),
                    "neg" => Some(UnOp::Neg),
                    _ => None,
                };
                if let Some(u) = un {
                    return Ok(Expr::Unary(u, arg(1)?));
                }
                let bin = match op {
                    "or" => BinOp::Or,
                    "and" => BinOp::And,
                    "xor" => BinOp::Xor,
                    "add" => BinOp::Add,
                    "sub" => BinOp::Sub,
                    "shl" => BinOp::Shl,
                    "shr" => BinOp::Shr,
                    "sar" => BinOp::Sar,
                    "rol" => BinOp::Rol,
                    "ror" => BinOp::Ror,
                    _ => return Err(format!("unknown operation {op:?}")),
                };
                Ok(Expr::Binary(bin, arg(1)?, arg(2)?))
            }
            other => Err(format!("invalid expression {other}")),
        }
    }

    /// Applies the operations to a raw value (32-bit unsigned arithmetic).
    pub fn eval(&self, v: u32) -> u32 {
        match self {
            Expr::V => v,
            Expr::Const(c) => *c,
            Expr::Unary(op, a) => {
                let a = a.eval(v);
                match op {
                    UnOp::Sx16 => a as u16 as i16 as i32 as u32,
                    UnOp::Zx16 => a & 0xFFFF,
                    UnOp::Sx8 => a as u8 as i8 as i32 as u32,
                    UnOp::Zx8 => a & 0xFF,
                    UnOp::Not => !a,
                    UnOp::Neg => a.wrapping_neg(),
                }
            }
            Expr::Binary(op, a, b) => {
                let a = a.eval(v);
                let b = b.eval(v);
                let s = b & 31;
                match op {
                    BinOp::Or => a | b,
                    BinOp::And => a & b,
                    BinOp::Xor => a ^ b,
                    BinOp::Add => a.wrapping_add(b),
                    BinOp::Sub => a.wrapping_sub(b),
                    BinOp::Shl => a << s,
                    BinOp::Shr => a >> s,
                    BinOp::Sar => ((a as i32) >> s) as u32,
                    BinOp::Rol => a.rotate_left(s),
                    BinOp::Ror => a.rotate_right(s),
                }
            }
        }
    }

    /// For the int obfuscation `(raw >>> a) | (raw << b)`, the `a` that encoding rotates back by.
    /// `None` when the expression has another shape (it cannot be inverted this way).
    pub fn rotation(&self) -> Option<u32> {
        match self {
            Expr::Binary(BinOp::Or, l, r) => match (&**l, &**r) {
                (Expr::Binary(BinOp::Shr, _, a), Expr::Binary(BinOp::Shl, _, _)) => match **a {
                    Expr::Const(a) => Some(a & 31),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rotation_roundtrip() {
        let x = Expr::parse(&json!(["or", ["shr", "v", 29], ["shl", "v", 3]])).unwrap();
        assert_eq!(x.rotation(), Some(29));
        for v in [0u32, 1, 0xDEADBEEF, u32::MAX, 12345] {
            assert_eq!(x.eval(v.rotate_left(29)), v);
        }
    }

    #[test]
    fn sign_extension() {
        let x = Expr::parse(&json!(["sx16", "v"])).unwrap();
        assert_eq!(x.eval(0xFFFF), u32::MAX);
        assert_eq!(x.eval(0x7FFF), 0x7FFF);
        let x = Expr::parse(&json!(["zx16", "v"])).unwrap();
        assert_eq!(x.eval(0xFFFF_FFFF), 0xFFFF);
    }
}
