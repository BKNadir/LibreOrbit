//! The game server (OpenOrbit's `game.py`, TCP 8081).
//!
//! Port status: the connection, the framing, the logs, `VersionRequest`, `KeepAlive`, and the login bundles of
//! `LoginRequest` and `ReadyRequest` (`bundles`) with the systems ported so far; the map simulation and the other systems
//! come next (see `docs/portage-jeu.md`). A request the server does not handle yet is logged as such and left unanswered.

pub mod booster;
pub mod bundles;
pub mod combat_items;
pub mod event_appearances;
pub mod event_calendar;
pub mod extras;
pub mod items;
pub mod login_markers;
pub mod map_layout;
pub mod map_links;
pub mod ship_ability;
pub mod technology;
pub mod templates;

use bundles::{Msg, Templates};
use lo_core::settings::Settings;
use lo_core::{GameData, Layout, LogDir};
use lo_protocol::display::{fmt, redact, secrets_of};
use lo_protocol::{Framer, Spec, Struct, frame};
use lo_save::Profile;
use serde_json::Value;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// Requests never written to the log (`UNLOGGED`).
const UNLOGGED: [&str; 4] = [
    "MoveRequest",
    "KeepAlive",
    "AttackRocketRequest",
    "EggTriggerRequest",
];

/// Settings, with the command-line names of `game.py`.
#[derive(Debug, Clone)]
pub struct Config {
    pub log_dir: PathBuf,
    /// `--profile`: the save slot's profile (default: the active slot's).
    pub profile: Option<PathBuf>,
    /// `--settings`: the game settings file.
    pub settings: Option<PathBuf>,
    /// `--stop-file`: when it appears the profile is saved and the server stops.
    pub stop_file: Option<PathBuf>,
    pub no_popups: bool,
    pub near_npc: bool,
    pub invulnerable: bool,
    pub control: bool,
    pub multiplayer: bool,
    pub address: String,
    pub port: u16,
}

impl Config {
    /// `<log_folder> [--profile=] [--settings=] [--stop-file=] [--no-popups] [--near-npc] [--invulnerable] [--control]
    /// [--multiplayer]`, plus LibreOrbit's `--port=8081`.
    pub fn from_args(args: &[String]) -> Result<Config, String> {
        let log_dir = args
            .iter()
            .find(|a| !a.starts_with("--"))
            .cloned()
            .unwrap_or_else(|| ".".into());
        let mut c = Config {
            log_dir: log_dir.into(),
            profile: None,
            settings: None,
            stop_file: None,
            no_popups: false,
            near_npc: false,
            invulnerable: false,
            control: false,
            multiplayer: false,
            address: "127.0.0.1".into(),
            port: 8081,
        };
        for a in args.iter().filter(|a| a.starts_with("--")) {
            let (k, v) = a[2..].split_once('=').unwrap_or((&a[2..], ""));
            match k {
                "profile" => c.profile = Some(v.into()),
                "settings" => c.settings = Some(v.into()),
                "stop-file" => c.stop_file = Some(v.into()),
                "no-popups" => c.no_popups = true,
                "near-npc" => c.near_npc = true,
                "invulnerable" => c.invulnerable = true,
                "control" => c.control = true,
                "multiplayer" => c.multiplayer = true,
                "port" => c.port = v.parse().map_err(|_| format!("invalid --port {v:?}"))?,
                _ => {}
            }
        }
        Ok(c)
    }
}

/// The game server's shared state.
pub struct Game {
    pub cfg: Config,
    pub spec: Spec,
    pub logs: LogDir,
    pub settings: Arc<Settings>,
    pub templates: Templates,
    pub links: map_links::MapLinks,
    pub items: items::ItemData,
    pub markers: login_markers::LoginMarkers,
    pub cal: event_calendar::EventCalendar,
    /// The player's profile (the server's own player).
    pub profile: Mutex<Profile>,
}

fn read_json(p: Option<PathBuf>) -> Value {
    p.and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| Value::Object(Default::default()))
}

impl Game {
    /// Loads the data, the templates, the settings and the profile. `args`: the command line (`--event`).
    pub fn new(cfg: Config, spec: Spec, layout: &Layout, args: &[String]) -> Result<Game, String> {
        let logs = LogDir::new(&cfg.log_dir).map_err(|e| format!("log folder: {e}"))?;
        let settings_path = cfg
            .settings
            .clone()
            .unwrap_or_else(|| layout.settings_path(None));
        let settings = Arc::new(Settings::bind(settings_path));
        let data = GameData::from_env(&layout.data);
        let links = map_links::MapLinks::load(&data)?;
        let templates = Templates::new(
            read_json(layout.template("login.json")),
            read_json(layout.template("maps.json")),
            &links,
        );
        let items = items::ItemData::load(&data)?;
        let markers = login_markers::LoginMarkers::load(&data)?;
        let mut cal = event_calendar::EventCalendar::load(&data, settings.clone())?;
        cal.configure(args);
        let profile_path = cfg
            .profile
            .clone()
            .unwrap_or_else(|| layout.profile_path(None));
        let seed = templates.entry.get("seed").cloned().unwrap_or(Value::Null);
        let profile =
            Profile::load(&profile_path, &seed, &settings).map_err(|e| format!("profile: {e}"))?;
        Ok(Game {
            cfg,
            spec,
            logs,
            settings,
            templates,
            links,
            items,
            markers,
            cal,
            profile: Mutex::new(profile),
        })
    }

