//! Map layout (OpenOrbit's `map_layout.py`, data `data/map_layout.json` with the World Editor's patches): obstacles,
//! portal positions and arrival points, the Palladium area, mine fields, safe zones and structures of the pirate, Black
//! Light and a few other maps; roaming around obstacles (A* on a grid).
//!
//! Obstacles go to the client as `MapAddPOICommand` NO_ACCESS rectangles (or polygons). Randomness takes a
//! [`Random`] so a comparison can draw the same numbers as OpenOrbit.

use lo_core::GameData;
use lo_core::pyrandom::Random;
use serde_json::{Map, Value, json};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};
use std::sync::Mutex;

/// `POIType`: NO_ACCESS, SHIELD_DAMAGE, MINES.
pub const NO_ACCESS: i64 = 5;
pub const SHIELD_DAMAGE: i64 = 3;
pub const MINES: i64 = 10;
/// The client's `ShapeType`.
pub const POLYGON: i64 = 1;
pub const RECTANGLE: i64 = 2;
/// `POIDesign`: the Palladium area's fog.
pub const NEBULA: i64 = 5;
/// Cell of the roaming grid.
pub const CELL: f64 = 200.0;
/// Margin an alien keeps from obstacles when spawning.
pub const SHIP_PAD: f64 = 150.0;
/// Margin when roaming.
pub const NAV_PAD: f64 = 60.0;
/// The diagonal step's cost: OpenOrbit's rounded 1.4142, not the exact square root (paths must tie-break the same way).
const DIAGONAL: f64 = 14142.0 / 10000.0;

/// An obstacle rectangle `(x0, y0, x1, y1)`.
pub type Rect = (f64, f64, f64, f64);

fn f(v: &Value) -> f64 {
    v.as_f64().unwrap_or(0.0)
}

/// Python's `int()` of a number (toward zero), kept as a JSON integer.
fn int_json(v: &Value) -> Value {
    match v {
        Value::Number(n) if n.is_f64() => json!(n.as_f64().unwrap_or(0.0).trunc() as i64),
        other => other.clone(),
    }
}

type Grid = (Vec<Vec<bool>>, i64, i64);

/// The map layout.
pub struct MapLayout {
    maps: HashMap<i64, Value>,
    walls: HashMap<i64, Vec<Rect>>,
    /// `mine`: the mine fields' settings (type, count, renew_sec, portal_far).
    pub mine: Value,
    grids: Mutex<HashMap<(i64, i64, i64), std::sync::Arc<Grid>>>,
}

#[derive(Clone, Copy, PartialEq)]
struct Node(f64, (i64, i64));

impl Eq for Node {}
impl PartialOrd for Node {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Node {
    /// Reversed: the heap pops the smallest (priority, cell), like `heapq`.
    fn cmp(&self, o: &Self) -> Ordering {
        o.0.total_cmp(&self.0).then(o.1.cmp(&self.1))
    }
}

impl MapLayout {
    pub fn from_value(d: &Value) -> MapLayout {
        let mut maps = HashMap::new();
        let mut walls = HashMap::new();
        for (k, v) in d
            .get("maps")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
        {
            let Ok(id) = k.trim().parse::<i64>() else {
                continue;
            };
            let w: Vec<Rect> = v
                .get("obstacles")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|e| (f(&e[1]), f(&e[2]), f(&e[3]), f(&e[4])))
                .collect();
            walls.insert(id, w);
            maps.insert(id, v.clone());
        }
        MapLayout {
            maps,
            walls,
            mine: d.get("mine").cloned().unwrap_or(Value::Null),
            grids: Mutex::new(HashMap::new()),
        }
    }

    /// `data/map_layout.json` with the world's patches.
    pub fn load(data: &GameData) -> Result<MapLayout, String> {
        let d = data
            .load("map_layout.json")
            .map_err(|e| format!("map_layout.json: {e}"))?;
        Ok(MapLayout::from_value(&d))
    }

