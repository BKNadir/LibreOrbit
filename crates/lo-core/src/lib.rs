//! Shared pieces of the LibreOrbit servers.
//!
//! * [`Layout`]: where the data, the CDN copy and the saves are (OpenOrbit's `paths.py`).
//! * [`GameData`]: `data/<file>` with the selected World Editor world's patches (`game_data.py`).
//! * [`pyjson`]: JSON written exactly like Python's `json.dumps` (the client and the panel read files and
//!   answers OpenOrbit wrote this way; keeping the same bytes keeps the comparisons exact).
//! * [`pyrepr`], [`xml`]: Python's `repr(bytes)` and `quoteattr`, used in logs and XML answers.
//! * [`LogDir`]: the per-server log files (`HH:MM:SS text` lines).

mod data;
mod layout;
mod logs;
pub mod pyjson;
pub mod pyrepr;
pub mod xml;

pub use data::{GameData, World};
pub use layout::{Layout, TEMPLATES};
pub use logs::LogDir;

/// Python's `str.title()` for ASCII-ish identifiers: a letter is upper-cased after a non-letter, lower-cased otherwise.
pub fn py_title(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_cased = false;
    for c in s.chars() {
        if c.is_alphabetic() {
            if prev_cased {
                out.extend(c.to_lowercase());
            } else {
                out.extend(c.to_uppercase());
            }
            prev_cased = true;
        } else {
            out.push(c);
            prev_cased = false;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn title() {
        assert_eq!(super::py_title("goliath"), "Goliath");
        assert_eq!(super::py_title("g-champion"), "G-Champion");
        assert_eq!(super::py_title("abc2def_x"), "Abc2Def_X");
    }
}
