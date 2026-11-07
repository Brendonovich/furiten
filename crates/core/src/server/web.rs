//! Web access: the desktop's core serving the HTTP API (docs/contract.md, "HTTP API") to browsers on the network,
//! so a phone can use this library directly instead of through Recall Cloud. Each route is a protocol method
//! underneath, so its answers are exactly the local core's. When the host bundles the web app it's served from the
//! same origin, and `/v1/events` streams `changed` as server-sent events.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Component, Path};
use std::sync::mpsc;
use std::sync::{Arc, Weak};
use std::time::Duration;

use serde_json::{Value, json};
use tiny_http::{Header, Method, Request, Response, StatusCode};

use super::Core;
use crate::protocol::WebStatus;
use crate::{Error, Result};

pub const DEFAULT_BIND: &str = "0.0.0.0";
pub const DEFAULT_PORT: u16 = 8787;
const MAX_BODY: u64 = 8 << 20;
const PING: Duration = Duration::from_secs(25);

/// Web access as kept in `$RECALL_HOME/web.json` (mode 0600): on across restarts until turned off, with the same
/// token each time so signed-in browsers stay signed in.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct Config {
    pub enabled: bool,
    pub bind: String,
    pub port: u16,
    pub token: String,
}

/// A running HTTP server. Dropping it stops accepting and waits (up to 2 s) until the port is free again, so it can
/// be bound straight away; open requests finish on their own threads.
pub(super) struct Running {
    server: Option<Arc<tiny_http::Server>>,
    accept: Option<std::thread::JoinHandle<()>>,
    addr: String,
    port: u16,
}

