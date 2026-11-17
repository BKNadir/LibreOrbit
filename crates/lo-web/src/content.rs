//! The answers of the web login (OpenOrbit's `web.py`): pages, JSON and XML.

use crate::Web;
use lo_core::{pyjson, xml::quoteattr};
use serde_json::{Map, Value, json};
use std::fmt::Write;
use std::path::{Path, PathBuf};

/// A session: (key, profile path, id on the server).
pub type Session = (String, PathBuf, Option<i64>);

/// (user id, ship, map id, 2D) from the profile; the defaults if it cannot be read.
pub fn profile(path: &Path) -> (Value, String, Value, bool) {
    let read = || -> Option<(Value, String, Value, bool)> {
        let d: Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
        let use2d = d
            .get("settings")
            .and_then(|s| s.get("displaySettingsModule"))
            .and_then(|s| s.get("displayUse2d"))
            .is_some_and(truthy);
        let ship = d.get("ship")?.get("typeId")?.as_str()?.to_string();
        Some((
            d.get("pilot")?.get("userId")?.clone(),
            ship,
            d.get("position")?.get("mapId")?.clone(),
            use2d,
        ))
    };
    read().unwrap_or_else(|| (json!(1), "ship_goliath".into(), json!(1), false))
}

fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64() != Some(0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(m) => !m.is_empty(),
    }
}

/// `web_settings.json` next to the profile (the language chosen on the login screen).
pub fn web_settings_path(profile: &Path) -> PathBuf {
    let abs = std::path::absolute(profile).unwrap_or_else(|_| profile.to_path_buf());
    abs.parent().map_or_else(
        || PathBuf::from("web_settings.json"),
        |d| d.join("web_settings.json"),
    )
}

pub fn web_settings(profile: &Path) -> Map<String, Value> {
    std::fs::read_to_string(web_settings_path(profile))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| match v {
            Value::Object(m) => Some(m),
            _ => None,
        })
        .unwrap_or_default()
}

/// `[a-z]{2}(_[a-z]{2})?`
pub fn valid_lang(s: &str) -> bool {
    let b = s.as_bytes();
    let two = |x: &[u8]| x.len() == 2 && x.iter().all(u8::is_ascii_lowercase);
    match b.len() {
        2 => two(b),
        5 => two(&b[..2]) && b[2] == b'_' && two(&b[3..]),
        _ => false,
    }
}

/// The saved login language (validated) or `None`.
pub fn stored_lang(profile: &Path) -> Option<String> {
    let s = web_settings(profile);
    let lang = match s.get("lang") {
        Some(Value::String(x)) => x.clone(),
        Some(Value::Null) | None => String::new(),
        Some(other) if truthy(other) => other.to_string(),
        Some(_) => String::new(),
    };
    valid_lang(&lang).then_some(lang)
}

