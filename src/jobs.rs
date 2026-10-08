//! Background jobs (bus scan, network discovery) polled by the UI.

use std::collections::hash_map::RandomState;
use std::collections::HashMap;
use std::hash::BuildHasher;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::core::{self, int, int_or, in_range, truthy};
use crate::discovery;
use crate::transport::open_connection;

pub const JOB_TTL: Duration = Duration::from_secs(600); // finished jobs are dropped after this
pub const MAX_ACTIVE_JOBS: usize = 4; // concurrent jobs per type (guards against thread flooding)

#[derive(Default)]
pub struct JobState {
    pub done: bool,
    pub error: Option<String>,
    pub progress: (usize, usize, String),
    pub found: Vec<Value>,
}

pub struct Job {
    pub id: String,
    created: Instant,
    pub cancelled: AtomicBool,
    pub state: Mutex<JobState>,
}

impl Job {
    fn new() -> Job {
        Job {
            id: new_id(),
            created: Instant::now(),
            cancelled: AtomicBool::new(false),
            state: Mutex::new(JobState { progress: (0, 1, "Starting...".into()), ..Default::default() }),
        }
    }

    pub fn with<R>(&self, f: impl FnOnce(&mut JobState) -> R) -> R {
        f(&mut self.state.lock().unwrap_or_else(|e| e.into_inner()))
    }

    fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub fn status_json(&self) -> Value {
        self.with(|s| {
            json!({
                "done": s.done,
                "progress": {"done": s.progress.0, "total": s.progress.1, "status": s.progress.2},
                "found": s.found,
                "error": s.error,
            })
        })
    }
}

/// 32 hex characters; RandomState is seeded from the OS random source.
fn new_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let state = RandomState::new();
    format!("{:016x}{:016x}", state.hash_one((n, 1u8)), state.hash_one((n, 2u8)))
}

