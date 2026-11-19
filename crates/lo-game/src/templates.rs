//! The login template's messages adapted to the profile (OpenOrbit's `Profile.patch_template`, `menu_status`, `lab_items`,
//! `window_item`): ship speed, laboratory, daily login day, boosters, P.E.T. window, jump panel, cloaking, group invite
//! block, config number, jump coupons, window items and the whole shortcut menu (`MenuInitializationCommand`: every item's
//! state, cooldowns, premium bars, the player's shortcuts and dragged bars).
//!
//! Messages are JSON dicts in OpenOrbit's form (`{"_": class, …}`, a template message keeping its `_m` key).

use crate::booster;
use crate::combat_items::{self, line, tips};
use crate::event_calendar::EventCalendar;
use crate::items::ItemData;
use crate::login_markers::{LoginMarkers, bar_layout};
use crate::map_links::MapLinks;
use crate::ship_ability;
use crate::technology::{self, READY};
use lo_core::settings::{Settings, truthy};
use lo_save::Profile;
use lo_save::hangar::{num, py_str};
use serde_json::{Map, Value, json};
use std::collections::HashMap;

/// Premium shortcut bars added to the menu: id and position (`PREMIUM_SLOT_BARS`).
pub const PREMIUM_SLOT_BARS: [(&str, &str); 2] = [
    ("premiumSlotBar", "50,79.5|0,0"),
    ("proActionBar", "50,73.5|0,0"),
];
/// Shortcut bar -> the window settings keys of its position and layout order (`SLOT_BAR_KEYS`).
pub const SLOT_BAR_KEYS: [(&str, &str, &str); 3] = [
    ("standardSlotBar", "slotmenuPosition", "slotMenuOrder"),
    (
        "premiumSlotBar",
        "slotmenuPremiumPosition",
        "slotMenuPremiumOrder",
    ),
    (
        "proActionBar",
        "slotmenuProActionPosition",
        "slotMenuProActionOrder",
    ),
];
const LAYOUT_COUNT: u64 = 6;

/// "Double click to buy" in the item bar tooltip of the quick-buy items (`QUICK_BUY_TIP`).
pub fn quick_buy_tip() -> Value {
    json!({"_": "MessageKeyListWildcardModule", "wildCardReplacements": [], "baseKey": "ttip_quick_buy_itembar",
           "baseKeyType": {"_": "ReplacementTypeModule", "typeValue": 5}, "textStyle": 0})
}

/// What a session remembers of the shortcut menu (`menu_status_cache`, `menu_sent`, `cpu_init_sent`).
#[derive(Debug, Default, Clone)]
pub struct MenuState {
    /// menuItemId -> the item's state in the template.
    pub status_cache: Map<String, Value>,
    /// menuItemId -> the state last reported (`json.dumps(st, sort_keys=True)`).
    pub sent: HashMap<String, String>,
    /// The last `CPUInitializationCommand`.
    pub cpu_init_sent: Option<Value>,
}

/// What `patch_template` reads besides the profile.
pub struct Ctx<'a> {
    pub items: &'a ItemData,
    pub maps: &'a MapLinks,
    pub markers: &'a LoginMarkers,
    pub cal: &'a EventCalendar,
    pub settings: &'a Settings,
    pub now: f64,
    /// A session's own answer for a menu item's visibility (`window_hooks`).
    pub hook: &'a dyn Fn(&str) -> Option<bool>,
    /// The daily login calendar's fields (`daily_fields`), when the session has bound it.
    pub daily: Option<Map<String, Value>>,
}

fn sorted_keys(v: &Value) -> Value {
    match v {
        Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            Value::Object(
                keys.into_iter()
                    .map(|k| (k.clone(), sorted_keys(&m[k])))
                    .collect(),
            )
        }
        Value::Array(a) => Value::Array(a.iter().map(sorted_keys).collect()),
        other => other.clone(),
    }
}

/// `json.dumps(st, sort_keys=True)`.
pub fn sort_dumps(v: &Value) -> String {
    lo_core::pyjson::dumps(&sorted_keys(v))
}

/// The laboratory's loaded ores (`lab_items`).
pub fn lab_items(p: &Profile) -> Value {
    let mut ups: Vec<(&String, &Value)> =
        p.d.get("upgrades")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
            .collect();
    ups.sort_by(|a, b| a.0.cmp(b.0));
    Value::Array(
        ups.into_iter()
            .map(|(k, v)| {
                json!({"_": "UpdateItemModule",
                       "itemToUpdate": {"_": "LabItemModule", "itemValue": k.trim().parse::<i64>().unwrap_or(0)},
                       "oreCountToUpdateWith": {"_": "OreCountModule", "oreType": {"_": "OreTypeModule", "typeValue": v[0]},
                                                "count": v[1]}})
            })
            .collect(),
    )
}

