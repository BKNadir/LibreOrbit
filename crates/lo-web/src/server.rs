//! HTTP and TCP listeners, request routing.

use crate::{Web, content};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::header::{CONTENT_TYPE, COOKIE, HeaderValue};
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use lo_core::{pyjson, pyrepr};
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, TcpStream};

/// Largest request body read (the client only posts short forms).
const MAX_BODY: usize = 1 << 20;

const CDN_OFFICIAL: &str = "https://darkorbit-22.bpsecure.com/";

/// Starts every listener and runs until the process ends. A port that cannot be opened is logged (errors.log)
/// and the others still run.
pub async fn run(web: Web) -> std::io::Result<()> {
    let web = Arc::new(web);
    let mut tasks = Vec::new();
    for &port in &web.cfg.http_ports {
        tasks.push(tokio::spawn(serve_http(web.clone(), port)));
    }
    for &port in &web.cfg.tcp_ports {
        tasks.push(tokio::spawn(serve_tcp(web.clone(), port)));
    }
    web.logs.log("server.log", "web started");
    for t in tasks {
        let _ = t.await;
    }
    // every listener failed or ended: stay up like web.py (the panel watches the process)
    std::future::pending::<()>().await;
    Ok(())
}

/// The address to listen on -> (address, only local connections). On macOS a port below 1024 can only be opened on the
/// wildcard address by a normal user: 0.0.0.0, filtered to this computer.
fn listen_address(address: &str, port: u16) -> (String, bool) {
    if cfg!(target_os = "macos")
        && port < 1024
        && (address == "127.0.0.1" || address == "localhost")
    {
        return ("0.0.0.0".into(), true);
    }
    (address.to_string(), false)
}

fn is_local(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => v.is_loopback(),
        IpAddr::V6(v) => v.is_loopback() || v.to_ipv4_mapped().is_some_and(|v| v.is_loopback()),
    }
}

async fn bind(web: &Web, kind: &str, port: u16) -> Option<(TcpListener, bool)> {
    let (address, filtered) = listen_address(&web.cfg.address, port);
    match TcpListener::bind((address.as_str(), port)).await {
        Ok(l) => Some((l, filtered)),
        Err(e) => {
            web.logs.log("errors.log", &format!("{kind} {port}: {e}"));
            None
        }
    }
}

async fn serve_http(web: Arc<Web>, port: u16) {
    let Some((listener, filtered)) = bind(&web, "http", port).await else {
        return;
    };
    loop {
        let Ok((stream, peer)) = listener.accept().await else {
            continue;
        };
        if filtered && !is_local(peer.ip()) {
            continue;
        }
        let web = web.clone();
        tokio::spawn(async move {
            let svc = hyper::service::service_fn(move |req| handle(web.clone(), port, peer, req));
            let conn = hyper::server::conn::http1::Builder::new()
                .serve_connection(TokioIo::new(stream), svc);
            let _ = conn.await;
        });
    }
}

/// 843, 9339, 8082: accepted and logged, nothing is answered.
async fn serve_tcp(web: Arc<Web>, port: u16) {
    let Some((listener, filtered)) = bind(&web, "tcp", port).await else {
        return;
    };
    loop {
        let Ok((mut stream, peer)) = listener.accept().await else {
            continue;
        };
        if filtered && !is_local(peer.ip()) {
            continue;
        }
        web.logs.log(
            "tcp.log",
            &format!("tcp {port} connect from {}", py_addr(peer)),
        );
        let web = web.clone();
        tokio::spawn(async move { read_and_log(&web, port, &mut stream).await });
    }
}

async fn read_and_log(web: &Web, port: u16, stream: &mut TcpStream) {
    let mut buf = vec![0u8; 65536];
    loop {
        match tokio::time::timeout(std::time::Duration::from_secs(300), stream.read(&mut buf)).await
        {
            Ok(Ok(0)) => break,
            Ok(Ok(n)) => {
                let hex: String = buf[..n.min(256)]
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect();
                web.logs
                    .log("tcp.log", &format!("tcp {port} recv {n}: {hex}"));
            }
            Ok(Err(e)) => {
                web.logs.log("tcp.log", &format!("tcp {port} closed: {e}"));
                break;
            }
            Err(_) => {
                web.logs
                    .log("tcp.log", &format!("tcp {port} closed: timed out"));
                break;
            }
        }
    }
}

/// `('127.0.0.1', 51000)` like Python's address tuple.
fn py_addr(a: SocketAddr) -> String {
    format!("('{}', {})", a.ip(), a.port())
}

