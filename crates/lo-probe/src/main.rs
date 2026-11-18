//! `lo-probe <host:port> <out.jsonl> [--session=<key>] [--steps=<list>] [--spec=<protocol_read.json>]`
//!
//! Plays a client on a game server and writes every message, sent (`->`) and received (`<-`), as one JSON line
//! `{"dir": "<-", "name": "...", "fields": {...}}` (the JSON form of `protocol.py`). Used to compare LibreOrbit with OpenOrbit.
//!
//! Steps (comma-separated, default `version,login,ready,keepalive`): `version`, `login`, `ready`, `keepalive`,
//! `move:<x>:<y>` (from the last known position), `wait:<seconds>`. After each step the replies are read until the server
//! has been quiet for 1.5 s after its first reply, or 15 s without any (`wait` reads for the whole time).

use lo_protocol::{Framer, Spec, Struct, VERSION_REQUEST_ID, frame};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

const QUIET: Duration = Duration::from_millis(1500);
/// How long the first reply to a step may take (OpenOrbit builds the login bundle in about 2 s).
const FIRST: Duration = Duration::from_secs(15);

struct Probe {
    spec: Spec,
    sock: TcpStream,
    framer: Framer,
    out: std::fs::File,
    pos: (i64, i64),
    first: bool,
}

impl Probe {
    fn record(&mut self, dir: &str, msg: &Struct, error: Option<&str>) {
        let mut line =
            serde_json::json!({"dir": dir, "name": msg.class, "fields": msg.to_json(true)});
        if let Some(e) = error {
            line["error"] = serde_json::json!(e);
        }
        writeln!(self.out, "{line}").expect("write");
    }

    fn send(&mut self, msg: Struct) {
        let payload = self.spec.encode(&msg).expect("encode");
        let bytes = if self.first {
            // the client's first frame (VersionRequest) has a 4-byte length
            let mut b = (payload.len() as u32).to_be_bytes().to_vec();
            b.extend(&payload);
            b
        } else {
            frame(&payload).expect("frame")
        };
        self.first = false;
        self.record("->", &msg, None);
        self.sock.write_all(&bytes).expect("send");
    }

    /// Reads replies for `total`, or until the server has been quiet for `QUIET` after its first reply (at most `FIRST` for that one).
    fn read(&mut self, total: Option<Duration>) {
        let start = Instant::now();
        let mut last: Option<Instant> = None;
        let mut buf = vec![0u8; 65536];
        loop {
            let now = Instant::now();
            let limit = match (total, last) {
                (Some(t), _) => start + t,
                (None, Some(l)) => l + QUIET,
                (None, None) => start + FIRST,
            };
            if now >= limit {
                break;
            }
            self.sock.set_read_timeout(Some(limit - now)).ok();
            match self.sock.read(&mut buf) {
                Ok(0) => {
                    eprintln!("connection closed by the server");
                    break;
                }
                Ok(n) => {
                    last = Some(Instant::now());
                    for p in self.framer.feed(&buf[..n]).expect("frame") {
                        let d = self.spec.decode(&p);
                        if d.name == "ShipInitializationCommand" {
                            if let (Some(x), Some(y)) = (d.msg.int("x"), d.msg.int("y")) {
                                self.pos = (x, y);
                            }
                        }
                        let mut m = d.msg.clone();
                        if m.class.is_empty() {
                            m.class = d.name.clone();
                        }
                        self.record("<-", &m, d.error.as_deref());
                    }
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(e) => {
                    eprintln!("read: {e}");
                    break;
                }
            }
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let pos: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    let opt = |k: &str| {
        args.iter()
            .find_map(|a| a.strip_prefix(&format!("--{k}=")).map(str::to_string))
    };
    if pos.len() < 2 {
        eprintln!(
            "usage: lo-probe <host:port> <out.jsonl> [--session=<key>] [--steps=version,login,ready] [--spec=<path>]"
        );
        std::process::exit(2);
    }
    let spec_path = opt("spec").unwrap_or_else(|| {
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/protocol_read.json").into()
    });
    let spec = Spec::load(&spec_path).expect("spec");
    assert_eq!(spec.id_of("VersionRequest"), Some(VERSION_REQUEST_ID));
    let session = opt("session").unwrap_or_else(|| "localtest".into());
    let steps = opt("steps").unwrap_or_else(|| "version,login,ready,keepalive".into());
    let sock = TcpStream::connect(pos[0].as_str()).expect("connect");
    sock.set_nodelay(true).ok();
    let mut p = Probe {
        spec,
        sock,
        framer: Framer::new(false),
        out: std::fs::File::create(pos[1]).expect("out"),
        pos: (0, 0),
        first: true,
    };
    for step in steps.split(',') {
        let parts: Vec<&str> = step.split(':').collect();
        match parts[0] {
            "version" => {
                p.send(Struct::new("VersionRequest").with("version", lo_protocol::CLIENT_HASH))
            }
            "login" => p.send(
                Struct::new("LoginRequest")
                    .with("userID", 1)
                    .with("factionID", 1)
                    .with("sessionID", session.as_str())
                    .with("version", lo_protocol::CLIENT_HASH)
                    .with("instanceId", 0)
                    .with("isMiniClient", false),
            ),
            "ready" => p.send(
                Struct::new("ReadyRequest")
                    .with("readyTypeStr", "MAP_LOADED")
                    .with("readyType", 0),
            ),
            "keepalive" => p.send(Struct::new("KeepAlive").with("MouseClick", false)),
            "move" => {
                let (x, y) = p.pos;
                let tx: i64 = parts.get(1).and_then(|v| v.parse().ok()).unwrap_or(x);
                let ty: i64 = parts.get(2).and_then(|v| v.parse().ok()).unwrap_or(y);
                p.send(
                    Struct::new("MoveRequest")
                        .with("positionX", x)
                        .with("positionY", y)
                        .with("targetX", tx)
                        .with("targetY", ty),
                );
            }
            "wait" => {
                let s: f64 = parts.get(1).and_then(|v| v.parse().ok()).unwrap_or(1.0);
                p.read(Some(Duration::from_secs_f64(s)));
                continue;
            }
            other => panic!("unknown step {other:?}"),
        }
        p.read(None);
    }
}
