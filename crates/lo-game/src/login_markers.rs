//! The login markers that need the game's systems (OpenOrbit's `Profile.generate` for `@gemi`, `@arayuz`, `@isaret`).
//!
//! * `@gemi` (`ShipInitializationCommand`): the seed's values, then the profile's pilot, ship and position, everyone
//!   Premium, the Galaxy Gate rings, the fitted N.O.V.A.'s energy (bound by the session), the GM console's and admin
//!   mode's rank and look;
//! * `@arayuz` (`UILayoutCommand`): the saved interface with the P.E.T. item, the mode windows, the event windows shown
//!   while their event runs, the mode and event windows moved to the second row and the bars the player dragged;
//! * `@isaret` (`BeaconCommand`): the repair robot, the Engineering skill, the position and the peace area.
//!
//! The markers that depend on the save alone are in `lo_save::Profile::generate`.

use crate::event_calendar::EventCalendar;
use lo_core::GameData;
use lo_core::settings::{Settings, truthy};
use lo_save::Profile;
use serde_json::{Map, Value, json};

/// Feature bar windows of the modes: always visible (`MODE_WINDOWS`).
pub const MODE_WINDOWS: [&str; 5] = [
    "voyagers",
    "trinityTrials",
    "expedition",
    "eternal_blacklight",
    "rogue_lite",
];

/// Event window -> the calendar's events it is tied to (`EVENT_WINDOWS`; `None`: found from the events' data).
pub const EVENT_WINDOWS: &[(&str, Option<&[&str]>)] = &[
    ("plutus", Some(&["plutus"])),
    ("frozen_labyrinth", Some(&["labyrinth"])),
    ("agatus_event", Some(&["agatus"])),
    ("npc_event", None),
    ("chrominEvent", Some(&["chromin"])),
    ("word_puzzle", Some(&["hitac"])),
    ("titleFrenzy", Some(&["title_hunt"])),
    ("space_search", Some(&["galaxy_trek"])),
    ("birthday_2025", Some(&["fiesta"])),
    ("infiltration", Some(&["infiltration"])),
    ("miniclient_reward", None),
    ("invasion", Some(&["invasion"])),
    ("ship_boost_event", Some(&["fever"])),
    ("diminish_quests", Some(&["urgent_quests"])),
    ("strikeAndExit", Some(&["vortex_of_greed"])),
    ("publicWorldBoss", Some(&["world_boss"])),
    ("payload_escort", Some(&["mimesis_mutiny"])),
    ("mileageMarket", Some(&["mileage_market"])),
    ("vipMerchantExchange", Some(&["vip_merchant"])),
    ("preBlackFridaySales", Some(&["pre_prometheus"])),
    ("return_of_hero", Some(&["return_of_hero"])),
    ("returnee_login", Some(&[])),
    ("apotheosis", Some(&["kristallon_apotheosis"])),
    ("recallBattlePass", Some(&["return_of_hero"])),
    ("pvp_summer", Some(&["galactic_strife"])),
    ("tdm", Some(&["team_deathmatch"])),
    ("sectorcontrol", Some(&["sector_control"])),
    ("spaceball", Some(&["spaceball"])),
    ("ctb", Some(&["capture_the_beacon"])),
    ("battle_pass", Some(&[])),
];