    /// The map's layout record (`of`; an empty object for a map without one).
    pub fn of(&self, map_id: i64) -> &Value {
        static EMPTY: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        self.maps
            .get(&map_id)
            .unwrap_or_else(|| EMPTY.get_or_init(|| json!({})))
    }

    /// Obstacle rectangles.
    pub fn walls(&self, map_id: i64) -> &[Rect] {
        self.walls.get(&map_id).map_or(&[], Vec::as_slice)
    }

    fn poi(
        pid: Value,
        ptype: i64,
        design: Value,
        rect: [&Value; 4],
        shape: i64,
        coords: Option<Value>,
    ) -> Value {
        let coords = coords.filter(lo_core::settings::truthy).unwrap_or_else(|| {
            let [x0, y0, x1, y1] = rect.map(int_json);
            json!([x0, y0, x1, y0, x1, y1, x0, y1])
        });
        json!({"_m": "MapAddPOICommand", "inverted": false, "poiTypeSpecification": "",
               "poiType": {"_": "POITypeModule", "typeValue": ptype}, "shape": shape, "active": true,
               "design": {"_": "POIDesignModule", "designValue": design}, "poiId": pid,
               "shapeCoordinates": coords})
    }

    /// The zones of the map's ready bundle: obstacles, the Palladium area, mine fields and other zones (`poi_msgs`).
    pub fn poi_msgs(&self, map_id: i64) -> Vec<Value> {
        let d = self.of(map_id);
        let arr = |k: &str| {
            d.get(k)
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        };
        let mut out = Vec::new();
        for e in arr("obstacles") {
            let (shape, coords) = if e.as_array().is_some_and(|a| a.len() > 6) {
                (POLYGON, Some(e[6].clone()))
            } else {
                (RECTANGLE, None)
            };
            out.push(Self::poi(
                e[0].clone(),
                NO_ACCESS,
                e[5].clone(),
                [&e[1], &e[2], &e[3], &e[4]],
                shape,
                coords,
            ));
        }
        if let Some(p) = d.get("palladium") {
            let a = &p["area"];
            out.push(Self::poi(
                p["id"].clone(),
                SHIELD_DAMAGE,
                json!(NEBULA),
                [&a[0], &a[1], &a[2], &a[3]],
                RECTANGLE,
                None,
            ));
        }
        for (i, r) in arr("mine_fields").iter().enumerate() {
            out.push(Self::poi(
                json!(format!("Mines{}", i + 1)),
                MINES,
                json!(0),
                [&r[0], &r[1], &r[2], &r[3]],
                RECTANGLE,
                None,
            ));
        }
        let zero = json!(0);
        for z in arr("zones") {
            out.push(Self::poi(
                z[0].clone(),
                z[1].as_i64().unwrap_or(0),
                z[3].clone(),
                [&zero, &zero, &zero, &zero],
                z[2].as_i64().unwrap_or(RECTANGLE),
                Some(z[4].clone()),
            ));
        }
        out
    }

    fn sub(&self, map_id: i64, section: &str, key: &str) -> Option<&Value> {
        self.of(map_id).get(section).and_then(|s| s.get(key))
    }

    /// The portal's position in the layout (`gate_pos`).
    pub fn gate_pos(&self, map_id: i64, key: i64) -> Option<(Value, Value)> {
        self.sub(map_id, "portals", &key.to_string())
            .filter(|p| lo_core::settings::truthy(p))
            .map(|p| (p[0].clone(), p[1].clone()))
    }

    /// The portal's arrival point on the target map: a point, `"random"`, or `None` (next to the return portal).
    pub fn arrival(&self, map_id: i64, key: i64) -> Option<Value> {
        self.sub(map_id, "arrival", &key.to_string()).cloned()
    }