/// The menu item's current state in the interface (`window_item`).
pub fn window_item(ctx: &Ctx, p: &Profile, item_id: &Value) -> Option<Map<String, Value>> {
    let ui = ctx.markers.ui_layout(p, ctx.cal, ctx.now, ctx.hook);
    ui["featureBars"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|b| b["featureBarItems"].as_array().into_iter().flatten())
        .find(|it| it.get("itemId") == Some(item_id))
        .map(|it| {
            it.as_object()
                .into_iter()
                .flatten()
                .filter(|(k, _)| *k != "_")
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        })
}

fn drop_line(st: &mut Map<String, Value>, key: &str) {
    for k in ["toolTipSlotBar", "toolTipItemBar"] {
        if let Some(lines) = st
            .get_mut(k)
            .filter(|t| truthy(t))
            .and_then(|t| t.get_mut("commandList"))
            .and_then(Value::as_array_mut)
            .filter(|l| !l.is_empty())
        {
            lines.retain(|l| l.get("baseKey").and_then(Value::as_str) != Some(key));
        }
    }
}

/// A shortcut item's current state (`menu_status`): from its template state, by the systems that own it.
pub fn menu_status(
    ctx: &Ctx,
    p: &mut Profile,
    state: &MenuState,
    menu_id: &str,
) -> Option<Map<String, Value>> {
    let base = state.status_cache.get(menu_id)?;
    let mut st = base.as_object().cloned().unwrap_or_default();
    let items = ctx.items;
    if menu_id.starts_with("drone_formation_") {
        let owned = menu_id == "drone_formation_default"
            || p.d["formations"]
                .as_array()
                .is_some_and(|f| f.iter().any(|x| x.as_str() == Some(menu_id)));
        let fid = items.e["formations"].get(menu_id).map(|f| f["id"].clone());
        let cur = p.d.get("formation").cloned().unwrap_or(json!(0));
        let sel = fid.is_some_and(|f| num(&f).is_some() && num(&f) == num(&cur));
        let need = items.e["formation_rules"]
            .get(menu_id)
            .and_then(|r| r.get("min_drones"))
            .and_then(num)
            .unwrap_or(0.0);
        let enough = p.d["drones"].as_array().map_or(0, Vec::len) as f64 >= need;
        st.insert("visible".into(), json!(owned));
        st.insert("available".into(), json!(owned && enough));
        st.insert("activatable".into(), json!(owned && enough));
        st.insert("selected".into(), json!(owned && sel));
        if owned {
            drop_line(&mut st, "ttip_item_not_owned");
        }
        if owned && !enough {
            tips(&mut st, &[line("ttip_inactive_not_enough_drones", &[], 5)]);
        }
        return Some(st);
    }
    if menu_id.starts_with("ability_") {
        return Some(ship_ability::menu_status(items, p, menu_id, st, ctx.now));
    }
    let mut out = combat_items::menu_status(items, ctx.maps, p, menu_id, st.clone());
    if out.is_none() && menu_id.starts_with("tech_") {
        out = technology::menu_status(items, p, menu_id, st.clone(), ctx.now);
    }
    if out.is_some() {
        return out;
    }
    if menu_id.starts_with("ammunition_") {
        let raw = p.d["ammunition"].get(menu_id).cloned().unwrap_or(json!(0));
        let n = num(&raw).unwrap_or(0.0);
        let kind = combat_items::ammo_kind(items, menu_id);
        let selected =
            kind.is_some_and(|k| p.d["selected"].get(k).and_then(Value::as_str) == Some(menu_id));
        let visible = st.get("visible").is_some_and(truthy) || n > 0.0;
        st.insert("counterValue".into(), raw.clone());
        st.insert("counterStyle".into(), json!(if n > 0.0 { 4 } else { 1 }));
        st.insert("available".into(), json!(n > 0.0));
        st.insert("activatable".into(), json!(n > 0.0));
        st.insert("selected".into(), json!(selected));
        st.insert("visible".into(), json!(visible));
        for k in ["toolTipSlotBar", "toolTipItemBar"] {
            for l in st
                .get_mut(k)
                .filter(|t| truthy(t))
                .and_then(|t| t.get_mut("commandList"))
                .and_then(Value::as_array_mut)
                .into_iter()
                .flatten()
            {
                for rep in l
                    .get_mut("wildCardReplacements")
                    .and_then(Value::as_array_mut)
                    .into_iter()
                    .flatten()
                {
                    if rep.get("wildcard").and_then(Value::as_str) == Some("%COUNT%") {
                        rep["replacement"] = json!(py_str(&raw));
                    }
                }
            }
        }
    }
    Some(st)
}

