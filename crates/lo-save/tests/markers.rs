//! The login markers match what OpenOrbit's `Profile.generate` gives for the same profiles
//! (`tests/fixtures/expected/markers.json`, written by `tools/save_diff/gen_markers.py`).

use lo_save::Profile;
use serde_json::Value;
use std::path::PathBuf;

#[test]
fn markers_like_openorbit() {
    let dir = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures"));
    let expected: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("expected/markers.json")).unwrap())
            .unwrap();
    let mut n = 0;
    for (file, markers) in expected.as_object().unwrap() {
        let p = Profile::open(dir.join(file), None).unwrap();
        for (marker, want) in markers.as_object().unwrap() {
            let (name, fields) = p
                .generate(marker)
                .unwrap_or_else(|| panic!("{file}: {marker} not generated"));
            assert_eq!(Value::from(name), want[0], "{file} {marker}: message");
            assert_eq!(fields, want[1], "{file} {marker}: fields");
            // same key order too (the client does not care, the logs and comparisons do)
            assert_eq!(
                serde_json::to_string(&fields).unwrap(),
                serde_json::to_string(&want[1]).unwrap(),
                "{file} {marker}: order"
            );
            n += 1;
        }
    }
    assert!(n >= 25, "only {n} markers checked");
}
