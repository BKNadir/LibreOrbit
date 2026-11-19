//! Map links (OpenOrbit's `map_links.py`): the maps and portals of the client's own `spacemap-config.xml` and
//! `maps-config.xml`, the portals the server adds (5-4, LoW), the worlds of the maps that are not in the recorded
//! template (stations moved and turned, structures, quest givers) and the portal and station hints.
//!
//! A portal's in-game position is its place on the 315x210 hint map scaled to the standard map and snapped to the
//! portal grid; the layout ([`MapLayout`]) overrides it on the pirate and Black Light maps. The player's company
//! (`PLAYER_FACTION` in OpenOrbit) is passed explicitly.

use crate::map_layout::MapLayout;
use lo_core::GameData;
use lo_core::pyrandom::Random;
use lo_core::settings::Settings;
use regex::Regex;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, HashMap};
use std::f64::consts::PI;

pub const GATE_ID_BASE: i64 = 150_000_167;
pub const MAP_W: i64 = 21000;
pub const MAP_H: i64 = 13100;
pub const JUMP_RADIUS: f64 = 600.0;
pub const JUMP_DELAY_SEC: f64 = 3.05;
pub const GATE_PEACE_RADIUS: f64 = 1000.0;
pub const NO_DMZ_MAPS: [&str; 4] = ["4-1", "4-2", "4-3", "4-4"];
pub const DMZ_AFTER_JUMP_SEC: [(&str, f64); 1] = [("4-5", 15.0)];
pub const GIVER_PEACE_RADIUS: f64 = GATE_PEACE_RADIUS;
/// Cubikon Graveyard and LoW: not in the jump CPU list.
pub const SPECIAL: [i64; 2] = [42, 200];
pub const TUTORIAL_GATE: i64 = 150_000_774;
pub const TUTORIAL_MAX_LEVEL: i64 = 10;
pub const LOW_DESIGN: i64 = 34;
pub const LOW_START: (i64, i64) = (1000, 11800);
pub const PIRATE_MAPS: [i64; 3] = [91, 92, 93];
pub const ENTRY_DESIGN: i64 = 51;
pub const ONE_WAY_DESIGN: i64 = 52;
pub const ONE_WAY_GATE_BASE: i64 = GATE_ID_BASE + 991_000;
/// Maps with stations -> company (X-1 and X-8).
pub const STATION_MAPS: [(i64, i64); 6] = [(1, 1), (5, 2), (9, 3), (20, 1), (24, 2), (28, 3)];
pub const FACTION_NAME: [(i64, &str); 3] = [(1, "MMO"), (2, "EIC"), (3, "VRU")];
pub const X8_HOME: [(i64, i64); 3] = [(1, 20), (2, 24), (3, 28)];
pub const UPPERS: [&str; 6] = ["X-5", "X-6", "X-7", "X-8", "4-4", "4-5"];
/// Asset types of the X-8 outpost.
pub const OUTPOST: [(i64, i64); 6] = [(46, 47), (48, 49), (50, 51), (53, 54), (55, 56), (57, 58)];
pub const OUTPOST_MAPS: [i64; 3] = [20, 24, 28];
pub const TPL_CENTER: (f64, f64) = (19000.0, 2000.0);
pub const CORNERS: [(f64, f64); 4] = [
    (19000.0, 2000.0),
    (2000.0, 2000.0),
    (19000.0, 11100.0),
    (2000.0, 11100.0),
];
pub const STATION_POS: [(i64, (f64, f64)); 4] = [
    (24, (10500.0, 2000.0)),
    (20, (2000.0, 6750.0)),
    (28, (19000.0, 6750.0)),
    (9, (19000.0, 11500.0)),
];
pub const STATION_TURN: [(i64, f64); 4] = [(1, 270.0), (9, 90.0), (20, 270.0), (28, 90.0)];
pub const OUTPOST_TURN: f64 = -PI / 4.0;
/// Structures outside stations: map -> asset type (34 quest giver, 50 refinery).
pub const STRUCTURES: [(i64, i64); 10] = [
    (4, 34),
    (8, 34),
    (12, 34),
    (17, 34),
    (21, 34),
    (25, 34),
    (306, 34),
    (307, 34),
    (308, 34),
    (92, 50),
];
pub const STRUCTURE_STATION: [(i64, i64); 1] = [(92, 6)];
pub const STRUCTURE_ASSET_TYPE: [(i64, i64); 1] = [(92, 52)];
pub const LOW_STATION: [(i64, i64); 1] = [(200, 4)];
pub const STATION_ASSETS: [(i64, &str); 9] = [
    (46, "ttip_company_hq"),
    (48, "ttip_hangar"),
    (50, "ttip_ore_trade"),
    (52, "ttip_ore_trade"),
    (53, "ttip_instant_repair"),
    (54, "ttip_instant_repair"),
    (47, "ttip_company_hq"),
    (49, "ttip_hangar"),
    (51, "ttip_ore_trade"),
];
pub const REPAIR_ASSETS: [i64; 2] = [53, 54];
pub const ASSET_RANGE: f64 = 500.0;
pub const QUEST_GIVER: i64 = 34;
pub const QUEST_GIVER_DESIGN: [(&str, i64); 4] = [("1", 3), ("8", 4), ("5", 5), ("4", 5)];
pub const QUEST_GIVER_NAME: [(i64, &str); 3] = [
    (1, "Morgus Petterson"),
    (2, "Reginald Crowley"),
    (3, "Vanessa Arkadium"),
];
pub const BL_QUEST_GIVER_NAME: &str = "Mysterious Outpost";
pub const MODE_TIPS: [(&str, &str); 5] = [
    ("voyagers", "ttip_jumpgate_voyagersascentrial"),
    ("trinity", "ttip_jumpgate_trinitytrials"),
    ("astral", "ttip_jumpgate_astralascension"),
    ("ebl", "ttip_jumpgate_eternal_blacklight"),
    ("labyrinth", "ttip_jumpgate_frozenLabyrinth"),
];
pub const DESIGN_TIPS: [(i64, &str); 14] = [
    (87, "ttip_jumpgate_astralascension_weapon_1"),
    (88, "ttip_jumpgate_astralascension_weapon_2"),
    (89, "ttip_jumpgate_astralascension_generator_1"),
    (90, "ttip_jumpgate_astralascension_generator_2"),
    (91, "ttip_jumpgate_astralascension_ammo_1"),
    (92, "ttip_jumpgate_astralascension_ammo_2"),
    (93, "ttip_jumpgate_astralascension_booster_1"),
    (94, "ttip_jumpgate_astralascension_booster_1"),
    (95, "ttip_jumpgate_astralascension_upgrademodule_1"),
    (96, "ttip_jumpgate_astralascension_upgrademodule_2"),
    (97, "ttip_jumpgate_astralascension_resource_1"),
    (98, "ttip_jumpgate_astralascension_resource_2"),
    (99, "ttip_jumpgate_astralascension_hp_1"),
    (100, "ttip_jumpgate_astralascension_finalboss"),
];
pub const MODE_NAMES: [(&str, &str); 4] = [
    ("echoes", "window_dse_title"),
    ("plutus", "title_plutus"),
    ("plutus_easy", "title_plutus"),
    ("invasion", "title_invasion"),
];

