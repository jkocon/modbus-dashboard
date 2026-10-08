# Modbus RTU Dashboard

A cross-platform desktop tool for finding, reading and configuring **Modbus RTU** devices over
**RS485 (serial)** or **the network** (RS485↔Ethernet/WiFi converters, or real Modbus TCP).
It runs as a native window (like ESPHome Device Builder) on Windows, Linux and macOS, as a
single executable written in Rust (version 2.0; earlier versions were Python).

A set of standalone **PowerShell** scripts (Windows) covers the same operations from the
command line.

> **Polish documentation:** [README.pl.md](README.pl.md)

---

## Features

| Tab | What it does |
|---|---|
| **Discover on network** | Finds RS485↔Ethernet converters on the LAN — like VirCOM's *Search*. Verified HF/Elfin UDP broadcast + TCP port sweep of the subnet. One click fills the device's IP into the connection panel. |
| **Scan** | Sweeps baud rates × slave addresses and lists every device that answers with a valid CRC. Any function code / register / quantity. Includes the Waveshare *DiscoverAddress* broadcast mode. |
| **Temperature (R4DCB08)** | Reads all 8 DS18B20 channels of an R4DCB08 module, once or continuously. |
| **Coils (relays)** | Grid of coils for relay boards: click to toggle one (0x05), *All ON* / *All OFF* for the whole range (0x0F), states re-read from the device (0x01). Start coil and coil count are configurable. |
| **Change address** | Rewrites a device's Modbus address (register 254, function 0x06) and verifies the echo. |
| **Change baud rate** | Rewrites an R4DCB08's baud rate (register 255) and verifies the echo. |

Common to all tabs:

- **Serial** (COM/RS485): port, baud rate, data bits 5–8, parity None/Even/Odd, stop bits 1/2, flow control None/RTS-CTS/XON-XOFF.
- **Network**: transparent RTU-over-TCP converters (Elfin EW11, USR-TCP232, Waveshare RS485↔ETH…)
  **or** native Modbus TCP (MBAP header, no CRC) — one checkbox switches the framing.
- English by default, Polish available; light/dark theme with a manual override. The chosen
  language and theme are remembered in `~/.config/modbus-dashboard/prefs.json`.
- Clear, actionable errors — e.g. when a USB-RS485 adapter's driver rejects an unsupported
  data-bit/stop-bit combination you get told exactly that, not `The parameter is incorrect`.

---

## Quick start

### Option A — prebuilt binary

Run the single `modbus-dashboard` executable (`modbus-dashboard.exe` on Windows). It opens its
own window; nothing is installed, and the server inside listens only on a random `127.0.0.1`
port. The UI files are built into the binary.

### Option B — from source

Requires **Rust 1.85+** (`cargo`).

```bash
cargo run --release                 # native window (recommended)
cargo run --release -- --serve      # HTTP server + your browser, http://127.0.0.1:6070/
```

`--serve` also takes `--address 0.0.0.0` (LAN, see *Security model*), `--port N` and
`--no-browser`.

Platform notes for the native window (`wry`):

- **Windows** — uses the built-in WebView2 (ships with Edge on Windows 10/11).
- **macOS** — uses WKWebView.
- **Linux** — needs WebKitGTK and GTK 3 to build and run: `webkit2gtk-4.1 gtk3` (Arch),
  `libwebkit2gtk-4.1-dev libgtk-3-dev` (Debian/Ubuntu). Also add yourself to the serial-port
  group (`uucp` on Arch, `dialout` on Debian/Ubuntu) and log in again.

### Building a standalone executable

```bash
cargo build --release
```

Output: `target/release/modbus-dashboard` (`.exe` on Windows). Build it **on each target OS**;
release builds on Windows have no console window. On CachyOS `install.sh` builds it and
installs it to `/opt/modbus-dashboard` with a menu entry.

---

## Using it

### 1. Connection panel (top of the window)

Choose **Serial port** or **Network (IP)**.

**Serial.** Defaults are **8N1, no flow control**, which matches most inexpensive RS485
modules (Waveshare relays, R4DCB08). The Modbus RTU specification's own default is **8E1**
(even parity) — if a device stays silent on 8N1, try `Even`. Flow control is practically
never used by Modbus RTU; leave it at `None` unless the device manual says otherwise.

Not every USB-RS485 adapter supports every combination. Many CH340-class adapters only do
7–8 data bits and 1/2 stop bits; the driver rejects 5/6 data bits or 1.5 stop bits at the OS
level. The app recognises that specific error (`ERROR_INVALID_PARAMETER`) and tells you to
go back to 8N1 instead of showing a raw exception. Mark/Space parity, 1.5 stop bits and
DSR/DTR flow control are not offered (the serial library does not support them; Modbus RTU
does not use them).

Responses are read until the frame is complete (its length follows from the function code or
the MBAP header) or the line stays silent for 3.5 characters (at least 20 ms, to cover USB
adapter latency) - so slow baud rates such as 1200 work too.

