//! The map world (OpenOrbit's `world.py`), first part: aliens, bonus boxes and ores are placed on the map, shown and
//! removed by view distance, the aliens wander, the player's ship follows `MoveRequest`.
//!
//! Not ported yet: combat, rewards, collecting boxes and ores, portals, the station's objects, the P.E.T., the
//! abilities and the timed systems that run from the world loop. OpenOrbit draws from an unseeded `random`: the places
//! differ from one run to the next on both servers.

use crate::event_calendar::EventCalendar;
use crate::map_links::{self, MapLinks};
use lo_core::GameData;
use lo_core::pyrandom::Random;
use lo_core::settings::Settings;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, HashSet};
use std::f64::consts::PI;

/// The client plays every movement in 1.05 times the sent duration (`CLIENT_TWEEN`).
pub const CLIENT_TWEEN: f64 = 1.05;
pub const CARGO_BOXES: [&str; 3] = ["FROM_SHIP", "CANDY_CARGO", "TURKEY_CARGO"];
pub const BLOCKED_BOX: &str = "FROM_SHIP_BLOCKED";

type Msg = (String, Value);

fn module(name: &str, mut f: Map<String, Value>) -> Value {
    f.insert("_".into(), json!(name));
    Value::Object(f)
}

/// Linear movement (`Mover`).
#[derive(Clone, Debug)]
pub struct Mover {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    pub t0: f64,
    pub t1: f64,
}

impl Mover {
    pub fn new(x: f64, y: f64) -> Mover {
        Mover {
            x0: x,
            y0: y,
            x1: x,
            y1: y,
            t0: 0.0,
            t1: 0.0,
        }
    }

    pub fn pos(&self, now: f64) -> (f64, f64) {
        if now >= self.t1 || self.t1 <= self.t0 {
            return (self.x1, self.y1);
        }
        let f = (now - self.t0) / (self.t1 - self.t0);
        (
            self.x0 + (self.x1 - self.x0) * f,
            self.y0 + (self.y1 - self.y0) * f,
        )
    }

    /// Starts a movement -> its duration for the client (ms).
    pub fn go(&mut self, x: f64, y: f64, speed: f64, now: f64, start: Option<(f64, f64)>) -> i64 {
        let (sx, sy) = start.unwrap_or_else(|| self.pos(now));
        let d = (x - sx).hypot(y - sy);
        let stop = d / speed.max(1.0);
        *self = Mover {
            x0: sx,
            y0: sy,
            x1: x,
            y1: y,
            t0: now,
            t1: now + stop * CLIENT_TWEEN,
        };
        (stop * 1000.0) as i64
    }

    /// The remaining time of an ongoing movement (timeToTarget, ms).
    pub fn ms_left(&self, now: f64) -> i64 {
        ((self.t1 - now) / CLIENT_TWEEN * 1000.0) as i64
    }
}

/// An alien (`NPC`).
pub struct Npc {
    pub id: i64,
    pub tip: String,
    pub d: Value,
    pub model: Value,
    pub mover: Mover,
    pub next_think: f64,
    pub alive: bool,
    ghost: Option<bool>,
}

impl Npc {
    fn speed(&self, settings: &Settings) -> f64 {
        let v = self.d["speed"].as_f64().unwrap_or(0.0);
        let k = settings.multiplier("npc_speed", None);
        if v > 0.0 && k != 1.0 {
            (v * k).trunc().max(1.0)
        } else {
            v
        }
    }
}

/// A box or an ore on the map.
#[derive(Clone, Debug)]
pub struct Item {
    pub tip: Value,
    pub x: i64,
    pub y: i64,
    pub end_time: Option<f64>,
    pub ore: bool,
}

/// The world's data (`npc.json`, the ores of `items.json`).
pub struct WorldData {
    pub cfg: Value,
    pub ores: Value,
    pub ghost_visual: Value,
}

impl WorldData {
    pub fn load(data: &GameData) -> Result<WorldData, String> {
        let cfg = data
            .load("npc.json")
            .map_err(|e| format!("npc.json: {e}"))?;
        let items = data
            .load("items.json")
            .map_err(|e| format!("items.json: {e}"))?;
        Ok(WorldData {
            cfg,
            ores: items["ores"].clone(),
            ghost_visual: items["combat_items"]["ghost"]["visual"].clone(),
        })
    }

