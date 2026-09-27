"""
Discovery of RS485<->Ethernet/WiFi converters on the local network - the
equivalent of "Search"/"Device Management" in tools like VirCOM (USR-IOT).

Two independent methods, run together:

1. UDP broadcast "HF-A11ASSISTHREAD" to port 48899 - a real, verified protocol
   used by the Hi-Flying/HF-LPB100 module family (including the Elfin
   EW10/EW11/EW12 converters). The device replies with plain text
   "IP, MAC, ModelID".

2. A TCP port sweep of the whole subnet (default 502/8899/4196/23 - typical
   transparent-mode/Modbus TCP ports across vendors, including USR-IOT). It
   cannot identify the brand/model, but it finds ANY converter with one of
   those ports open, regardless of vendor.

The actual UDP protocol used by VirCOM (USR-IOT) is proprietary and could not
be verified without a packet capture from a real device - hence the
"two methods" approach instead of guessing bytes.
"""

from __future__ import annotations

import ipaddress
import socket
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from typing import Optional

HF_BROADCAST_PORT = 48899
HF_BROADCAST_PAYLOAD = b"HF-A11ASSISTHREAD"
DEFAULT_PORTS = [502, 8899, 4196, 23]
MAX_HOSTS = 1024


def get_local_ip() -> str:
    """Best-effort local IPv4 address (no data is actually sent)."""
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    try:
        s.connect(("8.8.8.8", 80))
        return s.getsockname()[0]
    except OSError:
        return "127.0.0.1"
    finally:
        s.close()


def default_subnet() -> str:
    ip = get_local_ip()
    parts = ip.split(".")
    if len(parts) == 4:
        return f"{'.'.join(parts[:3])}.0/24"
    return "192.168.1.0/24"


def discover_hf_devices(timeout_s: float = 2.0) -> list[dict]:
    """Broadcast HF-A11ASSISTHREAD (Hi-Flying/Elfin/HF-LPB100) and collect the replies."""
    found = []
    seen_ips = set()

    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_BROADCAST, 1)
    sock.settimeout(0.3)
    try:
        try:
            sock.bind(("", 0))
        except OSError:
            pass
        sock.sendto(HF_BROADCAST_PAYLOAD, ("255.255.255.255", HF_BROADCAST_PORT))

        deadline = time.monotonic() + timeout_s
        while time.monotonic() < deadline:
            try:
                data, addr = sock.recvfrom(512)
            except socket.timeout:
                continue
            except OSError:
                break

            text = data.decode("ascii", errors="ignore").strip()
            if not text or text == HF_BROADCAST_PAYLOAD.decode():
                continue

            parts = [p.strip() for p in text.split(",")]
            ip = parts[0] if parts else addr[0]
            if ip in seen_ips:
                continue
            seen_ips.add(ip)
            found.append({
                "ip": ip,
                "method": "hf-broadcast",
                "mac": parts[1] if len(parts) > 1 else None,
                "model": parts[2] if len(parts) > 2 else None,
                "port": None,
                "raw": text,
            })
    finally:
        sock.close()

    return found


def _try_connect(ip: str, port: int, timeout_s: float) -> bool:
    try:
        with socket.create_connection((ip, port), timeout=timeout_s):
            return True
    except OSError:
        return False


def scan_tcp_ports(subnet: str, ports: list[int], timeout_s: float,
                    on_progress=None, is_cancelled=None) -> list[dict]:
    """Check every host in the subnet (CIDR) for open TCP ports.

    on_progress(done, total) - optional callback invoked after each host is checked.
    is_cancelled() -> bool - optional callback to abort mid-sweep.
    """
    net = ipaddress.ip_network(subnet, strict=False)
    hosts = list(net.hosts())
    if len(hosts) > MAX_HOSTS:
        raise ValueError(
            f"Subnet {subnet} has {len(hosts)} addresses - that's too many to scan "
            f"(limit {MAX_HOSTS}). Use a narrower subnet, e.g. /24."
        )

    found = []
    total = len(hosts)
    done = 0

    def check_host(ip: str) -> Optional[dict]:
        for port in ports:
            # A single connection attempt is unreliable under high concurrency
            # (ARP resolution, first connection to a host) - one retry removes
            # most false "no response" results without noticeably slowing the sweep.
            if _try_connect(ip, port, timeout_s) or _try_connect(ip, port, timeout_s):
                return {"ip": ip, "method": "port-scan", "mac": None, "model": None, "port": port}
        return None

    with ThreadPoolExecutor(max_workers=100) as pool:
        futures = {pool.submit(check_host, str(h)): str(h) for h in hosts}
        for future in as_completed(futures):
            done += 1
            if on_progress:
                on_progress(done, total)
            if is_cancelled and is_cancelled():
                for f in futures:
                    f.cancel()
                break
            result = future.result()
            if result:
                found.append(result)

    found.sort(key=lambda r: tuple(int(x) for x in r["ip"].split(".")))
    return found
