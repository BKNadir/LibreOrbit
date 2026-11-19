//! Quests (OpenOrbit's `quest.py`, profile `quests`): the list, the definitions with their condition states and the
//! minimap marks sent at login and on a map change.
//!
//! Ported so far: what the login needs (`login_msgs`, `list_cmd`, `definition`, `status`, `offered`, `maps_for`);
//! accepting, progress, rewards and the timers come with the world.

use crate::event_calendar::EventCalendar;
use crate::map_links::MapLinks;
use chrono::{Datelike, NaiveDate};
use lo_core::pyrandom::Random;
use lo_core::settings::{Settings, truthy};
use lo_core::{GameData, pytime};
use lo_save::Profile;
use lo_save::hangar::{num, py_str};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// QuestType of the event quests (their own slot).
pub const EVENT_QUEST: i64 = 5;
pub const HASTE: i64 = 2;
pub const REAL_TIME_HASTE: i64 = 63;
pub const SELL_ORE: i64 = 52;
pub const VISIT_QUEST_GIVER: i64 = 62;
/// Start time in the active quest's values.
pub const START: &str = "_bas";
pub const NOT_ACCEPTABLE: i64 = 0;
pub const NOT_STARTED: i64 = 1;
pub const RUNNING: i64 = 2;
pub const COMPLETED: i64 = 3;
pub const REQ_LEVEL: i64 = 0;
pub const REQ_PRE_QUEST: i64 = 3;
pub const REQ_EXCLUSIVE_POOL: i64 = 6;
pub const MARK_GIVER: i64 = 1;
pub const MARK_TRADE: i64 = 2;
pub const CAMPAIGN_SEC: f64 = 30.0 * 86400.0;

type Msg = (String, Value);

/// `mod(name, **f)`: the fields, then `_` last.
fn module(name: &str, mut f: Map<String, Value>) -> Value {
    f.insert("_".into(), json!(name));
    Value::Object(f)
}

macro_rules! fields {
    ($($k:expr => $v:expr),* $(,)?) => {{
        #[allow(unused_mut)]
        let mut m = Map::new();
        $(m.insert($k.to_string(), json!($v));)*
        m
    }};
}

/// Python's `a >= b` for the numbers of a condition (missing values are 0).
fn ge(a: &Value, b: &Value) -> bool {
    match (num(a), num(b)) {
        (Some(x), Some(y)) => x >= y,
        _ => false,
    }
}

/// Python's `min(a, b)` (the first on a tie, the type kept).
fn py_min(a: &Value, b: &Value) -> Value {
    if num(b) < num(a) {
        b.clone()
    } else {
        a.clone()
    }
}

/// The conditions of a case tree (those with an id) (`conditions`).
pub fn conditions(case: &Value) -> Vec<&Value> {
    let mut out = Vec::new();
    for el in case["modifier"].as_array().into_iter().flatten() {
        let c = &el["condition"];
        if truthy(&c["id"]) {
            out.push(c);
        }
        let qc = &el["questCase"];
        if truthy(&qc["id"]) || qc["modifier"].as_array().is_some_and(|m| !m.is_empty()) {
            out.extend(conditions(qc));
        }
    }
    out
}

/// Calls `f` on each condition of the tree, in the order of [`conditions`].
fn each_condition_mut(case: &mut Value, f: &mut dyn FnMut(&mut Value)) {
    for el in case["modifier"].as_array_mut().into_iter().flatten() {
        if truthy(&el["condition"]["id"]) {
            f(&mut el["condition"]);
        }
        let qc = &el["questCase"];
        if truthy(&qc["id"]) || qc["modifier"].as_array().is_some_and(|m| !m.is_empty()) {
            each_condition_mut(&mut el["questCase"], f);
        }
    }
}

/// The condition value limited by the target (unlimited for a target-less condition) (`cap`).
fn cap(c: &Value, v: &Value) -> Value {
    if truthy(&c["targetValue"]) {
        py_min(v, &c["targetValue"])
    } else {
        v.clone()
    }
}

