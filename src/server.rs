//! Local HTTP server: the UI (static files built into the binary) and the JSON API.
//!
//! Protection against DNS rebinding and CSRF, as before:
//! - a loopback-bound server only accepts requests whose Host header names a loopback address,
//! - if the browser sends Origin (every POST, every cross-origin request), it must match exactly
//!   how we were addressed,
//! - POST bodies must be application/json, which forces a CORS preflight that is never approved.

use std::io::Read;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

use serde_json::{json, Value};
use tiny_http::{Header, Method, Request, Response, Server};

use crate::core::{self, address, baud_code, int, int_or, in_range, truthy, u16_field, MAX_COILS};
use crate::discovery;
use crate::jobs::{is_job_id, run_net_discovery, run_scan, Store};
use crate::transport::{list_serial_ports, open_connection};

pub const MAX_BODY_BYTES: usize = 1024 * 1024;
const WORKERS: usize = 8;
const LOOPBACK_HOSTS: [&str; 3] = ["127.0.0.1", "localhost", "::1"];

const STATIC: [(&str, &str, &str); 4] = [
    ("/index.html", "text/html; charset=utf-8", include_str!("../static/index.html")),
    ("/app.js", "text/javascript; charset=utf-8", include_str!("../static/app.js")),
    ("/i18n.js", "text/javascript; charset=utf-8", include_str!("../static/i18n.js")),
    ("/style.css", "text/css; charset=utf-8", include_str!("../static/style.css")),
];

struct App {
    scans: Store,
    searches: Store,
    loopback: bool,
    /// File with UI preferences (language, theme); None = not remembered.
    prefs: Option<PathBuf>,
    prefs_lock: Mutex<()>,
}

/// Preferences the UI may save, with their allowed values.
const PREFS: [(&str, &[&str]); 2] = [("lang", &["en", "pl"]), ("theme", &["light", "dark"])];

/// Error that becomes a JSON response with this status.
struct HttpError(u16, String);

impl From<String> for HttpError {
    fn from(msg: String) -> Self {
        HttpError(400, msg) // the message must reach the UI in readable form
    }
}

type Reply = Result<(u16, Value), HttpError>;

/// Start the server in background threads; returns the bound address.
///
/// UI preferences are kept in `prefs` (a JSON file), not only in the browser's localStorage:
/// the window uses a new random port on every launch, and localStorage belongs to the origin
/// including the port, so it started empty each time and the language was never remembered.
pub fn start(addr: &str, port: u16, prefs: Option<PathBuf>) -> Result<SocketAddr, String> {
    let server = Server::http((addr, port)).map_err(|e| format!("Cannot listen on {addr}:{port}: {e}"))?;
    let bound = server.server_addr().to_ip().ok_or("Server has no IP address")?;
    let app = Arc::new(App {
        scans: Store::default(),
        searches: Store::default(),
        loopback: bound.ip().is_loopback(),
        prefs,
        prefs_lock: Mutex::new(()),
    });
    let server = Arc::new(server);
    for _ in 0..WORKERS {
        let (server, app) = (server.clone(), app.clone());
        thread::spawn(move || {
            while let Ok(req) = server.recv() {
                handle(&app, req);
            }
        });
    }
    Ok(bound)
}

fn header<'a>(req: &'a Request, name: &'static str) -> Option<&'a str> {
    req.headers().iter().find(|h| h.field.equiv(name)).map(|h| h.value.as_str())
}

fn allowed(app: &App, req: &Request) -> bool {
    let host = header(req, "Host").unwrap_or("").trim();
    let hostname = match host.rsplit_once(':') {
        Some((h, port)) if port.chars().all(|c| c.is_ascii_digit()) => h,
        _ => host,
    };
    let hostname = hostname.trim_matches(['[', ']']).to_lowercase();
    if app.loopback && !LOOPBACK_HOSTS.contains(&hostname.as_str()) {
        return false;
    }
    match header(req, "Origin") {
        Some(origin) => origin.trim().eq_ignore_ascii_case(&format!("http://{host}")),
        None => true,
    }
}

fn respond(req: Request, status: u16, content_type: &str, body: Vec<u8>) {
    let ct = Header::from_bytes(&b"Content-Type"[..], content_type.as_bytes()).expect("valid header");
    let nosniff = Header::from_bytes(&b"X-Content-Type-Options"[..], &b"nosniff"[..]).expect("valid header");
    let _ = req.respond(Response::from_data(body).with_status_code(status).with_header(ct).with_header(nosniff));
}