fn cookie(req_cookie: Option<&str>, name: &str) -> Option<String> {
    req_cookie?.split(';').find_map(|part| {
        let (k, v) = part.trim().split_once('=').unwrap_or((part.trim(), ""));
        (k == name).then(|| v.trim().to_string())
    })
}

fn first_query_value(query: &str, key: &str) -> Option<String> {
    form_urlencoded::parse(query.as_bytes())
        .find(|(k, v)| k == key && !v.is_empty())
        .map(|(_, v)| v.into_owned())
}

type Reply = (StatusCode, &'static str, Vec<u8>);

async fn handle(
    web: Arc<Web>,
    port: u16,
    _peer: SocketAddr,
    req: Request<Incoming>,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    if req.method() == Method::OPTIONS {
        // CORS preflight of the embedded browser's page requests
        let mut r = Response::new(Full::new(Bytes::new()));
        *r.status_mut() = StatusCode::NO_CONTENT;
        let h = r.headers_mut();
        h.insert("Access-Control-Allow-Origin", HeaderValue::from_static("*"));
        h.insert(
            "Access-Control-Allow-Methods",
            HeaderValue::from_static("GET, POST, PUT, HEAD, OPTIONS"),
        );
        h.insert(
            "Access-Control-Allow-Headers",
            HeaderValue::from_static("*"),
        );
        // web.py also sends "Content-Length: 0"; hyper leaves it out of a 204 as HTTP requires (browsers accept both)
        return Ok(r);
    }
    let method = req.method().clone();
    let name = method_name(&method);
    if !matches!(
        method,
        Method::GET | Method::POST | Method::PUT | Method::HEAD
    ) {
        let mut r = Response::new(Full::new(Bytes::from_static(b"Unsupported method")));
        *r.status_mut() = StatusCode::NOT_IMPLEMENTED;
        return Ok(r);
    }
    let mut target = req
        .uri()
        .path_and_query()
        .map_or_else(|| req.uri().to_string(), |p| p.as_str().to_string());
    if target.starts_with("//") {
        // like Python's http.server (open-redirect protection): a leading "//" is reduced to a single "/"
        target = format!("/{}", target.trim_start_matches('/'));
    }
    let cookie_header = req
        .headers()
        .get(COOKIE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let body = match req.into_body().collect().await {
        Ok(b) => b.to_bytes(),
        Err(_) => Bytes::new(),
    };
    let body = if body.len() > MAX_BODY {
        body.slice(..MAX_BODY)
    } else {
        body
    };

    let web2 = web.clone();
    let (target2, cookie2, body2) = (target.clone(), cookie_header.clone(), body.clone());
    // the handlers read files and may ask the game server or the CDN: off the async threads
    let (status, ctype, out) = tokio::task::spawn_blocking(move || {
        route(&web2, port, name, &target2, cookie2.as_deref(), &body2)
    })
    .await
    .unwrap_or((StatusCode::INTERNAL_SERVER_ERROR, "text/plain", Vec::new()));

    let shown_cookie = cookie_header
        .as_deref()
        .map_or("None".to_string(), str::to_string);
    web.logs.log(
        "http.log",
        &format!(
            "{} :{port} {target} cookie={shown_cookie} body={} -> {}, {} bytes {}",
            name,
            pyrepr::bytes(&body[..body.len().min(300)]),
            status.as_u16(),
            out.len(),
            pyrepr::bytes(&out[..out.len().min(160)])
        ),
    );
    let mut r = Response::new(Full::new(Bytes::from(out)));
    *r.status_mut() = status;
    let h = r.headers_mut();
    h.insert(CONTENT_TYPE, HeaderValue::from_static(ctype));
    // the client's GetPost rejects the login without this header
    h.insert(
        "X-Custom-Header",
        HeaderValue::from_static("DarkOrbitUnity"),
    );
    h.insert("Access-Control-Allow-Origin", HeaderValue::from_static("*"));
    Ok(r)
}

fn method_name(m: &Method) -> &'static str {
    match *m {
        Method::POST => "POST",
        Method::PUT => "PUT",
        Method::HEAD => "HEAD",
        _ => "GET",
    }
}

fn route(
    web: &Web,
    port: u16,
    name: &str,
    target: &str,
    cookie_header: Option<&str>,
    body: &[u8],
) -> Reply {
    let path = target.split('#').next().unwrap_or_default();
    let (base, query) = path.split_once('?').unwrap_or((path, ""));
    let ok = |ctype: &'static str, s: String| (StatusCode::OK, ctype, s.into_bytes());
    let xml = "text/xml;charset=UTF-8";
    match base {
        "/spacemap/xml/maps.php" => ok("text/xml", content::maps_xml(web)),
        "/unityApi/events/spaceSearch.php" | "//unityApi/events/spaceSearch.php" => {
            ok(xml, content::galaxy_trek_planets(web))
        }
        "/unityApi/events/ssRewards.php" | "//unityApi/events/ssRewards.php" => {
            ok(xml, content::galaxy_trek_rewards(web))
        }
        "/unityApi/guide/ggRewards.php" => ok(xml, content::gate_rewards_xml(web)),
        "/unityApi/links.php" => ok("application/json", pyjson::dumps(&content::links(web))),
        "/unityApi/login.php" => {
            let sid = web.request_session(cookie(cookie_header, "PHPSESSID"), true);
            let o = web.session(sid.as_deref());
            ok(
                "application/json",
                pyjson::dumps(&content::login(web, o.as_ref())),
            )
        }
        "/unityApi/setLang.php" => {
            let sid = web.request_session(cookie(cookie_header, "PHPSESSID"), false);
            web.set_lang(body, sid.as_deref());
            ok("text/plain", String::new())
        }
        // server selection: an empty list makes the client stop quietly (a listed server would send it to the official site)
        "/unityApi/instance.php" => ok("application/json", "{\"data\":[]}".into()),
        // the client's event-tracking notification
        "/unityApi/agent.php" => ok("text/plain", String::new()),
        "/" if port == web.cfg.portal_port => {
            let sid = first_query_value(query, "session");
            ok(
                "text/html; charset=utf-8",
                content::redirect_page(web, sid.as_deref()),
            )
        }
        b if b.starts_with("/info/") => ok(
            "text/html; charset=utf-8",
            content::info_page(web, &b["/info/".len()..]),
        ),
        _ if path.contains("indexInternal.es") => {
            let o = web.session(cookie(cookie_header, "dosid").as_deref());
            if let Some(o) = &o {
                if o.0 != web.cfg.session {
                    web.browser_came(&o.0);
                }
            }
            ok(
                "text/plain",
                o.map_or_else(|| "gecersiz".to_string(), |o| o.0),
            )
        }
        _ => match cdn_file(web, base) {
            Some((ctype, bytes)) => (StatusCode::OK, ctype, bytes),
            None => {
                web.logs.log(
                    "unknown.log",
                    &format!(
                        "{name} :{port} {target} body={}",
                        pyrepr::bytes(&body[..body.len().min(500)])
                    ),
                );
                (StatusCode::NOT_FOUND, "text/plain", Vec::new())
            }
        },
    }
}

fn cdn_type(path: &std::path::Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("xml") => "application/xml",
        Some("php") => "text/xml;charset=UTF-8",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("json") => "application/json",
        Some("webm") => "video/webm",
        _ => "application/octet-stream",
    }
}