impl Drop for Running {
    fn drop(&mut self) {
        // Ending our accept loop drops the last handle on the server, which closes its listener once tiny_http's own
        // accept thread wakes. It wakes itself by connecting to the listen address; in case that doesn't arrive
        // (0.0.0.0, say), poke it on loopback until the port is released.
        if let Some(server) = self.server.take() {
            server.unblock();
        }
        if let Some(accept) = self.accept.take() {
            let _ = accept.join();
        }
        let started = std::time::Instant::now();
        while std::net::TcpListener::bind(&self.addr).is_err() && started.elapsed() < Duration::from_secs(2) {
            let loopback = std::net::SocketAddr::from(([127, 0, 0, 1], self.port));
            let _ = std::net::TcpStream::connect_timeout(&loopback, Duration::from_millis(100));
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

pub(super) fn load(home: &Path) -> Option<Config> {
    serde_json::from_slice(&std::fs::read(home.join("web.json")).ok()?).ok()
}

pub(super) fn save(home: &Path, config: &Config) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut file =
        std::fs::OpenOptions::new().create(true).write(true).truncate(true).mode(0o600).open(home.join("web.json"))?;
    file.write_all(serde_json::to_string_pretty(config)?.as_bytes())?;
    Ok(())
}

/// `rw_` and 32 Crockford base32 characters: 160 bits from the OS's randomness.
pub(super) fn new_token() -> Result<String> {
    const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let mut bytes = [0u8; 20];
    getrandom::getrandom(&mut bytes).map_err(|e| Error::internal(format!("no randomness for a token: {e}")))?;
    let bits = bytes.iter().fold(String::new(), |acc, b| acc + &format!("{b:08b}"));
    let chars = bits.as_bytes().chunks(5).map(|c| {
        let i = c.iter().fold(0usize, |acc, b| acc * 2 + usize::from(*b == b'1'));
        ALPHABET[i] as char
    });
    Ok(format!("rw_{}", chars.collect::<String>()))
}

pub(super) fn start(core: &Arc<Core>, config: &Config) -> Result<Running> {
    let addr = format!("{}:{}", config.bind, config.port);
    let server = tiny_http::Server::http(&addr)
        .map_err(|e| Error::new("web_failed", format!("couldn't listen on {addr}: {e}")))?;
    let server = Arc::new(server);
    let accept = server.clone();
    let me = Arc::downgrade(core);
    let accept = std::thread::Builder::new()
        .name("recall-web".into())
        .spawn(move || {
            for request in accept.incoming_requests() {
                let me = me.clone();
                std::thread::spawn(move || handle(me, request));
            }
        })
        .map_err(Error::internal)?;
    Ok(Running { server: Some(server), accept: Some(accept), addr, port: config.port })
}

/// What `web.status` reports, with the addresses to open on another device.
pub(super) fn status(config: Option<&Config>, running: bool, error: Option<String>) -> WebStatus {
    let bind = config.map_or(DEFAULT_BIND.to_string(), |c| c.bind.clone());
    let port = config.map_or(DEFAULT_PORT, |c| c.port);
    let urls = if config.is_some_and(|c| c.enabled) { urls(&bind, port) } else { Vec::new() };
    let token = config.map(|c| c.token.clone());
    let link = match (urls.first(), &token) {
        (Some(url), Some(token)) => Some(format!("{url}/#token={token}")),
        _ => None,
    };
    WebStatus { enabled: config.is_some_and(|c| c.enabled), running, bind, port, token, urls, link, error }
}

/// Tailscale's name and IP (reachable from anywhere on the tailnet), then the LAN's address and `.local` name.
fn urls(bind: &str, port: u16) -> Vec<String> {
    if bind.starts_with("127.") || bind == "localhost" || bind == "::1" {
        return vec![format!("http://localhost:{port}")];
    }
    let mut hosts = Vec::new();
    if let Some((name, ip)) = tailscale() {
        hosts.extend(name);
        hosts.extend(ip);
    }
    if let Some(ip) = lan_ip() {
        hosts.push(ip.to_string());
    }
    if let Some(host) = host_name() {
        hosts.push(format!("{host}.local"));
    }
    hosts.dedup();
    hosts.into_iter().map(|h| format!("http://{h}:{port}")).collect()
}

/// This machine's address on the network it routes through (no packets are sent).
fn lan_ip() -> Option<std::net::IpAddr> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:9").ok()?;
    socket.local_addr().ok().map(|a| a.ip()).filter(|ip| !ip.is_loopback() && !ip.is_unspecified())
}

fn host_name() -> Option<String> {
    let out = std::process::Command::new("hostname").arg("-s").output().ok()?;
    let name = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (!name.is_empty()).then_some(name)
}

/// This machine's MagicDNS name and IPv4 address, when Tailscale is installed and up.
fn tailscale() -> Option<(Option<String>, Option<String>)> {
    const CLIS: [&str; 4] = [
        "tailscale",
        "/usr/local/bin/tailscale",
        "/opt/homebrew/bin/tailscale",
        "/Applications/Tailscale.app/Contents/MacOS/Tailscale",
    ];
    let out = CLIS.iter().find_map(|cli| {
        let out = std::process::Command::new(cli).args(["status", "--self", "--json"]).output().ok()?;
        out.status.success().then_some(out.stdout)
    })?;
    let status: Value = serde_json::from_slice(&out).ok()?;
    if status["BackendState"] != "Running" {
        return None;
    }
    let me = &status["Self"];
    let name = me["DNSName"].as_str().map(|n| n.trim_end_matches('.').to_string()).filter(|n| !n.is_empty());
    let ip = me["TailscaleIPs"].as_array()?.iter().filter_map(Value::as_str).find(|ip| !ip.contains(':'));
    Some((name, ip.map(str::to_string)))
}

fn handle(core: Weak<Core>, mut request: Request) {
    let Some(core) = core.upgrade() else { return };
    let url = request.url().to_string();
    let (path, query) = url.split_once('?').unwrap_or((&url, ""));
    let path = path.to_string();
    let query = parse_query(query);
    let method = request.method().clone();

    if method == Method::Options {
        return respond(request, Response::empty(204));
    }
    if path == "/health" {
        return json_response(request, 200, &json!({ "ok": true, "version": crate::VERSION }));
    }
    if !path.starts_with("/v1/") {
        return serve_static(&core, request, &path);
    }
    let Some(config) = load(&core.home).filter(|c| c.enabled) else {
        return error_response(request, &Error::new("unauthorized", "web access is off"));
    };
    let bearer = header(&request, "authorization").and_then(|h| h.strip_prefix("Bearer ").map(str::to_string));
    let token = bearer.or_else(|| (path == "/v1/events").then(|| query.get("token").cloned()).flatten());
    if !token.is_some_and(|t| constant_time_eq(t.trim().as_bytes(), config.token.as_bytes())) {
        return error_response(request, &Error::new("unauthorized", "missing or wrong token"));
    }
    if path == "/v1/events" && method == Method::Get {
        return events(&core, request);
    }

    let mut body = String::new();
    if request.body_length().is_some_and(|n| n as u64 > MAX_BODY)
        || request.as_reader().take(MAX_BODY).read_to_string(&mut body).is_err()
    {
        return error_response(request, &Error::invalid("the body must be UTF-8 JSON under 8 MiB"));
    }
    let body: Value = if body.trim().is_empty() {
        json!({})
    } else {
        match serde_json::from_str(&body) {
            Ok(v) => v,
            Err(e) => return error_response(request, &Error::invalid(format!("bad JSON: {e}"))),
        }
    };

    let result = route(&method, &path, &query, body).and_then(|(m, params, wrap)| {
        let value = core.dispatch(m, params)?;
        Ok(match wrap {
            Some(key) => json!({ key: value }),
            None => value,
        })
    });
    match result {
        Ok(value) => json_response(request, 200, &value),
        Err(e) => error_response(request, &e),
    }
}

/// The protocol method behind a route, its params, and the key the HTTP API wraps a list result in.
fn route(
    method: &Method,
    path: &str,
    query: &HashMap<String, String>,
    body: Value,
) -> Result<(&'static str, Value, Option<&'static str>)> {
    let parts: Vec<&str> = path.trim_start_matches("/v1/").split('/').collect();
    let with_id = |mut body: Value, key: &str, id: &str| {
        if let Value::Object(map) = &mut body {
            map.insert(key.to_string(), json!(id));
        }
        body
    };
    let deck_filter = || json!({ "deck_id": query.get("deck_id") });
    Ok(match (method, parts.as_slice()) {
        (Method::Get, ["decks"]) => ("decks.list", json!({}), Some("decks")),
        (Method::Post, ["decks"]) => ("decks.create", body, None),
        (Method::Patch, ["decks", id]) => ("decks.update", with_id(body, "id", id), None),
        (Method::Delete, ["decks", id]) => ("decks.delete", json!({ "id": id }), None),
        (Method::Get, ["cards"]) => ("cards.list", card_query(query)?, Some("cards")),
        (Method::Post, ["cards"]) => ("cards.create", body, None),
        (Method::Get, ["cards", id]) => ("cards.get", json!({ "id": id }), None),
        (Method::Patch, ["cards", id]) => ("cards.update", with_id(body, "id", id), None),
        (Method::Delete, ["cards", id]) => ("cards.delete", json!({ "id": id }), None),
        (Method::Post, ["cards", id, "reviews"]) => ("review.answer", with_id(body, "card_id", id), None),
        (Method::Get, ["review", "next"]) => ("review.next", deck_filter(), None),
        (Method::Get, ["stats"]) => ("stats", deck_filter(), None),
        (Method::Get, ["tags"]) => ("tags.list", deck_filter(), Some("tags")),
        _ => return Err(Error::not_found("route", &format!("{method} {path}"))),
    })
}

/// `/v1/cards`'s query string as `cards.list` params: numbers and flags parsed, the rest as given.
fn card_query(query: &HashMap<String, String>) -> Result<Value> {
    let mut params = serde_json::Map::new();
    for (key, value) in query {
        let value = match key.as_str() {
            "limit" | "offset" | "seed" => {
                json!(value.parse::<u32>().map_err(|_| Error::invalid(format!("{key} must be a whole number")))?)
            }
            "due" | "desc" => json!(value == "true"),
            "deck_id" | "q" | "tag" | "sort" => json!(value),
            _ => continue,
        };
        params.insert(key.clone(), value);
    }
    Ok(Value::Object(params))
}

/// `changed` as server-sent events, with a comment every 25 s to keep the connection (and its check) alive.
fn events(core: &Arc<Core>, request: Request) {
    let (tx, rx) = mpsc::channel();
    core.events.lock().unwrap_or_else(|e| e.into_inner()).push(tx);
    let mut out = request.into_writer();
    let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\n\
                Access-Control-Allow-Origin: *\r\nConnection: keep-alive\r\n\r\n: connected\n\n";
    if out.write_all(head.as_bytes()).and_then(|_| out.flush()).is_err() {
        return;
    }
    loop {
        let frame = match rx.recv_timeout(PING) {
            Ok(()) => {
                // Coalesce a burst of writes into one event.
                while rx.try_recv().is_ok() {}
                "data: changed\n\n"
            }
            Err(mpsc::RecvTimeoutError::Timeout) => ": ping\n\n",
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        };
        if out.write_all(frame.as_bytes()).and_then(|_| out.flush()).is_err() {
            return;
        }
    }
}

/// The web app's files, when the host bundles them; any other path gets `index.html`, for the app's routes.
fn serve_static(core: &Core, request: Request, path: &str) {
    let Some(dir) = core.web_host.get().cloned().flatten() else {
        return error_response(request, &Error::not_found("route", path));
    };
    let relative = Path::new(path.trim_start_matches('/'));
    let safe = relative.components().all(|c| matches!(c, Component::Normal(_)));
    let file = if safe && !path.ends_with('/') && dir.join(relative).is_file() {
        dir.join(relative)
    } else {
        dir.join("index.html")
    };
    match std::fs::File::open(&file) {
        Ok(f) => {
            let kind = content_type(&file);
            let cache = if file.ends_with("index.html") { "no-cache" } else { "public, max-age=3600" };
            let response = Response::from_file(f)
                .with_header(header_line("Content-Type", kind))
                .with_header(header_line("Cache-Control", cache));
            let _ = request.respond(response);
        }
        Err(_) => error_response(request, &Error::not_found("file", path)),
    }
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "webmanifest" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "woff2" => "font/woff2",
        "ico" => "image/x-icon",
        _ => "application/octet-stream",
    }
}