fn respond_json(req: Request, status: u16, value: &Value) {
    respond(req, status, "application/json; charset=utf-8", value.to_string().into_bytes());
}

fn handle(app: &App, mut req: Request) {
    if !allowed(app, &req) {
        return respond_json(req, 403, &json!({"error": "Forbidden: unexpected Host/Origin."}));
    }
    let path = req.url().split(['?', '#']).next().unwrap_or("/").to_string();
    let reply = match req.method() {
        Method::Get => match get(app, &path) {
            Some(r) => r,
            None => return serve_static(app, req, &path),
        },
        Method::Post => read_json(&mut req).and_then(|body| post(app, &path, &body)),
        _ => Err(HttpError(405, "Method not allowed.".into())),
    };
    match reply {
        Ok((status, value)) => respond_json(req, status, &value),
        Err(HttpError(status, msg)) => respond_json(req, status, &json!({"error": msg})),
    }
}

fn load_prefs(app: &App) -> serde_json::Map<String, Value> {
    let saved = app.prefs.as_ref().and_then(|p| std::fs::read(p).ok()).and_then(|raw| serde_json::from_slice::<Value>(&raw).ok());
    let mut out = serde_json::Map::new();
    // Only known keys with allowed values reach the page (the file could be edited by hand).
    for (key, allowed) in PREFS {
        if let Some(v) = saved.as_ref().and_then(|s| s.get(key)).and_then(Value::as_str).filter(|v| allowed.contains(v)) {
            out.insert(key.into(), json!(v));
        }
    }
    out
}

fn save_pref(app: &App, params: &Value) -> Reply {
    let key = params.get("key").and_then(Value::as_str).unwrap_or("");
    let value = params.get("value").and_then(Value::as_str).unwrap_or("");
    let Some((_, allowed)) = PREFS.iter().find(|(k, _)| *k == key) else {
        return Err(HttpError(400, format!("Unknown preference: {key}.")));
    };
    if !allowed.contains(&value) {
        return Err(HttpError(400, format!("Invalid value for {key}: {value}.")));
    }
    let Some(path) = &app.prefs else { return Ok((200, json!({"ok": false}))) };
    let _guard = app.prefs_lock.lock().unwrap_or_else(|e| e.into_inner());
    let mut prefs = load_prefs(app);
    prefs.insert(key.into(), json!(value));
    let write = || -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, Value::Object(prefs).to_string())?;
        std::fs::rename(&tmp, path)
    };
    write().map_err(|e| HttpError(500, format!("Cannot save preferences to {}: {e}", path.display())))?;
    Ok((200, json!({"ok": true})))
}

fn serve_static(app: &App, req: Request, path: &str) {
    if path == "/prefs.js" {
        // Loaded before i18n.js, so the saved language is known before the first render.
        let body = format!("window.SAVED_PREFS = {};\n", Value::Object(load_prefs(app)));
        return respond(req, 200, "text/javascript; charset=utf-8", body.into_bytes());
    }
    let path = if path == "/" { "/index.html" } else { path };
    match STATIC.iter().find(|(p, _, _)| *p == path) {
        Some((_, ct, body)) => respond(req, 200, ct, body.as_bytes().to_vec()),
        None => respond(req, 404, "text/plain; charset=utf-8", b"Not Found".to_vec()),
    }
}

fn read_json(req: &mut Request) -> Result<Value, HttpError> {
    let content_type = header(req, "Content-Type").unwrap_or("").split(';').next().unwrap_or("").trim().to_lowercase();
    if content_type != "application/json" {
        return Err(HttpError(415, "Content-Type must be application/json.".into()));
    }
    if req.body_length().unwrap_or(0) > MAX_BODY_BYTES {
        return Err(HttpError(413, "Request body too large.".into()));
    }
    let mut raw = Vec::new();
    req.as_reader().take(MAX_BODY_BYTES as u64 + 1).read_to_end(&mut raw).map_err(|e| HttpError(400, e.to_string()))?;
    if raw.len() > MAX_BODY_BYTES {
        return Err(HttpError(413, "Request body too large.".into()));
    }
    if raw.iter().all(u8::is_ascii_whitespace) {
        return Ok(json!({}));
    }
    let value: Value = serde_json::from_slice(&raw).map_err(|e| HttpError(400, format!("Invalid JSON body: {e}")))?;
    if !value.is_object() {
        return Err(HttpError(400, "JSON body must be an object.".into()));
    }
    Ok(value)
}

