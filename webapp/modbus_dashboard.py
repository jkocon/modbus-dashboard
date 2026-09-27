#!/usr/bin/env python3
"""
Modbus RTU Dashboard - a local web server for scanning, reading and
configuring Modbus RTU devices over RS485 (COM/serial port) or over the
network (RS485<->Ethernet/WiFi converter, or native Modbus TCP).

Runs on Windows, Linux and macOS (Python 3.9+ and pyserial only) - much like
`esphome dashboard`: start the script and the local UI opens in your browser.

Usage:
    pip install -r requirements.txt
    python modbus_dashboard.py
    (Linux/macOS: python3 modbus_dashboard.py)

By default the server listens only on localhost (127.0.0.1) and is not
reachable from the network. Use --address 0.0.0.0 to expose it on the LAN.
"""

from __future__ import annotations

import argparse
import json
import mimetypes
import re
import sys
import threading
import time
import uuid
import webbrowser
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import urlparse

import modbus_core as mc
import network_discovery as nd

if getattr(sys, "frozen", False) and hasattr(sys, "_MEIPASS"):
    # Built with PyInstaller (--onefile): data files are unpacked into a temporary directory.
    # .resolve() matters here: sys._MEIPASS can be a short 8.3 name (e.g. C:\...\JANKOC~1\...),
    # while Handler._serve_static() compares against candidate.resolve() (long form) - without
    # it relative_to() always fails and every static file gets a 403.
    STATIC_DIR = (Path(sys._MEIPASS) / "static").resolve()  # type: ignore[attr-defined]
else:
    STATIC_DIR = Path(__file__).resolve().parent / "static"

SCAN_JOBS: dict[str, "ScanJob"] = {}
SCAN_JOBS_LOCK = threading.Lock()
JOB_TTL_SECONDS = 600  # finished jobs are garbage-collected after this
MAX_ACTIVE_JOBS = 4  # concurrent jobs per type (guards against thread flooding)
MAX_BODY_BYTES = 1024 * 1024  # POST body size limit
LOOPBACK_HOSTS = {"127.0.0.1", "localhost", "::1"}


class ScanJob:
    def __init__(self) -> None:
        self.id = uuid.uuid4().hex
        self.created_at = time.monotonic()
        self.done = False
        self.cancelled = False
        self.error: str | None = None
        self.progress = {"done": 0, "total": 1, "status": "Starting..."}
        self.found: list[dict] = []


def _cleanup_old_jobs() -> None:
    with SCAN_JOBS_LOCK:
        stale = [jid for jid, j in SCAN_JOBS.items()
                 if j.done and (time.monotonic() - j.created_at) > JOB_TTL_SECONDS]
        for jid in stale:
            del SCAN_JOBS[jid]


NET_JOBS: dict[str, "ScanJob"] = {}
NET_JOBS_LOCK = threading.Lock()


def _cleanup_old_net_jobs() -> None:
    with NET_JOBS_LOCK:
        stale = [jid for jid, j in NET_JOBS.items()
                 if j.done and (time.monotonic() - j.created_at) > JOB_TTL_SECONDS]
        for jid in stale:
            del NET_JOBS[jid]


def run_net_discovery(job: "ScanJob", params: dict) -> None:
    try:
        _run_net_discovery(job, params)
    except Exception as e:  # noqa: BLE001
        job.error = str(e)
    finally:
        job.done = True


def _run_net_discovery(job: "ScanJob", params: dict) -> None:
    use_hf = bool(params.get("hfBroadcast", True))
    ports = [int(p) for p in (params.get("ports") or nd.DEFAULT_PORTS)]
    subnet = params.get("subnet") or nd.default_subnet()
    timeout_s = float(params.get("timeoutMs", 200)) / 1000.0

    if use_hf:
        job.progress["status"] = "Broadcast HF-A11ASSISTHREAD (Hi-Flying/Elfin)..."
        for device in nd.discover_hf_devices(timeout_s=2.0):
            job.found.append(device)

    if job.cancelled:
        return

    if ports:
        def on_progress(done: int, total: int) -> None:
            job.progress["done"] = done
            job.progress["total"] = total
            job.progress["status"] = f"Scanning {subnet}: {done}/{total} hosts"

        for device in nd.scan_tcp_ports(
            subnet, ports, timeout_s,
            on_progress=on_progress,
            is_cancelled=lambda: job.cancelled,
        ):
            job.found.append(device)


def run_scan(job: ScanJob, params: dict) -> None:
    try:
        _run_scan(job, params)
    except Exception as e:  # noqa: BLE001 - surface the error in the UI instead of dying silently in the thread
        job.error = str(e)
    finally:
        job.done = True


