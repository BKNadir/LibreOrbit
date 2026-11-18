//! Profiles written by OpenOrbit (`profile_tool.py new`, and one saved by `game.py` after a login) are read and written back
//! byte for byte.

use lo_save::Profile;
use std::path::PathBuf;

fn fixtures() -> Vec<PathBuf> {
    let dir = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures"));
    let mut v: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    v.sort();
    assert!(v.len() >= 4, "fixtures missing in {}", dir.display());
    v
}

#[test]
fn same_text_as_openorbit() {
    for p in fixtures() {
        let original = std::fs::read_to_string(&p).unwrap();
        let prof = Profile::open(&p, None).unwrap();
        assert!(
            prof.text() == original,
            "{}: the text written differs from OpenOrbit's",
            p.display()
        );
    }
}

#[test]
fn save_round_trip() {
    let dir = std::env::temp_dir().join(format!("lo_save_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for p in fixtures() {
        let target = dir.join(p.file_name().unwrap());
        std::fs::copy(&p, &target).unwrap();
        let mut prof = Profile::open(&target, None).unwrap();
        prof.save(true).unwrap();
        assert_eq!(
            std::fs::read(&target).unwrap(),
            std::fs::read(&p).unwrap(),
            "{}",
            p.display()
        );
        assert!(!target.with_extension("json.tmp").exists());
    }
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn new_profile_is_written_at_once() {
    let dir = std::env::temp_dir().join(format!("lo_save_new_{}", std::process::id()));
    let p = dir.join("slots").join("x").join("profile.json");
    let prof = Profile::open(&p, None).unwrap();
    assert!(p.is_file());
    assert_eq!(prof.d["pilot"]["userName"], "Pilot");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn old_format_is_refused() {
    let dir = std::env::temp_dir().join(format!("lo_save_old_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("profile.json");
    std::fs::write(&p, r#"{"pilot_eski": {}}"#).unwrap();
    assert!(matches!(
        Profile::open(&p, None),
        Err(lo_save::SaveError::OldFormat)
    ));
    std::fs::remove_dir_all(&dir).ok();
}