/// API GET routes; None = not an API path (static file).
fn get(app: &App, path: &str) -> Option<Reply> {
    let job_status = |store: &Store, id: &str, missing: &str| match store.get(id).filter(|_| is_job_id(id)) {
        Some(job) => Ok((200, job.status_json())),
        None => Err(HttpError(404, missing.into())),
    };
    Some(match path {
        "/api/ports" => list_serial_ports().map(|ports| (200, json!({"ports": ports}))).map_err(HttpError::from),
        "/api/network-discovery/default-subnet" => Ok((200, json!({"subnet": discovery::default_subnet()}))),
        _ => {
            if let Some(id) = path.strip_prefix("/api/scan/") {
                job_status(&app.scans, id, "Scan job not found.")
            } else if let Some(id) = path.strip_prefix("/api/network-discovery/") {
                job_status(&app.searches, id, "Search job not found.")
            } else if path.starts_with("/api/") {
                Err(HttpError(404, "Unknown API path.".into()))
            } else {
                return None;
            }
        }
    })
}

fn connection(params: &Value) -> (Value, bool, u64) {
    let conn = params.get("connection").cloned().unwrap_or(json!({}));
    let mbap = truthy(conn.get("mbap"));
    let timeout = int_or(&conn, "timeoutMs", 300).unwrap_or(300).clamp(1, 60_000) as u64;
    (conn, mbap, timeout)
}

fn write_reply((request, resp, success): core::WriteResult) -> Reply {
    let response = if resp.is_empty() { Value::Null } else { json!(core::hex(&resp)) };
    Ok((200, json!({"request": core::hex(&request), "response": response, "success": success})))
}

fn cancel(store: &Store, id: &str) -> Reply {
    if let Some(job) = store.get(id) {
        job.cancelled.store(true, std::sync::atomic::Ordering::SeqCst);
    }
    Ok((200, json!({"ok": true})))
}

fn post(app: &App, path: &str, params: &Value) -> Reply {
    let started = |r: Result<String, (u16, String)>| match r {
        Ok(id) => Ok((200, json!({"jobId": id}))),
        Err((status, msg)) => Err(HttpError(status, msg)),
    };
    match path {
        "/api/prefs" => return save_pref(app, params),
        "/api/scan" => return started(app.scans.start(run_scan, params.clone())),
        "/api/network-discovery" => return started(app.searches.start(run_net_discovery, params.clone())),
        _ => {}
    }
    if let Some(id) = path.strip_prefix("/api/scan/").and_then(|r| r.strip_suffix("/cancel")).filter(|id| is_job_id(id)) {
        return cancel(&app.scans, id);
    }
    if let Some(id) =
        path.strip_prefix("/api/network-discovery/").and_then(|r| r.strip_suffix("/cancel")).filter(|id| is_job_id(id))
    {
        return cancel(&app.searches, id);
    }

    let (conn_params, mbap, timeout) = connection(params);
    match path {
        "/api/read-temperature" => {
            let slave = address(params, "slaveId", 1)?;
            let mut conn = open_connection(&conn_params)?;
            let results = core::read_r4dcb08(conn.as_mut(), slave, mbap, timeout)?;
            Ok((200, json!({"results": results})))
        }
        "/api/set-address" => {
            // 0 = broadcast, allowed on purpose for a single device with an unknown address.
            let old = address(params, "oldAddress", 0)?;
            let new = address(params, "newAddress", 1)?;
            let mut conn = open_connection(&conn_params)?;
            write_reply(core::write_single_register(conn.as_mut(), old, 254, new as u16, mbap, timeout))
        }
        "/api/set-baudrate" => {
            let slave = address(params, "slaveId", 1)?;
            let code = baud_code(int(params, "newBaudRate")?)?;
            let mut conn = open_connection(&conn_params)?;
            write_reply(core::write_single_register(conn.as_mut(), slave, 255, code, mbap, timeout))
        }
        "/api/coils/read" => {
            let slave = address(params, "slaveId", 1)?;
            let start = u16_field(params, "start", 0)?;
            let quantity = in_range(int_or(params, "quantity", 8)?, "quantity", 1, MAX_COILS as i64)? as usize;
            let mut conn = open_connection(&conn_params)?;
            let coils = core::read_coils(conn.as_mut(), slave, start, quantity, mbap, timeout)?;
            Ok((200, json!({"coils": coils})))
        }
        "/api/coils/write" => {
            let slave = address(params, "slaveId", 1)?;
            let coil = Ok(int(params, "coil")?)
                .and_then(|c| in_range(c, "coil", 0, 0xFFFF).map_err(|_| "coil must be between 0 and 65535.".to_string()))?
                as u16;
            let mut conn = open_connection(&conn_params)?;
            write_reply(core::write_single_coil(conn.as_mut(), slave, coil, truthy(params.get("on")), mbap, timeout))
        }
        "/api/coils/write-multiple" => {
            let slave = address(params, "slaveId", 1)?;
            let start = u16_field(params, "start", 0)?;
            let values: Vec<bool> =
                params.get("values").and_then(Value::as_array).map(|a| a.iter().map(|v| truthy(Some(v))).collect()).unwrap_or_default();
            let mut conn = open_connection(&conn_params)?;
            write_reply(core::write_multiple_coils(conn.as_mut(), slave, start, &values, mbap, timeout)?)
        }
        _ => Err(HttpError(404, "Unknown API path.".into())),
    }
}