fn lookup<K: PartialEq + Copy, V: Copy>(table: &[(K, V)], k: K) -> Option<V> {
    table.iter().find(|(x, _)| *x == k).map(|(_, v)| *v)
}

fn factions(s: &str) -> Option<i64> {
    match s {
        "mmo" => Some(1),
        "eic" => Some(2),
        "vru" => Some(3),
        _ => None,
    }
}

/// `int(round(x))`: half to even.
fn round_i(x: f64) -> i64 {
    x.round_ties_even() as i64
}

fn snap_x(x: i64) -> i64 {
    if x < 80 {
        return 2000;
    }
    if x > 235 {
        return 18500;
    }
    round_i(x as f64 * MAP_W as f64 / 315.0 / 100.0) * 100
}

fn snap_y(y: i64) -> i64 {
    if y < 60 {
        return 2000;
    }
    if y > 150 {
        return 11500;
    }
    round_i(y as f64 * MAP_H as f64 / 210.0 / 100.0) * 100
}

/// A portal record (the same fields, in the same order, as OpenOrbit's dict).
pub type Gate = Map<String, Value>;

/// The maps and portals.
pub struct MapLinks {
    pub layout: MapLayout,
    /// Map id -> name (`1-1`, `5-2`, `BL-1`, `???` …).
    pub names: BTreeMap<i64, String>,
    /// Map id -> (habitant, foreign) required level.
    pub levels: HashMap<i64, (i64, i64)>,
    /// Map id -> scale factor (`maps-config.xml`).
    pub scale: HashMap<i64, f64>,
    /// Map id -> its portals.
    pub gates: HashMap<i64, Vec<Gate>>,
    client_maps: HashMap<i64, String>,
}

