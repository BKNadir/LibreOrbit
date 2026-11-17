//! Python's `repr(bytes)` (the request bodies in OpenOrbit's logs).

use std::fmt::Write;

/// `repr(b)`: `b'...'`, or `b"..."` when the bytes hold a `'` and no `"`.
pub fn bytes(b: &[u8]) -> String {
    let quote = if b.contains(&b'\'') && !b.contains(&b'"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::with_capacity(b.len() + 3);
    out.push('b');
    out.push(quote);
    for &c in b {
        match c {
            b'\\' => out.push_str("\\\\"),
            b'\t' => out.push_str("\\t"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            c if c as char == quote => {
                out.push('\\');
                out.push(quote);
            }
            0x20..=0x7e => out.push(c as char),
            c => write!(out, "\\x{c:02x}").unwrap(),
        }
    }
    out.push(quote);
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn like_python() {
        assert_eq!(super::bytes(b"lang=fr"), "b'lang=fr'");
        assert_eq!(super::bytes(b"it's"), "b\"it's\"");
        assert_eq!(super::bytes(b"a'\"\x00\xff\n"), "b'a\\'\"\\x00\\xff\\n'");
    }
}
