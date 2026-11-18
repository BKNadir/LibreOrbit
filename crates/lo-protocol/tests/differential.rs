//! Differential test against OpenOrbit's reference implementation (`server/protocol.py`).
//!
//! The cases come from `tools/protocol_diff/gen_cases.py`: random messages of every class encoded
//! and decoded by protocol.py, and random or damaged payloads it decoded. LibreOrbit must produce
//! the same bytes, the same values, the same leftover byte count and fail on the same inputs.
//!
//! By default the committed fixture is used; `LO_PROTOCOL_CASES=<file.jsonl>` runs another set
//! (see `tools/protocol_diff/run.sh`).

use lo_protocol::{Spec, Struct};
use serde_json::Value as Json;
use std::path::PathBuf;

fn spec() -> Spec {
    Spec::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../data/protocol_read.json"
    ))
    .expect("protocol_read.json")
}

fn hex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex"))
        .collect()
}

fn to_hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// JSON equality where 3 and 3.0 are the same number (Python ints and floats).
fn same(a: &Json, b: &Json) -> bool {
    match (a, b) {
        (Json::Number(x), Json::Number(y)) => match (x.as_i64(), y.as_i64()) {
            (Some(i), Some(j)) => i == j,
            _ => x.as_f64() == y.as_f64(),
        },
        (Json::Object(x), Json::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w)))
        }
        (Json::Array(x), Json::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(v, w)| same(v, w))
        }
        _ => a == b,
    }
}

#[test]
fn matches_openorbit() {
    let path = std::env::var_os("LO_PROTOCOL_CASES")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/protocol_cases.jsonl"
            ))
        });
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let spec = spec();
    let mut failures = Vec::new();
    let (mut n_enc, mut n_dec, mut classes) = (0, 0, std::collections::BTreeSet::new());

    for (no, line) in text.lines().enumerate() {
        let case: Json = serde_json::from_str(line).expect("case JSON");
        let mut fail = |what: String| failures.push(format!("line {}: {what}", no + 1));
        let check_decode = |payload: &[u8], case: &Json, fail: &mut dyn FnMut(String)| {
            let d = spec.decode(payload);
            let got = d.msg.to_json(true);
            if !same(&got, &case["decoded"]) {
                fail(format!(
                    "{} decoded\n  rust:   {got}\n  python: {}",
                    d.name, case["decoded"]
                ));
            }
            if case["remaining"].as_u64() != Some(d.remaining as u64) {
                fail(format!(
                    "{} remaining {} != {}",
                    d.name, d.remaining, case["remaining"]
                ));
            }
            if let Some(want) = case.get("fmt").and_then(Json::as_str) {
                let got = lo_protocol::display::fmt(&d.msg, true, 300);
                if got != want {
                    fail(format!("{} fmt\n  rust:   {got}\n  python: {want}", d.name));
                }
            }
            if Json::Bool(d.error.is_some()) != case["error"] {
                fail(format!(
                    "{} error {:?} != python error {}",
                    d.name, d.error, case["error"]
                ));
            }
            d
        };
        match case["kind"].as_str() {
            Some("encode") => {
                n_enc += 1;
                let name = case["name"].as_str().expect("name");
                classes.insert(name.to_string());
                let input = Struct::from_json(&case["input"], Some(name)).expect("input");
                let res = spec.encode(&input);
                if case.get("encode_error").is_some() {
                    if res.is_ok() {
                        fail(format!(
                            "{name}: python failed ({}), rust encoded",
                            case["encode_error"]
                        ));
                    }
                    continue;
                }
                let want = case["payload"].as_str().expect("payload");
                match res {
                    Err(e) => fail(format!("{name}: rust failed: {e}")),
                    Ok(b) if to_hex(&b) != want => fail(format!(
                        "{name} bytes\n  rust:   {}\n  python: {want}",
                        to_hex(&b)
                    )),
                    Ok(_) => {}
                }
                check_decode(&hex(want), &case, &mut fail);
            }
            Some("decode") => {
                n_dec += 1;
                let payload = hex(case["payload"].as_str().expect("payload"));
                let d = check_decode(&payload, &case, &mut fail);
                if case["id"].as_u64().map(|i| i as u16) != d.id
                    || case["name"].as_str() != Some(d.name.as_str())
                {
                    fail(format!(
                        "id/name {:?} {} != {} {}",
                        d.id, d.name, case["id"], case["name"]
                    ));
                }
            }
            other => panic!("unknown case kind {other:?}"),
        }
    }
    println!(
        "{n_enc} encode cases ({} classes), {n_dec} decode cases",
        classes.len()
    );
    assert!(n_enc + n_dec > 0, "no cases in {}", path.display());
    if !failures.is_empty() {
        panic!(
            "{} differences with protocol.py:\n{}",
            failures.len(),
            failures
                .iter()
                .take(15)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

#[test]
fn every_class_of_the_spec_is_loaded() {
    let spec = spec();
    assert_eq!(spec.len(), 1374);
    assert_eq!(
        spec.id_of("VersionRequest"),
        Some(lo_protocol::VERSION_REQUEST_ID)
    );
}