def _run_scan(job: ScanJob, params: dict) -> None:
    conn_base = dict(params.get("connection") or {})
    transport = conn_base.get("transport")
    mbap = bool(conn_base.get("mbap"))
    timeout_ms = int(conn_base.get("timeoutMs", 300))

    start_addr = int(params["startAddress"])
    end_addr = int(params["endAddress"])
    function_code = int(params["functionCode"])
    register = int(params["register"])
    quantity = int(params["quantity"])
    discover = bool(params.get("discoverAddress"))

    bauds = params.get("bauds") if transport == "serial" else None
    if not bauds:
        bauds = [None]

    total_addrs = max(end_addr - start_addr + 1, 1)
    job.progress["total"] = max(len(bauds) * total_addrs, 1)
    done = 0

    for baud in bauds:
        if job.cancelled:
            break

        conn_params = dict(conn_base)
        conn_params["timeoutMs"] = timeout_ms
        if baud is not None:
            conn_params["baud"] = baud

        label = (f"{conn_params.get('ip')}:{conn_params.get('tcpPort', 502)}"
                  if transport == "tcp" else f"{conn_params.get('port')} @ {baud}")
        job.progress["status"] = f"Connecting: {label}"

        try:
            conn = mc.open_connection(conn_params)
        except Exception:
            done += total_addrs
            job.progress["done"] = done
            continue

        try:
            if discover:
                request = (mc.build_mbap_read(0, 0x03, 0x4000, 1) if mbap
                           else mc.build_rtu_request(0, 0x03, 0x4000, 1))
                try:
                    resp = conn.send_request(request, timeout_ms)
                except Exception:
                    resp = b""
                ok = (len(resp) >= 9) if mbap else (len(resp) >= 5 and mc.test_rtu_crc(resp))
                if ok:
                    addr = resp[6] if mbap else resp[0]
                    job.found.append({"baud": baud, "slaveId": addr, "response": mc.format_hex(resp)})
                continue

            for addr in range(start_addr, end_addr + 1):
                if job.cancelled:
                    break
                done += 1
                job.progress["done"] = done
                job.progress["status"] = f"{label}, address {addr}"

                request = (mc.build_mbap_read(addr, function_code, register, quantity) if mbap
                           else mc.build_rtu_request(addr, function_code, register, quantity))
                try:
                    resp = conn.send_request(request, timeout_ms)
                except Exception:
                    resp = b""

                ok = (mc.test_mbap_response(resp, addr) if mbap
                      else (len(resp) >= 5 and resp[0] == addr and mc.test_rtu_crc(resp)))
                if ok:
                    job.found.append({"baud": baud, "slaveId": addr, "response": mc.format_hex(resp)})
        finally:
            conn.close()


class _HttpError(Exception):
    def __init__(self, status: int, message: str) -> None:
        super().__init__(message)
        self.status = status


