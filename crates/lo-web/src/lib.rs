#![recursion_limit = "256"]
//! The web login of the DarkOrbit Unity client (OpenOrbit's `web.py`).
//!
//! HTTP on 8090 (the embedded browser's first page) and 80 (the client's `unityApi`): `links.php`, `login.php`,
//! `maps.php`, `setLang.php`, the Gate and Galaxy Trek guides, local info pages and the CDN files. TCP 843, 9339 and
//! 8082 are accepted and only logged (the client opens them; nothing answers there on the official servers either).
//!
//! Sessions: the main player's key (`--session`, default `localtest`) or, with multi-client, an extra key the game server
//! knows (`GET /local/session` on the game server). An unknown extra key never falls back to the main player.

mod content;
mod server;

use lo_core::{GameData, Layout, LogDir};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

pub use server::run;

/// Settings, with the defaults and command-line names of `web.py`.
#[derive(Debug, Clone)]
pub struct Config {
    /// Folder of the log files (first positional argument).
    pub log_dir: PathBuf,
    /// `--profile`: the save slot's profile (default: the active slot's).
    pub profile: Option<PathBuf>,
    /// `--session`: the main player's session key.
    pub session: String,
    /// `--address`: the address listened on and written into the client's URLs.
    pub address: String,
    /// `--chat`: the chat server given to the client.
    pub chat: String,
    /// `--uid`: guest of a friend server: the id the server gave.
    pub uid: Option<i64>,
    /// `--remote`: the game server written into maps.php (and asked for extra sessions).
    pub remote: String,
    /// HTTP ports; the first page is served on `portal_port`.
    pub http_ports: Vec<u16>,
    pub portal_port: u16,
    /// Ports that are only accepted and logged.
    pub tcp_ports: Vec<u16>,
}

impl Config {
    pub fn new(log_dir: impl Into<PathBuf>) -> Config {
        Config {
            log_dir: log_dir.into(),
            profile: None,
            session: "localtest".into(),
            address: "127.0.0.1".into(),
            chat: "127.0.0.1:7771".into(),
            uid: None,
            remote: "127.0.0.1:8081".into(),
            http_ports: vec![80, 8090],
            portal_port: 8090,
            tcp_ports: vec![843, 9339, 8082],
        }
    }

    /// Reads `web.py`'s arguments: `<log_folder> [--profile=] [--session=] [--address=] [--chat=] [--uid=] [--remote=]`,
    /// plus LibreOrbit's `--http-ports=80,8090`, `--portal-port=8090` and `--tcp-ports=843,9339,8082`.
    pub fn from_args(args: &[String]) -> Result<Config, String> {
        let log_dir = args
            .iter()
            .find(|a| !a.starts_with("--"))
            .ok_or("usage: web <log_folder> [--profile=<path>] …")?;
        let mut c = Config::new(log_dir);
        let ports = |v: &str| -> Result<Vec<u16>, String> {
            v.split(',')
                .filter(|p| !p.is_empty())
                .map(|p| p.trim().parse().map_err(|_| format!("invalid port {p:?}")))
                .collect()
        };
        for a in args.iter().filter(|a| a.starts_with("--")) {
            let (k, v) = a[2..].split_once('=').unwrap_or((&a[2..], ""));
            match k {
                "profile" => c.profile = Some(PathBuf::from(v)),
                "session" => c.session = v.to_string(),
                "address" => c.address = v.to_string(),
                "chat" => c.chat = v.to_string(),
                "uid" if !v.is_empty() => {
                    c.uid = Some(v.parse().map_err(|_| format!("invalid --uid {v:?}"))?)
                }
                "remote" => c.remote = v.to_string(),
                "http-ports" => c.http_ports = ports(v)?,
                "portal-port" => {
                    c.portal_port = v
                        .parse()
                        .map_err(|_| format!("invalid --portal-port {v:?}"))?
                }
                "tcp-ports" => c.tcp_ports = ports(v)?,
                _ => {}
            }
        }
        Ok(c)
    }
}

/// How long an extra session the game server confirmed (or refused) is kept.
const EXTRA_OK: Duration = Duration::from_secs(60);
const EXTRA_MISS: Duration = Duration::from_secs(2);
/// An extra client whose browser reached internalWebGL this recently and has not logged in yet (see `request_session`).
const BROWSER_SEC: Duration = Duration::from_secs(10);

/// The web server's state.
pub struct Web {
    pub cfg: Config,
    pub layout: Layout,
    pub data: GameData,
    pub logs: LogDir,
    extra: Mutex<HashMap<String, (Instant, Option<Extra>)>>,
    browser: Mutex<HashMap<String, Instant>>,
    map_ids: OnceLock<Vec<u64>>,
    galaxy_trek: OnceLock<Value>,
    cdn_missing: Mutex<std::collections::HashSet<String>>,
    write_lock: Mutex<()>,
}

/// An extra client's session, as the game server gives it.
#[derive(Debug, Clone)]
struct Extra {
    uid: i64,
    route: String,
}

impl Web {
    pub fn new(cfg: Config, layout: Layout) -> std::io::Result<Web> {
        let logs = LogDir::new(&cfg.log_dir)?;
        let data = GameData::from_env(&layout.data);
        Ok(Web {
            cfg,
            layout,
            data,
            logs,
            extra: Mutex::default(),
            browser: Mutex::default(),
            map_ids: OnceLock::new(),
            galaxy_trek: OnceLock::new(),
            cdn_missing: Mutex::default(),
            write_lock: Mutex::new(()),
        })
    }