/// A file of the local CDN copy; game images (`do_img/`) missing locally are downloaded once from the official CDN.
fn cdn_file(web: &Web, base: &str) -> Option<(&'static str, Vec<u8>)> {
    let mut rel = base.replace('\\', "/").trim_start_matches('/').to_string();
    while rel.contains("//") {
        rel = rel.replace("//", "/");
    }
    // no way out of the CDN folder (".." segments, drive letters)
    if rel.is_empty() || rel.split('/').any(|s| s == ".." || s.contains(':')) {
        return None;
    }
    let p = rel
        .split('/')
        .fold(web.layout.cdn.clone(), |p, s| p.join(s));
    if !p.is_file() && !cdn_fetch(web, &rel, &p) {
        return None;
    }
    Some((cdn_type(&p), std::fs::read(&p).ok()?))
}

fn cdn_fetch(web: &Web, rel: &str, p: &std::path::Path) -> bool {
    if !rel.starts_with("do_img/")
        || web
            .cdn_missing
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(rel)
    {
        return false;
    }
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(8))
        .build();
    let res = agent
        .get(&format!("{CDN_OFFICIAL}{rel}"))
        .set("User-Agent", "Mozilla/5.0")
        .call();
    let data = res.map_err(|e| e.to_string()).and_then(|r| {
        let mut v = Vec::new();
        std::io::Read::read_to_end(&mut r.into_reader(), &mut v)
            .map(|_| v)
            .map_err(|e| e.to_string())
    });
    match data {
        Ok(v) => {
            if let Some(d) = p.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let part = p.with_extension(format!(
                "{}.part",
                p.extension().and_then(|e| e.to_str()).unwrap_or("")
            ));
            std::fs::write(&part, v).is_ok() && std::fs::rename(&part, p).is_ok()
        }
        Err(e) => {
            web.cdn_missing
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(rel.to_string());
            web.logs
                .log("unknown.log", &format!("CDN download failed: {rel} ({e})"));
            false
        }
    }
}