/// `LoginNodeData` of the client (field types checked against the client's code).
pub fn login_node(web: &Web, o: &Session) -> Value {
    let now = chrono::Local::now();
    let (sid, path, uid) = o;
    let (user_id, ship, map_id, use2d) = profile(path);
    let user_id = match uid {
        Some(u) if *u != 0 => json!(u),
        _ => user_id,
    };
    let lang = stored_lang(path).unwrap_or_else(|| "en".into());
    let base = web.base();
    let hangar = json!({
        "modelId": 10, "slotNumber": 1, "mapId": map_id, "hangarId": 1,
        "shipName": lo_core::py_title(&ship.replace("ship_", "")), "tooltipIcon": "",
        "shipDestroyed": false, "shipId": ship, "hangarActive": true
    });
    json!({
        "gameclientHash": "local", "chatHost": web.cfg.chat, "profileXmlHash": "1", "loadingscreenHash": "1",
        "pid": 175, "opid": 0, "sessionID": sid, "userID": user_id, "platform": "client",
        "loadingscreenAssetsXmlHash": "1", "showAdvertisingHint": false, "useHash": 0, "resourceJumpGateXmlHash": "1",
        "browser": "unity", "host": base, "useDeviceFonts": 0, "theme": base, "mapID": map_id, "lang": lang,
        "itemXmlHash": "1", "eventStreamContext": "", "resourcesXmlHash": "1", "allowChat": 0, "nextAction": "internalWebGL",
        "requestScheme": "http", "resources3dparticlesXmlHash": "1", "cdn": base, "resourceEventsXmlHash": "1",
        "gameclientAllowedInitDelay": 0, "gameclientPath": "spacemap/", "helpLink": base, "basePath": "spacemap",
        // the client parses the time as "H:mm" and the date as "dd.MM.yyyy" and sets the game time from them
        "fullscreen": 0, "serverInfo": {"timeStamp": now.timestamp(), "serverDate": now.format("%d.%m.%Y").to_string(),
                                        "instanceName": "Local", "serverTime": now.format("%H:%M").to_string()},
        // display2d: the client sets 2D/3D from here on every login (1 = 3D, 2 = 2D)
        "sharedImages": format!("{base}do_img/"), "display2d": if use2d { 2 } else { 1 }, "resourceachievementsXmlHash": "1",
        "loadingClaim": "", "resources3dXmlHash": "1", "hangars": {"data": {"faction": "1", "hangars": [hangar]}},
        "autoStartEnabled": 1, "boardLink": format!("{base}info/forum"), "crossdomainHash": "1", "userProfileImage": ""
    })
}

/// login.php: `LoginDataNew` — the node, plus its JSON text in `data` / `loginData` (parsed a second time by the client).
pub fn login(web: &Web, o: Option<&Session>) -> Value {
    let Some(o) = o else {
        return json!({"success": 0, "message": "Session expired: close this game client and open it again from the DarkOrbit panel."});
    };
    let n = login_node(web, o);
    let text = pyjson::dumps(&n);
    let mut m = n.as_object().cloned().unwrap_or_default();
    m.insert("success".into(), json!(1));
    m.insert("message".into(), json!(""));
    m.insert("data".into(), json!(text));
    m.insert("loginData".into(), json!(text));
    Value::Object(m)
}

/// (title, text) of an info page.
type Page = (&'static str, &'static str);

/// Settings -> Support, Account Center…: local pages in place of the official sites (English, Turkish).
const INFO: [(&str, Page, Page); 6] = [
    (
        "forum",
        (
            "Forum",
            "This local server has no forum. See the panel and the project README for the rules and explanations.",
        ),
        (
            "Forum",
            "Bu yerel sunucunun forumu yok. Kurallar ve açıklamalar için panele ve projenin README dosyasına bak.",
        ),
    ),
    (
        "help",
        ("Help", "For in-game help, see the panel."),
        ("Yardım", "Oyun içi yardım için panele bak."),
    ),
    (
        "credits",
        ("Credits", "OpenOrbit: a single-player local server."),
        ("Emeği geçenler", "OpenOrbit: tek oyunculu yerel sunucu."),
    ),
    (
        "imprint",
        (
            "Imprint",
            "OpenOrbit: a single-player local server; not a commercial service.",
        ),
        (
            "Künye",
            "OpenOrbit: tek oyunculu yerel sunucu; ticari bir hizmet değildir.",
        ),
    ),
    (
        "support",
        (
            "Support",
            "Instead of a support ticket, look at the panel logs (server/logs); /report reports from the chat are written to server/logs/chat_reports.jsonl.",
        ),
        (
            "Destek",
            "Destek bileti yerine panelin günlüklerine (server/logs) bak; sohbetteki /report raporları server/logs/chat_reports.jsonl dosyasına yazılır.",
        ),
    ),
    (
        "accountCenter",
        (
            "Account Center",
            "Account settings (name, password, newsletter) are kept in the local profile; you can change your pilot name in the in-game Pilot Profile window.",
        ),
        (
            "Hesap Merkezi",
            "Hesap ayarları (ad, şifre, bülten) yerel profilde tutulur; pilot adını oyundaki Pilot Profili penceresinden değiştirebilirsin.",
        ),
    ),
];

