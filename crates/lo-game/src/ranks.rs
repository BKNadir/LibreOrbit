//! Rank, counters and rank points (OpenOrbit's `ranks.py`, data `ranks.json` with the World Editor's patches).
//!
//! Ported so far: the counters, the rank points formula, the rank by thresholds, the daily rank written at login and on a
//! map change (`daily_rank`), the rank recomputed when the points change (`live_rank`) and the monthly ranking reward.
//! The company ranking with bot players, the Hall of Fame, the logbook and the account history come later; without bots
//! OpenOrbit also uses the thresholds alone.

use crate::items::ItemData;
use crate::login_markers::{Player, admin_open};
use lo_core::GameData;
use lo_core::pytime;
use lo_core::settings::Settings;
use lo_save::Profile;
use lo_save::hangar::num;
use serde_json::{Map, Value, json};

/// The monthly ranking reward: Champion Tokens.
pub const CHAMPION: &str = "resource_collectable_champion-token";
/// Monthly tiers: (at most how many ranks below the top, tokens, title, achievement event).
pub const MONTHLY: [(i64, i64, &str, &str); 5] = [
    (0, 100, "title_monthlyranking-general", "monthly_general"),
    (3, 80, "title_monthlyranking-pilot", "monthly_pilot"),
    (6, 60, "title_monthlyranking-pilot", "monthly_pilot"),
    (10, 40, "title_monthlyranking-pilot", "monthly_pilot"),
    (15, 20, "title_monthlyranking-pilot", "monthly_pilot"),
];

/// The rank data.
pub struct Ranks {
    pub r: Value,
}

/// `time.strftime('%Y-%m-%d', time.localtime(t))`.
pub fn today(t: f64) -> String {
    pytime::localtime(t.floor() as i64)
        .map_or_else(String::new, |x| x.at.format("%Y-%m-%d").to_string())
}

fn obj(v: &mut Value) -> &mut Map<String, Value> {
    if !v.is_object() {
        *v = json!({});
    }
    v.as_object_mut().unwrap()
}

/// `int(v)` of a number kept in the profile.
fn int(v: Option<&Value>) -> i64 {
    v.and_then(num).map_or(0, |f| f.trunc() as i64)
}

/// The profile's counters, completed with their defaults (`counters`).
pub fn counters(p: &mut Profile) -> &mut Map<String, Value> {
    let done =
        p.d.get("quests")
            .filter(|q| lo_core::settings::truthy(q))
            .and_then(|q| q.get("done"))
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
    let c = obj(p
        .d
        .as_object_mut()
        .expect("profile")
        .entry("counters")
        .or_insert_with(|| json!({})));
    for (k, v) in [
        ("npc", json!({})),
        ("player", json!({})),
        ("death_by_enemy", json!(0)),
        ("death_by_radiation", json!(0)),
        ("death_other", json!(0)),
        ("quest", json!(done)),
        ("friendly", json!(0)),
        ("phoenix", json!(0)),
    ] {
        c.entry(k).or_insert(v);
    }
    c
}

/// A game event counted (`event`): `kill` (an alien or a player ship), `death`, `quest`.
pub fn event(p: &mut Profile, kind: &str, kw: &Map<String, Value>) {
    if !["kill", "death", "quest"].contains(&kind) {
        return;
    }
    let avatar = kw.get("avatar").and_then(Value::as_str).unwrap_or("NPC");
    let c = counters(p);
    let bump = |v: &mut Value| *v = json!(v.as_i64().unwrap_or(0) + 1);
    match kind {
        "kill" if avatar == "NPC" && kw.get("npc").is_some_and(|n| !n.is_null()) => {
            let k = lo_save::hangar::py_str(&kw["npc"]);
            bump(obj(c.get_mut("npc").unwrap()).entry(k).or_insert(json!(0)));
        }
        "kill" if avatar == "PLAYER" => {
            let k = kw
                .get("ship")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            bump(
                obj(c.get_mut("player").unwrap())
                    .entry(k)
                    .or_insert(json!(0)),
            );
        }
        "death" => {
            let k = match kw.get("reason").and_then(Value::as_str) {
                Some("RADIATION") => "death_by_radiation",
                Some("NPC") | Some("PLAYER") => "death_by_enemy",
                _ => "death_other",
            };
            bump(c.get_mut(k).unwrap());
        }
        "quest" => bump(c.get_mut("quest").unwrap()),
        _ => return,
    }
    p.mark_dirty();
}