/// Windows sent closed at login (`CLOSED_AT_LOGIN`).
pub const CLOSED_AT_LOGIN: [&str; 1] = ["diminish_quests"];
/// Never shown in the menu (`HIDDEN_WINDOWS`).
pub const HIDDEN_WINDOWS: [&str; 1] = ["rzone"];
/// Event windows driven by an indicator command in the event's map section (`INDICATOR_WINDOWS`).
const INDICATOR_WINDOWS: [(&str, &str); 2] = [
    ("AgatusEventInfoCommand", "agatus_event"),
    ("NpcEventInfoCommand", "npc_event"),
];
/// Feature bar type -> the window settings keys of its position and layout order (`FEATURE_BAR_KEYS`).
const FEATURE_BAR_KEYS: [(i64, &str, Option<&str>); 3] = [
    (1, "gameFeatureMenuPosition", Some("gameFeatureMenuOrder")),
    (
        2,
        "genericFeatureMenuPosition",
        Some("genericFeatureMenuOrder"),
    ),
    (3, "gameFeatureMenuPosition", None),
];
/// `WindowConstants.LayoutsMap`.
const LAYOUT_COUNT: u64 = 6;
/// The Galaxy Gate rings (`galaxy_gates.RINGS`).
pub const RINGS: [(&str, i64); 7] = [
    ("alpha", 1),
    ("beta", 2),
    ("gamma", 4),
    ("delta", 8),
    ("epsilon", 16),
    ("zeta", 32),
    ("kronos", 64),
];
/// The client's admin rank (`gm_console.ADMIN_RANK`, `admin_mode.BADGE`).
pub const ADMIN_RANK: i64 = 6000;
/// The admin ship looks of the panel's `admin_ship` setting (`admin_mode.SHIPS`).
pub const ADMIN_SHIPS: [Option<&str>; 5] = [
    None,
    Some("ship_admin"),
    Some("ship_police"),
    Some("ship_g-admincloak"),
    Some("ship_goliath_design_admincloak"),
];
/// Engineering: at its final level the repair robot is the blue one.
const ENGINEERING: &str = "13";

fn event_windows() -> impl Iterator<Item = (&'static str, Option<&'static [&'static str]>)> {
    EVENT_WINDOWS.iter().copied()
}

/// Whether the menu item is an event window.
pub fn is_event_window(item: &str) -> bool {
    event_windows().any(|(k, _)| k == item)
}

/// Who the profile belongs to, for the GM console and admin mode.
#[derive(Debug, Clone, Copy)]
pub struct Player {
    /// A guest on a friend server.
    pub guest: bool,
    /// The server's own player (or no server: single player, tests).
    pub host: bool,
}

impl Player {
    /// The single player of a local game.
    pub const LOCAL: Player = Player {
        guest: false,
        host: true,
    };
}

/// The station near the ship, for `@isaret`'s peace area.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Station {
    pub pos_x: f64,
    pub pos_y: f64,
    pub peace_radius: f64,
    pub pvp_peace: bool,
}

impl Station {
    /// From a station record (`posX`, `posY`, `peaceRadius`, `pvpPeace`).
    pub fn from_value(v: &Value) -> Option<Station> {
        Some(Station {
            pos_x: v.get("posX")?.as_f64()?,
            pos_y: v.get("posY")?.as_f64()?,
            peace_radius: v.get("peaceRadius").and_then(Value::as_f64).unwrap_or(0.0),
            pvp_peace: v.get("pvpPeace").is_none_or(truthy),
        })
    }
}

/// The ship's situation for a `BeaconCommand` (`Profile.beacon`'s arguments).
#[derive(Debug, Clone, Copy, Default)]
pub struct BeaconState {
    pub station: Option<Station>,
    pub near_gate: bool,
    pub pos: Option<(f64, f64)>,
    pub repairing: bool,
    pub gate_peace: bool,
    pub radiation: bool,
    pub void: bool,
}

/// Data the markers read.
#[derive(Debug, Clone)]
pub struct LoginMarkers {
    /// Skill id -> number of levels (`skill_tree.SKILLS[sid]['values']`).
    skill_levels: Map<String, Value>,
}

fn obj(v: &Value) -> Map<String, Value> {
    v.as_object().cloned().unwrap_or_default()
}

/// Python's `int(x)` of a number (toward zero).
fn py_int(v: &Value) -> Value {
    match v {
        Value::Number(n) if n.is_f64() => json!(n.as_f64().unwrap_or(0.0).trunc() as i64),
        other => other.clone(),
    }
}

impl LoginMarkers {
    pub fn from_skills(skills: &Value) -> LoginMarkers {
        LoginMarkers {
            skill_levels: skills
                .get("skills")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default(),
        }
    }

    /// Reads `data/skills.json` (as it is, like `skill_tree.py`).
    pub fn load(data: &GameData) -> Result<LoginMarkers, String> {
        let skills = data
            .raw("skills.json")
            .map_err(|e| format!("skills.json: {e}"))?;
        Ok(LoginMarkers::from_skills(&skills))
    }