    fn world<'a>(&'a self, seed: &'a Value, now: f64) -> bundles::World<'a> {
        bundles::World {
            templates: &self.templates,
            links: &self.links,
            items: &self.items,
            markers: &self.markers,
            cal: &self.cal,
            settings: &self.settings,
            seed,
            now,
            no_popups: self.cfg.no_popups,
        }
    }

    /// A line in `gs.log`.
    pub fn log(&self, text: &str) {
        self.logs.log("gs.log", text);
    }
}

/// `('127.0.0.1', 51000)` like Python's address tuple.
fn py_addr(a: SocketAddr) -> String {
    format!("('{}', {})", a.ip(), a.port())
}

/// Listens on the game port and serves every connection.
pub async fn run(game: Game) -> std::io::Result<()> {
    let game = Arc::new(game);
    let listener = TcpListener::bind((game.cfg.address.as_str(), game.cfg.port)).await?;
    game.log(&format!(
        "gs listening {}:{} | LibreOrbit {} (login not ported yet)",
        game.cfg.address,
        game.cfg.port,
        env!("CARGO_PKG_VERSION")
    ));
    loop {
        let (sock, addr) = listener.accept().await?;
        let game = game.clone();
        tokio::spawn(async move { Session::new(game, sock, addr).run().await });
    }
}

/// A client connection.
struct Session {
    game: Arc<Game>,
    sock: TcpStream,
    addr: SocketAddr,
    secrets: Vec<String>,
    menu: templates::MenuState,
    /// The login bundle was sent; the ready bundle answers the next `ReadyRequest`.
    map_pending: bool,
    ready_n: u32,
}

impl Session {
    fn new(game: Arc<Game>, sock: TcpStream, addr: SocketAddr) -> Session {
        let _ = sock.set_nodelay(true);
        Session {
            game,
            sock,
            addr,
            secrets: Vec::new(),
            menu: templates::MenuState::default(),
            map_pending: false,
            ready_n: 0,
        }
    }

