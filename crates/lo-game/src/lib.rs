//! The game server (OpenOrbit's `game.py`, TCP 8081).
//!
//! Port status: the connection, the framing, the logs and the first exchanges (`VersionRequest`, `KeepAlive`) behave like
//! `game.py`. The login bundles and the game systems come next (see `docs/portage-jeu.md`); a request the server does not
//! handle yet is logged as such and left unanswered.

pub mod event_appearances;
pub mod event_calendar;
pub mod login_markers;
pub mod map_layout;
pub mod map_links;

use lo_core::LogDir;
use lo_protocol::display::{fmt, redact, secrets_of};
use lo_protocol::{Framer, Spec, Struct, frame};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
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
}

impl Game {
    pub fn new(cfg: Config, spec: Spec) -> std::io::Result<Game> {
        let logs = LogDir::new(&cfg.log_dir)?;
        Ok(Game { cfg, spec, logs })
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
}

impl Session {
    fn new(game: Arc<Game>, sock: TcpStream, addr: SocketAddr) -> Session {
        let _ = sock.set_nodelay(true);
        Session {
            game,
            sock,
            addr,
            secrets: Vec::new(),
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
            _ => {
                if !UNLOGGED.contains(&name) {
                    self.game.log("   LibreOrbit: not ported yet, no reply");
                }
                Ok(())
            }
        }
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