    /// The map's one-way arrival portals `[(x, y, company)]`.
    pub fn one_way_gates(&self, map_id: i64) -> Vec<Value> {
        self.of(map_id)
            .get("arrival_portals")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    }

    /// The position of a gate or mode portal by its design (`design_pos`).
    pub fn design_pos(&self, map_id: i64, design: i64) -> Option<(Value, Value)> {
        self.sub(map_id, "design_spot", &design.to_string())
            .filter(|p| lo_core::settings::truthy(p))
            .map(|p| (p[0].clone(), p[1].clone()))
    }

    pub fn structure_pos(&self, map_id: i64) -> Option<(f64, f64)> {
        self.of(map_id)
            .get("structure")
            .filter(|p| lo_core::settings::truthy(p))
            .map(|p| (f(&p[0]), f(&p[1])))
    }

    /// The structure's entity position (the 5-2 trade point) and its approach radius.
    pub fn structure_asset(&self, map_id: i64) -> (Option<(Value, Value)>, Option<Value>) {
        let d = self.of(map_id);
        (
            d.get("structure_asset")
                .map(|p| (p[0].clone(), p[1].clone())),
            d.get("structure_radius").cloned(),
        )
    }

    /// The types' fixed spawn points: `{type: [[x, y], …]}`.
    pub fn lairs(&self, map_id: i64) -> Map<String, Value> {
        self.of(map_id)
            .get("lairs")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default()
    }