fn with(fields: &Map<String, Value>, kv: &[(&str, Value)]) -> Value {
    let mut f = fields.clone();
    for (k, v) in kv {
        f.insert(k.to_string(), v.clone());
    }
    Value::Object(f)
}

fn layout_ok(v: &Value) -> Option<String> {
    let text = match v {
        Value::String(s) => s.clone(),
        Value::Number(n) if n.is_u64() || n.is_i64() => n.to_string(),
        _ => return None,
    };
    let ok = !text.is_empty()
        && text.chars().all(|c| c.is_ascii_digit())
        && text.parse::<u64>().is_ok_and(|n| n < LAYOUT_COUNT);
    ok.then_some(text)
}

fn walk(ctx: &Ctx, p: &mut Profile, state: &mut MenuState, x: &mut Value) {
    match x {
        Value::Object(m) => {
            let class = m
                .get("_")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let mid = m.get("menuItemId").cloned().unwrap_or(Value::Null);
            let mid_s = mid.as_str().unwrap_or_default().to_string();
            if class == "MenuItemTimerCommand" {
                let timer = if ship_ability::is_ability(ctx.items, &mid_s) {
                    Some(ship_ability::timer_fields(ctx.items, p, &mid_s, ctx.now))
                } else {
                    let t = if technology::is_item(ctx.items, &mid_s) {
                        Some(technology::timer_fields(p, &mid_s, ctx.now))
                    } else {
                        combat_items::timer_fields(ctx.items, p, &mid_s, ctx.now)
                    };
                    t.filter(|t| t.state != READY)
                };
                if let Some(f) = timer {
                    let ability = ship_ability::is_ability(ctx.items, &mid_s);
                    m.insert(
                        "activatable".into(),
                        json!(if ability { f.activatable } else { false }),
                    );
                    m.insert("maxTime".into(), f.max_time);
                    m.insert("time".into(), json!(f.time));
                    m.insert(
                        "timerState".into(),
                        json!({"_": "TimerStateModule", "timerStateValue": f.state}),
                    );
                }
            }
            if class == "MenuItemStatusCommand" && truthy(&mid) {
                let quick = mid_s.starts_with("buy_");
                if quick
                    && let Some(lines) = m
                        .get_mut("toolTipItemBar")
                        .filter(|t| truthy(t))
                        .and_then(|t| t.get_mut("commandList"))
                        .and_then(Value::as_array_mut)
                {
                    let tip = quick_buy_tip();
                    if !lines.iter().any(|l| l.get("baseKey") == tip.get("baseKey")) {
                        lines.push(tip);
                    }
                }
                state
                    .status_cache
                    .entry(py_str(&mid))
                    .or_insert_with(|| Value::Object(m.clone()));
                if let Some(cur) =
                    menu_status(ctx, p, state, &py_str(&mid)).filter(|c| !c.is_empty())
                {
                    for (k, v) in &cur {
                        m.insert(k.clone(), v.clone());
                    }
                    let key = py_str(cur.get("menuItemId").unwrap_or(&Value::Null));
                    state.sent.insert(key, sort_dumps(&Value::Object(cur)));
                }
            }
            let keys: Vec<String> = m.keys().cloned().collect();
            for k in keys {
                if let Some(v) = m.get_mut(&k) {
                    walk(ctx, p, state, v);
                }
            }
        }
        Value::Array(a) => {
            for v in a.iter_mut() {
                walk(ctx, p, state, v);
            }
        }
        _ => {}
    }
}

