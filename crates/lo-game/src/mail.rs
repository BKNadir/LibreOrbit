//! Mail (OpenOrbit's `mail.py`, profile `mailer`): the client's messaging window.
//!
//! Ported so far: the record set up when a session starts (with the welcome message) and the unread count; the
//! window's requests come with the social stage.

use lo_core::pytime;
use lo_save::Profile;
use serde_json::{Map, Value, json};

pub const BOX_LIMIT: usize = 500;
pub const UNREAD: i64 = 1;
pub const READ: i64 = 3;
pub const USER: i64 = 10;
pub const CLAN: i64 = 20;
pub const SYSTEM: i64 = 50;
pub const AUCTION: i64 = 60;
pub const INFO: i64 = 70;

/// `time.strftime('%Y-%m-%d %H:%M:%S', time.localtime(t))`.
pub fn stamp(t: f64) -> String {
    pytime::from_timestamp(t.floor())
        .map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_default()
}

fn d(p: &mut Profile) -> &mut Map<String, Value> {
    p.d["mailer"].as_object_mut().expect("mailer")
}

/// The record when the session starts (`Mail.__init__`): missing boxes, the old greeting removed, the welcome message.
pub fn open(p: &mut Profile, now: f64) {
    let m =
        p.d.as_object_mut()
            .expect("profile")
            .entry("mailer")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .expect("mailer");
    for (k, v) in [
        ("order", json!(1)),
        ("incoming", json!([])),
        ("outgoing", json!([])),
        ("notes", json!([])),
        ("blacklist", json!([])),
    ] {
        m.entry(k).or_insert(v);
    }
    // the Turkish "Yerel sunucu" greeting in old records
    let keep: Vec<Value> = m["incoming"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| {
            !(r.get("sender") == Some(&json!("DarkOrbit"))
                && r.get("subject") == Some(&json!("Yerel sunucu")))
        })
        .cloned()
        .collect();
    m.insert("incoming".into(), Value::Array(keep));
    if !m.get("welcome").is_some_and(lo_core::settings::truthy) {
        m.insert("welcome".into(), json!(true));
        add(
            p,
            now,
            INFO,
            "messaging_systemMessage_1_subject",
            "messaging_systemMessage_1_body",
        );
    }
}

/// A new message without attachments in the inbox (`Mail.add`).
pub fn add(p: &mut Profile, now: f64, kind: i64, subject: &str, body: &str) -> Value {
    let m = d(p);
    let mid = m["order"].clone();
    m["order"] = match mid.as_i64() {
        Some(i) => json!(i + 1),
        None => json!(lo_save::hangar::num(&mid).unwrap_or(0.0) + 1.0),
    };
    let rec = json!({
        "id": mid, "type": kind, "state": UNREAD, "date": stamp(now), "subject": subject, "text": body,
        "sender": "", "sender_id": 0, "attachments": [], "attachment_claimed": true,
    });
    let bx = m["incoming"].as_array_mut().expect("incoming");
    bx.insert(0, rec.clone());
    if bx.len() > BOX_LIMIT {
        let tail: Vec<Value> = bx
            .split_off(BOX_LIMIT)
            .into_iter()
            .filter(|r| {
                !r.get("attachment_claimed")
                    .is_none_or(lo_core::settings::truthy)
            })
            .collect();
        bx.extend(tail);
    }
    p.mark_dirty();
    rec
}

/// Unread messages in the inbox (`unread`).
pub fn unread(p: &Profile) -> usize {
    p.d["mailer"]["incoming"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| lo_save::hangar::num(&r["state"]) == Some(UNREAD as f64))
        .count()
}