    // -------------------------------------------------------------- @gemi

    /// `ShipInitializationCommand` (`Profile.ship_init`). `captain_energy`: the fitted N.O.V.A.'s fields, when the session
    /// has bound it.
    pub fn ship_init(
        &self,
        p: &Profile,
        seed: &Value,
        settings: &Settings,
        who: Player,
        captain_energy: Option<&Map<String, Value>>,
    ) -> Map<String, Value> {
        let mut f = lo_save::default_ship_init();
        let d = &p.d;
        for src in [
            seed.get("ShipInitializationCommand")
                .filter(|v| truthy(v))
                .cloned()
                .unwrap_or(Value::Null),
            d["pilot"].clone(),
            d["ship"].clone(),
            d["position"].clone(),
        ] {
            f.extend(obj(&src));
        }
        f.insert("premium".into(), json!(true));
        f.insert("galaxyGatesDone".into(), json!(rings(p)));
        if let Some(c) = captain_energy {
            f.extend(c.clone());
        }
        f.insert("refreshMap".into(), json!(true));
        if settings.switch("gm_console") && !who.guest {
            f.insert("dailyRank".into(), json!(ADMIN_RANK));
        }
        if admin_open(settings, who) {
            f.insert("dailyRank".into(), json!(ADMIN_RANK));
            if let Some(ship) = admin_ship(settings, who) {
                f.insert("typeId".into(), json!(ship));
            }
        }
        f
    }

    // -------------------------------------------------------------- @isaret

    /// The skill's level (`skill_tree.level`; like OpenOrbit, an empty skill record is added to a profile without one).
    pub fn skill_level(p: &mut Profile, sid: &str) -> i64 {
        let d = p.d.as_object_mut().expect("profile");
        let st = d
            .entry("skills")
            .or_insert_with(|| json!({"level": {}, "points": 0, "converted": 0, "resets": 0}));
        st.get("level")
            .and_then(|l| l.get(sid))
            .and_then(Value::as_i64)
            .unwrap_or(0)
    }

    /// Whether the skill is at its final level (`skill_tree.maxed`).
    pub fn skill_maxed(&self, p: &mut Profile, sid: &str) -> bool {
        let n = self
            .skill_levels
            .get(sid)
            .and_then(|s| s.get("values"))
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        Self::skill_level(p, sid) >= n as i64
    }

    /// `BeaconCommand` (`Profile.beacon`).
    pub fn beacon(&self, p: &mut Profile, seed: &Value, s: BeaconState) -> Map<String, Value> {
        let mut b = obj(seed.get("BeaconCommand").unwrap_or(&Value::Null));
        let rep = extras_equipped(p)
            .into_iter()
            .find(|e| e.starts_with("equipment_extra_repbot_"));
        b.insert("repairRobotActive".into(), json!(s.repairing));
        b.insert(
            "repairBotSkilled".into(),
            json!(self.skill_maxed(p, ENGINEERING)),
        );
        if let Some(rep) = rep {
            b.insert("repairRobotLootId".into(), json!(rep));
        }
        let (x, y) = match s.pos {
            Some((x, y)) => (json!(x), json!(y)),
            None => (p.d["position"]["x"].clone(), p.d["position"]["y"].clone()),
        };
        let (xi, yi) = (py_int(&x), py_int(&y));
        b.insert("positionX".into(), xi.clone());
        b.insert("positionY".into(), yi.clone());
        b.insert("positionAheadX".into(), xi);
        b.insert("positionAheadY".into(), yi);
        b.insert("inRadiationZone".into(), json!(s.radiation));
        b.insert("nearJumpGate".into(), json!(s.near_gate));
        let (xf, yf) = (x.as_f64().unwrap_or(0.0), y.as_f64().unwrap_or(0.0));
        let mut inside = false;
        let mut pvp_peace = false;
        if let Some(st) = s.station {
            let r = st.peace_radius;
            inside = (xf - st.pos_x).powi(2) + (yf - st.pos_y).powi(2) <= r * r;
            pvp_peace = st.pvp_peace;
        }
        b.insert(
            "inPeaceArea".into(),
            json!(((inside && pvp_peace) || s.gate_peace) && !s.void),
        );
        b.insert("inStation".into(), json!(inside));
        b
    }