**Network.** Enter the converter's IP and TCP port. Which port depends on the converter's
configuration — 502, 8899, 4196 and 23 are all common. Two framings exist:

| Framing | When | Checkbox |
|---|---|---|
| **RTU over TCP** | Converter in *transparent* mode: raw RTU bytes (with CRC) are tunnelled. Baud rate/parity are configured **in the converter**, not here. | off (default) |
| **Modbus TCP (MBAP)** | Converter/gateway/PLC speaks native Modbus TCP: 7-byte MBAP header, no CRC. Port 502 usually means this. | **on** |

If a scan over IP on port 502 finds nothing, tick the MBAP box and try again.

### 2. Discover on network

The subnet (CIDR) is auto-filled from your local IP; edit it if needed. Two methods run
together:

1. **HF/Elfin broadcast** — sends `HF-A11ASSISTHREAD` to UDP port 48899. Modules built on
   the Hi-Flying HF-LPB100 family (Elfin EW10/EW11/EW12 and many white-label clones) reply
   with `IP, MAC, model`. This protocol is public and verified against several independent
   implementations.
2. **TCP port sweep** — tries 502/8899/4196/23 (editable) on every host in the subnet, with
   one automatic retry per port to avoid missing devices on the first cold connection. It
   cannot tell you the brand, but it finds **any** converter with one of those ports open —
   including devices whose discovery protocol is proprietary (USR-IOT/VirCOM).

Limits: at most 1024 hosts per sweep (a `/22`); use a narrower subnet for speed.

### 3. Scan

Pick the baud rates to try (serial only — over IP the converter fixes the baud rate, so the
sweep runs once), an address range, the function code, start register and quantity. Use
`0x03` / register 0 / quantity 8 for an R4DCB08; `0x01` (Read Coils) for Waveshare relay
boards. *DiscoverAddress* sends one broadcast to register `0x4000`, to which some Waveshare
boards answer with their real address.

### 4. Coils (relays)

