//! JSON text identical to Python's `json.dumps` / `json.dump`.
//!
//! * [`dumps`]: `json.dumps(v)` — `", "` and `": "` separators, non-ASCII escaped (`ensure_ascii=True`).
//! * [`dumps_with`]: other `ensure_ascii` / `indent` choices (`json.dump(d, fh, ensure_ascii=False, indent=1)`
//!   is how OpenOrbit writes its save files).
//!
//! Floats follow Python's `repr`: `1.0`, `0.1`, `1e+20`, `1.5e-05`.

use serde_json::Value;
use std::fmt::Write;

/// `json.dumps(v)`.
pub fn dumps(v: &Value) -> String {
    dumps_with(v, true, None)
}

/// `json.dumps(v, ensure_ascii=..., indent=...)`.
pub fn dumps_with(v: &Value, ensure_ascii: bool, indent: Option<usize>) -> String {
    let mut out = String::new();
    write(&mut out, v, ensure_ascii, indent, 0);
    out
}

/// A JSON string literal (`json.dumps(s)` for a str).
pub fn string(s: &str, ensure_ascii: bool) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    write_str(&mut out, s, ensure_ascii);
    out
}

fn newline(out: &mut String, indent: Option<usize>, level: usize) {
    if let Some(n) = indent {
        out.push('\n');
        out.extend(std::iter::repeat_n(' ', n * level));
    }
}

fn write(out: &mut String, v: &Value, ascii: bool, indent: Option<usize>, level: usize) {
    // with an indent Python's item separator is "," (plus the newline); without it ", "
    let item_sep = if indent.is_some() { "," } else { ", " };
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                write!(out, "{i}").unwrap();
            } else if let Some(u) = n.as_u64() {
                write!(out, "{u}").unwrap();
            } else {
                out.push_str(&float_repr(n.as_f64().unwrap_or(f64::NAN)));
            }
        }
        Value::String(s) => write_str(out, s, ascii),
        Value::Array(a) => {
            if a.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push_str(item_sep);
                }
                newline(out, indent, level + 1);
                write(out, x, ascii, indent, level + 1);
            }
            newline(out, indent, level);
            out.push(']');
        }
        Value::Object(m) => {
            if m.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push('{');
            for (i, (k, x)) in m.iter().enumerate() {
                if i > 0 {
                    out.push_str(item_sep);
                }
                newline(out, indent, level + 1);
                write_str(out, k, ascii);
                out.push_str(": ");
                write(out, x, ascii, indent, level + 1);
            }
            newline(out, indent, level);
            out.push('}');
        }
    }
}

fn write_str(out: &mut String, s: &str, ascii: bool) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => write!(out, "\\u{:04x}", c as u32).unwrap(),
            // ensure_ascii escapes everything outside ' '..'~' (0x7f included)
            c if ascii && !(' '..='~').contains(&c) => {
                let mut buf = [0u16; 2];
                for unit in c.encode_utf16(&mut buf) {
                    write!(out, "\\u{unit:04x}").unwrap();
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Python's `repr(float)`: the shortest round-trip digits, in exponent form below 1e-4 and from 1e16.
pub fn float_repr(f: f64) -> String {
    if f.is_nan() {
        return "NaN".into();
    }
    if f.is_infinite() {
        return if f > 0.0 {
            "Infinity".into()
        } else {
            "-Infinity".into()
        };
    }
    if f == 0.0 {
        return if f.is_sign_negative() {
            "-0.0".into()
        } else {
            "0.0".into()
        };
    }
    // "{:e}" gives the shortest round-trip digits: "1.2345e3", "-5e-7"
    let e = format!("{f:e}");
    let (mant, exp) = e.split_once('e').expect("exponent form");
    let exp: i32 = exp.parse().expect("exponent");
    let neg = mant.starts_with('-');
    let digits: String = mant.chars().filter(char::is_ascii_digit).collect();
    let sign = if neg { "-" } else { "" };
    if (-4..16).contains(&exp) {
        let n = digits.len() as i32;
        let s = if exp >= n - 1 {
            // integral: all digits then zeros, plus ".0"
            format!("{digits}{}.0", "0".repeat((exp - (n - 1)) as usize))
        } else if exp >= 0 {
            let (a, b) = digits.split_at(exp as usize + 1);
            format!("{a}.{b}")
        } else {
            format!("0.{}{digits}", "0".repeat((-exp - 1) as usize))
        };
        format!("{sign}{s}")
    } else {
        let m = if digits.len() > 1 {
            format!("{}.{}", &digits[..1], &digits[1..])
        } else {
            digits
        };
        let es = if exp < 0 { '-' } else { '+' };
        format!("{sign}{m}e{es}{:02}", exp.abs())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn floats_like_python() {
        for (f, s) in [
            (1.0, "1.0"),
            (0.1, "0.1"),
            (1e20, "1e+20"),
            (1.5e-5, "1.5e-05"),
            (123456.789, "123456.789"),
            (1e16, "1e+16"),
            (9999999999999998.0, "9999999999999998.0"),
            (0.0001, "0.0001"),
            (-2.5, "-2.5"),
            (1.7976931348623157e308, "1.7976931348623157e+308"),
        ] {
            assert_eq!(float_repr(f), s, "{f}");
        }
    }

    #[test]
    fn dumps_like_python() {
        let v = json!({"a": [1, 2.5, "\u{e9}\u{20ac}\u{1d11e}"], "b": {}, "c": [], "d": null, "e": "q\"\\\n\u{7f}"});
        assert_eq!(
            dumps(&v),
            r#"{"a": [1, 2.5, "\u00e9\u20ac\ud834\udd1e"], "b": {}, "c": [], "d": null, "e": "q\"\\\n\u007f"}"#
        );
        assert_eq!(
            dumps_with(&json!({"lang": "fr", "x": [1]}), false, Some(1)),
            "{\n \"lang\": \"fr\",\n \"x\": [\n  1\n ]\n}"
        );
    }
}
