//! `Profile::migrate` against OpenOrbit's `Profile._migrate` (`tests/fixtures/expected/migrate.json`, written by
//! `tools/save_diff/gen_migrate.py`): the document after, in the same key order, and whether it is marked as changed.

use lo_core::settings::{Settings, Values};
use lo_save::Profile;
use serde_json::{Value, json};
use std::path::PathBuf;

fn fixture() -> Value {
    let p = PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/expected/migrate.json"
    ));
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

#[test]
fn migrate_like_openorbit() {
    let fx = fixture();
    let cases = fx["cases"].as_array().unwrap();
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let seed = &fx["seeds"][case["seed"].as_str().unwrap()];
        let settings = Settings::fixed(Values::clean(
            &json!({"infinite_cargo": case["infinite_cargo"]}),
        ));
        let mut p = Profile::from_value("x.json", case["before"].clone());
        p.migrate(seed, &settings);
        assert_eq!(p.d, case["after"], "{name}");
        assert_eq!(
            serde_json::to_string(&p.d).unwrap(),
            serde_json::to_string(&case["after"]).unwrap(),
            "{name}: key order"
        );
        assert_eq!(
            p.is_dirty(),
            case["dirty"].as_bool().unwrap(),
            "{name}: dirty"
        );
    }
    assert!(cases.len() >= 16);
}

#[test]
fn new_profile_written_like_openorbit() {
    let fx = fixture();
    let dir = std::env::temp_dir().join(format!("lo-save-migrate-{}", std::process::id()));
    let path = dir.join("new").join("profile.json");
    let _ = std::fs::remove_dir_all(&dir);
    let p = Profile::load(&path, &fx["seeds"]["template"], &Settings::unbound()).unwrap();
    assert!(!p.is_dirty());
    let text = std::fs::read_to_string(&path).unwrap();
    let created = p.d["created"].as_str().unwrap().to_string();
    let text = text.replace(&created, fx["created"].as_str().unwrap());
    assert_eq!(text, fx["new_file"].as_str().unwrap());
    let _ = std::fs::remove_dir_all(&dir);
}