impl Ranks {
    pub fn load(data: &GameData) -> Result<Ranks, String> {
        Ok(Ranks {
            r: data
                .load("ranks.json")
                .map_err(|e| format!("ranks.json: {e}"))?,
        })
    }

    fn f(&self, k: &str) -> f64 {
        num(&self.r["formula"][k]).unwrap_or(0.0)
    }

    /// Whole days since the profile was created (`days_registered`).
    pub fn days_registered(p: &Profile, now: f64) -> i64 {
        let Some(created) = p.d.get("created").and_then(Value::as_str) else {
            return 0;
        };
        let Ok(t) = chrono::NaiveDateTime::parse_from_str(created, "%Y-%m-%d %H:%M:%S") else {
            return 0;
        };
        0i64.max(((now - pytime::timestamp(t)) / 86400.0).floor() as i64)
    }

    fn ship_type(&self, p: &Profile) -> f64 {
        let loot = p.ship(None)["lootId"].as_str().unwrap_or_default();
        num(&self.r["ship_type"][loot]).unwrap_or(0.0)
    }

    fn points_of(&self, table: &str, counts: &Value) -> f64 {
        counts
            .as_object()
            .into_iter()
            .flatten()
            .map(|(k, n)| num(&self.r[table][k]["points"]).unwrap_or(0.0) * num(n).unwrap_or(0.0))
            .sum()
    }

    /// The parts of the rank points formula and the rank points (`breakdown`), in OpenOrbit's order.
    pub fn breakdown(&self, p: &mut Profile, now: f64) -> Map<String, Value> {
        let c = counters(p).clone();
        let pl = p.d["pilot"].clone();
        let player = self.points_of("ship_points", &c["player"]);
        let npc = self.points_of("npc_points", &c["npc"]);
        let num_json = |f: f64| {
            if f.fract() == 0.0 {
                json!(f as i64)
            } else {
                json!(f)
            }
        };
        let parts: Vec<(&str, Value)> = vec![
            ("ExperiencePoints", json!(int(pl.get("ep")))),
            ("honorPoints", json!(int(pl.get("honourPoints")))),
            ("playerShotPoints", num_json(player)),
            (
                "yourLevel",
                json!(pl.get("level").map_or(1, |v| int(Some(v)))),
            ),
            (
                "daysSinceRegistration",
                json!(Self::days_registered(p, now)),
            ),
            ("yourShipType", num_json(self.ship_type(p))),
            ("npcShotPoints", num_json(npc)),
            ("missionsCompleted", c["quest"].clone()),
            ("friendlyPlayersDestroyed", c["friendly"].clone()),
            ("DestroyedByEnemies", c["death_by_enemy"].clone()),
            ("DestroyedByRadiationZone", c["death_by_radiation"].clone()),
            ("phoenixDestroyed", c["phoenix"].clone()),
        ];
        let g = |k: &str| num(&parts.iter().find(|(x, _)| *x == k).unwrap().1).unwrap_or(0.0);
        let pts = g("ExperiencePoints") * self.f("ep")
            + g("honorPoints") * self.f("honor")
            + g("playerShotPoints") * self.f("player")
            + g("yourLevel") * self.f("level")
            + g("daysSinceRegistration") * self.f("day")
            + g("yourShipType") * self.f("ship")
            + g("npcShotPoints") * self.f("npc")
            + g("missionsCompleted") * self.f("quest")
            + g("friendlyPlayersDestroyed") * self.f("friendly")
            + g("DestroyedByEnemies") * self.f("death_by_enemy")
            + g("DestroyedByRadiationZone") * self.f("death_by_radiation")
            + g("phoenixDestroyed") * self.f("phoenix");
        let mut out: Map<String, Value> =
            parts.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        out.insert("rankPoints".into(), json!(0i64.max(pts.trunc() as i64)));
        out
    }