    /// Safe zones `[(x, y, radius)]`.
    pub fn safe_zones(&self, map_id: i64) -> Vec<(f64, f64, f64)> {
        self.of(map_id)
            .get("safe")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|s| (f(&s[0]), f(&s[1]), f(&s[2])))
            .collect()
    }

    pub fn gate_peace_radius(&self, map_id: i64, default: f64) -> f64 {
        self.of(map_id).get("portal_safe").map_or(default, f)
    }

    pub fn in_safe(&self, map_id: i64, x: f64, y: f64, margin: f64) -> bool {
        self.safe_zones(map_id)
            .iter()
            .any(|&(cx, cy, r)| (x - cx).hypot(y - cy) <= r + margin)
    }

    pub fn blocked(&self, map_id: i64, x: f64, y: f64, pad: f64) -> bool {
        self.walls(map_id).iter().any(|&(x0, y0, x1, y1)| {
            x0 - pad <= x && x <= x1 + pad && y0 - pad <= y && y <= y1 + pad
        })
    }

    /// Whether the segment stays clear of the obstacles (with a margin).
    pub fn clear(&self, map_id: i64, a: (f64, f64), b: (f64, f64), pad: f64) -> bool {
        !self
            .walls(map_id)
            .iter()
            .any(|&(x0, y0, x1, y1)| seg_hits(a, b, (x0 - pad, y0 - pad, x1 + pad, y1 + pad)))
    }

    /// A random point in the rectangle outside obstacles (`free_spot`).
    pub fn free_spot(
        &self,
        map_id: i64,
        rect: Rect,
        pad: f64,
        tries: u32,
        rng: &mut Random,
    ) -> Option<(i64, i64)> {
        let (x0, y0, x1, y1) = rect;
        for _ in 0..tries {
            let x = rng.uniform(x0, x1).trunc();
            let y = rng.uniform(y0, y1).trunc();
            if !self.blocked(map_id, x, y, pad) {
                return Some((x as i64, y as i64));
            }
        }
        None
    }

    /// Whether aliens walk around the obstacles on this map.
    pub fn navigates(&self, map_id: i64) -> bool {
        self.of(map_id)
            .get("roaming")
            .is_some_and(lo_core::settings::truthy)
    }

    fn grid(&self, map_id: i64, w: i64, h: i64) -> std::sync::Arc<Grid> {
        let mut grids = self.grids.lock().unwrap_or_else(|e| e.into_inner());
        grids
            .entry((map_id, w, h))
            .or_insert_with(|| {
                let nx = (w as f64 / CELL).ceil() as i64;
                let ny = (h as f64 / CELL).ceil() as i64;
                let mut g = vec![vec![false; nx.max(0) as usize]; ny.max(0) as usize];
                for &(x0, y0, x1, y1) in self.walls(map_id) {
                    let ys = (
                        0i64.max(((y0 - NAV_PAD) / CELL).floor() as i64),
                        ny.min(((y1 + NAV_PAD) / CELL).floor() as i64 + 1),
                    );
                    let xs = (
                        0i64.max(((x0 - NAV_PAD) / CELL).floor() as i64),
                        nx.min(((x1 + NAV_PAD) / CELL).floor() as i64 + 1),
                    );
                    for cy in ys.0..ys.1 {
                        for cx in xs.0..xs.1 {
                            g[cy as usize][cx as usize] = true;
                        }
                    }
                }
                std::sync::Arc::new((g, nx, ny))
            })
            .clone()
    }

    /// Roaming: the first waypoint of the path to the target (`waypoint`): 8-direction A* on the grid, shortened to the
    /// last visible point; to the nearest free cell if the target is in an obstacle, to the nearest reachable cell if
    /// there is no path.
    pub fn waypoint(
        &self,
        map_id: i64,
        w: i64,
        h: i64,
        a: (f64, f64),
        b: (f64, f64),
    ) -> (f64, f64) {
        let (ax, ay) = a;
        let (mut bx, mut by) = b;
        if self.clear(map_id, a, b, NAV_PAD) {
            return b;
        }
        let grid = self.grid(map_id, w, h);
        let (g, nx, ny) = (&grid.0, grid.1, grid.2);
        let blocked = |c: (i64, i64)| g[c.1 as usize][c.0 as usize];
        let cell = |x: f64, y: f64| {
            (
                (nx - 1).min(0i64.max((x / CELL).floor() as i64)),
                (ny - 1).min(0i64.max((y / CELL).floor() as i64)),
            )
        };
        let center = |c: (i64, i64)| {
            (
                c.0 as f64 * CELL + CELL / 2.0,
                c.1 as f64 * CELL + CELL / 2.0,
            )
        };
        let s = cell(ax, ay);
        let mut t = cell(bx, by);
        if blocked(s) {
            return center(nearest_free(g, nx, ny, s));
        }
        if blocked(t) {
            t = nearest_free(g, nx, ny, t);
            (bx, by) = center(t);
        }
        let dist = |p: (i64, i64), q: (i64, i64)| ((p.0 - q.0) as f64).hypot((p.1 - q.1) as f64);
        let mut came: HashMap<(i64, i64), Option<(i64, i64)>> = HashMap::from([(s, None)]);
        let mut cost: HashMap<(i64, i64), f64> = HashMap::from([(s, 0.0)]);
        let mut todo = BinaryHeap::from([Node(0.0, s)]);
        let mut near = s;
        while let Some(Node(_, c)) = todo.pop() {
            if c == t {
                break;
            }
            if dist(t, c) < dist(t, near) {
                near = c;
            }
            for (dx, dy) in [
                (1, 0),
                (-1, 0),
                (0, 1),
                (0, -1),
                (1, 1),
                (1, -1),
                (-1, 1),
                (-1, -1),
            ] {
                let n = (c.0 + dx, c.1 + dy);
                if !(0 <= n.0 && n.0 < nx && 0 <= n.1 && n.1 < ny) || blocked(n) {
                    continue;
                }
                if dx != 0 && dy != 0 && (blocked((n.0, c.1)) || blocked((c.0, n.1))) {
                    continue;
                }
                let k = cost[&c] + if dx != 0 && dy != 0 { DIAGONAL } else { 1.0 };
                if k < *cost.get(&n).unwrap_or(&1e18) {
                    cost.insert(n, k);
                    came.insert(n, Some(c));
                    todo.push(Node(k + dist(t, n), n));
                }
            }
        }
        if !came.contains_key(&t) {
            t = near;
            (bx, by) = center(t);
        }
        let mut path = vec![t];
        while let Some(Some(p)) = came.get(path.last().unwrap()) {
            path.push(*p);
        }
        path.reverse();
        let mut best = if path.len() > 1 {
            center(path[1])
        } else {
            (bx, by)
        };
        for c in &path[1..] {
            let p = center(*c);
            if !self.clear(map_id, a, p, NAV_PAD) {
                break;
            }
            best = p;
        }
        if best == center(t) && self.clear(map_id, a, (bx, by), NAV_PAD) {
            return (bx, by);
        }
        best
    }

    /// Mine positions scattered over the mine fields, outside obstacles and away from portals (`mine_spots`). `size`:
    /// the map's size (`map_links.map_size`).
    pub fn mine_spots(
        &self,
        map_id: i64,
        gates: &[(f64, f64)],
        n: Option<i64>,
        size: (i64, i64),
        rng: &mut Random,
    ) -> Vec<(i64, i64)> {
        let count =
            n.unwrap_or_else(|| self.mine.get("count").and_then(Value::as_i64).unwrap_or(0));
        let far = self.mine.get("portal_far").map_or(0.0, f);
        let mut out = Vec::new();
        for r in self.mine_fields(map_id) {
            for _ in 0..count {
                let p = self.free_spot(map_id, in_map(r, size), 100.0, 40, rng);
                if let Some(p) = p
                    && gates
                        .iter()
                        .all(|g| (p.0 as f64 - g.0).hypot(p.1 as f64 - g.1) > far)
                {
                    out.push(p);
                }
            }
        }
        out
    }

    fn mine_fields(&self, map_id: i64) -> Vec<Rect> {
        self.of(map_id)
            .get("mine_fields")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|r| (f(&r[0]), f(&r[1]), f(&r[2]), f(&r[3])))
            .collect()
    }

    /// The part inside the map of the mine field containing the point (`mine_field_of`).
    pub fn mine_field_of(&self, map_id: i64, x: f64, y: f64, size: (i64, i64)) -> Option<Rect> {
        self.mine_fields(map_id)
            .into_iter()
            .find(|r| r.0 <= x && x <= r.2 && r.1 <= y && y <= r.3)
            .map(|r| in_map(r, size))
    }
}