#[cfg(test)]
mod tests {
    //! The real server on an ephemeral loopback port: DNS rebinding, CSRF, request size and
    //! input validation. No hardware or network scanning is exercised.
    use super::*;
    use std::io::Write;
    use std::net::TcpStream;
    use std::sync::OnceLock;
    use std::time::Duration;

    fn port() -> u16 {
        static ADDR: OnceLock<SocketAddr> = OnceLock::new();
        ADDR.get_or_init(|| start("127.0.0.1", 0, Some(prefs_file())).unwrap()).port()
    }

    fn prefs_file() -> PathBuf {
        std::env::temp_dir().join(format!("modbus-dashboard-test-{}", std::process::id())).join("prefs.json")
    }

    /// (status, body) of a raw HTTP/1.1 request; `headers` replace the defaults with the same name.
    fn request(method: &str, path: &str, body: Option<&str>, headers: &[(&str, &str)]) -> (u16, String) {
        let port = port();
        let host = format!("127.0.0.1:{port}");
        let mut all: Vec<(String, String)> = vec![("Host".into(), host)];
        if let Some(b) = body {
            all.push(("Content-Type".into(), "application/json".into()));
            all.push(("Content-Length".into(), b.len().to_string()));
        }
        for (k, v) in headers {
            all.retain(|(name, _)| !name.eq_ignore_ascii_case(k));
            all.push((k.to_string(), v.to_string()));
        }
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut text = format!("{method} {path} HTTP/1.1\r\nConnection: close\r\n");
        for (k, v) in &all {
            text += &format!("{k}: {v}\r\n");
        }
        text += "\r\n";
        text += body.unwrap_or("");
        s.write_all(text.as_bytes()).unwrap();
        let mut raw = Vec::new();
        let _ = s.read_to_end(&mut raw);
        let raw = String::from_utf8_lossy(&raw).into_owned();
        let status = raw.split_whitespace().nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
        let body = raw.split_once("\r\n\r\n").map(|(_, b)| b.to_string()).unwrap_or_default();
        (status, body)
    }

    fn error(body: &str) -> String {
        serde_json::from_str::<Value>(body).unwrap()["error"].as_str().unwrap_or("").to_string()
    }

    #[test]
    fn index_and_api_work_for_loopback_host() {
        let (status, body) = request("GET", "/", None, &[]);
        assert_eq!(status, 200);
        assert!(body.contains("<html"));
        let (status, body) = request("GET", "/api/ports", None, &[]);
        assert_eq!(status, 200, "{body}");
        assert!(body.contains("ports"));
    }

