//! The login bundles (OpenOrbit's `game.py`: `Templates`, `login_batch`, `no_popups`, the replacements of
//! `Session.with_quests`): the recorded login template (`login.json`) and map worlds (`maps.json`), with the markers
//! produced from the save, every other message adapted to the profile (`patch_template`), the map's world and the event
//! calendar's messages.
//!
//! Systems not ported yet (quests, passes, daily rewards, shop, P.E.T., new systems, social…) still leave their recorded
//! messages as they are; `tools/game_diff` shows what remains.

use crate::event_calendar::EventCalendar;
use crate::items::ItemData;
use crate::login_markers::{LoginMarkers, Player, Station};
use crate::map_links::{self, MapLinks, TUTORIAL_GATE, TUTORIAL_MAX_LEVEL};
use crate::templates::{Ctx, MenuState, patch_template};
use lo_core::settings::{Settings, truthy};
use lo_save::Profile;
use serde_json::{Map, Value, json};
use std::collections::{BTreeSet, HashMap};
use std::sync::Mutex;

/// Template messages that are never sent (`DEAD_TEMPLATE`, `DEAD_LEGACY`).
pub const DEAD_TEMPLATE: [&str; 1] = ["CTBMatchStateCommand"];
pub const DEAD_LEGACY: [&str; 2] = ["0|TR", "0|8"];
/// The mode portals in the record that the server places itself (`SERVER_GATE_DESIGNS`).
pub const SERVER_GATE_DESIGNS: [i64; 2] = [300, 304];
/// Windows that open on their own at login (`POPUP_MSGS`).
pub const POPUP_MSGS: [&str; 9] = [
    "DisplayOfferCommand",
    "HintWindowAddComand",
    "HintWindowListInitCommand",
    "DailyLoginRewardDayCommand",
    "MonthlyDeluxeInfoCommand",
    "CommandCenterDontShowInfoCommand",
    "SeasonPassStatusCommand",
    "SeasonPassMissionInfoCommand",
    "SeasonPassRewardInfoCommand",
];
/// Windows opened from the layout at login, sent closed without pop-ups (`POPUP_WINDOWS`).
pub const POPUP_WINDOWS: [&str; 1] = ["miniclient_reward"];
/// Real-money shopping windows, never sent (`PAYMENT_POPUPS`).
pub const PAYMENT_POPUPS: [&str; 2] = ["EventPaymentActorCommand", "DisplayOfferCommand"];

/// A message: its name and fields.
pub type Msg = (String, Value);

/// The recorded templates.
pub struct Templates {
    /// `login.json`: `entry`, `ready`, `seed`, `source`.
    pub entry: Value,
    /// `maps.json`: map id -> `{entry, ready}`.
    pub maps: Map<String, Value>,
    produced: Mutex<HashMap<(i64, Option<i64>), Value>>,
}

impl Templates {
    /// From the two templates (`{}` when missing), the portals cleaned like OpenOrbit.
    pub fn new(entry: Value, maps: Value, links: &MapLinks) -> Templates {
        let mut maps = maps.as_object().cloned().unwrap_or_default();
        for (map_id, w) in maps.iter_mut() {
            let mid: i64 = map_id.parse().unwrap_or(0);
            let ready = w
                .get("ready")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let mut out = Vec::new();
            let mut seen = BTreeSet::new();
            for mut m in ready {
                if m["_m"] == "JumpgateCreateCommand" {
                    if m["designId"]
                        .as_i64()
                        .is_some_and(|d| SERVER_GATE_DESIGNS.contains(&d))
                    {
                        continue;
                    }
                    let gid = m["gateId"].as_i64().unwrap_or(0);
                    if !seen.insert(gid) {
                        continue;
                    }
                    if let Some((x, y)) = links.layout.gate_pos(mid, gid - map_links::GATE_ID_BASE)
                    {
                        m["x"] = x;
                        m["y"] = y;
                    }
                }
                out.push(m);
            }
            if let Some(o) = w.as_object_mut() {
                o.insert("ready".into(), Value::Array(out));
            }
        }
        Templates {
            entry,
            maps,
            produced: Mutex::new(HashMap::new()),
        }
    }