pub fn links(web: &Web) -> Value {
    let base = web.base();
    Value::Object(
        INFO.iter()
            .map(|(k, ..)| (k.to_string(), json!(format!("{base}info/{k}"))))
            .collect(),
    )
}

pub fn info_page(web: &Web, kind: &str) -> String {
    let (title, text) = match INFO.iter().find(|(k, ..)| *k == kind) {
        Some((_, en, tr)) => {
            if stored_lang(&web.main_profile()).as_deref() == Some("tr") {
                *tr
            } else {
                *en
            }
        }
        None => ("OpenOrbit", ""),
    };
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{title}</title></head><body style=\"background:#000;color:#9cf;\
         font-family:Verdana;padding:24px\"><h2>{title}</h2><p>{text}</p></body></html>"
    )
}

/// The map ids of the client's maps-config.xml, each pointing at the game server.
pub fn maps_xml(web: &Web) -> String {
    let ids = web.map_ids();
    let mut body = String::new();
    for i in ids.iter() {
        write!(
            body,
            "<map id=\"{i}\"><gameserverIP>{}</gameserverIP></map>",
            web.cfg.remote
        )
        .unwrap();
    }
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><maps>{body}</maps>")
}

/// `<map id="(\d+)"` in maps-config.xml -> sorted, unique ids.
pub fn parse_map_ids(text: &str) -> Vec<u64> {
    let mut ids: Vec<u64> = text
        .match_indices("<map id=\"")
        .filter_map(|(i, m)| {
            let rest = &text[i + m.len()..];
            let end = rest.find(|c: char| !c.is_ascii_digit())?;
            (end > 0 && rest[end..].starts_with('"'))
                .then(|| rest[..end].parse().ok())
                .flatten()
        })
        .collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// The Gate window's rewards guide (`ggRewards.php`): per gate, each reward with a chance class (1-5) and an amount.
pub fn gate_rewards_xml(web: &Web) -> String {
    const GATE_LOOT: [(&str, &str); 7] = [
        ("xenomit", "resource_ore_xenomit"),
        ("log_disk", "resource_logfile"),
        ("voucher", "resource_repairvoucher"),
        ("energy", "resource_extra-energy"),
        ("abr", "resource_collectable_axion-beam-regulator"),
        ("schism", "resource_collectable_schism"),
        ("green_key", "resource_booty-key"),
    ];
    let loot_of = |k: &str| {
        GATE_LOOT
            .iter()
            .find(|(a, _)| *a == k)
            .map_or(k.to_string(), |(_, b)| b.to_string())
    };
    let gates = web
        .data
        .load("gates.json")
        .ok()
        .and_then(|d| d.get("gates").and_then(Value::as_object).cloned())
        .unwrap_or_default();
    let mut out = String::new();
    for (key, g) in &gates {
        let empty = Map::new();
        let r = g
            .get("reward")
            .and_then(Value::as_object)
            .filter(|m| !m.is_empty())
            .unwrap_or(&empty);
        let num = |k: &str| r.get(k).cloned().unwrap_or(json!(0));
        // (lootId, amount, probability)
        let mut rows: Vec<(String, Value, f64)> = vec![
            ("currency_experience".into(), num("ep"), 1.0),
            ("currency_honour".into(), num("honor"), 1.0),
            ("currency_uridium".into(), num("uridium"), 1.0),
        ];
        let sorted = |k: &str| -> Vec<(String, Value)> {
            let mut v: Vec<(String, Value)> = r
                .get(k)
                .and_then(Value::as_object)
                .map(|m| m.iter().map(|(a, b)| (a.clone(), b.clone())).collect())
                .unwrap_or_default();
            v.sort_by(|a, b| a.0.cmp(&b.0));
            v
        };
        rows.extend(sorted("ammunition").into_iter().map(|(l, n)| (l, n, 1.0)));
        for k in ["xenomit", "log_disk", "voucher", "energy"] {
            rows.push((loot_of(k), num(k), 1.0));
        }
        rows.extend(
            sorted("booty")
                .into_iter()
                .map(|(k, n)| (loot_of(&k), n, 1.0)),
        );
        for s in r
            .get("chance")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let p = s.get("probability").and_then(Value::as_f64).unwrap_or(0.0);
            let item = s.get("item").and_then(Value::as_str);
            if item == Some("ore_pack") {
                for e in s
                    .get("package")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    let loot = e
                        .get(0)
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    let n = e.get(1).and_then(|n| n.get(0)).cloned().unwrap_or(json!(0));
                    let w = e.get(2).and_then(Value::as_f64).unwrap_or(0.0);
                    rows.push((loot, n, p * w));
                }
                continue;
            }
            let opts: Vec<Option<String>> = if item == Some("design_choice") {
                s.get("options")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().map(|x| x.as_str().map(str::to_string)).collect())
                    .unwrap_or_default()
            } else {
                vec![item.map(str::to_string)]
            };
            let count = s.get("count").cloned().unwrap_or(json!(1));
            let k = opts.len().max(1) as f64;
            rows.extend(
                opts.into_iter()
                    .map(|loot| (loot.unwrap_or_default(), count.clone(), p / k)),
            );
            if let Some(t) = s.get("together").and_then(Value::as_array) {
                for e in t {
                    let loot = e
                        .get(0)
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    let n = e.get(1).cloned().unwrap_or(json!(0));
                    let n = if let Value::Array(a) = &n {
                        a.first().cloned().unwrap_or(json!(0))
                    } else {
                        n
                    };
                    rows.push((loot, n, p / t.len() as f64));
                }
            }
        }
        let mut items = String::new();
        for (loot, n, p) in rows {
            if loot.is_empty() || !truthy(&n) {
                continue;
            }
            write!(
                items,
                "<item id={} chance=\"{}\" min=\"{}\"/>",
                quoteattr(&loot),
                chance_class(p),
                py_int(&n)
            )
            .unwrap();
        }
        write!(out, "<gate name={}>{items}</gate>", quoteattr(key)).unwrap();
    }
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><ggRewards>{out}</ggRewards>")
}