    async fn run(mut self) {
        self.game.log(&format!("connect {}", py_addr(self.addr)));
        let mut framer = Framer::new(true);
        let mut buf = vec![0u8; 65536];
        loop {
            let n = match self.sock.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) => {
                    self.game.log(&format!("   connection closed: {e}"));
                    break;
                }
            };
            let packets = match framer.feed(&buf[..n]) {
                Ok(p) => p,
                Err(e) => {
                    self.game.log(&format!("error\n{e}"));
                    break;
                }
            };
            for p in packets {
                if let Err(e) = self.on_packet(&p).await {
                    self.game.log(&format!("   connection closed: {e}"));
                    self.game.log(&format!("disconnect {}", py_addr(self.addr)));
                    return;
                }
            }
        }
        self.game.log(&format!("disconnect {}", py_addr(self.addr)));
    }

    async fn on_packet(&mut self, payload: &[u8]) -> std::io::Result<()> {
        let d = self.game.spec.decode(payload);
        let name = d.name.as_str();
        if name == "LoginRequest" {
            secrets_of(&d.msg, &mut self.secrets);
        }
        if !UNLOGGED.contains(&name) {
            let err = d
                .error
                .as_ref()
                .map(|e| format!(" [ERROR {e}]"))
                .unwrap_or_default();
            let mid = d.id.map_or_else(|| "None".into(), |i| i.to_string());
            self.game.log(&format!(
                "<- {name}({mid}) {}{err}",
                fmt(&redact(&d.msg, &self.secrets), true, 300)
            ));
        }
        match name {
            "VersionRequest" => {
                let version = d.msg.str("version").unwrap_or_default().to_string();
                self.send(
                    vec![
                        Struct::new("VersionCommand")
                            .with("version", version)
                            .with("equal", true),
                    ],
                    false,
                )
                .await
            }
            "KeepAlive" => self.send(vec![Struct::new("StayinAlive")], true).await,
            "LoginRequest" => {
                let msgs = {
                    let game = self.game.clone();
                    let mut p = game.profile.lock().unwrap_or_else(|e| e.into_inner());
                    self.revive_on_login(&mut p);
                    let seed = game
                        .templates
                        .entry
                        .get("seed")
                        .cloned()
                        .unwrap_or(Value::Null);
                    let w = game.world(&seed, lo_core::pytime::now());
                    bundles::with_systems(
                        &w,
                        bundles::login_batch(&w, &mut p, &mut self.menu, "entry", true),
                    )
                };
                self.send_bundle(msgs, "login bundle").await?;
                self.map_pending = true;
                Ok(())
            }
            "ReadyRequest" if self.map_pending => {
                self.map_pending = false;
                self.ready_n += 1;
                let msgs = {
                    let game = self.game.clone();
                    let mut p = game.profile.lock().unwrap_or_else(|e| e.into_inner());
                    let seed = game
                        .templates
                        .entry
                        .get("seed")
                        .cloned()
                        .unwrap_or(Value::Null);
                    let w = game.world(&seed, lo_core::pytime::now());
                    bundles::login_batch(&w, &mut p, &mut self.menu, "ready", true)
                };
                self.send_bundle(msgs, "ready bundle").await
            }
            _ => {
                if !UNLOGGED.contains(&name) {
                    self.game.log("   LibreOrbit: not ported yet, no reply");
                }
                Ok(())
            }
        }
    }

    /// If the player left the game with the ship destroyed: repaired at the station (`revive_on_login`).
    fn revive_on_login(&self, p: &mut Profile) {
        let g = &p.d["ship"];
        if g.get("hitPoints").and_then(Value::as_f64).unwrap_or(1.0) > 0.0 {
            return;
        }
        let game = &self.game;
        let fac = p.d["pilot"].get("factionId").and_then(Value::as_i64);
        let mut map_id = p.d["position"]["mapId"].as_i64().unwrap_or(0);
        let mut st = game.templates.station(&game.links, map_id, fac);
        if st
            .as_ref()
            .is_none_or(|s| s.get("type").and_then(Value::as_i64) != fac)
        {
            let level = p.d["pilot"]
                .get("level")
                .and_then(Value::as_i64)
                .unwrap_or(1);
            map_id = fac
                .and_then(|f| game.links.x8_home(f, level, map_id))
                .unwrap_or(match fac {
                    Some(1) => 1,
                    Some(3) => 9,
                    _ => 5,
                });
            p.d["position"]["mapId"] = map_id.into();
            st = game.templates.station(&game.links, map_id, fac);
        }
        if let Some(st) = st {
            p.d["position"]["x"] = st["posX"].clone();
            p.d["position"]["y"] = st["posY"].clone();
        }
        if let Some(pos) = p.d["position"].as_object_mut() {
            pos.shift_remove("at_portal");
        }
        let ship = p.d["ship"].as_object_mut().expect("ship");
        let clamp = |v: Option<&Value>, max: Option<&Value>| {
            let v = v.and_then(Value::as_f64).unwrap_or(0.0).trunc() as i64;
            let m = max.and_then(Value::as_f64).map_or(v, |m| m as i64);
            0.max(v.min(m))
        };
        let hp = clamp(
            ship.get("hitMax").or(Some(&Value::from(1))),
            ship.get("hitMax"),
        );
        let sh = clamp(
            ship.get("shieldMax").or(Some(&Value::from(0))),
            ship.get("shieldMax"),
        );
        ship.insert("hitPoints".into(), hp.into());
        ship.insert("shield".into(), sh.into());
        p.mark_dirty();
        let name = game.links.name(map_id);
        game.log(&format!(
            "   left with the ship destroyed: repaired at the {name} station"
        ));
    }

    /// Sends a bundle and logs it as one line (`send(msgs, note)`).
    async fn send_bundle(&mut self, msgs: Vec<Msg>, note: &str) -> std::io::Result<()> {
        let mut data = Vec::new();
        let n = msgs.len();
        for (name, fields) in msgs {
            let mut f = fields;
            if let Some(o) = f.as_object_mut() {
                o.shift_remove("_m");
            }
            let encoded = Struct::from_json(&f, Some(&name))
                .map_err(|e| e.to_string())
                .and_then(|mut s| {
                    s.class = name.clone();
                    self.game.spec.encode(&s).map_err(|e| e.to_string())
                })
                .and_then(|p| frame(&p).map_err(|e| e.to_string()));
            match encoded {
                Ok(fr) => data.extend(fr),
                Err(e) => self
                    .game
                    .log(&format!("   could not encode, not sent: {name}: {e}")),
            }
        }
        self.sock.write_all(&data).await?;
        self.game
            .log(&format!("-> {note} ({n} messages, {} bytes)", data.len()));
        Ok(())
    }

    /// Encodes and sends; a message that cannot be encoded is logged and skipped (`encode` in `game.py`).
    async fn send(&mut self, msgs: Vec<Struct>, quiet: bool) -> std::io::Result<()> {
        let mut data = Vec::new();
        for m in &msgs {
            match self
                .game
                .spec
                .encode(m)
                .map_err(|e| e.to_string())
                .and_then(|p| frame(&p).map_err(|e| e.to_string()))
            {
                Ok(f) => data.extend(f),
                Err(e) => self
                    .game
                    .log(&format!("   could not encode, not sent: {}: {e}", m.class)),
            }
        }
        self.sock.write_all(&data).await?;
        if !quiet {
            for m in &msgs {
                self.game
                    .log(&format!("-> {} {}", m.class, fmt(m, true, 160)));
            }
        }
        Ok(())
    }
}