    fn combat(&self, k: &str) -> f64 {
        self.cfg["combat"][k].as_f64().unwrap_or(0.0)
    }
}

/// What the world needs from the game.
pub struct Ctx<'a> {
    pub data: &'a WorldData,
    pub links: &'a MapLinks,
    pub cal: &'a EventCalendar,
    pub settings: &'a Settings,
}

pub struct World {
    pub map_id: i64,
    pub map_w: i64,
    pub map_h: i64,
    pub npcs: BTreeMap<i64, Npc>,
    pub items: indexmap::IndexMap<String, Item>,
    pub ore_fields: Vec<(i64, i64)>,
    pub visible: HashSet<i64>,
    pub seen_items: HashSet<String>,
    pub hero: Mover,
    pub hero_speed: f64,
    pub entered: f64,
    /// `box_sight`, `box_drop` (npc.json combat).
    sight: (f64, f64),
    next_uid: i64,
    zones: Vec<(f64, f64, f64)>,
    rng: Random,
    out: Vec<Msg>,
    pub log: Vec<String>,
}

impl World {
    /// `station`: the map's `SpaceStationCommand` (`TEMPLATE.station`); `entry`: the map's world messages (quest givers).
    pub fn new(
        c: &Ctx,
        map_id: i64,
        pos: (f64, f64),
        faction: Option<i64>,
        station: Option<&Value>,
        entry: &[Msg],
        now: f64,
    ) -> World {
        let rules = &c.data.cfg["maps"][map_id.to_string()];
        let (map_w, map_h) = match rules.get("size").and_then(Value::as_array) {
            Some(s) => (s[0].as_i64().unwrap_or(0), s[1].as_i64().unwrap_or(0)),
            None => c.links.map_size(map_id),
        };
        let name = c.links.name(map_id);
        let mut zones: Vec<(f64, f64, f64)> = Vec::new();
        let f = |v: &Value| v.as_f64().unwrap_or(0.0);
        if let Some(s) = station {
            zones.push((f(&s["posX"]), f(&s["posY"]), f(&s["peaceRadius"])));
        }
        zones.extend(c.links.layout.safe_zones(map_id));
        // quest givers of the pilot's company (giver_zones): not inside the home station's zone
        let home = station.filter(|s| s.get("type").and_then(Value::as_i64) == faction);
        if c.links.giver_dmz(map_id) && c.links.company(map_id).is_none_or(|co| Some(co) == faction)
        {
            for (n, a) in entry {
                if n == "AssetCreateCommand"
                    && a["type"]["typeValue"].as_i64() == Some(map_links::QUEST_GIVER)
                {
                    let (x, y) = (f(&a["posX"]), f(&a["posY"]));
                    if !home.is_some_and(|s| {
                        (x - f(&s["posX"])).hypot(y - f(&s["posY"])) <= f(&s["peaceRadius"])
                    }) {
                        zones.push((x, y, map_links::GIVER_PEACE_RADIUS));
                    }
                }
            }
        }
        // portals with a safe zone (peace_gates); 4-5's lasts only a while after entering: left out of the aliens' rules
        let co = c.links.company(map_id);
        if !map_links::NO_DMZ_MAPS.contains(&name.as_str())
            && co.is_none_or(|co| Some(co) == faction)
            && !map_links::DMZ_AFTER_JUMP_SEC
                .iter()
                .any(|(m, _)| *m == name)
        {
            let r = c
                .links
                .layout
                .gate_peace_radius(map_id, map_links::GATE_PEACE_RADIUS);
            for g in c.links.gates(map_id, faction) {
                if !g.get("gate").is_some_and(lo_core::settings::truthy)
                    && !g.get("hidden").is_some_and(lo_core::settings::truthy)
                {
                    zones.push((f(&g["x"]), f(&g["y"]), r));
                }
            }
        }
        World {
            map_id,
            map_w,
            map_h,
            npcs: BTreeMap::new(),
            items: indexmap::IndexMap::new(),
            ore_fields: Vec::new(),
            visible: HashSet::new(),
            seen_items: HashSet::new(),
            hero: Mover::new(pos.0, pos.1),
            hero_speed: 300.0,
            entered: now,
            sight: (c.data.combat("box_sight"), c.data.combat("box_drop")),
            next_uid: 150_300_000, // shared_maps.UID_BASE
            zones,
            rng: Random::new((now * 1e6) as i64),
            out: Vec::new(),
            log: Vec::new(),
        }
    }