fn chance_class(p: f64) -> u8 {
    if p >= 1.0 {
        5
    } else if p >= 0.5 {
        4
    } else if p >= 0.2 {
        3
    } else if p >= 0.05 {
        2
    } else {
        1
    }
}

/// `'%d' % n`: integers as they are, floats truncated.
fn py_int(n: &Value) -> String {
    match n {
        Value::Number(x) if x.is_f64() => format!("{}", x.as_f64().unwrap_or(0.0).trunc() as i64),
        Value::Number(x) => x.to_string(),
        Value::Bool(b) => (*b as i32).to_string(),
        other => other.to_string(),
    }
}

/// Galaxy Trek planets and events (`spaceSearch.php`), from `extra_events.json` `galaxy_trek`.
pub fn galaxy_trek_planets(web: &Web) -> String {
    let d = web.galaxy_trek();
    let planets = d.get("planet").and_then(Value::as_u64).unwrap_or(40);
    let mut ev = String::new();
    for e in d
        .get("events")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        for i in e.get("ids").and_then(Value::as_array).into_iter().flatten() {
            let i = py_int(i);
            let mut rewards = String::new();
            for f in e
                .get("fixed")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let loot = f.get(0).and_then(Value::as_str).unwrap_or_default();
                write!(
                    rewards,
                    "<reward lootId={} amount=\"{}\"/>",
                    quoteattr(loot),
                    py_int(f.get(1).unwrap_or(&json!(0)))
                )
                .unwrap();
            }
            let text = e
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .replacen("%d", &i, 1)
                .replacen("%s", &i, 1);
            let grade = e.get("grade").and_then(Value::as_str).unwrap_or_default();
            write!(
                ev,
                "<event eventId=\"{i}\" resKey={} degree={}>{rewards}</event>",
                quoteattr(&text),
                quoteattr(grade)
            )
            .unwrap();
        }
    }
    let mut pl = String::new();
    for i in 1..=planets {
        write!(
            pl,
            "<planet planetId=\"{i}\" resKey=\"SS_planet_name_{i}\"/>"
        )
        .unwrap();
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><spaceSearch><planets>{pl}</planets><planetEvents>{ev}</planetEvents></spaceSearch>"
    )
}