    /// The rank (1-21) from rank points by the thresholds (`rank_for`).
    pub fn rank_for(&self, points: i64) -> i64 {
        self.r["thresholds"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
            .filter(|(_, t)| num(t).is_some_and(|t| points as f64 >= t))
            .map(|(i, _)| i as i64 + 1)
            .max()
            .unwrap_or(1)
    }

    /// The player's rank: without bot players, by the thresholds (`rank_of`).
    pub fn rank_of(&self, points: i64) -> i64 {
        self.rank_for(points)
    }

    /// The rank icon: the rank, or the outlaw icon at -500 honor or below (`rank_icon`).
    pub fn rank_icon(&self, p: &Profile, rank: i64) -> i64 {
        let honor = p.d["pilot"]
            .get("honourPoints")
            .and_then(num)
            .unwrap_or(0.0);
        if honor <= num(&self.r["outlaw"]["honor"]).unwrap_or(-500.0) {
            self.r["outlaw"]["icon"].as_i64().unwrap_or(5000)
        } else {
            rank
        }
    }

    /// Once a day (at login and on a map change): the rank written to `pilot.dailyRank`, the daily record kept and the
    /// monthly reward given -> the new rank icon, or `None` (`daily_rank`). `achievement`: the achievement event of a
    /// monthly reward, for the session.
    #[allow(clippy::too_many_arguments)] // mirrors `ranks.daily_rank`
    pub fn daily_rank(
        &self,
        items: &ItemData,
        p: &mut Profile,
        settings: &Settings,
        who: Player,
        now: f64,
        force: bool,
        achievement: &mut dyn FnMut(&str),
    ) -> Option<i64> {
        if admin_open(settings, who) {
            return None;
        }
        let day = today(now);
        let st =
            p.d.as_object_mut()
                .expect("profile")
                .entry("rank")
                .or_insert_with(|| json!({}));
        if st.get("day").and_then(Value::as_str) == Some(day.as_str()) && !force {
            return None;
        }
        let b = self.breakdown(p, now);
        let points = b["rankPoints"].as_i64().unwrap_or(0);
        let rank = self.rank_of(points);
        let icon = self.rank_icon(p, rank);
        let st = obj(p.d.get_mut("rank").unwrap()).clone();
        let prev = st
            .get("ay")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        let prev_rank = st
            .get("rank")
            .filter(|v| lo_core::settings::truthy(v))
            .map_or(1, |v| int(Some(v)));
        let month = &day[..7.min(day.len())];
        let monthly = prev.as_ref().is_some_and(|pr| {
            pr != month && active_month(st.get("history").unwrap_or(&json!({})), pr)
        });
        let ep = int(p.d["pilot"].get("ep"));
        let honor = int(p.d["pilot"].get("honourPoints"));
        let st = obj(p.d.get_mut("rank").unwrap());
        st.insert("day".into(), json!(day));
        st.insert("points".into(), json!(points));
        st.insert("rank".into(), json!(rank));
        st.insert("ay".into(), json!(month));
        let hist = obj(st.entry("history").or_insert_with(|| json!({})));
        hist.insert(
            day.clone(),
            json!({"points": points, "ep": ep, "honor": honor}),
        );
        let mut days: Vec<String> = hist.keys().cloned().collect();
        days.sort();
        if days.len() > 40 {
            for old in &days[..days.len() - 40] {
                hist.shift_remove(old);
            }
        }
        p.d["pilot"]["dailyRank"] = json!(icon);
        p.mark_dirty();
        let n = self.r["thresholds"].as_array().map_or(0, Vec::len) as i64;
        let tier = monthly
            .then(|| MONTHLY.iter().find(|t| n - prev_rank <= t.0))
            .flatten();
        if let Some(&(_, tokens, title, kind)) = tier {
            achievement(kind);
            add_resource(items, p, CHAMPION, tokens);
            grant_title(p, title);
        }
        Some(icon)
    }

    /// When the rank points change: the new rank at once -> (old, new) or `None` (`live_rank`).
    pub fn live_rank(
        &self,
        p: &mut Profile,
        settings: &Settings,
        who: Player,
        now: f64,
    ) -> Option<(Value, i64)> {
        if admin_open(settings, who) {
            return None;
        }
        let points = self.breakdown(p, now)["rankPoints"].as_i64().unwrap_or(0);
        let rank = self.rank_of(points);
        let icon = self.rank_icon(p, rank);
        let old = p.d["pilot"].get("dailyRank").cloned().unwrap_or(json!(1));
        if num(&old) == Some(icon as f64) {
            return None;
        }
        p.d["pilot"]["dailyRank"] = json!(icon);
        obj(p
            .d
            .as_object_mut()
            .unwrap()
            .entry("rank")
            .or_insert_with(|| json!({})))
        .insert("rank".into(), json!(rank));
        p.mark_dirty();
        Some((old, icon))
    }
}

/// Whether the rank points rose within the month (`active_month`).
pub fn active_month(hist: &Value, month: &str) -> bool {
    let Some(h) = hist.as_object() else {
        return false;
    };
    let mut days: Vec<&String> = h.keys().collect();
    days.sort();
    let pts = |d: &String| num(&h[d.as_str()]["points"]).unwrap_or(0.0);
    let inside: Vec<f64> = days
        .iter()
        .filter(|d| d.starts_with(month))
        .map(|d| pts(d))
        .collect();
    let before: Vec<f64> = days
        .iter()
        .filter(|d| d.as_str() < month)
        .map(|d| pts(d))
        .collect();
    !inside.is_empty()
        && inside.iter().cloned().fold(f64::MIN, f64::max) > before.last().copied().unwrap_or(0.0)
}

/// The achievement state, completed (`achievement.state`).
pub fn achievement_state(p: &mut Profile) -> &mut Map<String, Value> {
    obj(p
        .d
        .as_object_mut()
        .expect("profile")
        .entry("achievements")
        .or_insert_with(
            || json!({"score": {}, "set": {}, "level": {}, "date": {}, "titles": [], "title": -1}),
        ))
}

/// A title earned (`grant_title`).
pub fn grant_title(p: &mut Profile, key: &str) {
    let st = achievement_state(p);
    let titles = st.entry("titles").or_insert_with(|| json!([]));
    if let Some(t) = titles.as_array_mut()
        && !t.iter().any(|x| x == key)
    {
        t.push(json!(key));
        p.mark_dirty();
    }
}

/// Adds to storage; below zero is not possible and what goes over the client's stack limit is lost -> the new count
/// (`add_resource`).
pub fn add_resource(items: &ItemData, p: &mut Profile, loot: &str, n: i64) -> i64 {
    let cap = items.stack_cap(loot);
    let d = obj(p
        .d
        .as_object_mut()
        .expect("profile")
        .entry("other_items")
        .or_insert_with(|| json!({})));
    let cur = d.get(loot).and_then(num).map_or(0, |f| f as i64);
    let mut v = 0i64.max(cur + n);
    if n > 0
        && let Some(c) = cap
        && v > c
    {
        v = c.max(v - n);
    }
    if v == 0 {
        d.shift_remove(loot);
    } else {
        d.insert(loot.to_string(), json!(v));
    }
    p.mark_dirty();
    v
}
