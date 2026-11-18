//! Log helpers of `protocol.py`: `fmt` (a short single line), `secrets_of` and `redact` (hide the session key).

use crate::value::{Struct, Value};

/// Field-name parts that mark a secret (`SECRET_FIELDS`).
pub const SECRET_FIELDS: [&str; 4] = ["sessionid", "password", "token", "secret"];
/// What a hidden secret is replaced with in the logs.
pub const HIDDEN: &str = "<gizlendi>";

fn secret_name(k: &str) -> bool {
    let k = k.to_lowercase();
    SECRET_FIELDS.iter().any(|s| k.contains(s))
}

/// `fmt(v, limit)`: `{a=1, b="x", m=Class{...}}`, cut at `limit` characters with `…`.
pub fn fmt(s: &Struct, top: bool, limit: usize) -> String {
    let mut out = String::new();
    fmt_struct(&mut out, s, top);
    if out.chars().count() <= limit {
        out
    } else {
        let mut cut: String = out.chars().take(limit).collect();
        cut.push('…');
        cut
    }
}

fn fmt_struct(out: &mut String, s: &Struct, top: bool) {
    if !top {
        out.push_str(&s.class);
    }
    out.push('{');
    for (i, (k, v)) in s
        .fields
        .iter()
        .filter(|(k, _)| !k.starts_with('_'))
        .enumerate()
    {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(k);
        out.push('=');
        fmt_value(out, v);
    }
    out.push('}');
}

fn fmt_value(out: &mut String, v: &Value) {
    match v {
        Value::Null => out.push_str("None"),
        Value::Bool(b) => out.push_str(if *b { "True" } else { "False" }),
        Value::Int(i) => out.push_str(&i.to_string()),
        Value::BigInt(f) => out.push_str(&format!("{f:.0}")),
        Value::Float(f) => out.push_str(&g6(*f)),
        Value::Str(s) => out.push_str(&json_str(s)),
        Value::List(l) => {
            out.push('[');
            for (i, x) in l.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                fmt_value(out, x);
            }
            out.push(']');
        }
        Value::Struct(s) => fmt_struct(out, s, false),
    }
}

/// `json.dumps(s, ensure_ascii=False)`.
fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
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
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Python's `'%.6g' % x`.
pub fn g6(x: f64) -> String {
    if x.is_nan() {
        return "nan".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "inf".into() } else { "-inf".into() };
    }
    if x == 0.0 {
        return if x.is_sign_negative() {
            "-0".into()
        } else {
            "0".into()
        };
    }
    // rounded to 6 significant digits: "d.ddddde±x"
    let e = format!("{x:.5e}");
    let (mant, exp) = e.split_once('e').expect("exponent form");
    let exp: i32 = exp.parse().expect("exponent");
    let strip = |s: String| -> String {
        if s.contains('.') {
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        } else {
            s
        }
    };
    if !(-4..6).contains(&exp) {
        let m = strip(mant.to_string());
        format!("{m}e{}{:02}", if exp < 0 { '-' } else { '+' }, exp.abs())
    } else {
        let decimals = (5 - exp).max(0) as usize;
        strip(format!("{x:.decimals$}"))
    }
}

/// `secrets_of`: the values (8 characters or more) of fields with a secret name, nested ones included.
pub fn secrets_of(s: &Struct, out: &mut Vec<String>) {
    for (k, v) in &s.fields {
        match v {
            Value::Str(x) if x.chars().count() >= 8 && secret_name(k) => {
                if !out.contains(x) {
                    out.push(x.clone());
                }
            }
            _ => secrets_in(v, out),
        }
    }
}

fn secrets_in(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::Struct(s) => secrets_of(s, out),
        Value::List(l) => l.iter().for_each(|x| secrets_in(x, out)),
        _ => {}
    }
}

/// `redact`: secret-named text fields become `<gizlendi>`, and the given values are hidden inside every text.
pub fn redact(s: &Struct, values: &[String]) -> Struct {
    let mut r = Struct {
        class: s.class.clone(),
        fields: Default::default(),
        supers: s.supers.clone(),
    };
    for (k, v) in &s.fields {
        let nv = match v {
            Value::Str(x) if !x.is_empty() && secret_name(k) => Value::Str(HIDDEN.into()),
            other => redact_value(other, values),
        };
        r.fields.insert(k.clone(), nv);
    }
    r
}

fn redact_value(v: &Value, values: &[String]) -> Value {
    match v {
        Value::Struct(s) => Value::Struct(Box::new(redact(s, values))),
        Value::List(l) => Value::List(l.iter().map(|x| redact_value(x, values)).collect()),
        Value::Str(x) => {
            let mut t = x.clone();
            for s in values {
                if t.contains(s.as_str()) {
                    t = t.replace(s.as_str(), HIDDEN);
                }
            }
            Value::Str(t)
        }
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn g6_like_python() {
        for (x, s) in [
            (1.5, "1.5"),
            (0.1, "0.1"),
            (123456.0, "123456"),
            (1234567.0, "1.23457e+06"),
            (0.0001, "0.0001"),
            (0.00001, "1e-05"),
            (-2.0, "-2"),
            (999999.5, "1e+06"),
            (1.23456789, "1.23457"),
        ] {
            assert_eq!(g6(x), s, "{x}");
        }
    }

    #[test]
    fn redaction() {
        let m = Struct::new("LoginRequest")
            .with("sessionID", "abcdefgh1234")
            .with("text", "key abcdefgh1234 here");
        let mut sec = Vec::new();
        secrets_of(&m, &mut sec);
        assert_eq!(
            fmt(&redact(&m, &sec), true, 300),
            "{sessionID=\"<gizlendi>\", text=\"key <gizlendi> here\"}"
        );
    }
}