pub fn is_job_id(s: &str) -> bool {
    s.len() == 32 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub type Runner = fn(&Job, &Value) -> Result<(), String>;

#[derive(Default)]
pub struct Store {
    jobs: Mutex<HashMap<String, Arc<Job>>>,
}

impl Store {
    /// Start `runner` in a thread; Err = (HTTP status, message) when too many jobs run.
    pub fn start(&self, runner: Runner, params: Value) -> Result<String, (u16, String)> {
        let job = {
            let mut jobs = self.jobs.lock().unwrap_or_else(|e| e.into_inner());
            jobs.retain(|_, j| !(j.with(|s| s.done) && j.created.elapsed() > JOB_TTL));
            if jobs.values().filter(|j| !j.with(|s| s.done)).count() >= MAX_ACTIVE_JOBS {
                return Err((
                    429,
                    format!("Too many jobs running at once (limit {MAX_ACTIVE_JOBS}). Wait or cancel one."),
                ));
            }
            let job = Arc::new(Job::new());
            jobs.insert(job.id.clone(), job.clone());
            job
        };
        let id = job.id.clone();
        thread::spawn(move || {
            // An error (or a bug) must reach the UI instead of dying silently in the thread.
            let result = catch_unwind(AssertUnwindSafe(|| runner(&job, &params)))
                .unwrap_or_else(|_| Err("Internal error in the background job.".into()));
            job.with(|s| {
                if let Err(e) = result {
                    s.error = Some(e);
                }
                s.done = true;
            });
        });
        Ok(id)
    }

    pub fn get(&self, id: &str) -> Option<Arc<Job>> {
        self.jobs.lock().unwrap_or_else(|e| e.into_inner()).get(id).cloned()
    }
}

// ===================== bus scan =====================

pub fn run_scan(job: &Job, params: &Value) -> Result<(), String> {
    let conn_base = params.get("connection").cloned().unwrap_or(json!({}));
    let transport = conn_base.get("transport").and_then(Value::as_str).unwrap_or("").to_string();
    let mbap = truthy(conn_base.get("mbap"));
    let timeout_ms = in_range(int_or(&conn_base, "timeoutMs", 300)?, "timeoutMs", 1, 60_000)? as u64;

    let start_addr = in_range(int(params, "startAddress")?, "startAddress", 0, 255)?;
    let end_addr = in_range(int(params, "endAddress")?, "endAddress", 0, 255)?;
    let function_code = in_range(int(params, "functionCode")?, "functionCode", 1, 127)? as u8;
    let register = in_range(int(params, "register")?, "register", 0, 0xFFFF)? as u16;
    let quantity = in_range(int(params, "quantity")?, "quantity", 1, 2000)? as u16;
    let discover = truthy(params.get("discoverAddress"));

    let mut bauds: Vec<Option<i64>> = Vec::new();
    if transport == "serial" {
        for b in params.get("bauds").and_then(Value::as_array).into_iter().flatten() {
            bauds.push(Some(int(&json!({"baud": b}), "baud")?));
        }
    }
    if bauds.is_empty() {
        bauds.push(None);
    }

    let addrs = (end_addr - start_addr + 1).max(1) as usize;
    job.with(|s| s.progress.1 = (bauds.len() * addrs).max(1));
    let mut done = 0;
    let mut open_errors: Vec<String> = Vec::new();

    for baud in &bauds {
        if job.cancelled() {
            break;
        }
        let mut conn_params = conn_base.clone();
        conn_params["timeoutMs"] = json!(timeout_ms);
        if let Some(b) = baud {
            conn_params["baud"] = json!(b);
        }
        let label = if transport == "tcp" {
            format!(
                "{}:{}",
                conn_params.get("ip").and_then(Value::as_str).unwrap_or(""),
                int_or(&conn_params, "tcpPort", 502).unwrap_or(502)
            )
        } else {
            format!("{} @ {}", conn_params.get("port").and_then(Value::as_str).unwrap_or(""), baud.unwrap_or(0))
        };
        job.with(|s| s.progress.2 = format!("Connecting: {label}"));

        let mut conn = match open_connection(&conn_params) {
            Ok(c) => c,
            Err(e) => {
                // Used to be skipped silently: a busy port looked like "no devices found".
                open_errors.push(e);
                done += addrs;
                job.with(|s| s.progress.0 = done);
                continue;
            }
        };

        if discover {
            let request = if mbap { core::mbap_read(0, 0x03, 0x4000, 1) } else { core::rtu_request(0, 0x03, 0x4000, 1) };
            let resp = conn.send_request(&request, timeout_ms, mbap).unwrap_or_default();
            let ok = if mbap { resp.len() >= 9 } else { resp.len() >= 5 && core::check_crc(&resp) };
            done += addrs; // progress moves per baud rate, not only at the very end
            job.with(|s| {
                s.progress.0 = done;
                if ok {
                    let addr = if mbap { resp[6] } else { resp[0] };
                    s.found.push(json!({"baud": baud, "slaveId": addr, "response": core::hex(&resp)}));
                }
            });
            continue;
        }

        for addr in start_addr..=end_addr {
            if job.cancelled() {
                break;
            }
            done += 1;
            let addr = addr as u8;
            job.with(|s| {
                s.progress.0 = done;
                s.progress.2 = format!("{label}, address {addr}");
            });
            let request = if mbap {
                core::mbap_read(addr, function_code, register, quantity)
            } else {
                core::rtu_request(addr, function_code, register, quantity)
            };
            let resp = conn.send_request(&request, timeout_ms, mbap).unwrap_or_default();
            let ok = if mbap {
                core::mbap_ok(&resp, addr)
            } else {
                resp.len() >= 5 && resp[0] == addr && core::check_crc(&resp)
            };
            if ok {
                job.with(|s| s.found.push(json!({"baud": baud, "slaveId": addr, "response": core::hex(&resp)})));
            }
        }
    }

    if !open_errors.is_empty() {
        let attempts = bauds.len();
        if open_errors.len() == attempts {
            return Err(open_errors.pop().unwrap_or_default());
        }
        let note = format!("{} of {attempts} connection attempts failed: {}", open_errors.len(), open_errors[0]);
        job.with(|s| s.progress.2 = note);
    }
    Ok(())
}

// ===================== network discovery =====================

pub fn run_net_discovery(job: &Job, params: &Value) -> Result<(), String> {
    let use_hf = params.get("hfBroadcast").map(|v| truthy(Some(v))).unwrap_or(true);
    let ports: Vec<u16> = match params.get("ports").and_then(Value::as_array) {
        Some(list) if !list.is_empty() => list
            .iter()
            .map(|p| int(&json!({"port": p}), "port").and_then(|p| in_range(p, "port", 1, 65535)).map(|p| p as u16))
            .collect::<Result<_, _>>()?,
        _ => discovery::DEFAULT_PORTS.to_vec(),
    };
    let subnet = params.get("subnet").and_then(Value::as_str).filter(|s| !s.is_empty()).map(String::from).unwrap_or_else(discovery::default_subnet);
    let timeout = Duration::from_millis(in_range(int_or(params, "timeoutMs", 200)?, "timeoutMs", 1, 10_000)? as u64);

    if use_hf {
        job.with(|s| s.progress.2 = "Broadcast HF-A11ASSISTHREAD (Hi-Flying/Elfin)...".into());
        let found = discovery::discover_hf(Duration::from_secs(2));
        job.with(|s| s.found.extend(found));
    }
    if job.cancelled() || ports.is_empty() {
        return Ok(());
    }
    let on_progress = |done: usize, total: usize| {
        job.with(|s| s.progress = (done, total, format!("Scanning {subnet}: {done}/{total} hosts")));
    };
    let found = discovery::scan_tcp(&subnet, &ports, timeout, &on_progress, &|| job.cancelled())?;
    job.with(|s| {
        for hit in found {
            // A converter that answered the broadcast gets the open port instead of a second row.
            match s.found.iter_mut().find(|f| f["ip"] == hit["ip"]) {
                Some(existing) => existing["port"] = hit["port"].clone(),
                None => s.found.push(hit),
            }
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wait(job: &Job) {
        for _ in 0..500 {
            if job.with(|s| s.done) {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("job did not finish");
    }

    #[test]
    fn scan_reports_unopenable_port() {
        let store = Store::default();
        let params = json!({
            "connection": {"transport": "serial", "port": "/dev/does-not-exist", "timeoutMs": 50},
            "bauds": [9600, 19200], "startAddress": 1, "endAddress": 3,
            "functionCode": 3, "register": 0, "quantity": 1,
        });
        let id = store.start(run_scan, params).unwrap();
        let job = store.get(&id).unwrap();
        wait(&job);
        let status = job.status_json();
        assert!(status["error"].as_str().unwrap().contains("/dev/does-not-exist"), "{status}");
        assert_eq!(status["progress"]["done"], 6);
    }

    #[test]
    fn scan_rejects_out_of_range_addresses() {
        let job = Job::new();
        let params = json!({"connection": {"transport": "tcp", "ip": "127.0.0.1"}, "startAddress": 1,
                            "endAddress": 300, "functionCode": 3, "register": 0, "quantity": 1});
        assert!(run_scan(&job, &params).unwrap_err().contains("endAddress"));
    }

    #[test]
    fn ids() {
        let a = new_id();
        assert!(is_job_id(&a));
        assert_ne!(a, new_id());
        assert!(!is_job_id("not-a-job-id"));
    }
}