/// Condition states {condition: (active, done)} and whether the case is complete (`evaluate`).
pub fn evaluate(
    case: &Value,
    values: &Map<String, Value>,
    enabled: bool,
    out: &mut HashMap<String, (bool, bool)>,
) -> bool {
    let mut done = 0usize;
    let mut gate = enabled;
    let ordered = truthy(&case["ordered"]);
    let modifier = case["modifier"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    for el in modifier {
        let c = &el["condition"];
        let comp = if truthy(&c["id"]) {
            let v = values.get(&py_str(&c["id"])).cloned().unwrap_or(json!(0));
            let comp = truthy(&c["mandatory"]) && ge(&v, &c["targetValue"]);
            out.insert(py_str(&c["id"]), (gate && !comp, comp));
            comp
        } else {
            evaluate(&el["questCase"], values, gate, out)
        };
        done += usize::from(comp);
        if !comp && ordered {
            gate = false;
        }
    }
    let count = case["mandatoryCount"].as_u64().unwrap_or(0) as usize;
    let need = if ordered || count == 0 {
        modifier.len()
    } else {
        count
    };
    done >= need
}

fn flags(case: &Value, values: &Map<String, Value>) -> HashMap<String, (bool, bool)> {
    let mut out = HashMap::new();
    evaluate(case, values, true, &mut out);
    out
}

fn date(now: f64) -> NaiveDate {
    pytime::from_timestamp(now).map_or(NaiveDate::MIN, |t| t.date())
}

pub struct QuestData {
    pub quests: BTreeMap<i64, Value>,
    pub max_event_quests: i64,
    /// ('daily' | 'weekly', difficulty) -> quests, in id order.
    pools: HashMap<(String, String), Vec<i64>>,
    /// Level (3 | 10 | 15 | 17) -> daily quests, in level order of first appearance.
    level_pools: Vec<(i64, Vec<i64>)>,
    extras: Value,
    gates: Map<String, Value>,
    event_gates: Map<String, Value>,
}

impl QuestData {
    pub fn load(data: &GameData) -> Result<QuestData, String> {
        let get = |f: &str| data.load(f).map_err(|e| format!("{f}: {e}"));
        let base = get("quests.json")?;
        let extras = get("quest_extras.json")?;
        let gates = get("gates.json")?["gates"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        let event_gates = data
            .raw("event_gates.json")
            .map_err(|e| format!("event_gates.json: {e}"))?["gates"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        let mut quests = BTreeMap::new();
        for src in [&base["quests"], &extras["quests"]] {
            for (k, v) in src.as_object().into_iter().flatten() {
                let id = k.parse::<i64>().map_err(|_| format!("quest id {k}"))?;
                quests.insert(id, v.clone());
            }
        }
        let mut pools: HashMap<(String, String), Vec<i64>> = HashMap::new();
        let mut level_pools: Vec<(i64, Vec<i64>)> = Vec::new();
        for (&q, v) in &quests {
            let pool = &v["pool"];
            if !truthy(pool) {
                continue;
            }
            if let Some(level) = pool.get("level") {
                let level = level.as_i64().unwrap_or(0);
                match level_pools.iter_mut().find(|(l, _)| *l == level) {
                    Some((_, list)) => list.push(q),
                    None => level_pools.push((level, vec![q])),
                }
            } else {
                pools
                    .entry((py_str(&pool["type"]), py_str(&pool["difficulty"])))
                    .or_default()
                    .push(q);
            }
        }
        Ok(QuestData {
            quests,
            max_event_quests: extras["max_event_quests"].as_i64().unwrap_or(1),
            pools,
            level_pools,
            extras,
            gates,
            event_gates,
        })
    }

    /// The map ids of a match ("own X-3", "enemy X-5", "4-5", "Epsilon Gate"…) for the player's company (`maps_for`).
    pub fn maps_for(&self, links: &MapLinks, spec: &str, faction: i64) -> Vec<Value> {
        let ids: HashMap<&str, i64> = links.names.iter().map(|(k, v)| (v.as_str(), *k)).collect();
        if let Some(g) = spec
            .strip_suffix(" Gate")
            .map(|_| spec.split_whitespace().next().unwrap_or("").to_lowercase())
            && let Some(gate) = self.gates.get(&g)
        {
            return gate["maps"].as_array().cloned().unwrap_or_default();
        }
        if let Some(g) = self
            .event_gates
            .values()
            .find(|g| g.get("name").and_then(Value::as_str) == Some(spec))
        {
            return g["maps"].as_array().cloned().unwrap_or_default();
        }
        let enemies = || (1..=3).filter(move |f| *f != faction);
        let names: Vec<String> = if spec.ends_with(" X-BL") {
            let facs: Vec<i64> = match spec.split_whitespace().next() {
                Some("own") => vec![faction],
                Some("enemy") => enemies().collect(),
                Some("any") => vec![1, 2, 3],
                _ => panic!("quest match {spec}"),
            };
            facs.iter().map(|f| format!("BL-{f}")).collect()
        } else if spec.chars().count() == 3
            && "1235".contains(spec.chars().next().unwrap_or(' '))
            && &spec[1..] == "-X"
        {
            (1..=8).map(|n| format!("{}-{n}", &spec[..1])).collect()
        } else if spec == "own 4-2" {
            vec![format!("4-{faction}")]
        } else if let Some(x) = spec.strip_prefix("own X-") {
            vec![format!("{faction}-{x}")]
        } else if let Some(x) = spec.strip_prefix("enemy X-") {
            enemies().map(|f| format!("{f}-{x}")).collect()
        } else if let Some(x) = spec.strip_prefix("any X-") {
            (1..=3).map(|f| format!("{f}-{x}")).collect()
        } else if spec == "any enemy map" {
            enemies()
                .flat_map(|f| (1..=8).map(move |n| format!("{f}-{n}")))
                .collect()
        } else {
            vec![spec.to_string()]
        };
        names
            .iter()
            .filter_map(|n| ids.get(n.as_str()).map(|id| json!(id)))
            .collect()
    }

    fn pool(&self, qid: i64) -> &Value {
        &self.quests[&qid]["pool"]
    }

    /// The day of a daily quest or the week of a weekly one (`period_key`).
    pub fn period_key(&self, qid: i64, now: f64) -> String {
        let pool = self.pool(qid);
        let t = if pool.get("difficulty").is_some() {
            now - 6.5 * 3600.0
        } else {
            now
        };
        let d = date(t);
        if pool["type"] == "daily" {
            d.format("%Y-%m-%d").to_string()
        } else {
            let w = d.iso_week();
            format!("{}-W{:02}", w.year(), w.week())
        }
    }

    /// Today's dailies and this week's weeklies (`offered`).
    pub fn offered(&self, now: f64) -> BTreeSet<i64> {
        let d = date(now);
        let dz = date(now - 6.5 * 3600.0);
        let mut out = BTreeSet::new();
        for kind in ["daily", "weekly"] {
            let e = &self.extras[kind];
            let d0 = e["date"]
                .as_str()
                .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
                .unwrap_or(NaiveDate::MIN);
            let days = (dz - d0).num_days();
            let steps = if kind == "daily" {
                days
            } else {
                (days + i64::from(d0.weekday().num_days_from_monday())
                    - i64::from(dz.weekday().num_days_from_monday()))
                .div_euclid(7)
            };
            for first in e["selected"].as_array().into_iter().flatten() {
                let first = first.as_i64().unwrap_or(0);
                let diff = py_str(&self.pool(first)["difficulty"]);
                let pool = &self.pools[&(kind.to_string(), diff)];
                let i = pool.iter().position(|q| *q == first).unwrap_or(0) as i64;
                out.insert(pool[(i + steps).rem_euclid(pool.len() as i64) as usize]);
            }
        }
        let ds = d.format("%Y-%m-%d").to_string();
        for (level, pool) in &self.level_pools {
            out.insert(*Random::from_str_seed(&format!("{ds}:{level}")).choice(pool));
        }
        out
    }

    fn events(q: &Value) -> impl Iterator<Item = &str> {
        q["event"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
    }

    /// Is the special quest's event open (`event_open`).
    pub fn event_open(&self, cal: &EventCalendar, q: &Value, now: f64) -> bool {
        !truthy(&q["event"]) || Self::events(q).any(|k| cal.is_active(k, now))
    }

    /// The period of a repeating special quest (`repeat_key`).
    pub fn repeat_key(&self, cal: &EventCalendar, qid: i64, now: f64) -> String {
        let q = &self.quests[&qid];
        if q["repeat"] == "daily" {
            return date(now).format("%Y-%m-%d").to_string();
        }
        for k in Self::events(q) {
            if cal.event(k).is_some() && cal.is_active(k, now) {
                return match cal.period(k, now) {
                    Some((a, _)) => (a.trunc() as i64).to_string(),
                    None => "force".into(),
                };
            }
        }
        "force".into()
    }
}

/// What a quest session needs (the session's `Quests` state lives in [`Quests`]).
pub struct QuestCtx<'a> {
    pub data: &'a QuestData,
    pub links: &'a MapLinks,
    pub cal: &'a EventCalendar,
    pub settings: &'a Settings,
    pub now: f64,
}

/// The session's quest state (`Quests`): the quest giver and refinery on this map, the marks shown, the last offer.
#[derive(Default)]
pub struct Quests {
    pub map_id: Option<i64>,
    /// (asset id, x, y).
    pub giver: Option<(Value, Value, Value)>,
    /// (x, y).
    pub trade: Option<(Value, Value)>,
    pub marks: BTreeMap<i64, BTreeSet<i64>>,
    pub offer: Option<BTreeSet<i64>>,
}

fn st(p: &Profile) -> &Value {
    &p.d["quests"]
}

fn active(p: &Profile) -> Vec<String> {
    st(p)["active"]
        .as_object()
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default()
}

fn faction(p: &Profile) -> i64 {
    p.d["pilot"]
        .get("factionId")
        .and_then(Value::as_i64)
        .unwrap_or(2)
}

impl Quests {
    fn values(p: &Profile, qid: i64) -> Map<String, Value> {
        st(p)["active"]
            .get(qid.to_string())
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default()
    }

    fn is_active(p: &Profile, qid: i64) -> bool {
        st(p)["active"].get(qid.to_string()).is_some()
    }

    fn resolve(c: &QuestCtx, p: &Profile, matches: &Value) -> Value {
        let mut out = Vec::new();
        for x in matches.as_array().into_iter().flatten() {
            match x.as_str().and_then(|s| s.strip_prefix('@')) {
                Some(spec) => out.extend(
                    c.data
                        .maps_for(c.links, spec, faction(p))
                        .iter()
                        .map(|m| json!(py_str(m))),
                ),
                None => out.push(json!(py_str(x))),
            }
        }
        Value::Array(out)
    }

    /// Is the quest in the list (`visible`).
    pub fn visible(c: &QuestCtx, p: &Profile, qid: i64, offer: &BTreeSet<i64>) -> bool {
        let q = &c.data.quests[&qid];
        let act = Self::is_active(p, qid);
        if truthy(&q["off"]) && !act {
            return false;
        }
        if truthy(&q["campaign"])
            && !act
            && !truthy(&c.settings.get("campaign"))
            && c.now >= p.d.get("campaign").and_then(num).unwrap_or(-CAMPAIGN_SEC) + CAMPAIGN_SEC
        {
            return false;
        }
        if truthy(&q["pool"]) && !offer.contains(&qid) && !act {
            return false;
        }
        if !act
            && ((truthy(&q["event"]) && !c.data.event_open(c.cal, q, c.now))
                || (truthy(&q["hidden"])
                    && !q["previous"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .all(|x| Self::done(c, p, x))))
        {
            return false;
        }
        let fac = json!(faction(p));
        q.get("company_excluded") != Some(&fac) && *q.get("company").unwrap_or(&fac) == fac
    }

    /// Is the quest finished (`done`).
    pub fn done(c: &QuestCtx, p: &Profile, qid: &Value) -> bool {
        let n = num(qid);
        if st(p)["done"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|x| n.is_some() && num(x) == n)
        {
            return true;
        }
        let Some(id) = qid.as_i64() else {
            return false;
        };
        let Some(q) = c.data.quests.get(&id) else {
            return false;
        };
        let key = if truthy(&q["pool"]) {
            c.data.period_key(id, c.now)
        } else if truthy(&q["repeat"]) {
            c.data.repeat_key(c.cal, id, c.now)
        } else {
            return false;
        };
        st(p)
            .get("period")
            .and_then(|m| m.get(id.to_string()))
            .and_then(Value::as_str)
            == Some(key.as_str())
    }

    fn exclusive_running(c: &QuestCtx, p: &Profile, qid: i64) -> bool {
        let h = &c.data.quests[&qid]["pool"];
        if h.get("difficulty").is_none() {
            return false;
        }
        active(p).iter().any(|a| {
            let a: i64 = a.parse().unwrap_or(i64::MIN);
            a != qid
                && c.data.quests.get(&a).is_some_and(|q| {
                    q["pool"].get("difficulty").is_some() && q["pool"]["type"] == h["type"]
                })
        })
    }

    fn portrait(p: &Profile, q: &Value) -> Value {
        q.get("portrait_company")
            .and_then(|m| m.get(faction(p).to_string()))
            .or_else(|| q.get("portrait"))
            .cloned()
            .unwrap_or(json!(""))
    }

    /// (state, missing requirements) (`status`).
    pub fn status(c: &QuestCtx, p: &Profile, qid: i64) -> (i64, Vec<Value>) {
        let q = &c.data.quests[&qid];
        if Self::done(c, p, &json!(qid)) {
            return (COMPLETED, Vec::new());
        }
        if Self::is_active(p, qid) {
            return (RUNNING, Vec::new());
        }
        let mut miss = Vec::new();
        let level = p.d["pilot"].get("level").cloned().unwrap_or(json!(1));
        if !ge(&level, &q["level"]) {
            miss.push(module(
                "QuestRequirementModule",
                fields! {"requirementType" => REQ_LEVEL, "minValue" => q["level"], "maxValue" => 0,
                "matches" => Vec::<Value>::new(), "missingMatches" => Vec::<Value>::new()},
            ));
        }
        if Self::exclusive_running(c, p, qid) {
            miss.push(module(
                "QuestRequirementModule",
                fields! {"requirementType" => REQ_EXCLUSIVE_POOL, "minValue" => 0, "maxValue" => 0,
                "matches" => Vec::<Value>::new(), "missingMatches" => Vec::<Value>::new()},
            ));
        }
        let pre: Vec<&Value> = q["previous"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|x| {
                x.as_i64().is_some_and(|i| c.data.quests.contains_key(&i)) && !Self::done(c, p, x)
            })
            .collect();
        if !pre.is_empty() {
            let missing: Vec<Value> = pre
                .iter()
                .map(|x| {
                    module(
                        "QuestLocalizationModule",
                        fields! {"questId" => x, "textKeyFoundation" => ""},
                    )
                })
                .collect();
            miss.push(module(
                "QuestRequirementModule",
                fields! {"requirementType" => REQ_PRE_QUEST, "minValue" => 0, "maxValue" => pre.len(),
                "matches" => Vec::<Value>::new(), "missingMatches" => missing},
            ));
        }
        (
            if miss.is_empty() {
                NOT_STARTED
            } else {
                NOT_ACCEPTABLE
            },
            miss,
        )
    }

    fn kinds(q: &Value) -> Vec<Value> {
        q["kinds"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|t| module("QuestTypeModule", fields! {"type" => t}))
            .collect()
    }

    fn slim(c: &QuestCtx, p: &Profile, qid: i64) -> Value {
        let q = &c.data.quests[&qid];
        let (s, miss) = Self::status(c, p, qid);
        module(
            "QuestSlimInfoModule",
            fields! {
                "questId" => qid, "rootCaseId" => q["root"]["id"], "minLevel" => q["level"],
                "priority" => q["priority"], "types" => Self::kinds(q),
                "icon" => module("QuestIconModule", fields! {"icon" => q["icon"]}),
                "acceptabilityStatus" => module("QuestAcceptabilityStatusModule", fields! {"type" => s}),
                "missingAcceptRequirements" => miss, "textKeyFoundation" => "",
                "portraitId" => Self::portrait(p, q), "star" => 0,
            },
        )
    }

    /// Quests that hold a slot (`running`).
    pub fn running(c: &QuestCtx, p: &Profile, event: bool) -> i64 {
        active(p)
            .iter()
            .filter(|a| {
                a.parse::<i64>()
                    .ok()
                    .and_then(|q| c.data.quests.get(&q))
                    .is_some_and(|q| {
                        q["kinds"]
                            .as_array()
                            .is_some_and(|k| k.contains(&json!(EVENT_QUEST)))
                            == event
                    })
            })
            .count() as i64
    }

    pub fn free_slots(c: &QuestCtx, p: &Profile, event: bool) -> i64 {
        let max = if event {
            c.data.max_event_quests
        } else {
            c.settings.get("max_quests").as_i64().unwrap_or(0)
        };
        0.max(max - Self::running(c, p, event))
    }

    /// `QuestListCommand` (`list_cmd`).
    pub fn list_cmd(&mut self, c: &QuestCtx, p: &Profile) -> Msg {
        let offer = c.data.offered(c.now);
        let list: Vec<Value> = c
            .data
            .quests
            .keys()
            .filter(|q| Self::visible(c, p, **q, &offer))
            .map(|q| Self::slim(c, p, *q))
            .collect();
        self.offer = Some(offer);
        (
            "QuestListCommand".into(),
            Value::Object(fields! {
                "list" => list, "onlyStarter" => false, "maxQuests" => Self::free_slots(c, p, false),
                "maxEventQuests" => Self::free_slots(c, p, true),
            }),
        )
    }

    fn elapsed_ms(c: &QuestCtx, p: &Profile, qid: i64, target: &Value) -> i64 {
        let vals = Self::values(p, qid);
        let Some(start) = vals.get(START) else {
            return 0;
        };
        let x = 0f64.max((c.now - num(start).unwrap_or(0.0)) * 1000.0);
        let t = num(target).unwrap_or(0.0) - 1.0;
        (if t <= x { t } else { x }).trunc() as i64
    }

    /// `QuestDefinitionModule` with the profile's condition states (`definition`).
    pub fn definition(c: &QuestCtx, p: &Profile, qid: i64) -> Value {
        let q = &c.data.quests[&qid];
        let mut root = q["root"].clone();
        let vals = Self::values(p, qid);
        let fl = flags(&root, &vals);
        each_condition_mut(&mut root, &mut |cond| {
            let id = py_str(&cond["id"]);
            let (act, comp) = fl[&id];
            let cur = cap(cond, vals.get(&id).unwrap_or(&json!(0)));
            let st = &mut cond["state"];
            st["currentValue"] = cur;
            st["active"] = json!(act);
            st["completed"] = json!(comp);
            let kind = cond["type"].as_i64();
            if kind == Some(HASTE) {
                cond["state"]["currentValue"] =
                    json!(Self::elapsed_ms(c, p, qid, &cond["targetValue"]));
            } else if kind == Some(REAL_TIME_HASTE) {
                let e = Self::elapsed_ms(c, p, qid, &cond["targetValue"]);
                cond["state"]["currentValue"] = match cond["targetValue"].as_i64() {
                    Some(t) => json!(t - e),
                    None => json!(num(&cond["targetValue"]).unwrap_or(0.0) - e as f64),
                };
            }
            cond["matches"] = Self::resolve(c, p, &cond["matches"]);
            for s in cond["subConditions"].as_array_mut().into_iter().flatten() {
                s["matches"] = Self::resolve(c, p, &s["matches"]);
            }
        });
        let rewards: Vec<Value> = q["rewards"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|r| {
                module(
                    "LootModule",
                    fields! {"lootId" => r[0], "amount" => r[1], "modifier" => ""},
                )
            })
            .collect();
        module(
            "QuestDefinitionModule",
            fields! {
                "id" => qid, "types" => Self::kinds(q), "rootCase" => root, "rewards" => rewards,
                "icons" => [module("QuestIconModule", fields! {"icon" => q["icon"]})],
                "textKeyFoundation" => "", "portraitId" => Self::portrait(p, q), "star" => 0,
            },
        )
    }

    /// The quest giver and the refinery of the map, from its world messages (`Session.quest_map`).
    pub fn set_map(&mut self, map_id: i64, world: &[Msg]) {
        self.map_id = Some(map_id);
        self.giver = None;
        self.trade = None;
        for (n, a) in world {
            if n != "AssetCreateCommand" {
                continue;
            }
            let t = a["type"]["typeValue"].as_i64();
            if t == Some(crate::map_links::QUEST_GIVER) {
                self.giver = Some((a["assetId"].clone(), a["posX"].clone(), a["posY"].clone()));
            }
            if crate::map_links::STATION_ASSETS
                .iter()
                .any(|(k, v)| Some(*k) == t && *v == "ttip_ore_trade")
            {
                self.trade = Some((a["posX"].clone(), a["posY"].clone()));
            }
        }
    }

    /// Minimap marks of the active "return to the quest giver" / "sell ore" conditions (`mark_msgs`).
    pub fn mark_msgs(&mut self, c: &QuestCtx, p: &Profile, qid: i64) -> Vec<Msg> {
        let root = &c.data.quests[&qid]["root"];
        let fl = flags(root, &Self::values(p, qid));
        let mut want = BTreeSet::new();
        for cond in conditions(root) {
            if fl[&py_str(&cond["id"])].0 {
                let t = cond["type"].as_i64();
                if t == Some(VISIT_QUEST_GIVER) && self.giver.is_some() {
                    want.insert(MARK_GIVER);
                } else if t == Some(SELL_ORE) && self.trade.is_some() {
                    want.insert(MARK_TRADE);
                }
            }
        }
        let have = self.marks.entry(qid).or_default().clone();
        let mut msgs = Vec::new();
        for n in want.difference(&have) {
            let (x, y) = if *n == MARK_GIVER {
                let g = self.giver.as_ref().expect("giver");
                (g.1.clone(), g.2.clone())
            } else {
                self.trade.clone().expect("trade")
            };
            msgs.push((
                "CreateMinimapMark".into(),
                Value::Object(fields! {
                    "count" => -1, "scale" => 1.0, "uid" => format!("quest_{qid}_{n}"), "positionX" => x,
                    "positionY" => y, "interval" => 0.5, "inverse" => false,
                    "soundModule" => module("SoundTriggerModule", fields! {"playSound" => true, "soundId" => 132}),
                    "type" => 0,
                }),
            ));
        }
        for n in have.difference(&want) {
            msgs.push((
                "RemoveMinimapMark".into(),
                json!({"uid": format!("quest_{qid}_{n}")}),
            ));
        }
        self.marks.insert(qid, want);
        msgs
    }

    /// On login and on a map change: the active quests, the list and the marks on this map (`login_msgs`).
    pub fn login_msgs(&mut self, c: &QuestCtx, p: &Profile) -> Vec<Msg> {
        self.marks.clear();
        let ids: Vec<i64> = active(p)
            .iter()
            .filter_map(|q| q.parse::<i64>().ok())
            .filter(|q| c.data.quests.contains_key(q))
            .collect();
        let mut msgs: Vec<Msg> = ids
            .iter()
            .map(|q| {
                (
                    "QuestInitializationCommand".into(),
                    json!({"quest": Self::definition(c, p, *q)}),
                )
            })
            .collect();
        msgs.push(self.list_cmd(c, p));
        for q in ids {
            msgs.extend(self.mark_msgs(c, p, q));
        }
        msgs
    }
}