Set the device address, the first coil and how many coils the board has (8/16/32 for
Waveshare relay boards), then **Read states**. Each coil is a button: click it to flip that
relay with *Write Single Coil* (0x05). **All ON** / **All OFF** write the whole range in one
*Write Multiple Coils* (0x0F) request. After every write the states are read back from the
device, so the grid always shows what the hardware reports, not what was requested. A Modbus
exception (e.g. illegal address when the count exceeds the board's coils) is shown with its
code.

### 5–6. Change address / baud rate

Both send a *Write Single Register* and treat an exact echo as success. **Address
collisions:** if two devices share an address, the write changes both. Connect devices one
at a time to give them unique addresses. After changing the baud rate, some modules need a
power cycle; over IP, also update the baud rate in the converter's configuration.

---

## PowerShell scripts (Windows)

Same engine, no GUI. All scripts dot-source `ModbusCommon.ps1`, which must sit next to them.
Every script takes **either** `-ComPort COM3` **or** `-IpAddress 192.168.1.50 [-TcpPort 502]`,
plus the serial settings `-BaudRate`, `-DataBits 5..8`, `-Parity None|Even|Odd|Mark|Space`,
`-StopBits One|OnePointFive|Two`, `-Handshake None|XOnXOff|RequestToSend|RequestToSendXOnXOff`
and `-TimeoutMs`.

```powershell
# Find devices (R4DCB08 profile, serial)
.\Scan-Modbus.ps1 -ComPort COM3 -BaudRates 1200,2400,4800,9600,19200 -FunctionCode 3 -Register 0 -Quantity 8

# Same over a Modbus TCP gateway
.\Scan-Modbus.ps1 -IpAddress 10.10.0.201 -TcpPort 502 -Mbap -FunctionCode 3 -Quantity 8 -StartAddress 1 -EndAddress 20

# Read temperatures every 5 s
.\Read-Temperature.ps1 -ComPort COM3 -SlaveId 3 -Loop -IntervalSeconds 5

# Re-address a device, change its baud rate
.\Set-ModbusAddress.ps1  -ComPort COM3 -OldAddress 3 -NewAddress 2
.\Set-ModbusBaudRate.ps1 -ComPort COM3 -CurrentBaudRate 4800 -SlaveId 2 -NewBaudRate 9600
```

`ModbusGUI.ps1` is a WinForms front-end for the same scripts (Windows only, Polish UI). It
predates the dashboard and is kept for users who want a zero-dependency GUI; the dashboard
is the maintained UI.

---

## Architecture

```
src/
  main.rs        entry point: native window (wry/tao) around the server on a random loopback
                 port, or --serve for headless/LAN use with a browser
  server.rs      HTTP server + JSON API (tiny_http), Host/Origin/Content-Type checks;
                 the static UI is compiled into the binary
  core.rs        protocol: CRC16, RTU and MBAP framing, frame-length detection, input
                 validation, R4DCB08 decoding, coils, Write Single Register with echo check
  transport.rs   serial (serialport crate) and TCP transports
  discovery.rs   HF/Elfin UDP broadcast + concurrent TCP port sweep
  jobs.rs        background jobs (scan, discovery) the UI polls
static/          single-page UI (vanilla HTML/CSS/JS, no build step)
  i18n.js        EN/PL dictionaries; English is the default
ModbusCommon.ps1 …  PowerShell port of the same protocol layer + CLI scripts
```

No web framework, no JavaScript bundler: the dependencies are `serialport` (serial ports),
`tiny_http` (server), `serde_json`, `clap` and `wry`/`tao` (native window). The frontend talks to the backend through a
small JSON API; long operations (scan, discovery) run as background jobs the UI polls.

### JSON API

All request bodies use one `connection` object:

```json
{
  "transport": "serial" | "tcp",
  "port": "COM3", "baud": 9600, "dataBits": 8, "parity": "None",
  "stopBits": "One" | "Two",
  "flowControl": "None" | "RtsCts" | "XonXoff",
  "ip": "10.10.0.201", "tcpPort": 502, "mbap": false,
  "timeoutMs": 300
}
```

| Method & path | Body / result |
|---|---|
| `GET /api/ports` | `{ "ports": ["COM3", …] }` |
| `POST /api/scan` | `{connection, bauds?, startAddress, endAddress, functionCode, register, quantity, discoverAddress?}` → `{ "jobId" }` |
| `GET /api/scan/{jobId}` | `{ done, progress:{done,total,status}, found:[{baud,slaveId,response}], error }` |
| `POST /api/scan/{jobId}/cancel` | `{ "ok": true }` |
| `POST /api/read-temperature` | `{connection, slaveId}` → `{ results:[{channel,temperatureC,status}] }` |
| `POST /api/set-address` | `{connection, oldAddress, newAddress}` → `{ request, response, success }` |
| `POST /api/set-baudrate` | `{connection, slaveId, newBaudRate}` → `{ request, response, success }` |
| `POST /api/coils/read` | `{connection, slaveId, start?, quantity?}` → `{ coils:[bool, …] }` (0x01, max 256) |
| `POST /api/coils/write` | `{connection, slaveId, coil, on}` → `{ request, response, success }` (0x05) |
| `POST /api/coils/write-multiple` | `{connection, slaveId, start?, values:[bool, …]}` → `{ request, response, success }` (0x0F, max 256) |
| `GET /prefs.js` / `POST /api/prefs` | saved UI preferences / `{key: "lang"\|"theme", value}` → `{ ok }`; kept in `~/.config/modbus-dashboard/prefs.json` |
| `GET /api/network-discovery/default-subnet` | `{ "subnet": "192.168.1.0/24" }` |
| `POST /api/network-discovery` | `{subnet, ports?, timeoutMs?, hfBroadcast?}` → `{ "jobId" }` |
| `GET /api/network-discovery/{jobId}` | as for scan; `found:[{ip,method,mac,model,port}]` |
| `POST /api/network-discovery/{jobId}/cancel` | `{ "ok": true }` |

Errors are `{ "error": "…" }` with 4xx status. Device addresses are validated (1–247;
`oldAddress` also 0 = broadcast) and never truncated to a byte. A scan whose connection
cannot be opened (busy port, wrong IP) ends with that error instead of "nothing found". `POST` bodies must be `application/json`.
Backend error messages are always English regardless of the UI language.

---

## Security model (summary)

This is a **single-user, local** tool. The server binds to `127.0.0.1` by default and the
desktop app uses a random port. The API validates the `Host` and `Origin` headers (blocks
DNS-rebinding and cross-site requests), requires `application/json` on `POST`, caps request
size and concurrent jobs, and serves only the UI files built into the binary.

There is **no authentication**. If you start `modbus-dashboard --serve --address 0.0.0.0`, anyone
who can reach that port can reconfigure your Modbus devices and run port sweeps from your
machine. Only do that on a network you trust. Details and reporting: [SECURITY.md](SECURITY.md).

---

## Tests

```bash
cargo test
```

Protocol tests use canned frames (no hardware); a pseudo-terminal pair simulates a device at
1200 baud. API tests start the real server on an ephemeral port and exercise the security
controls and input validation.

---

## Hardware this was developed against

- **R4DCB08** 8-channel DS18B20 RS485 module (temperature, address/baud registers 254/255)
- **Waveshare Modbus RTU Relay** boards (Read Coils, `0x4000` DiscoverAddress)
- CH340-based USB-RS485 adapter (7–8 data bits, 1/2 stop bits only)
- A Modbus TCP gateway at port 502 (MBAP framing)
- Elfin EW11 / USR-TCP232 style RS485↔Ethernet converters (transparent mode)

The USR-IOT/VirCOM proprietary discovery protocol is **not** implemented (it is undocumented).
If your USR-IOT device is not found by the port sweep, capture VirCOM's *Search* traffic with
Wireshark and open an issue with the request/response bytes.

---

## License

MIT — see [LICENSE](LICENSE).