class Handler(BaseHTTPRequestHandler):
    server_version = "ModbusDashboard/1.0"
    protocol_version = "HTTP/1.1"

    def log_message(self, fmt: str, *args) -> None:  # quiet: don't echo every request to the console
        pass

    # ---------- helpers ----------

    def _send_json(self, obj, status: int = 200) -> None:
        body = json.dumps(obj).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _read_json(self) -> dict:
        content_type = (self.headers.get("Content-Type") or "").split(";")[0].strip().lower()
        if content_type != "application/json":
            # Requiring JSON blocks cross-site "simple requests" (CSRF): the browser then has to
            # send a CORS preflight, which this server never approves.
            raise _HttpError(415, "Content-Type must be application/json.")
        length = int(self.headers.get("Content-Length", 0) or 0)
        if length > MAX_BODY_BYTES:
            raise _HttpError(413, "Request body too large.")
        if length == 0:
            return {}
        raw = self.rfile.read(length)
        try:
            data = json.loads(raw.decode("utf-8")) if raw else {}
        except (ValueError, UnicodeDecodeError) as e:
            raise _HttpError(400, f"Invalid JSON body: {e}") from e
        if not isinstance(data, dict):
            raise _HttpError(400, "JSON body must be an object.")
        return data

    def _request_allowed(self) -> bool:
        """Protection against DNS rebinding and CSRF.

        - A loopback-bound server only accepts requests whose Host header names a
          loopback address (a foreign domain rebound to 127.0.0.1 is rejected).
        - If the browser sends Origin (every POST and every cross-origin request),
          it must match exactly how we were addressed.
        """
        host = (self.headers.get("Host") or "").strip()
        hostname = host.rsplit(":", 1)[0].strip("[]").lower() if host else ""
        bound_addr = self.server.server_address[0]
        if bound_addr in LOOPBACK_HOSTS and hostname not in LOOPBACK_HOSTS:
            return False
        origin = self.headers.get("Origin")
        if origin is not None and origin.strip().lower() != f"http://{host}".lower():
            return False
        return True

    def _start_job(self, store: dict, lock: threading.Lock, cleanup, runner, params: dict) -> None:
        cleanup()
        with lock:
            active = sum(1 for j in store.values() if not j.done)
            if active >= MAX_ACTIVE_JOBS:
                raise _HttpError(429, f"Too many jobs running at once (limit {MAX_ACTIVE_JOBS}). Wait or cancel one.")
            job = ScanJob()
            store[job.id] = job
        threading.Thread(target=runner, args=(job, params), daemon=True).start()
        self._send_json({"jobId": job.id})

    def _job_status(self, store: dict, job_id: str, not_found_msg: str) -> None:
        job = store.get(job_id)
        if not job:
            return self._send_json({"error": not_found_msg}, 404)
        return self._send_json({
            "done": job.done,
            "progress": job.progress,
            "found": job.found,
            "error": job.error,
        })

    def _serve_static(self, path: str) -> None:
        if path == "/":
            path = "/index.html"
        candidate = (STATIC_DIR / path.lstrip("/")).resolve()
        try:
            candidate.relative_to(STATIC_DIR)
        except ValueError:
            self.send_error(403, "Forbidden")
            return
        if not candidate.is_file():
            self.send_error(404, "Not Found")
            return
        content_type, _ = mimetypes.guess_type(str(candidate))
        data = candidate.read_bytes()
        self.send_response(200)
        self.send_header("Content-Type", content_type or "application/octet-stream")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    # ---------- routing ----------

    def do_GET(self) -> None:
        if not self._request_allowed():
            return self._send_json({"error": "Forbidden: unexpected Host/Origin."}, 403)
        path = urlparse(self.path).path

        if path == "/api/ports":
            try:
                return self._send_json({"ports": mc.list_serial_ports()})
            except Exception as e:
                return self._send_json({"error": str(e)}, 400)

        m = re.match(r"^/api/scan/([0-9a-f]{32})$", path)
        if m:
            return self._job_status(SCAN_JOBS, m.group(1), "Scan job not found.")

        if path == "/api/network-discovery/default-subnet":
            try:
                return self._send_json({"subnet": nd.default_subnet()})
            except Exception as e:
                return self._send_json({"error": str(e)}, 400)

        m = re.match(r"^/api/network-discovery/([0-9a-f]{32})$", path)
        if m:
            return self._job_status(NET_JOBS, m.group(1), "Search job not found.")

        self._serve_static(path)

    def do_POST(self) -> None:
        if not self._request_allowed():
            return self._send_json({"error": "Forbidden: unexpected Host/Origin."}, 403)
        path = urlparse(self.path).path
        try:
            if path == "/api/scan":
                return self._start_job(SCAN_JOBS, SCAN_JOBS_LOCK, _cleanup_old_jobs, run_scan, self._read_json())

            m = re.match(r"^/api/scan/([0-9a-f]{32})/cancel$", path)
            if m:
                job = SCAN_JOBS.get(m.group(1))
                if job:
                    job.cancelled = True
                return self._send_json({"ok": True})

            if path == "/api/network-discovery":
                return self._start_job(NET_JOBS, NET_JOBS_LOCK, _cleanup_old_net_jobs, run_net_discovery, self._read_json())

            m = re.match(r"^/api/network-discovery/([0-9a-f]{32})/cancel$", path)
            if m:
                job = NET_JOBS.get(m.group(1))
                if job:
                    job.cancelled = True
                return self._send_json({"ok": True})

            if path == "/api/read-temperature":
                params = self._read_json()
                conn_params = params.get("connection") or {}
                timeout_ms = int(conn_params.get("timeoutMs", 300))
                conn = mc.open_connection(conn_params)
                try:
                    results = mc.read_r4dcb08(
                        conn, int(params["slaveId"]), bool(conn_params.get("mbap")), timeout_ms
                    )
                finally:
                    conn.close()
                return self._send_json({"results": results})

            if path == "/api/set-address":
                params = self._read_json()
                conn_params = params.get("connection") or {}
                old_addr = int(params["oldAddress"])
                new_addr = int(params["newAddress"])
                timeout_ms = int(conn_params.get("timeoutMs", 300))
                conn = mc.open_connection(conn_params)
                try:
                    request, resp, success = mc.write_single_register(
                        conn, old_addr, 254, new_addr, bool(conn_params.get("mbap")), timeout_ms
                    )
                finally:
                    conn.close()
                return self._send_json({
                    "request": mc.format_hex(request),
                    "response": mc.format_hex(resp) if resp else None,
                    "success": success,
                })

            if path == "/api/set-baudrate":
                params = self._read_json()
                conn_params = params.get("connection") or {}
                slave_id = int(params["slaveId"])
                new_baud = int(params["newBaudRate"])
                if new_baud not in mc.BAUD_CODE_MAP:
                    raise ValueError(f"Unsupported baud rate: {new_baud}. Allowed: {sorted(mc.BAUD_CODE_MAP)}.")
                code = mc.BAUD_CODE_MAP[new_baud]
                timeout_ms = int(conn_params.get("timeoutMs", 300))
                conn = mc.open_connection(conn_params)
                try:
                    request, resp, success = mc.write_single_register(
                        conn, slave_id, 255, code, bool(conn_params.get("mbap")), timeout_ms
                    )
                finally:
                    conn.close()
                return self._send_json({
                    "request": mc.format_hex(request),
                    "response": mc.format_hex(resp) if resp else None,
                    "success": success,
                })

            if path == "/api/coils/read":
                params = self._read_json()
                conn_params = params.get("connection") or {}
                timeout_ms = int(conn_params.get("timeoutMs", 300))
                conn = mc.open_connection(conn_params)
                try:
                    coils = mc.read_coils(
                        conn, int(params["slaveId"]), int(params.get("start", 0)),
                        int(params.get("quantity", 8)), bool(conn_params.get("mbap")), timeout_ms,
                    )
                finally:
                    conn.close()
                return self._send_json({"coils": coils})

            if path == "/api/coils/write":
                params = self._read_json()
                conn_params = params.get("connection") or {}
                timeout_ms = int(conn_params.get("timeoutMs", 300))
                coil = int(params["coil"])
                if not 0 <= coil <= 0xFFFF:
                    raise ValueError("coil must be between 0 and 65535.")
                conn = mc.open_connection(conn_params)
                try:
                    request, resp, success = mc.write_single_coil(
                        conn, int(params["slaveId"]), coil, bool(params.get("on")),
                        bool(conn_params.get("mbap")), timeout_ms,
                    )
                finally:
                    conn.close()
                return self._send_json({
                    "request": mc.format_hex(request),
                    "response": mc.format_hex(resp) if resp else None,
                    "success": success,
                })

            if path == "/api/coils/write-multiple":
                params = self._read_json()
                conn_params = params.get("connection") or {}
                timeout_ms = int(conn_params.get("timeoutMs", 300))
                values = [bool(v) for v in (params.get("values") or [])]
                conn = mc.open_connection(conn_params)
                try:
                    request, resp, success = mc.write_multiple_coils(
                        conn, int(params["slaveId"]), int(params.get("start", 0)), values,
                        bool(conn_params.get("mbap")), timeout_ms,
                    )
                finally:
                    conn.close()
                return self._send_json({
                    "request": mc.format_hex(request),
                    "response": mc.format_hex(resp) if resp else None,
                    "success": success,
                })

            self._send_json({"error": "Unknown API path."}, 404)

        except _HttpError as e:
            self._send_json({"error": str(e)}, e.status)
        except Exception as e:  # noqa: BLE001 - the error must reach the UI as a readable message
            self._send_json({"error": str(e)}, 400)


def main() -> None:
    parser = argparse.ArgumentParser(description="Modbus RTU Dashboard - web interface for Modbus RTU devices (COM/RS485 and network).")
    parser.add_argument("--address", default="127.0.0.1",
                         help="Server listen address (default 127.0.0.1 - this machine only; 0.0.0.0 exposes it on the local network)")
    parser.add_argument("--port", type=int, default=6070, help="HTTP port (default 6070)")
    parser.add_argument("--no-browser", action="store_true", help="Do not automatically open a browser")
    args = parser.parse_args()

    server = ThreadingHTTPServer((args.address, args.port), Handler)
    url = f"http://{args.address if args.address != '0.0.0.0' else 'localhost'}:{args.port}/"

    print(f"Modbus Dashboard running at: {url}")
    print("Stop: Ctrl+C")

    if not args.no_browser:
        threading.Timer(0.5, lambda: webbrowser.open(url)).start()

    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.shutdown()


if __name__ == "__main__":
    main()