impl MapLinks {
    /// From the two XML files' text (`None`: a file that does not exist) and the layout.
    pub fn from_sources(
        maps_config: Option<&str>,
        spacemap: Option<&str>,
        layout: MapLayout,
    ) -> MapLinks {
        let mut names = BTreeMap::new();
        let mut levels = HashMap::new();
        let mut scale = HashMap::new();
        let mut gates: HashMap<i64, Vec<Gate>> = HashMap::new();
        let mut client_maps = HashMap::new();
        if let Some(mc) = maps_config {
            let text = Regex::new(r"(?s)<!--.*?-->").unwrap().replace_all(mc, "");
            for c in Regex::new(r#"<map id="(\d+)"[^>]*scaleFactor="([\d.]+)""#)
                .unwrap()
                .captures_iter(&text)
            {
                if let (Ok(id), Ok(f)) = (c[1].parse::<i64>(), c[2].parse::<f64>()) {
                    scale.insert(id, f);
                }
            }
            let lv = Regex::new(r#"<level [^>]*habitant="(\d+)"[^>]*foreign="(\d+)""#).unwrap();
            for c in Regex::new(r#"(?s)<map id="(\d+)"[^>]*>(.*?)</map>"#)
                .unwrap()
                .captures_iter(&text)
            {
                if let Some(l) = lv.captures(&c[2]) {
                    levels.insert(
                        c[1].parse().unwrap_or(0),
                        (l[1].parse().unwrap_or(0), l[2].parse().unwrap_or(0)),
                    );
                }
            }
            // the client's names: the whole file, comments included (client_map_name)
            for c in Regex::new(r#"<map id="(\d+)"[^>]*?\bname="([^"]*)""#)
                .unwrap()
                .captures_iter(mc)
            {
                client_maps.insert(c[1].parse().unwrap_or(0), c[2].to_string());
            }
        }
        if let Some(text) = spacemap {
            let pos_re = Regex::new(r#"<position x="(\d+)" y="(\d+)""#).unwrap();
            let to_re = Regex::new(r"<leadsToMap>(\d+)</leadsToMap>").unwrap();
            let fac_re = Regex::new(r"<faction>(\w+)</faction>").unwrap();
            let own_re = Regex::new(r"<visibleOwnFaction>(\w+)</visibleOwnFaction>").unwrap();
            for m in Regex::new(r#"(?s)<jumpgate key="(\d+)" ref="(\d+)">(.*?)</jumpgate>"#)
                .unwrap()
                .captures_iter(text)
            {
                let (key, rf, body) = (
                    m[1].parse::<i64>().unwrap_or(0),
                    m[2].parse::<i64>().unwrap_or(0),
                    &m[3],
                );
                let (Some(pos), Some(to)) = (pos_re.captures(body), to_re.captures(body)) else {
                    continue;
                };
                let sf = scale.get(&rf).copied().filter(|s| *s > 1.0).unwrap_or(1.0);
                let x = (snap_x(pos[1].parse().unwrap_or(0)) as f64 * sf).trunc() as i64;
                let y = (snap_y(pos[2].parse().unwrap_or(0)) as f64 * sf).trunc() as i64;
                let mut g = Map::new();
                g.insert("gateId".into(), json!(GATE_ID_BASE + key));
                g.insert("key".into(), json!(key));
                g.insert("x".into(), json!(x));
                g.insert("y".into(), json!(y));
                g.insert("target".into(), json!(to[1].parse::<i64>().unwrap_or(0)));
                let fac = fac_re.captures(body);
                let own = own_re.captures(body);
                if let Some(fac) = &fac {
                    g.insert("company".into(), json!(factions(&fac[1])));
                }
                if let (Some(fac), Some(own)) = (&fac, &own)
                    && &own[1] == "true"
                {
                    g.insert("only_company".into(), json!(factions(&fac[1])));
                }
                gates.entry(rf).or_default().push(g);
            }
            for c in Regex::new(r"(?m)^\s*(\d+)\s+(\d-\d|\d-BL|BL-\d)\s*$")
                .unwrap()
                .captures_iter(text)
            {
                names.insert(c[1].parse().unwrap_or(0), c[2].to_string());
            }
        }
        for (m, n) in [(94, "5-4"), (42, "???"), (200, "LoW")] {
            names.entry(m).or_insert_with(|| n.to_string());
        }
        let silver = |c: i64| json!({"fee": "silver_key", "tooltip": "ttip_jumpgate_hiddenPirateMap", "company": c, "only_company": c});
        let low = json!({"low": true, "arrival": [LOW_START.0, LOW_START.1], "design": LOW_DESIGN});
        let extra: [(i64, i64, i64, i64, i64, Value); 7] = [
            (92, 990006, 18500, 11500, 94, silver(1)),
            (92, 990001, 18500, 11500, 94, silver(2)),
            (92, 990007, 18500, 11500, 94, silver(3)),
            (94, 990002, 4000, 13000, 92, json!({})),
            (3, 990003, 2000, 2000, 200, low.clone()),
            (7, 990004, 2000, 2000, 200, low.clone()),
            (11, 990005, 18500, 2000, 200, low),
        ];
        for (m, k, x, y, to, more) in extra {
            let mut g = Map::new();
            g.insert("gateId".into(), json!(GATE_ID_BASE + k));
            g.insert("key".into(), json!(k));
            g.insert("x".into(), json!(x));
            g.insert("y".into(), json!(y));
            g.insert("target".into(), json!(to));
            g.extend(more.as_object().cloned().unwrap_or_default());
            gates.entry(m).or_default().push(g);
        }
        for (m, gs) in gates.iter_mut() {
            for g in gs.iter_mut() {
                let key = g["key"].as_i64().unwrap_or(0);
                if let Some((x, y)) = layout.gate_pos(*m, key) {
                    g.insert("x".into(), x);
                    g.insert("y".into(), y);
                }
                if let Some(v) = layout.arrival(*m, key).filter(lo_core::settings::truthy) {
                    g.insert("arrival".into(), v);
                }
                let company = g
                    .get("company")
                    .filter(|c| lo_core::settings::truthy(c))
                    .cloned();
                let target = g["target"].as_i64().unwrap_or(0);
                if let Some(c) = company
                    && (PIRATE_MAPS.contains(m) || PIRATE_MAPS.contains(&target))
                {
                    g.insert("design".into(), json!(ENTRY_DESIGN));
                    g.insert("decorations".into(), json!([c]));
                }
            }
        }
        for (m, n) in &names {
            if n.starts_with("5-") {
                levels.insert(*m, (12, 12));
            }
        }
        MapLinks {
            layout,
            names,
            levels,
            scale,
            gates,
            client_maps,
        }
    }

    /// Reads `data/maps-config.xml`, `data/spacemap-config.xml` (as they are) and the layout (with the world's patches).
    pub fn load(data: &GameData) -> Result<MapLinks, String> {
        let read = |f: &str| std::fs::read_to_string(data.dir().join(f)).ok();
        Ok(MapLinks::from_sources(
            read("maps-config.xml").as_deref(),
            read("spacemap-config.xml").as_deref(),
            MapLayout::load(data)?,
        ))
    }

    pub fn name(&self, map_id: i64) -> String {
        self.names
            .get(&map_id)
            .cloned()
            .unwrap_or_else(|| map_id.to_string())
    }

    /// Does a quest giver on this map have a demilitarized zone?
    pub fn giver_dmz(&self, map_id: i64) -> bool {
        let n = self.name(map_id);
        !NO_DMZ_MAPS.contains(&n.as_str())
            && lookup(&DMZ_AFTER_JUMP_SEC, n.as_str()).is_none()
            && !n.starts_with("BL-")
    }

    /// The map's company (1 MMO, 2 EIC, 3 VRU) or `None` (common maps).
    pub fn company(&self, map_id: i64) -> Option<i64> {
        let n = self.name(map_id);
        let c: Vec<char> = n.chars().collect();
        if let Some(rest) = n.strip_prefix("BL-")
            && ["1", "2", "3"].contains(&rest)
        {
            return rest.parse().ok();
        }
        (c.len() >= 3 && "123".contains(c[0]) && c[1] == '-').then(|| i64::from(c[0] as u8 - b'0'))
    }

    /// The level needed to enter the map (`level_required`).
    pub fn level_required(&self, map_id: i64, faction: Option<i64>, settings: &Settings) -> i64 {
        if settings.switch("no_level_limit") {
            return 0;
        }
        let (hab, foreign) = self.levels.get(&map_id).copied().unwrap_or((0, 0));
        match self.company(map_id) {
            Some(c) if Some(c) != faction => foreign,
            _ => hab,
        }
    }

    /// The map's size: the standard size times its scale factor.
    pub fn map_size(&self, map_id: i64) -> (i64, i64) {
        let s = self.scale.get(&map_id).copied().unwrap_or(1.0);
        (
            (MAP_W as f64 * s).trunc() as i64,
            (MAP_H as f64 * s).trunc() as i64,
        )
    }

    /// `X-n` for company maps (1-3 and 2-3 -> X-3), the others their own name.
    pub fn map_class(&self, map_id: i64) -> String {
        let n = self.name(map_id);
        let c: Vec<char> = n.chars().collect();
        if c.len() == 3 && "123".contains(c[0]) && c[1] == '-' && c[2].is_numeric() {
            format!("X-{}", c[2])
        } else {
            n
        }
    }

    /// The company's X-8 to respawn at after dying on an upper map without a station (level 12+), otherwise `None`.
    pub fn x8_home(&self, faction: i64, level: i64, map_id: i64) -> Option<i64> {
        if level >= 12 && UPPERS.contains(&self.map_class(map_id).as_str()) {
            lookup(&X8_HOME, faction)
        } else {
            None
        }
    }

    /// The map's portals, the player's own company's first (`gates`).
    pub fn gates(&self, map_id: i64, faction: Option<i64>) -> Vec<&Gate> {
        let mut out: Vec<&Gate> = self
            .gates
            .get(&map_id)
            .map(|g| g.iter().collect())
            .unwrap_or_default();
        let other = |g: &&Gate| {
            let only = g
                .get("only_company")
                .filter(|v| lo_core::settings::truthy(v));
            only.is_some_and(|o| faction.is_some_and(|f| Some(f) != o.as_i64()))
        };
        out.sort_by_key(|g| other(g));
        out
    }

    /// The point where the ship lands on the target map: the return portal, otherwise the map's center.
    pub fn arrival(&self, src: i64, dst: i64, faction: Option<i64>) -> (Value, Value) {
        if let Some(back) = self
            .gates(dst, faction)
            .into_iter()
            .find(|g| g["target"].as_i64() == Some(src))
        {
            return (back["x"].clone(), back["y"].clone());
        }
        let (w, h) = self.map_size(dst);
        if self
            .layout
            .of(dst)
            .get("center")
            .is_some_and(lo_core::settings::truthy)
        {
            let c = self.center(dst);
            return (c.0, c.1);
        }
        (json!(w / 2), json!(h / 2))
    }

    /// A random arrival point away from the edges and the obstacles.
    pub fn random_arrival(&self, map_id: i64, rng: &mut Random) -> (i64, i64) {
        let (w, h) = self.map_size(map_id);
        self.layout
            .free_spot(
                map_id,
                (2000.0, 2000.0, (w - 2000) as f64, (h - 2000) as f64),
                crate::map_layout::SHIP_PAD,
                40,
                rng,
            )
            .unwrap_or((w / 2, h / 2))
    }

    /// Can the ship be left on this map: a known map that is not special.
    pub fn parked(&self, map_id: i64) -> bool {
        self.names.contains_key(&map_id) && !SPECIAL.contains(&map_id)
    }

    /// The middle of the map, or the layout's `center`.
    pub fn center(&self, map_id: i64) -> (Value, Value) {
        let (w, h) = self.map_size(map_id);
        match self
            .layout
            .of(map_id)
            .get("center")
            .filter(|c| lo_core::settings::truthy(c))
        {
            Some(c) => (c[0].clone(), c[1].clone()),
            None => (json!(w / 2), json!(h / 2)),
        }
    }

    /// The map's name in the client's `maps-config.xml`.
    pub fn client_map_name(&self, map_id: i64) -> Option<&str> {
        self.client_maps.get(&map_id).map(String::as_str)
    }

    /// The portal's hint: key and `[(value, type)]` (5 flashres key, 6 map id, 0 plain text).
    pub fn gate_tip(&self, g: &Map<String, Value>) -> (String, Vec<(String, i64)>) {
        let truthy = |k: &str| g.get(k).filter(|v| lo_core::settings::truthy(v));
        if let Some(t) = truthy("tooltip") {
            let vals = truthy("hint_target")
                .map(|h| vec![(py_str(h), 6)])
                .unwrap_or_default();
            return (py_str(t), vals);
        }
        let dest = truthy("hint_target")
            .or_else(|| truthy("target"))
            .map_or(0, |v| v.as_f64().unwrap_or(0.0) as i64);
        if truthy("exit").is_none() && !self.names.contains_key(&dest) {
            let by_design = g
                .get("designId")
                .and_then(Value::as_i64)
                .and_then(|d| lookup(&DESIGN_TIPS, d));
            let gate = g.get("gate").and_then(Value::as_str);
            let by_mode = gate.and_then(|m| lookup(&MODE_TIPS, m));
            if let Some(k) = by_design.or(by_mode) {
                return (k.to_string(), Vec::new());
            }
            if let Some(n) = gate.and_then(|m| lookup(&MODE_NAMES, m)) {
                return ("ttip_jumpgate".into(), vec![(n.to_string(), 5)]);
            }
        }
        if dest != 0 && self.client_map_name(dest).is_some_and(|n| !n.is_empty()) {
            return ("ttip_jumpgate".into(), vec![(dest.to_string(), 6)]);
        }
        if let Some(n) = truthy("name") {
            return ("ttip_jumpgate".into(), vec![(py_str(n), 0)]);
        }
        ("faq_jumpgate".into(), Vec::new())
    }

    /// `MapAssetActionAvailableCommand` on approaching / leaving a portal (`gate_available`).
    pub fn gate_available(&self, g: &Map<String, Value>, near: bool) -> (&'static str, Value) {
        let (key, vals) = self.gate_tip(g);
        let reps: Vec<Value> = vals
            .into_iter()
            .map(|(v, t)| {
                json!({"_": "MessageWildcardReplacementModule", "wildcard": "%MAP%", "replacement": v,
                       "replacementType": {"_": "ReplacementTypeModule", "typeValue": t}})
            })
            .collect();
        let tip = json!({"_": "MessageKeyListWildcardCommand", "commandList": [
            {"_": "MessageKeyListWildcardModule", "wildCardReplacements": reps, "baseKey": key,
             "baseKeyType": {"_": "ReplacementTypeModule", "typeValue": 5}, "textStyle": 0}]});
        (
            "MapAssetActionAvailableCommand",
            json!({"mapAssetId": g.get("gateId").cloned().unwrap_or(Value::Null), "state": if near { 0 } else { 1 },
                   "activatable": near, "toolTip": tip,
                   "additionalInformation": {"_": "AssetInformationModule"}}),
        )
    }

    /// The quest giver's model and name by the map's company and base type (`quest_giver`).
    pub fn quest_giver(&self, m: &Map<String, Value>, map_id: i64) -> Map<String, Value> {
        let fac = self.company(map_id);
        let n = self.name(map_id);
        let kind = m
            .get("type")
            .and_then(|t| t.get("typeValue"))
            .and_then(Value::as_i64);
        let Some(fac) = fac.filter(|_| kind == Some(QUEST_GIVER)) else {
            return m.clone();
        };
        let bl = n.starts_with("BL-");
        let design = if bl {
            12
        } else {
            lookup(&QUEST_GIVER_DESIGN, n.get(2..).unwrap_or("")).unwrap_or(0) + 3 * (fac - 1)
        };
        let mut out = m.clone();
        out.insert("designId".into(), json!(design));
        out.insert(
            "userName".into(),
            json!(if bl {
                BL_QUEST_GIVER_NAME
            } else {
                lookup(&QUEST_GIVER_NAME, fac).unwrap_or("")
            }),
        );
        out
    }

    /// The only structure of a map without a station -> (entry messages, ready messages) (`structure`).
    pub fn structure(&self, map_id: i64, tpl: &Value, gs: &[&Gate]) -> (Vec<Value>, Vec<Value>) {
        let Some(kind) = lookup(&STRUCTURES, map_id) else {
            return (Vec::new(), Vec::new());
        };
        let entry_tpl = tpl
            .get("entry")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let Some(src) = entry_tpl.iter().find(|m| {
            m["_m"] == "AssetCreateCommand" && m["type"]["typeValue"].as_i64() == Some(kind)
        }) else {
            return (Vec::new(), Vec::new());
        };
        let mut spots = CORNERS.to_vec();
        spots.extend([
            (10500.0, 6550.0),
            (10500.0, 2000.0),
            (10500.0, 11100.0),
            (2000.0, 6550.0),
            (19000.0, 6550.0),
        ]);
        let (cx, cy) = self
            .layout
            .structure_pos(map_id)
            .or_else(|| lookup(&STATION_POS, map_id))
            .unwrap_or_else(|| farthest(&spots, gs));
        let aid = 151_000_000 + map_id * 100 + 90;
        let (pos, radius) = self.layout.structure_asset(map_id);
        let (ax, ay) = pos.unwrap_or((num(cx), num(cy)));
        let mut a = src.as_object().cloned().unwrap_or_default();
        a.insert("assetId".into(), json!(aid));
        a.insert("posX".into(), ax.clone());
        a.insert("posY".into(), ay.clone());
        let faction = if kind == QUEST_GIVER {
            src["factionId"].clone()
        } else {
            json!(self.company(map_id).unwrap_or(0))
        };
        a.insert("factionId".into(), faction);
        let mut e0 = self.quest_giver(&a, map_id);
        if let Some(t) = lookup(&STRUCTURE_ASSET_TYPE, map_id) {
            let mut ty = src["type"].as_object().cloned().unwrap_or_default();
            ty.insert("typeValue".into(), json!(t));
            e0.insert("type".into(), Value::Object(ty));
        }
        let mut ready = Vec::new();
        if let Some(st) = lookup(&STRUCTURE_STATION, map_id) {
            let peace = self
                .layout
                .safe_zones(map_id)
                .into_iter()
                .find(|&(x, y, _)| (x, y) == (cx, cy))
                .map_or(json!(0), |(_, _, r)| num(r));
            ready.push(json!({"_m": "SpaceStationCommand", "posX": num(cx), "posY": num(cy), "peaceRadius": peace,
                              "id": 0, "type": st}));
        }
        let src_id = src["assetId"].clone();
        let sid = py_str(&src_id);
        for m in tpl
            .get("ready")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let pid = m["poiId"].as_str().unwrap_or_default();
            if m["_m"] == "MapAddPOICommand"
                && (pid == format!("mapAssetRangeZone-{sid}") || pid == format!("questGiver_{sid}"))
            {
                let mut sc = m["shapeCoordinates"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                if sc.len() >= 3 {
                    sc[0] = ax.clone();
                    sc[1] = ay.clone();
                    if let Some(r) = radius.clone().filter(lo_core::settings::truthy) {
                        sc[2] = r;
                    }
                }
                let mut out = m.as_object().cloned().unwrap_or_default();
                out.insert("poiId".into(), json!(pid.replace(&sid, &aid.to_string())));
                out.insert("shapeCoordinates".into(), Value::Array(sc));
                ready.push(Value::Object(out));
            }
        }
        (vec![Value::Object(e0)], ready)
    }

    /// `JumpgateCreateCommand` (`jumpgate_create`): on 5-X another company's portal is invisible.
    pub fn jumpgate_create(
        &self,
        g: &Map<String, Value>,
        faction: Option<i64>,
    ) -> (&'static str, Value) {
        let t = |k: &str| g.get(k).is_some_and(lo_core::settings::truthy);
        let only = g
            .get("only_company")
            .filter(|v| lo_core::settings::truthy(v));
        let hidden =
            t("invisible") || only.is_some_and(|o| faction.is_some_and(|f| Some(f) != o.as_i64()));
        let design = g
            .get("designId")
            .or_else(|| g.get("design"))
            .cloned()
            .unwrap_or(json!(1));
        (
            "JumpgateCreateCommand",
            json!({"gateId": g.get("gateId").cloned().unwrap_or(Value::Null), "factionId": 0, "designId": design,
                   "x": g.get("x").cloned().unwrap_or(Value::Null), "y": g.get("y").cloned().unwrap_or(Value::Null),
                   "isVisibleOnMinimap": !(hidden || t("hidden")), "showBubble": !hidden,
                   "decoratorAssetTypes": g.get("decorations").cloned().unwrap_or(json!([]))}),
        )
    }

    /// World messages `{entry, ready}` (`_m` key) for a map not in the recorded template (`map_world`). `tpl`: the
    /// template's 2-1 world; `faction`: the player's company.
    pub fn map_world(&self, map_id: i64, tpl: Option<&Value>, faction: Option<i64>) -> Value {
        let gs = self.gates(map_id, faction);
        let with_m = |(_, v): (&str, Value)| {
            let mut m = Map::new();
            m.extend(v.as_object().cloned().unwrap_or_default());
            m.insert("_m".into(), json!("JumpgateCreateCommand"));
            Value::Object(m)
        };
        let mut gate_msgs: Vec<Value> = self
            .gates
            .get(&map_id)
            .into_iter()
            .flatten()
            .map(|g| with_m(self.jumpgate_create(g, faction)))
            .collect();
        for v in self.layout.one_way_gates(map_id) {
            let fc = v[2].as_i64().unwrap_or(0);
            let g = json!({"gateId": ONE_WAY_GATE_BASE + map_id * 10 + fc, "x": v[0], "y": v[1],
                           "designId": ONE_WAY_DESIGN, "decorations": [v[2]], "invisible": true});
            gate_msgs.push(with_m(self.jumpgate_create(g.as_object().unwrap(), None)));
        }
        let tpl = tpl.filter(|t| lo_core::settings::truthy(t));
        let fac = lookup(&STATION_MAPS, map_id);
        let (Some(fac), Some(tpl)) = (fac, tpl) else {
            let (entry, mut ready) = match tpl {
                Some(t) => self.structure(map_id, t, &gs),
                None => (Vec::new(), Vec::new()),
            };
            if let Some(t) = lookup(&LOW_STATION, map_id) {
                let (w, h) = self.map_size(map_id);
                ready.push(json!({"_m": "SpaceStationCommand", "posX": w / 2, "posY": h / 2, "peaceRadius": 0, "id": 0,
                                  "type": t}));
            }
            ready.extend(gate_msgs);
            ready.extend(self.layout.poi_msgs(map_id));
            return json!({"entry": entry, "ready": ready});
        };
        let (cx, cy) = lookup(&STATION_POS, map_id).unwrap_or_else(|| farthest(&CORNERS, &gs));
        let turn = if OUTPOST_MAPS.contains(&map_id) {
            OUTPOST_TURN
        } else {
            0.0
        } + lookup(&STATION_TURN, map_id).unwrap_or(0.0).to_radians();
        let (cos, sin) = (turn.cos(), turn.sin());
        let place = |x: f64, y: f64| {
            let (ox, oy) = (x - TPL_CENTER.0, y - TPL_CENTER.1);
            (
                round_i(cx + ox * cos - oy * sin),
                round_i(cy + ox * sin + oy * cos),
            )
        };
        let base = 151_000_000 + map_id * 100;
        let mut ids: Vec<(Value, i64)> = Vec::new();
        let mut entry = Vec::new();
        let mut ready = Vec::new();
        let tpl_entry = tpl
            .get("entry")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for m in &tpl_entry {
            if m["_m"] != "AssetCreateCommand" {
                continue;
            }
            let new_id = base + ids.len() as i64;
            match ids.iter_mut().find(|(k, _)| *k == m["assetId"]) {
                Some(slot) => slot.1 = new_id,
                None => ids.push((m["assetId"].clone(), new_id)),
            }
            let (x, y) = place(num_f(&m["posX"]), num_f(&m["posY"]));
            let mut a = m.as_object().cloned().unwrap_or_default();
            a.insert("posX".into(), json!(x));
            a.insert("posY".into(), json!(y));
            let qg = m["type"]["typeValue"].as_i64() == Some(QUEST_GIVER);
            a.insert(
                "factionId".into(),
                if qg {
                    m["factionId"].clone()
                } else {
                    json!(fac)
                },
            );
            a.insert("assetId".into(), json!(new_id));
            let mut a = self.quest_giver(&a, map_id);
            let tv = a["type"]["typeValue"].as_i64().unwrap_or(-1);
            if OUTPOST_MAPS.contains(&map_id)
                && let Some(o) = lookup(&OUTPOST, tv)
            {
                let mut ty = a["type"].as_object().cloned().unwrap_or_default();
                ty.insert("typeValue".into(), json!(o));
                a.insert("type".into(), Value::Object(ty));
            }
            entry.push(Value::Object(a));
        }
        let id_of = |k: &Value| ids.iter().find(|(x, _)| x == k).map(|(_, v)| *v);
        for m in &tpl_entry {
            if m["_m"] == "MapAssetActionAvailableCommand"
                && let Some(n) = id_of(&m["mapAssetId"])
            {
                let mut a = m.as_object().cloned().unwrap_or_default();
                a.insert("mapAssetId".into(), json!(n));
                entry.push(Value::Object(a));
            }
        }
        let fname = lookup(&FACTION_NAME, fac).unwrap_or("");
        for m in tpl
            .get("ready")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if m["_m"] == "SpaceStationCommand" {
                let mut a = m.as_object().cloned().unwrap_or_default();
                a.insert("posX".into(), num(cx));
                a.insert("posY".into(), num(cy));
                a.insert("type".into(), json!(fac));
                ready.push(Value::Object(a));
            } else if m["_m"] == "MapAddPOICommand" {
                let mut pid = m["poiId"].as_str().unwrap_or_default().to_string();
                if pid.starts_with("mapAssetRangeZone-") || pid.starts_with("questGiver_") {
                    let old_s = pid.rsplit(['-', '_']).next().unwrap_or("").to_string();
                    let Ok(old) = old_s.parse::<i64>() else {
                        continue;
                    };
                    let Some(n) = id_of(&json!(old)) else {
                        continue;
                    };
                    pid = format!("{}{}", &pid[..pid.len() - old.to_string().len()], n);
                } else {
                    pid = pid.replace("EIC", fname);
                }
                let mut sc = m["shapeCoordinates"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                if sc.len() >= 2 {
                    let (x, y) = place(num_f(&sc[0]), num_f(&sc[1]));
                    sc[0] = json!(x);
                    sc[1] = json!(y);
                }
                let mut a = m.as_object().cloned().unwrap_or_default();
                a.insert("poiId".into(), json!(pid));
                a.insert("shapeCoordinates".into(), Value::Array(sc));
                ready.push(Value::Object(a));
            }
        }
        ready.extend(gate_msgs);
        json!({"entry": entry, "ready": ready})
    }
}

/// A station object's `MapAssetActionAvailableCommand` (`asset_available`): active with a hint when near.
pub fn asset_available(
    asset_id: &Value,
    tip: &str,
    near: bool,
    wild: &[(String, Value)],
) -> (&'static str, Value) {
    let reps: Vec<Value> = wild
        .iter()
        .map(|(k, v)| {
            json!({"_": "MessageWildcardReplacementModule", "wildcard": k, "replacement": py_str(v),
                   "replacementType": {"_": "ReplacementTypeModule", "typeValue": 0}})
        })
        .collect();
    let lines = if near {
        vec![
            json!({"_": "MessageKeyListWildcardModule", "wildCardReplacements": reps, "baseKey": tip,
                    "baseKeyType": {"_": "ReplacementTypeModule", "typeValue": 5}, "textStyle": 0}),
        ]
    } else {
        Vec::new()
    };
    (
        "MapAssetActionAvailableCommand",
        json!({"mapAssetId": asset_id, "state": if near { 0 } else { 1 }, "activatable": near,
               "toolTip": {"_": "MessageKeyListWildcardCommand", "commandList": lines},
               "additionalInformation": {"_": "AssetInformationModule"}}),
    )
}

/// The candidate farthest from the nearest portal (`max(spots, key=min distance)`; the first of equals).
fn farthest(spots: &[(f64, f64)], gs: &[&Gate]) -> (f64, f64) {
    let score = |c: &(f64, f64)| {
        gs.iter()
            .map(|g| (c.0 - num_f(&g["x"])).hypot(c.1 - num_f(&g["y"])))
            .fold(None, |m: Option<f64>, d| Some(m.map_or(d, |m| m.min(d))))
            .unwrap_or(0.0)
    };
    let mut best = spots[0];
    let mut best_s = score(&best);
    for c in &spots[1..] {
        let s = score(c);
        if s > best_s {
            best = *c;
            best_s = s;
        }
    }
    best
}

fn num_f(v: &Value) -> f64 {
    v.as_f64().unwrap_or(0.0)
}

/// A coordinate kept as Python keeps it: an integer when it is whole in the source tables.
fn num(x: f64) -> Value {
    if x.fract() == 0.0 && x.abs() < 9e15 {
        json!(x as i64)
    } else {
        json!(x)
    }
}

/// `str(v)`.
fn py_str(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => match n.as_i64() {
            Some(i) => i.to_string(),
            None => lo_core::pyjson::float_repr(n.as_f64().unwrap_or(0.0)),
        },
        Value::Bool(b) => if *b { "True" } else { "False" }.into(),
        Value::Null => "None".into(),
        other => other.to_string(),
    }
}