fn status_of(code: &str) -> u16 {
    match code {
        "invalid_params" | "bad_request" | "unknown_method" => 400,
        "unauthorized" => 401,
        "not_found" => 404,
        "conflict" => 409,
        _ => 500,
    }
}

fn error_response(request: Request, error: &Error) {
    json_response(request, status_of(&error.code), &json!({ "error": error }));
}

fn json_response(request: Request, status: u16, body: &Value) {
    let response = Response::from_string(body.to_string())
        .with_status_code(StatusCode(status))
        .with_header(header_line("Content-Type", "application/json"));
    respond(request, response);
}

fn respond<R: Read>(request: Request, response: Response<R>) {
    let response = response
        .with_header(header_line("Access-Control-Allow-Origin", "*"))
        .with_header(header_line("Access-Control-Allow-Headers", "authorization, content-type, x-recall-timezone"))
        .with_header(header_line("Access-Control-Allow-Methods", "GET, POST, PATCH, DELETE, OPTIONS"));
    let _ = request.respond(response);
}

fn header_line(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("a valid header")
}

fn header(request: &Request, name: &str) -> Option<String> {
    request
        .headers()
        .iter()
        .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name))
        .map(|h| h.value.as_str().to_string())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn parse_query(query: &str) -> HashMap<String, String> {
    query
        .split('&')
        .filter(|p| !p.is_empty())
        .filter_map(|pair| {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            Some((decode(k)?, decode(v)?))
        })
        .collect()
}

/// Percent-decoding, with `+` as a space.
fn decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' => {
                let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                i += 2;
            }
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8(out).ok()
}