/// Galaxy Trek reward tiers (`ssRewards.php`).
pub fn galaxy_trek_rewards(web: &Web) -> String {
    let d = web.galaxy_trek();
    let mut out = String::new();
    for t in d
        .get("steps")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let mut rows = String::new();
        for it in t
            .get("items")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let loot = it.get(0).and_then(Value::as_str).unwrap_or_default();
            write!(
                rows,
                "<item lootId={} amount=\"{}\"/>",
                quoteattr(loot),
                py_int(it.get(1).unwrap_or(&json!(0)))
            )
            .unwrap();
        }
        let title = match t.get("title").and_then(Value::as_str) {
            Some(x) if !x.is_empty() => format!("<title name={}/>", quoteattr(x)),
            _ => String::new(),
        };
        let req = py_int(t.get("required").unwrap_or(&json!(0)));
        write!(
            out,
            "<rangeBoundPackageEntry special=\"0\" name=\"{req}\"><package><range from=\"{req}\"/>{rows}{title}</package></rangeBoundPackageEntry>"
        )
        .unwrap();
    }
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><ssRewards>{out}</ssRewards>")
}

/// The embedded browser's first page: the session cookie, the saved language and the redirect to the game login.
pub fn redirect_page(web: &Web, sid: Option<&str>) -> String {
    let o = match sid {
        Some(s) => web.session(Some(s)),
        None => Some((web.cfg.session.clone(), web.main_profile(), None)),
    };
    let Some(o) = o else {
        return "<!doctype html><html><body style=\"background:#000;color:#f96;font-family:Verdana\">Bu istemcinin oturumu sunucuda \
                yok: istemciyi kapatıp paneldeki ek istemci düğmesiyle yeniden aç.<br>This client's session is not on the server: \
                close it and open it again from the panel.</body></html>"
            .into();
    };
    let mut js = format!("document.cookie=\"dosid={}; path=/\";", o.0);
    js.push_str("var gone=false;function go(){if(!gone){gone=true;location.href=\"/indexInternal.es?action=internalWebGL\";}}");
    match stored_lang(&o.1) {
        Some(lang) => {
            let msg = pyjson::dumps(
                &json!({"success": 1, "message": format!("Unity_ChangeLanguage{lang}")}),
            );
            write!(js, "document.cookie=\"lang={lang}; path=/\";").unwrap();
            write!(
                js,
                "setTimeout(function(){{console.log({});setTimeout(go,400);}},300);",
                pyjson::string(&msg, true)
            )
            .unwrap();
        }
        None => js.push_str("go();"),
    }
    format!(
        "<!doctype html><html><body style=\"background:#000;color:#9cf;font-family:Verdana\">OpenOrbit local server<script>{js}</script></body></html>"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_ids() {
        assert_eq!(
            parse_map_ids("<map id=\"3\"/><map id=\"1\"><map id=\"3\"><map id=\"x\">"),
            vec![1, 3]
        );
    }

    #[test]
    fn langs() {
        assert!(valid_lang("fr") && valid_lang("pt_br"));
        assert!(!valid_lang("FR") && !valid_lang("fra") && !valid_lang("") && !valid_lang("pt-br"));
    }
}