    /// The map's world `{entry, ready}`: the recorded one, otherwise produced from the client's data (`map_world`).
    pub fn map_world(&self, links: &MapLinks, map_id: i64, faction: Option<i64>) -> Value {
        if let Some(w) = self.maps.get(&map_id.to_string()) {
            return w.clone();
        }
        let mut p = self.produced.lock().unwrap_or_else(|e| e.into_inner());
        p.entry((map_id, faction))
            .or_insert_with(|| links.map_world(map_id, self.maps.get("5"), faction))
            .clone()
    }

    /// The map's station (decorative ones excluded) (`station`).
    pub fn station(&self, links: &MapLinks, map_id: i64, faction: Option<i64>) -> Option<Value> {
        let decor: Vec<i64> = map_links::STRUCTURE_STATION
            .iter()
            .chain(map_links::LOW_STATION.iter())
            .map(|(_, t)| *t)
            .collect();
        self.map_world(links, map_id, faction)["ready"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|m| {
                m["_m"] == "SpaceStationCommand"
                    && !m["type"].as_i64().is_some_and(|t| decor.contains(&t))
            })
            .cloned()
    }

    /// The map's world messages of one kind (`world_msgs`).
    pub fn world_msgs(
        &self,
        links: &MapLinks,
        map_id: i64,
        kind: &str,
        faction: Option<i64>,
    ) -> Vec<Msg> {
        self.map_world(links, map_id, faction)[kind]
            .as_array()
            .into_iter()
            .flatten()
            .map(|w| {
                let mut f = w.as_object().cloned().unwrap_or_default();
                let name = f
                    .shift_remove("_m")
                    .and_then(|v| v.as_str().map(str::to_string))
                    .unwrap_or_default();
                (name, Value::Object(f))
            })
            .collect()
    }

    /// The old map's station objects removed on a map change (`asset_removes`).
    pub fn asset_removes(&self, links: &MapLinks, map_id: i64, faction: Option<i64>) -> Vec<Msg> {
        self.map_world(links, map_id, faction)["entry"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|w| w["_m"] == "AssetCreateCommand")
            .map(|w| {
                (
                    "AssetRemoveCommand".to_string(),
                    json!({"assetType": w["type"], "uid": w["assetId"]}),
                )
            })
            .collect()
    }
}

/// Everything a bundle reads.
pub struct World<'a> {
    pub templates: &'a Templates,
    pub links: &'a MapLinks,
    pub items: &'a ItemData,
    pub markers: &'a LoginMarkers,
    pub cal: &'a EventCalendar,
    pub settings: &'a Settings,
    pub seed: &'a Value,
    pub now: f64,
    /// `--no-popups`.
    pub no_popups: bool,
}

fn faction(p: &Profile) -> Option<i64> {
    p.d["pilot"].get("factionId").and_then(Value::as_i64)
}

/// A login marker (`Profile.generate`) -> (message name, fields).
fn generate(w: &World, p: &mut Profile, marker: &str, station: Option<Station>) -> Option<Msg> {
    let hook = |_: &str| None;
    let (name, f) = match marker {
        "@gemi" => (
            "ShipInitializationCommand",
            Value::Object(
                w.markers
                    .ship_init(p, w.seed, w.settings, Player::LOCAL, None),
            ),
        ),
        "@arayuz" => (
            "UILayoutCommand",
            w.markers.ui_layout(p, w.cal, w.now, &hook),
        ),
        "@isaret" => (
            "BeaconCommand",
            Value::Object(w.markers.login_beacon(p, w.seed, station)),
        ),
        other => {
            let (n, f) = p.generate(other)?;
            (n, f)
        }
    };
    Some((name.to_string(), f))
}