    /// The messages gathered since the last call, after the view filter of boxes and ores (`deliver` / `item_filter`).
    pub fn take(&mut self, now: f64) -> Vec<Msg> {
        let out = std::mem::take(&mut self.out);
        let (hx, hy) = self.hero.pos(now);
        let mut keep = Vec::with_capacity(out.len());
        for (n, f) in out {
            match n.as_str() {
                "AddBoxCommand" | "AddOreCommand" => {
                    let h = f["hash"].as_str().unwrap_or("").to_string();
                    if !self.item_in_view(&h, hx, hy) {
                        continue;
                    }
                    self.seen_items.insert(h);
                }
                "RemoveCollectableCommand" => {
                    let h = f["hash"].as_str().unwrap_or("");
                    if !self.seen_items.remove(h) {
                        continue;
                    }
                }
                _ => {}
            }
            keep.push((n, f));
        }
        keep
    }

    fn send(&mut self, msgs: Vec<Msg>) {
        self.out.extend(msgs);
    }

    fn near_zone(&self, x: f64, y: f64, margin: f64) -> bool {
        self.zones
            .iter()
            .any(|&(sx, sy, r)| (x - sx).hypot(y - sy) <= r + margin)
    }

    fn random_spot(&mut self, c: &Ctx) -> (i64, i64) {
        let (w, h) = (self.map_w as f64, self.map_h as f64);
        for _ in 0..50 {
            let x = self.rng.uniform(600.0, w - 600.0);
            let y = self.rng.uniform(600.0, h - 600.0);
            if !self.near_zone(x, y, 700.0) && !c.links.layout.blocked(self.map_id, x, y, 300.0) {
                return (x as i64, y as i64);
            }
        }
        for row in 0..9 {
            for col in 0..9 {
                let x = 600.0 + (w - 1200.0) * f64::from(col) / 8.0;
                let y = 600.0 + (h - 1200.0) * f64::from(row) / 8.0;
                if !self.near_zone(x, y, 700.0) && !c.links.layout.blocked(self.map_id, x, y, 300.0)
                {
                    return (x as i64, y as i64);
                }
            }
        }
        (self.map_w / 2, self.map_h / 2)
    }