    /// `http://<address>/`
    pub fn base(&self) -> String {
        format!("http://{}/", self.cfg.address)
    }

    /// The main player's profile.
    pub fn main_profile(&self) -> PathBuf {
        self.cfg
            .profile
            .clone()
            .unwrap_or_else(|| self.layout.profile_path(None))
    }

    fn main_session(&self) -> content::Session {
        (self.cfg.session.clone(), self.main_profile(), self.cfg.uid)
    }

    /// The request's session: an empty or the main key -> the main player; another key -> the extra client the game server
    /// knows, `None` if it does not (it never falls back to the main player).
    pub fn session(&self, sid: Option<&str>) -> Option<content::Session> {
        match sid {
            None | Some("") => Some(self.main_session()),
            Some(s) if s == self.cfg.session => Some(self.main_session()),
            Some(s) => self
                .extra_session(s)
                .map(|e| (s.to_string(), PathBuf::from(e.route), Some(e.uid))),
        }
    }

    fn extra_session(&self, sid: &str) -> Option<Extra> {
        let valid = (16..=64).contains(&sid.len())
            && sid
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-');
        if sid == self.cfg.session || self.cfg.uid.is_some() || !valid {
            return None;
        }
        let now = Instant::now();
        if let Some((t, v)) = self
            .extra
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(sid)
        {
            if now - *t < if v.is_some() { EXTRA_OK } else { EXTRA_MISS } {
                return v.clone();
            }
        }
        let url = format!(
            "http://{}/local/session?session={}",
            self.cfg.remote,
            encode_component(sid)
        );
        // a direct connection (the system proxy is not used to reach the game server)
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(3))
            .try_proxy_from_env(false)
            .build();
        let v = agent
            .get(&url)
            .call()
            .ok()
            .and_then(|r| r.into_string().ok())
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .and_then(|d| {
                let uid = match d.get("uid")? {
                    Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64))?,
                    Value::String(s) => s.trim().parse().ok()?,
                    _ => return None,
                };
                let route = match d.get("route")? {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                Some(Extra { uid, route })
            });
        self.extra
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(sid.to_string(), (now, v.clone()));
        v
    }

    /// The session of login.php and setLang.php: the `PHPSESSID` cookie, or else the extra client whose browser came to
    /// internalWebGL in the last seconds and has not logged in (clients sharing a browser folder sometimes read an empty cookie).
    fn request_session(&self, cookie: Option<String>, entry: bool) -> Option<String> {
        let now = Instant::now();
        let mut browser = self.browser.lock().unwrap_or_else(|e| e.into_inner());
        let sid = match cookie.filter(|c| !c.is_empty()) {
            Some(c) => Some(c),
            None => browser
                .iter()
                .filter(|(_, t)| now - **t < BROWSER_SEC)
                .max_by_key(|(_, t)| **t)
                .map(|(s, _)| s.clone()),
        };
        if entry {
            if let Some(s) = &sid {
                browser.remove(s);
            }
        }
        sid
    }

    fn browser_came(&self, sid: &str) {
        self.browser
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(sid.to_string(), Instant::now());
    }

    /// setLang.php: the language chosen on the login screen, kept in `web_settings.json` next to the profile.
    fn set_lang(&self, body: &[u8], sid: Option<&str>) {
        let text = String::from_utf8_lossy(body);
        let lang = form_urlencoded::parse(text.as_bytes())
            .find(|(k, v)| k == "lang" && !v.is_empty())
            .map(|(_, v)| v.trim().to_lowercase())
            .unwrap_or_default();
        let o = sid
            .filter(|s| !s.is_empty())
            .and_then(|s| self.session(Some(s)));
        if !content::valid_lang(&lang) || (o.is_none() && sid.is_some_and(|s| !s.is_empty())) {
            return;
        }
        let profile = o.map_or_else(|| self.main_profile(), |o| o.1);
        let mut d: Map<String, Value> = content::web_settings(&profile);
        d.insert("lang".into(), Value::String(lang));
        let p = content::web_settings_path(&profile);
        let tmp = p.with_extension("json.tmp");
        let _guard = self.write_lock.lock().unwrap_or_else(|e| e.into_inner());
        let text = lo_core::pyjson::dumps_with(&Value::Object(d), false, Some(1));
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, &p);
        }
    }

    fn map_ids(&self) -> &[u64] {
        self.map_ids.get_or_init(|| {
            match std::fs::read_to_string(self.layout.data.join("maps-config.xml")) {
                Ok(t) => content::parse_map_ids(&t),
                Err(_) => (1..30).collect(),
            }
        })
    }

    fn galaxy_trek(&self) -> &Value {
        self.galaxy_trek.get_or_init(|| {
            std::fs::read_to_string(self.layout.data.join("extra_events.json"))
                .ok()
                .and_then(|t| serde_json::from_str::<Value>(&t).ok())
                .and_then(|d| d.get("galaxy_trek").cloned())
                .unwrap_or(Value::Object(Map::new()))
        })
    }
}

/// Percent-encoding of a query value (`urllib.parse.quote`: letters, digits and `_.-~/` stay).
fn encode_component(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b'.' | b'-' | b'~' | b'/' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}