/// The template's bundle of one kind (`entry` or `ready`) for the profile (`Templates.batch`).
pub fn batch(w: &World, p: &mut Profile, menu: &mut MenuState, kind: &str) -> Vec<Msg> {
    let tpl = &w.templates.entry;
    if !truthy(tpl) {
        if kind == "entry" {
            return ["@gemi", "@giris_yaniti"]
                .iter()
                .filter_map(|m| generate(w, p, m, None))
                .collect();
        }
        return Vec::new();
    }
    let map_id = p.d["position"]["mapId"].as_i64().unwrap_or(0);
    let fac = faction(p);
    let hook = |_: &str| None;
    let ctx = Ctx {
        items: w.items,
        maps: w.links,
        markers: w.markers,
        cal: w.cal,
        settings: w.settings,
        now: w.now,
        hook: &hook,
        daily: None,
    };
    let mut out = Vec::new();
    let mut windows: BTreeSet<String> = BTreeSet::new();
    for m in tpl[kind].as_array().into_iter().flatten() {
        let mk = m["_m"].as_str().unwrap_or_default();
        let message = m.get("message").and_then(Value::as_str);
        if DEAD_TEMPLATE.contains(&mk) || message.is_some_and(|s| DEAD_LEGACY.contains(&s)) {
            continue;
        }
        if mk == "WindowItemCommand" {
            let id = lo_save::hangar::py_str(m.get("itemId").unwrap_or(&Value::Null));
            if kind == "ready" || windows.contains(&id) {
                continue;
            }
            windows.insert(id);
        }
        if mk == "@harita_giris" || mk == "@harita_hazir" {
            let lv = p.d["pilot"]
                .get("level")
                .and_then(Value::as_i64)
                .unwrap_or(1);
            let k = if mk == "@harita_giris" {
                "entry"
            } else {
                "ready"
            };
            out.extend(
                w.templates
                    .world_msgs(w.links, map_id, k, fac)
                    .into_iter()
                    .filter(|(n, f)| {
                        !(n == "JumpgateCreateCommand"
                            && f["gateId"].as_i64() == Some(TUTORIAL_GATE)
                            && lv >= TUTORIAL_MAX_LEVEL)
                    }),
            );
        } else if kind == "ready"
            && (mk == "EquipReadyCommand"
                || message.is_some_and(|s| s.starts_with("0|n|MSG|3|2|msg_equip_")))
        {
            continue;
        } else if mk.starts_with('@') {
            let st = w
                .templates
                .station(w.links, map_id, fac)
                .filter(|s| s.get("type").and_then(Value::as_i64) == fac)
                .and_then(|s| Station::from_value(&s));
            if let Some(g) = generate(w, p, mk, st) {
                out.push(g);
            }
        } else {
            out.push((mk.to_string(), patch_template(&ctx, p, menu, mk, m)));
            if mk == "EventActivationStateCommand" {
                let active = w.cal.active(w.now);
                out.extend(
                    w.cal
                        .mood_cmds(&active, None)
                        .into_iter()
                        .map(|s| (s.class.clone(), s.to_json(true))),
                );
            }
        }
    }
    out
}

/// Drops the windows that open on their own at login (`no_popups`).
pub fn no_popups(w: &World, msgs: Vec<Msg>) -> Vec<Msg> {
    let popups = w.no_popups || w.settings.switch("no_ads");
    let mut out = Vec::new();
    for (n, mut f) in msgs {
        let drop =
            PAYMENT_POPUPS.contains(&n.as_str()) || (popups && POPUP_MSGS.contains(&n.as_str()));
        if drop && n != "CommandCenterDontShowInfoCommand" {
            continue;
        }
        if n == "UILayoutCommand" && popups {
            for bar in f["featureBars"].as_array_mut().into_iter().flatten() {
                for it in bar["featureBarItems"].as_array_mut().into_iter().flatten() {
                    if it
                        .get("itemId")
                        .and_then(Value::as_str)
                        .is_some_and(|i| POPUP_WINDOWS.contains(&i))
                    {
                        it["maximized"] = json!(false);
                    }
                }
            }
        }
        if n == "CommandCenterDontShowInfoCommand" {
            f["inspected"] = json!(true);
        }
        out.push((n, f));
    }
    out
}

/// The login bundle of one kind; `first = false` on a map change (`login_batch`).
pub fn login_batch(
    w: &World,
    p: &mut Profile,
    menu: &mut MenuState,
    kind: &str,
    first: bool,
) -> Vec<Msg> {
    let msgs = no_popups(w, batch(w, p, menu, kind));
    if first {
        return msgs;
    }
    msgs.into_iter()
        .filter(|(n, _)| {
            (!POPUP_MSGS.contains(&n.as_str()) || n == "CommandCenterDontShowInfoCommand")
                && n != "WindowItemCommand"
        })
        .collect()
}

/// The systems' replacements in the login bundle (`with_quests`), for the systems ported so far: the event calendar.
pub fn with_systems(w: &World, msgs: Vec<Msg>) -> Vec<Msg> {
    msgs.into_iter()
        .map(|(n, f)| {
            if n == "EventUpdateCommand" {
                let c = w.cal.event_cmd(w.now);
                (c.class.clone(), c.to_json(true))
            } else {
                (n, f)
            }
        })
        .collect()
}
