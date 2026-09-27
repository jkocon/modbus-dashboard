# Security

## Threat model

Modbus RTU Dashboard is a **single-user tool that runs on the operator's own machine** and
talks to industrial devices that have **no authentication of their own** (Modbus RTU/TCP
carry no credentials). Anything that can reach the dashboard's HTTP API can therefore
re-address devices, change their baud rates, and launch port sweeps of the local network
from the operator's IP address.

The design goal is: **only the operator, from the dashboard's own page, can trigger those
actions** — not another website open in the same browser, and not another host on the LAN
unless the operator explicitly opts in.

## Controls in place

| Risk | Control | Where |
|---|---|---|
| Exposure to the network | Server binds to `127.0.0.1` by default; the desktop app picks a random loopback port per launch. LAN exposure requires an explicit `--address 0.0.0.0`. | `modbus_dashboard.py` `main()`, `modbus_app.py` |
| **DNS rebinding** (a malicious site's domain resolving to 127.0.0.1) | Every request to a loopback-bound server must carry a `Host` header naming `127.0.0.1`, `localhost` or `::1`; anything else is `403`. | `Handler._request_allowed` |
| **CSRF** from another origin | If the browser sends `Origin`, it must equal `http://<Host>` exactly (`Origin: null` is rejected). Additionally `POST` bodies must be `application/json`, so a cross-site *simple request* (`text/plain`) is refused with `415` before reaching any handler — and a JSON request forces a CORS preflight the server never approves. | `_request_allowed`, `_read_json` |
| Memory exhaustion via large bodies | `Content-Length` capped at 1 MiB (`413`). | `_read_json` |
| Thread exhaustion via job spam | At most 4 concurrent scan jobs and 4 discovery jobs (`429`). | `_start_job` |
| Path traversal in static files | Resolved path must stay inside the bundled `static/` directory (`403`). | `_serve_static` |
| XSS via device data | All device-supplied strings (responses, discovered names) are inserted with `textContent`, never `innerHTML`. | `static/app.js` |
| Malformed input | JSON is parsed defensively (`400`), numeric fields are cast, subnet is parsed with `ipaddress`, serial parameters are validated against allow-lists before the port is opened. | throughout |

Tests for these controls live in `webapp/tests/test_api_security.py`.

## Residual risks — read before exposing the server

- **No authentication.** `--address 0.0.0.0` (or binding to any non-loopback address)
  disables the `Host` check by design. Everyone on that network can then control your
  Modbus devices and run port sweeps that originate from your machine. Use it only on a
  trusted network, or put it behind a reverse proxy that adds authentication.
- **Port sweeps are active network probing.** The *Discover on network* tab connects to
  every host in the chosen subnet. Only scan networks you own or are authorised to test.
- **Writes are irreversible from the tool's point of view.** Changing an address or baud
  rate takes effect immediately on the device; with duplicated addresses it affects every
  device that shares the address.
- **Serial port access** is whatever the OS grants the user running the app.
- The prebuilt `.exe` is not code-signed. Verify it came from this repository's Releases
  page, or build it yourself with `build.py`.

## Reporting a vulnerability

Open a GitHub issue describing the problem. If it allows remote control of devices or code
execution, please say so in the title so it can be prioritised; do not post working
exploit payloads against third-party hardware.