    fn rules<'a>(&self, c: &'a Ctx) -> &'a Value {
        &c.data.cfg["maps"][self.map_id.to_string()]
    }

    fn lairs(&self, c: &Ctx, tip: &str) -> Option<Vec<(f64, f64)>> {
        let pts = self.rules(c)["fixed_spots"]
            .get(tip)
            .cloned()
            .or_else(|| c.links.layout.lairs(self.map_id).get(tip).cloned())?;
        let v: Vec<(f64, f64)> = pts
            .as_array()?
            .iter()
            .map(|p| (p[0].as_f64().unwrap_or(0.0), p[1].as_f64().unwrap_or(0.0)))
            .collect();
        (!v.is_empty()).then_some(v)
    }

    fn fixed_spot(&mut self, c: &Ctx, tip: &str) -> Option<(i64, i64)> {
        let pts = self.lairs(c, tip)?;
        let free: Vec<(f64, f64)> = pts
            .iter()
            .filter(|p| {
                !self
                    .npcs
                    .values()
                    .any(|n| n.tip == tip && (n.mover.x1 - p.0).hypot(n.mover.y1 - p.1) < 800.0)
            })
            .copied()
            .collect();
        let p = *self.rng.choice(if free.is_empty() { &pts } else { &free });
        Some((p.0 as i64, p.1 as i64))
    }

    fn zone_spot(&mut self, c: &Ctx, tip: &str) -> Option<(i64, i64)> {
        let z = self.rules(c)["npc"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|g| g["tip"] == tip && g.get("region").is_some_and(|r| !r.is_null()))?["region"]
            .clone();
        let z: Vec<f64> = z
            .as_array()?
            .iter()
            .map(|v| v.as_f64().unwrap_or(0.0))
            .collect();
        for _ in 0..30 {
            let a = self.rng.uniform(0.0, 2.0 * PI);
            let r = z[2] * self.rng.random().sqrt();
            let x = (z[0] + a.cos() * r)
                .max(300.0)
                .min(self.map_w as f64 - 300.0);
            let y = (z[1] + a.sin() * r)
                .max(300.0)
                .min(self.map_h as f64 - 300.0);
            if !c.links.layout.blocked(self.map_id, x, y, 200.0) {
                return Some((x as i64, y as i64));
            }
        }
        Some((z[0] as i64, z[1] as i64))
    }

    fn spawn_npc(&mut self, c: &Ctx, tip: &str, now: f64) {
        let Some(d) = c.data.cfg["types"].get(tip).cloned() else {
            return;
        };
        if let Some(pts) = self.lairs(c, tip)
            && self
                .npcs
                .values()
                .filter(|o| o.alive && o.tip == tip)
                .count()
                >= pts.len()
        {
            return;
        }
        self.next_uid += 1;
        let (x, y) = match self.fixed_spot(c, tip) {
            Some(p) => p,
            None => match self.zone_spot(c, tip) {
                Some(p) => p,
                None => self.random_spot(c),
            },
        };
        let model = d.get("model").cloned().unwrap_or_else(|| json!(tip));
        let next_think = now + self.rng.uniform(1.0, 6.0);
        let n = Npc {
            id: self.next_uid,
            tip: tip.to_string(),
            d,
            model,
            mover: Mover::new(x as f64, y as f64),
            next_think,
            alive: true,
            ghost: None,
        };
        self.npcs.insert(n.id, n);
    }

    fn ghostly(c: &Ctx, n: &mut Npc, now: f64) -> bool {
        *n.ghost.get_or_insert_with(|| {
            n.d.get("gate") == Some(&json!("vortex"))
                || (n.d["name"].as_str().is_some_and(|s| s.contains("Boss"))
                    && c.cal.active(now).contains("halloween"))
        })
    }

    fn ship_create(c: &Ctx, n: &mut Npc, now: f64) -> Msg {
        let (x, y) = n.mover.pos(now);
        let mut modifier = Vec::new();
        if Self::ghostly(c, n, now) {
            modifier.push(module(
                "VisualModifierCommand",
                Map::from_iter([
                    ("userId".into(), json!(n.id)),
                    ("modifier".into(), c.data.ghost_visual.clone()),
                    ("attribute".into(), json!(0)),
                    ("additionalAttribute".into(), json!("")),
                    ("count".into(), json!(1)),
                    ("activated".into(), json!(true)),
                ]),
            ));
        }
        (
            "ShipCreateCommand".into(),
            json!({
                "userId": n.id, "typeId": n.model, "expansionStage": n.d.get("spread").cloned().unwrap_or(json!(1)),
                "clanTag": "", "userName": n.d["name"], "x": x as i64, "y": y as i64, "factionId": 0, "clanId": 0,
                "dailyRank": 0, "warnBox": false, "clanDiplomacy": {"type": 0, "_": "ClanRelationModule"},
                "galaxyGatesDone": 0, "useSystemFont": false, "npc": true, "cloaked": false, "motherShipId": 0,
                "positionIndex": 0, "designSet": "", "modifier": modifier,
                "minimapStrategyModule": {"minimapStrategy": 0, "_": "MinimapStrategyModule"}, "specialNpcType": "",
            }),
        )
    }

    fn in_view(&self, c: &Ctx, n: &Npc, now: f64, hx: f64, hy: f64) -> bool {
        let (nx, ny) = n.mover.pos(now);
        let lim = c.data.combat(if self.visible.contains(&n.id) {
            "view_drop"
        } else {
            "sight_range"
        });
        (nx - hx).hypot(ny - hy) <= lim
    }

    fn item_in_view(&self, h: &str, hx: f64, hy: f64) -> bool {
        let Some(it) = self.items.get(h) else {
            return false;
        };
        let lim = if self.seen_items.contains(h) {
            self.sight.1
        } else {
            self.sight.0
        };
        (it.x as f64 - hx).hypot(it.y as f64 - hy) <= lim
    }

    fn show(c: &Ctx, n: &mut Npc, now: f64) -> Vec<Msg> {
        let mut msgs = vec![Self::ship_create(c, n, now)];
        let m = &n.mover;
        if m.t1 > now {
            msgs.push((
                "MoveCommand".into(),
                json!({"userId": n.id, "x": m.x1 as i64, "y": m.y1 as i64, "timeToTarget": m.ms_left(now)}),
            ));
        }
        msgs
    }

    fn item_msg(&self, h: &str, _settings: &Settings) -> Msg {
        let it = &self.items[h];
        if it.ore {
            (
                "AddOreCommand".into(),
                json!({"hash": h, "oreType": {"typeValue": it.tip, "_": "OreTypeModule"}, "x": it.x, "y": it.y}),
            )
        } else {
            // someone else's cargo box would be FROM_SHIP_BLOCKED (cargo_ownership): no other players yet
            let tip = it.tip.clone();
            (
                "AddBoxCommand".into(),
                json!({"hash": h, "boxType": tip, "x": it.x, "y": it.y}),
            )
        }
    }

    fn rand_hash(&mut self) -> String {
        const CH: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
        let n = self.rng.randint(4, 6);
        (0..n).map(|_| *self.rng.choice(CH) as char).collect()
    }

    fn new_hash(&mut self) -> String {
        loop {
            let h = self.rand_hash();
            if !self.items.contains_key(&h) {
                return h;
            }
        }
    }

    fn drop_box(&mut self, c: &Ctx, kind: &str, x: i64, y: i64, life: Option<f64>, now: f64) {
        let h = self.new_hash();
        self.items.insert(
            h.clone(),
            Item {
                tip: json!(kind),
                x,
                y,
                end_time: life.map(|l| now + l),
                ore: false,
            },
        );
        let m = self.item_msg(&h, c.settings);
        self.send(vec![m]);
    }

    fn spawn_bonus_box(&mut self, c: &Ctx, now: f64) {
        let (x, y) = self.random_spot(c);
        self.drop_box(c, "BONUS_BOX", x, y, None, now);
    }

    fn spawn_ores(&mut self, c: &Ctx) {
        let ores = &c.data.ores;
        let counts = ores["maps"]
            .get(c.links.name(self.map_id))
            .or_else(|| ores["maps"].get(c.links.map_class(self.map_id)))
            .and_then(Value::as_object)
            .cloned();
        let Some(counts) = counts.filter(|m| !m.is_empty()) else {
            return;
        };
        let total: i64 = counts.values().filter_map(Value::as_i64).sum();
        let per = ores["per_cluster"].as_i64().unwrap_or(10).max(1);
        let n = 3.max(total / per);
        self.ore_fields = (0..n).map(|_| self.random_spot(c)).collect();
        for (t, k) in &counts {
            let k = c
                .settings
                .count("ore_abundance", k.as_i64().unwrap_or(0), None);
            for _ in 0..k {
                self.add_ore(c, t.parse().unwrap_or(0));
            }
        }
        let fields: Vec<String> = self
            .ore_fields
            .iter()
            .map(|(x, y)| format!("({x},{y})"))
            .collect();
        self.log
            .push(format!("   ore clusters: {}", fields.join(" ")));
    }

    fn ore_spot(&mut self, c: &Ctx) -> (i64, i64) {
        if let Some(field) = c.links.layout.of(self.map_id).get("palladium") {
            let a: Vec<f64> = field["area"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|v| v.as_f64().unwrap_or(0.0))
                .collect();
            if a.len() == 4 {
                let spot = c.links.layout.free_spot(
                    self.map_id,
                    (a[0], a[1], a[2], a[3]),
                    crate::map_layout::SHIP_PAD,
                    40,
                    &mut self.rng,
                );
                return spot.unwrap_or_else(|| self.random_spot(c));
            }
        }
        let radius = c.data.ores["cluster_radius"].as_f64().unwrap_or(700.0);
        for _ in 0..30 {
            let (cx, cy) = *self.rng.choice(&self.ore_fields.clone());
            let a = self.rng.uniform(0.0, 2.0 * PI);
            let r = self.rng.uniform(0.0, radius);
            let x = cx as f64 + a.cos() * r;
            let y = cy as f64 + a.sin() * r;
            if 300.0 < x
                && x < self.map_w as f64 - 300.0
                && 300.0 < y
                && y < self.map_h as f64 - 300.0
                && !self.near_zone(x, y, 200.0)
            {
                return (x as i64, y as i64);
            }
        }
        self.random_spot(c)
    }

    fn add_ore(&mut self, c: &Ctx, t: i64) {
        let h = self.new_hash();
        let (x, y) = self.ore_spot(c);
        self.items.insert(
            h,
            Item {
                tip: json!(t),
                x,
                y,
                end_time: None,
                ore: true,
            },
        );
    }

    /// Places the aliens, bonus boxes and ores; the visible ones go to the client (`start`).
    pub fn start(&mut self, c: &Ctx, now: f64) {
        let rules = self.rules(c).clone();
        if rules.is_null() {
            self.log
                .push(format!("   world: no NPC list for map {}", self.map_id));
        }
        for g in rules["npc"].as_array().into_iter().flatten() {
            let tip = g["tip"].as_str().unwrap_or("").to_string();
            let k = c
                .settings
                .count("npc_count", g["count"].as_i64().unwrap_or(0), Some("map"));
            for _ in 0..k {
                self.spawn_npc(c, &tip, now);
            }
        }
        let vis = self.visible_msgs(c, now);
        self.send(vis);
        let boxes = c.settings.count(
            "box_abundance",
            rules["bonus_boxes"].as_i64().unwrap_or(0),
            Some("bonus"),
        );
        for _ in 0..boxes {
            self.spawn_bonus_box(c, now);
        }
        self.spawn_ores(c);
        let ores: Vec<String> = self
            .items
            .iter()
            .filter(|(_, it)| it.ore)
            .map(|(h, _)| h.clone())
            .collect();
        let msgs: Vec<Msg> = ores.iter().map(|h| self.item_msg(h, c.settings)).collect();
        self.send(msgs);
        let boxes = self.items.values().filter(|i| !i.ore).count();
        self.log.push(format!(
            "   world: map {}, {} NPCs, {} bonus boxes, {} ores",
            self.map_id,
            self.npcs.len(),
            boxes,
            self.items.len() - boxes
        ));
    }

    fn visible_msgs(&mut self, c: &Ctx, now: f64) -> Vec<Msg> {
        self.visible.clear();
        let (hx, hy) = self.hero.pos(now);
        let ids: Vec<i64> = self
            .npcs
            .values()
            .filter(|n| self.in_view(c, n, now, hx, hy))
            .map(|n| n.id)
            .collect();
        let mut msgs = Vec::new();
        for id in ids {
            self.visible.insert(id);
            let n = self.npcs.get_mut(&id).expect("npc");
            msgs.extend(Self::show(c, n, now));
        }
        msgs
    }

    /// Aliens, boxes and ores entering or leaving view (`view_tick`).
    fn view_tick(&mut self, c: &Ctx, now: f64) {
        let (hx, hy) = self.hero.pos(now);
        let mut msgs = Vec::new();
        let ids: Vec<i64> = self.npcs.keys().copied().collect();
        for id in ids {
            let seen = self.visible.contains(&id);
            let inside = self.in_view(c, &self.npcs[&id], now, hx, hy);
            if inside && !seen {
                self.visible.insert(id);
                let n = self.npcs.get_mut(&id).expect("npc");
                msgs.extend(Self::show(c, n, now));
            } else if !inside && seen {
                self.visible.remove(&id);
                msgs.push(("ShipRemoveCommand".into(), json!({"userId": id})));
            }
        }
        let hashes: Vec<String> = self.items.keys().cloned().collect();
        for h in hashes {
            let seen = self.seen_items.contains(&h);
            if self.item_in_view(&h, hx, hy) {
                if !seen {
                    let m = self.item_msg(&h, c.settings);
                    msgs.push(m);
                }
            } else if seen {
                msgs.push((
                    "RemoveCollectableCommand".into(),
                    json!({"collected": false, "hash": h}),
                ));
            }
        }
        self.send(msgs);
    }

    fn spot_away(&mut self, c: &Ctx, hx: f64, hy: f64) -> (i64, i64) {
        let mut best = None;
        for _ in 0..20 {
            let (x, y) = self.random_spot(c);
            if (x as f64 - hx).hypot(y as f64 - hy) > 3000.0 {
                return (x, y);
            }
            best = best.or(Some((x, y)));
        }
        best.expect("spot")
    }

    fn path_near_zone(&self, x: f64, y: f64, tx: f64, ty: f64) -> bool {
        let (dx, dy) = (tx - x, ty - y);
        let l2 = dx * dx + dy * dy;
        self.zones.iter().any(|&(sx, sy, r)| {
            let f = if l2 > 0.0 {
                (((sx - x) * dx + (sy - y) * dy) / l2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let dist = (x + f * dx - sx).hypot(y + f * dy - sy);
            let start = (x - sx).hypot(y - sy);
            dist < r && !(start < r && f == 0.0 && (tx - sx).hypot(ty - sy) > start)
        })
    }

    /// An alien's wandering (`think`, the wander branch; chasing and firing come with combat).
    fn think(&mut self, c: &Ctx, id: i64, now: f64) {
        let n = &self.npcs[&id];
        if now < n.next_think || n.d.get("friendly").is_some_and(lo_core::settings::truthy) {
            return;
        }
        let speed = n.speed(c.settings);
        if speed <= 0.0 {
            self.npcs.get_mut(&id).expect("npc").next_think = now + 3600.0;
            return;
        }
        let (hx, hy) = self.hero.pos(now);
        let (nx, ny) = n.mover.pos(now);
        let r = 30f64.max(speed * 0.9 * self.rng.uniform(0.1, 2.9));
        let a = self.rng.uniform(0.0, 2.0 * PI);
        let mut tx = (nx + a.cos() * r).max(300.0).min(self.map_w as f64 - 300.0);
        let mut ty = (ny + a.sin() * r).max(300.0).min(self.map_h as f64 - 300.0);
        let leaving = self.near_zone(nx, ny, 500.0);
        if leaving || self.near_zone(tx, ty, 500.0) {
            let p = self.spot_away(c, hx, hy);
            (tx, ty) = (p.0 as f64, p.1 as f64);
        }
        let nav = c.links.layout.navigates(self.map_id);
        if nav {
            (tx, ty) =
                c.links
                    .layout
                    .waypoint(self.map_id, self.map_w, self.map_h, (nx, ny), (tx, ty));
        }
        let mut tries = 0;
        while self.path_near_zone(nx, ny, tx, ty) {
            tries += 1;
            if tries > 5 {
                let n = self.npcs.get_mut(&id).expect("npc");
                n.mover = Mover::new(nx, ny);
                n.next_think = now + 2.0;
                self.send(vec![(
                    "MoveCommand".into(),
                    json!({"userId": id, "x": nx as i64, "y": ny as i64, "timeToTarget": 0}),
                )]);
                return;
            }
            let p = self.spot_away(c, hx, hy);
            (tx, ty) = (p.0 as f64, p.1 as f64);
            if nav {
                (tx, ty) = c.links.layout.waypoint(
                    self.map_id,
                    self.map_w,
                    self.map_h,
                    (nx, ny),
                    (tx, ty),
                );
            }
        }
        let wait = self.rng.uniform(0.0, 1.0);
        let n = self.npcs.get_mut(&id).expect("npc");
        let ms = n
            .mover
            .go(tx, ty, speed * if leaving { 1.0 } else { 0.9 }, now, None);
        n.next_think = now + ms as f64 / 1000.0 + wait;
        if self.visible.contains(&id) {
            self.send(vec![(
                "MoveCommand".into(),
                json!({"userId": id, "x": tx as i64, "y": ty as i64, "timeToTarget": ms}),
            )]);
        }
    }

    /// One step of the loop (0.1 s) (`step`).
    pub fn step(&mut self, c: &Ctx, now: f64) {
        self.view_tick(c, now);
        let ids: Vec<i64> = self.npcs.keys().copied().collect();
        for id in ids {
            self.think(c, id, now);
        }
        let expired: Vec<String> = self
            .items
            .iter()
            .filter(|(_, b)| b.end_time.is_some_and(|e| now > e))
            .map(|(h, _)| h.clone())
            .collect();
        for h in expired {
            self.items.shift_remove(&h);
            self.send(vec![(
                "RemoveCollectableCommand".into(),
                json!({"collected": false, "hash": h}),
            )]);
        }
    }

    /// The player's flight (`on_move`): the client moves its own ship, the server follows it.
    pub fn on_move(&mut self, f: &Value, now: f64) {
        let g = |k: &str| f[k].as_f64().unwrap_or(0.0);
        let (x, y) = (g("targetX"), g("targetY"));
        if self.hero_speed <= 0.0 {
            let (px, py) = self.hero.pos(now);
            self.hero = Mover::new(px, py);
        } else {
            self.hero.go(
                x,
                y,
                self.hero_speed,
                now,
                Some((g("positionX"), g("positionY"))),
            );
        }
    }
}