/// The part of a field inside the map.
fn in_map(r: Rect, (w, h): (i64, i64)) -> Rect {
    (
        r.0.max(0.0),
        r.1.max(0.0),
        r.2.min(w as f64),
        r.3.min(h as f64),
    )
}

/// Whether the segment crosses the rectangle (Liang-Barsky).
fn seg_hits(a: (f64, f64), b: (f64, f64), r: Rect) -> bool {
    let (x0, y0, x1, y1) = r;
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for (p, q) in [
        (-dx, a.0 - x0),
        (dx, x1 - a.0),
        (-dy, a.1 - y0),
        (dy, y1 - a.1),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return false;
            }
            continue;
        }
        let t = q / p;
        if p < 0.0 {
            t0 = t0.max(t);
        } else {
            t1 = t1.min(t);
        }
        if t0 > t1 {
            return false;
        }
    }
    true
}

fn nearest_free(g: &[Vec<bool>], nx: i64, ny: i64, c: (i64, i64)) -> (i64, i64) {
    for r in 1..nx.max(ny) {
        for dy in -r..=r {
            let xs: Vec<i64> = if dy.abs() != r {
                vec![-r, r]
            } else {
                (-r..=r).collect()
            };
            for dx in xs {
                let (x, y) = (c.0 + dx, c.1 + dy);
                if 0 <= x && x < nx && 0 <= y && y < ny && !g[y as usize][x as usize] {
                    return (x, y);
                }
            }
        }
    }
    c
}
