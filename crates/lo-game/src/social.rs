//! Social systems (OpenOrbit's `social.py`): chat, mail, clan, auction.
//!
//! Ported so far: what replaces the recorded messages in the login bundle (`Social.login_replace`).

use crate::mail;
use lo_save::Profile;
use serde_json::{Value, json};

/// The chat server's port when the session has none (`multiplayer.CHAT_PORT`, the default of OpenOrbit's chat server).
pub const CHAT_PORT: u16 = 7771;
/// `social.CHAT_HOST`.
pub fn chat_host(port: u16) -> String {
    format!("127.0.0.1:{port}")
}

/// In place of the recorded message (None if none): the chat address on the local server, unread mail from the profile.
/// `host`: a friend server guest's chat address (`chat_host`).
pub fn login_replace(
    name: &str,
    f: &Value,
    p: &Profile,
    port: u16,
    host: Option<&str>,
) -> Option<Vec<(String, Value)>> {
    match name {
        "ChatParamCommand" => {
            let list: Vec<Value> = f["moduleList"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|m| {
                    let mut m = m.clone();
                    let v = match m.get("param0").and_then(Value::as_str) {
                        Some("chat_ip") => {
                            json!(host.map_or_else(|| chat_host(port), str::to_string))
                        }
                        Some("chat_cross_ip") => json!("http://127.0.0.1/crossdomain.xml"),
                        _ => m.get("param1").cloned().unwrap_or(json!("")),
                    };
                    m["param1"] = v;
                    m
                })
                .collect();
            Some(vec![(name.into(), json!({"moduleList": list}))])
        }
        "UserMessageNewCommand" => Some(vec![(
            name.into(),
            json!({"userMessageInfoCommand": null, "unreadCount": mail::unread(p)}),
        )]),
        _ => None,
    }
}
