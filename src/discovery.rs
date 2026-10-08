//! Discovery of RS485<->Ethernet/WiFi converters on the local network - the equivalent of
//! "Search"/"Device Management" in tools like VirCOM (USR-IOT).
//!
//! Two independent methods, run together:
//!
//! 1. UDP broadcast "HF-A11ASSISTHREAD" to port 48899 - a real, verified protocol used by the
//!    Hi-Flying/HF-LPB100 module family (including the Elfin EW10/EW11/EW12 converters). The
//!    device replies with plain text "IP, MAC, ModelID".
//! 2. A TCP port sweep of the whole subnet (default 502/8899/4196/23 - typical transparent-mode/
//!    Modbus TCP ports across vendors, including USR-IOT). It cannot identify the brand/model,
//!    but it finds ANY converter with one of those ports open.
//!
//! The UDP protocol used by VirCOM (USR-IOT) is proprietary and could not be verified without a
//! packet capture from a real device - hence two methods instead of guessing bytes.

use std::collections::HashSet;
use std::net::{Ipv4Addr, SocketAddr, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

pub const HF_BROADCAST_PORT: u16 = 48899;
pub const HF_BROADCAST_PAYLOAD: &[u8] = b"HF-A11ASSISTHREAD";
pub const DEFAULT_PORTS: [u16; 4] = [502, 8899, 4196, 23];
pub const MAX_HOSTS: usize = 1024;
const WORKERS: usize = 100;

/// Best-effort local IPv4 address (connecting a UDP socket sends nothing).
pub fn local_ip() -> Ipv4Addr {
    let probe = || -> Option<Ipv4Addr> {
        let s = UdpSocket::bind("0.0.0.0:0").ok()?;
        s.connect("8.8.8.8:80").ok()?;
        match s.local_addr().ok()?.ip() {
            std::net::IpAddr::V4(ip) => Some(ip),
            _ => None,
        }
    };
    probe().unwrap_or(Ipv4Addr::LOCALHOST)
}

pub fn default_subnet() -> String {
    let [a, b, c, _] = local_ip().octets();
    format!("{a}.{b}.{c}.0/24")
}

/// Broadcast HF-A11ASSISTHREAD (Hi-Flying/Elfin/HF-LPB100) and collect the replies.
pub fn discover_hf(timeout: Duration) -> Vec<Value> {
    let mut found = Vec::new();
    let Ok(sock) = UdpSocket::bind("0.0.0.0:0") else { return found };
    let _ = sock.set_broadcast(true);
    let _ = sock.set_read_timeout(Some(Duration::from_millis(300)));
    if sock.send_to(HF_BROADCAST_PAYLOAD, ("255.255.255.255", HF_BROADCAST_PORT)).is_err() {
        return found;
    }
    let mut seen = HashSet::new();
    let deadline = Instant::now() + timeout;
    let mut buf = [0u8; 512];
    while Instant::now() < deadline {
        let (n, addr) = match sock.recv_from(&mut buf) {
            Ok(r) => r,
            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => continue,
            Err(_) => break,
        };
        let text: String = String::from_utf8_lossy(&buf[..n]).chars().filter(char::is_ascii).collect();
        let text = text.trim();
        if text.is_empty() || text.as_bytes() == HF_BROADCAST_PAYLOAD {
            continue;
        }
        let parts: Vec<&str> = text.split(',').map(str::trim).collect();
        let ip = parts.first().filter(|p| !p.is_empty()).map(|p| p.to_string()).unwrap_or_else(|| addr.ip().to_string());
        if !seen.insert(ip.clone()) {
            continue;
        }
        found.push(json!({
            "ip": ip, "method": "hf-broadcast",
            "mac": parts.get(1), "model": parts.get(2), "port": null, "raw": text,
        }));
    }
    found
}

/// Hosts of an IPv4 CIDR subnet ("192.168.1.0/24"), like Python's ip_network(...).hosts().
pub fn hosts(subnet: &str) -> Result<Vec<Ipv4Addr>, String> {
    let bad = || format!("Invalid subnet: {subnet} (expected e.g. 192.168.1.0/24).");
    let (ip, prefix) = subnet.trim().split_once('/').unwrap_or((subnet.trim(), "32"));
    let ip: Ipv4Addr = ip.parse().map_err(|_| bad())?;
    let prefix: u32 = prefix.parse().map_err(|_| bad())?;
    if prefix > 32 {
        return Err(bad());
    }
    let size = 1u64 << (32 - prefix);
    let usable = if prefix >= 31 { size } else { size - 2 };
    if usable as usize > MAX_HOSTS {
        return Err(format!(
            "Subnet {subnet} has {usable} addresses - that's too many to scan (limit {MAX_HOSTS}). \
             Use a narrower subnet, e.g. /24."
        ));
    }
    let mask = if prefix == 0 { 0 } else { u32::MAX << (32 - prefix) };
    let network = u32::from(ip) & mask;
    let (first, last) = if prefix >= 31 { (network, network + (size - 1) as u32) } else { (network + 1, network + (size - 2) as u32) };
    Ok((first..=last).map(Ipv4Addr::from).collect())
}

fn try_connect(ip: Ipv4Addr, port: u16, timeout: Duration) -> bool {
    TcpStream::connect_timeout(&SocketAddr::from((ip, port)), timeout).is_ok()
}

/// Check every host of the subnet for one of the open TCP ports.
pub fn scan_tcp(
    subnet: &str,
    ports: &[u16],
    timeout: Duration,
    on_progress: &dyn Fn(usize, usize),
    cancelled: &dyn Fn() -> bool,
) -> Result<Vec<Value>, String> {
    let hosts = hosts(subnet)?;
    let total = hosts.len();
    let next = AtomicUsize::new(0);
    let stop = AtomicBool::new(false);
    let (tx, rx) = mpsc::channel::<Option<Value>>();
    let mut found = Vec::new();
    thread::scope(|scope| {
        for _ in 0..WORKERS.min(total) {
            let tx = tx.clone();
            let (hosts, next, stop) = (&hosts, &next, &stop);
            scope.spawn(move || loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                if i >= hosts.len() || stop.load(Ordering::SeqCst) {
                    return;
                }
                let ip = hosts[i];
                // One connection attempt is unreliable under high concurrency (ARP resolution,
                // first connection to a host) - one retry removes most false negatives.
                let open = ports.iter().find(|p| try_connect(ip, **p, timeout) || try_connect(ip, **p, timeout));
                let hit = open.map(|p| json!({"ip": ip.to_string(), "method": "port-scan", "mac": null, "model": null, "port": p}));
                if tx.send(hit).is_err() {
                    return;
                }
            });
        }
        drop(tx);
        let mut done = 0;
        for hit in rx {
            done += 1;
            on_progress(done, total);
            if let Some(h) = hit {
                found.push(h);
            }
            if cancelled() {
                stop.store(true, Ordering::SeqCst);
                break;
            }
        }
    });
    found.sort_by_key(|r| r["ip"].as_str().and_then(|ip| ip.parse::<Ipv4Addr>().ok()).map(u32::from).unwrap_or(0));
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn subnet_hosts() {
        let h = hosts("192.168.1.77/24").unwrap();
        assert_eq!(h.len(), 254);
        assert_eq!(h[0], Ipv4Addr::new(192, 168, 1, 1));
        assert_eq!(h[253], Ipv4Addr::new(192, 168, 1, 254));
        assert_eq!(hosts("10.0.0.5/32").unwrap(), [Ipv4Addr::new(10, 0, 0, 5)]);
        assert_eq!(hosts("10.0.0.4/31").unwrap().len(), 2);
        assert!(hosts("10.0.0.0/16").unwrap_err().contains("too many"));
        assert!(hosts("bogus").is_err());
    }

    #[test]
    fn finds_open_port_on_loopback() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let found = scan_tcp("127.0.0.1/32", &[port], Duration::from_millis(200), &|_, _| {}, &|| false).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0]["port"], port);
    }
}