/// The template message adapted to the profile (`patch_template`).
pub fn patch_template(
    ctx: &Ctx,
    p: &mut Profile,
    state: &mut MenuState,
    name: &str,
    fields: &Value,
) -> Value {
    let f = fields.as_object().cloned().unwrap_or_default();
    let msg = f
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    match name {
        "AttributeShipSpeedUpdateCommand" => {
            let speed = p.d["ship"]
                .get("speed")
                .cloned()
                .unwrap_or_else(|| f["newSpeed"].clone());
            let shown =
                p.d.get("shown_speed")
                    .cloned()
                    .unwrap_or_else(|| f["displaySpeed"].clone());
            return with(&f, &[("newSpeed", speed), ("displaySpeed", shown)]);
        }
        "LabUpdateItemCommand" => return with(&f, &[("itemsUpdatedInfo", lab_items(p))]),
        "DailyLoginRewardDayCommand" if ctx.daily.is_some() => {
            let d = ctx.daily.clone().unwrap_or_default();
            let kv: Vec<(&str, Value)> = d.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
            return with(&f, &kv);
        }
        "AttributeBoosterUpdateCommand" => {
            return booster::update_cmd(ctx.items, p, ctx.settings, &[], ctx.now);
        }
        "PetInitializationCommand" => {
            let pet = p.d.get("pet").cloned().unwrap_or(Value::Null);
            let has = truthy(&pet);
            let fuel = has && num(&pet["fuel"]).is_some_and(|x| x > 0.0);
            let alive = has && !truthy(&pet["destroyed"]);
            return with(
                &f,
                &[
                    ("hasPet", json!(has)),
                    ("hasFuel", json!(fuel)),
                    ("petIsAlive", json!(alive)),
                ],
            );
        }
        "CPUInitializationCommand" => {
            let v = with(
                &f,
                &[(
                    "advancedJumpCPUEquipped",
                    json!(combat_items::has_ajp(ctx.items, p)),
                )],
            );
            state.cpu_init_sent = Some(v.clone());
            return v;
        }
        "LegacyModule" if msg.starts_with("0|n|INV|") => {
            let part = msg.split('|').nth(3).unwrap_or_default().to_string();
            let hidden = i64::from(p.d.get("cpu_hidden").is_some_and(truthy));
            return with(
                &f,
                &[("message", json!(format!("0|n|INV|{part}|{hidden}")))],
            );
        }
        "InvitationBlockModule" => {
            return with(
                &f,
                &[("active", json!(p.d.get("invite_block").is_some_and(truthy)))],
            );
        }
        "LegacyModule" if msg.starts_with("0|S|CFG|") => {
            let c = py_str(&p.ship(None)["active_config"]);
            return with(&f, &[("message", json!(format!("0|S|CFG|{c}")))]);
        }
        "LegacyModule" if msg.starts_with("0|A|JV|") => {
            return combat_items::vouchers_msg(ctx.items, p).1;
        }
        "WindowItemCommand" => {
            let id = f.get("itemId").cloned().unwrap_or(Value::Null);
            return match window_item(ctx, p, &id).filter(|c| !c.is_empty()) {
                Some(cur) => {
                    let mut kv: Vec<(&str, Value)> = ["posX", "posY", "width", "height", "visible"]
                        .into_iter()
                        .filter_map(|k| cur.get(k).map(|v| (k, v.clone())))
                        .collect();
                    kv.push(("maximized", json!(false)));
                    with(&f, &kv)
                }
                None => fields.clone(),
            };
        }
        "MenuInitializationCommand" => {}
        _ => return fields.clone(),
    }
    let mut f = Value::Object(f);
    walk(ctx, p, state, &mut f);
    let ws = bar_layout(p);
    let shortcuts = p.d.get("shortcuts").cloned().unwrap_or(json!({}));
    let fm = f.as_object_mut().expect("menu");
    let bars = fm
        .entry("slotBars")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .expect("slotBars");
    let have: Vec<Value> = bars
        .iter()
        .map(|b| b.get("slotBarId").cloned().unwrap_or(Value::Null))
        .collect();
    for (bid, pos) in PREMIUM_SLOT_BARS {
        if !have.iter().any(|h| h == bid) {
            bars.push(json!({"_": "SlotBarInitializationCommand", "slots": [], "slotBarId": bid, "slotBarOrder": "0",
                             "slotBarPosition": pos, "visible": true}));
        }
    }
    for bar in bars.iter_mut() {
        let id = bar.get("slotBarId").cloned().unwrap_or(Value::Null);
        if let Some(cur) = shortcuts
            .get(py_str(&id))
            .filter(|_| id.is_string())
            .and_then(Value::as_object)
        {
            let mut slots: Vec<(i64, &Value)> = cur
                .iter()
                .map(|(k, v)| (k.trim().parse().unwrap_or(0), v))
                .collect();
            slots.sort_by_key(|(k, _)| *k);
            bar["slots"] = Value::Array(
                slots
                    .into_iter()
                    .map(|(k, v)| json!({"_": "SlotInfoCommand", "menuItemId": v, "slotId": k}))
                    .collect(),
            );
        }
        if let Some(&(_, pos, order)) = SLOT_BAR_KEYS
            .iter()
            .find(|(k, _, _)| id.as_str() == Some(*k))
        {
            if let Some(v) = ws.get(pos).filter(|v| truthy(v)).and_then(Value::as_str) {
                bar["slotBarPosition"] =
                    json!(format!("{}|0,0", v.split('|').next().unwrap_or("")));
            }
            if let Some(o) = ws.get(order).and_then(layout_ok) {
                bar["slotBarOrder"] = json!(o);
            }
        }
    }
    if let Some(v) = ws.get("mainmenuPosition").filter(|v| truthy(v)) {
        fm.insert("itemBarPosition".into(), v.clone());
    }
    f
}