    #[test]
    fn same_origin_post_reaches_the_handler() {
        let origin = format!("http://127.0.0.1:{}", port());
        let (status, body) =
            request("POST", "/api/read-temperature", Some(r#"{"connection":{},"slaveId":1}"#), &[("Origin", &origin)]);
        assert_eq!(status, 400);
        assert!(error(&body).contains("transport"), "{body}");
    }

    #[test]
    fn foreign_host_is_rejected() {
        assert_eq!(request("GET", "/", None, &[("Host", "evil.example.com")]).0, 403);
        assert_eq!(request("GET", "/api/ports", None, &[("Host", "evil.example.com:80")]).0, 403);
    }

    #[test]
    fn foreign_and_null_origin_are_rejected() {
        let body = r#"{"connection":{},"oldAddress":1,"newAddress":2}"#;
        assert_eq!(request("POST", "/api/set-address", Some(body), &[("Origin", "http://attacker.example")]).0, 403);
        assert_eq!(request("POST", "/api/scan", Some("{}"), &[("Origin", "null")]).0, 403);
    }

    #[test]
    fn post_without_json_content_type_is_rejected() {
        let body = r#"{"connection":{},"slaveId":1,"newBaudRate":9600}"#;
        let (status, body) = request("POST", "/api/set-baudrate", Some(body), &[("Content-Type", "text/plain")]);
        assert_eq!(status, 415);
        assert!(error(&body).contains("application/json"));
    }

    #[test]
    fn oversized_body_is_rejected() {
        let len = (MAX_BODY_BYTES + 1).to_string();
        assert_eq!(request("POST", "/api/scan", Some("{}"), &[("Content-Length", &len)]).0, 413);
    }

    #[test]
    fn malformed_json_is_400() {
        let (status, body) = request("POST", "/api/scan", Some("{not json"), &[]);
        assert_eq!(status, 400);
        assert!(error(&body).contains("Invalid JSON"));
        assert_eq!(request("POST", "/api/scan", Some("[1,2,3]"), &[]).0, 400);
    }

    #[test]
    fn static_path_traversal_is_blocked() {
        for path in ["/../Cargo.toml", "/static/../../Cargo.toml", "/..%2f..%2fCargo.toml", "/src/main.rs"] {
            let (status, body) = request("GET", path, None, &[]);
            assert!(matches!(status, 400 | 403 | 404), "{path}: {status}");
            assert!(!body.contains("[package]"), "{path}");
        }
    }

    #[test]
    fn unknown_api_path_and_job_ids() {
        assert_eq!(request("POST", "/api/does-not-exist", Some("{}"), &[]).0, 404);
        assert_eq!(request("GET", &format!("/api/scan/{}", "0".repeat(32)), None, &[]).0, 404);
        assert_eq!(request("GET", "/api/scan/not-a-job-id", None, &[]).0, 404);
    }

    #[test]
    fn preferences_survive_in_a_file() {
        let (_, js) = request("GET", "/prefs.js", None, &[]);
        assert!(js.starts_with("window.SAVED_PREFS = {"), "{js}");
        assert_eq!(request("POST", "/api/prefs", Some(r#"{"key":"lang","value":"en"}"#), &[]).0, 200);
        let (_, js) = request("GET", "/prefs.js", None, &[]);
        assert!(js.contains(r#""lang":"en""#), "{js}");
        assert_eq!(request("POST", "/api/prefs", Some(r#"{"key":"lang","value":"<script>"}"#), &[]).0, 400);
        assert_eq!(request("POST", "/api/prefs", Some(r#"{"key":"evil","value":"x"}"#), &[]).0, 400);
        let _ = std::fs::remove_dir_all(prefs_file().parent().unwrap());
    }

    #[test]
    fn addresses_are_validated_not_masked() {
        // 256 used to be masked to 0 = broadcast: "change address" would hit every device.
        let body = r#"{"connection":{"transport":"tcp","ip":"127.0.0.1","tcpPort":1},"oldAddress":256,"newAddress":5}"#;
        let (status, body) = request("POST", "/api/set-address", Some(body), &[]);
        assert_eq!(status, 400);
        assert!(error(&body).contains("oldAddress must be between 0 and 247"), "{body}");
        let body = r#"{"connection":{},"oldAddress":1,"newAddress":0}"#;
        assert!(error(&request("POST", "/api/set-address", Some(body), &[]).1).contains("newAddress"));
        let body = r#"{"connection":{},"slaveId":null}"#;
        assert!(error(&request("POST", "/api/read-temperature", Some(body), &[]).1).contains("slaveId"));
    }
}