    /// `@isaret` at login: at a portal the area is peaceful.
    pub fn login_beacon(
        &self,
        p: &mut Profile,
        seed: &Value,
        station: Option<Station>,
    ) -> Map<String, Value> {
        let at_portal = p.d["position"].get("at_portal").is_some_and(truthy);
        self.beacon(
            p,
            seed,
            BeaconState {
                station,
                near_gate: at_portal,
                gate_peace: at_portal,
                ..Default::default()
            },
        )
    }

    // -------------------------------------------------------------- @arayuz

    /// `UILayoutCommand` at `now`. `hook`: a session's own answer for a menu item's visibility (`window_hooks`).
    pub fn ui_layout(
        &self,
        p: &Profile,
        cal: &EventCalendar,
        now: f64,
        hook: &dyn Fn(&str) -> Option<bool>,
    ) -> Value {
        let mut ui = p.d["ui"].clone();
        let has_pet = p.d.get("pet").is_some_and(truthy);
        for bar in bars_mut(&mut ui) {
            for it in items_mut(bar) {
                let Some(id) = it.get("itemId").and_then(Value::as_str).map(str::to_string) else {
                    continue;
                };
                let Some(it) = it.as_object_mut() else {
                    continue;
                };
                if id == "pet" {
                    it.insert("visible".into(), json!(has_pet));
                }
                if MODE_WINDOWS.contains(&id.as_str()) {
                    it.insert("visible".into(), json!(true));
                }
                if is_event_window(&id) {
                    let v = hook(&id).unwrap_or_else(|| {
                        window_events(cal, &id)
                            .iter()
                            .any(|k| cal.is_active(k, now))
                    });
                    it.insert("visible".into(), json!(v));
                }
                if CLOSED_AT_LOGIN.contains(&id.as_str()) {
                    it.insert("maximized".into(), json!(false));
                }
                if HIDDEN_WINDOWS.contains(&id.as_str()) {
                    it.insert("visible".into(), json!(false));
                }
            }
        }
        // the mode and event windows go to the second row (type 3): the top row's left icons that do not fit are never shown
        let bars = ui
            .get_mut("featureBars")
            .and_then(Value::as_array_mut)
            .map(|b| b.as_mut_slice())
            .unwrap_or_default();
        let last_of = |bars: &[Value], t: i64| {
            bars.iter()
                .rposition(|b| b.get("featureBarType").and_then(Value::as_f64) == Some(t as f64))
        };
        if let (Some(i1), Some(i3)) = (last_of(bars, 1), last_of(bars, 3))
            && i1 != i3
        {
            let second = |it: &Value| {
                it.get("itemId")
                    .and_then(Value::as_str)
                    .is_some_and(|id| MODE_WINDOWS.contains(&id) || is_event_window(id))
            };
            let items1 = bars[i1]["featureBarItems"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let (moving, staying): (Vec<Value>, Vec<Value>) = items1.into_iter().partition(second);
            bars[i1]["featureBarItems"] = Value::Array(staying);
            let mut items3 = bars[i3]["featureBarItems"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            items3.extend(moving);
            bars[i3]["featureBarItems"] = Value::Array(items3);
        }
        // feature bars the player dragged (WindowSettingsRequest)
        let ws = bar_layout(p);
        for bar in bars.iter_mut() {
            let t = bar.get("featureBarType").and_then(Value::as_f64);
            let Some(&(_, pos, order)) = FEATURE_BAR_KEYS
                .iter()
                .find(|(k, _, _)| Some(*k as f64) == t)
            else {
                continue;
            };
            let Some(bar) = bar.as_object_mut() else {
                continue;
            };
            if let Some(v) = ws.get(pos).filter(|v| truthy(v)) {
                bar.insert("featureBarPosition".into(), v.clone());
            }
            if let Some(order) = order
                && let Some(v) = ws.get(order).and_then(layout_ok)
            {
                bar.insert("featureBarOrder".into(), json!(v));
            }
        }
        ui
    }
}

fn bars_mut(ui: &mut Value) -> impl Iterator<Item = &mut Value> {
    ui.get_mut("featureBars")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
}

fn items_mut(bar: &mut Value) -> impl Iterator<Item = &mut Value> {
    bar.get_mut("featureBarItems")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
}

/// The positions of the bars dragged in the interface configuration (`bar_layout`).
pub fn bar_layout(p: &Profile) -> Map<String, Value> {
    p.d.get("settings")
        .and_then(|s| s.get("windowSettingsModule"))
        .filter(|v| truthy(v))
        .map(obj)
        .unwrap_or_default()
}

/// A valid layout order (0-5) as text (`_layout_ok` then `str()`).
fn layout_ok(v: &Value) -> Option<String> {
    let text = match v {
        Value::String(s) => s.clone(),
        Value::Number(n) if n.is_u64() || n.is_i64() => n.to_string(),
        _ => return None,
    };
    let digits = !text.is_empty() && text.chars().all(|c| c.is_ascii_digit());
    (digits && text.parse::<u64>().is_ok_and(|n| n < LAYOUT_COUNT)).then_some(text)
}

/// The calendar events an event window is tied to (`window_events`).
pub fn window_events(cal: &EventCalendar, item: &str) -> Vec<String> {
    if let Some((_, Some(keys))) = event_windows().find(|(k, _)| *k == item) {
        return keys.iter().map(|k| k.to_string()).collect();
    }
    let mut out = Vec::new();
    for k in cal.keys() {
        let Some(e) = cal.event(k) else { continue };
        let hit = if item == "miniclient_reward" {
            e.data
                .get("kinds")
                .and_then(Value::as_array)
                .is_some_and(|ks| ks.iter().any(|x| x == "MiniClientReward"))
        } else {
            let mut secs: Vec<&Value> = e.data.get("map").into_iter().collect();
            secs.extend(
                e.data
                    .get("map_extra")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten(),
            );
            secs.iter().any(|s| {
                let cmd = s
                    .get("info")
                    .filter(|i| truthy(i))
                    .and_then(|i| i.get("command"))
                    .and_then(Value::as_str);
                INDICATOR_WINDOWS
                    .iter()
                    .any(|(c, w)| Some(*c) == cmd && *w == item)
            })
        };
        if hit {
            out.push(k.to_string());
        }
    }
    out
}

/// The gate rings: the bits of the gates completed at least once (`galaxy_gates.rings`).
pub fn rings(p: &Profile) -> i64 {
    let gates = p.d.get("gates").and_then(|g| g.get("gates"));
    RINGS
        .iter()
        .filter(|(k, _)| {
            gates
                .and_then(|g| g.get(*k))
                .and_then(|g| g.get("done"))
                .is_some_and(truthy)
        })
        .map(|(_, bit)| bit)
        .sum()
}

/// Whether admin mode is on for this player (`admin_mode.is_open`).
pub fn admin_open(settings: &Settings, who: Player) -> bool {
    settings.switch("admin_mode") && !who.guest && who.host
}

/// The admin ship's look, or `None` for the player's own ship (`admin_mode.ship`).
pub fn admin_ship(settings: &Settings, who: Player) -> Option<&'static str> {
    if !admin_open(settings, who) {
        return None;
    }
    let i = settings
        .level("admin_ship")
        .clamp(0, ADMIN_SHIPS.len() as i64 - 1);
    ADMIN_SHIPS[i as usize]
}

/// The extras fitted on the active ship and its drones (`extras.equipped`).
pub fn extras_equipped(p: &Profile) -> Vec<String> {
    let strs = |v: Option<&Value>| -> Vec<String> {
        v.and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect()
    };
    let mut out = strs(p.d.get("equipment").and_then(|e| e.get("extra")));
    for pair in
        p.d.get("drone_equipment")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
    {
        out.extend(
            strs(pair.get(1))
                .into_iter()
                .filter(|l| l.starts_with("equipment_extra_")),
        );
    }
    out
}
