# Modbus RTU Dashboard

A cross-platform desktop tool for finding, reading and configuring **Modbus RTU** devices over
**RS485 (serial)** or **the network** (RS485↔Ethernet/WiFi converters, or real Modbus TCP).
It runs as a native window (like ESPHome Device Builder) on Windows, Linux and macOS, and can
be built into a single standalone executable that needs no Python on the target machine.

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

- **Serial** (COM/RS485): port, baud rate, data bits 5–8, parity, stop bits 1/1.5/2, flow control.
- **Network**: transparent RTU-over-TCP converters (Elfin EW11, USR-TCP232, Waveshare RS485↔ETH…)
  **or** native Modbus TCP (MBAP header, no CRC) — one checkbox switches the framing.
- English by default, Polish available; light/dark theme with a manual override.
- Clear, actionable errors — e.g. when a USB-RS485 adapter's driver rejects an unsupported
  data-bit/stop-bit combination you get told exactly that, not `The parameter is incorrect`.

---

## Quick start

### Option A — prebuilt binary (no Python needed)

Download `Modbus Dashboard.exe` (Windows) from the Releases page and run it. It opens its
own window; nothing is installed and nothing listens outside `127.0.0.1`.

macOS/Linux binaries must currently be built on the target OS — see *Building* below.

### Option B — from source

Requires **Python 3.9+**.

```bash
cd webapp
pip install -r requirements.txt
python modbus_app.py            # native window (recommended)
# or
python modbus_dashboard.py      # HTTP server + your browser, http://127.0.0.1:6070/
```

On Linux/macOS use `python3` / `pip3` if `python` is not Python 3.

Platform notes for the native window (`pywebview`):

- **Windows** — uses the built-in WebView2 (ships with Edge on Windows 10/11). `pythonnet` is
  installed automatically by pip.
- **macOS** — uses WKWebView; `pyobjc` is installed automatically by pip.
- **Linux** — needs the system GTK WebKit, which pip cannot install:
  `sudo apt install python3-gi gir1.2-webkit2-4.1 python3-gi-cairo` (Debian/Ubuntu).
  Also add yourself to the serial group: `sudo usermod -a -G dialout $USER` and log in again.

### Building a standalone executable

```bash
cd webapp
pip install -r requirements.txt -r requirements-build.txt
python build.py
```

Output lands in `webapp/dist/` (`Modbus Dashboard.exe` / `Modbus Dashboard.app` / `Modbus Dashboard`).
PyInstaller does not cross-compile: run `build.py` **on each target OS**. Drop an
`icon.ico` / `icon.icns` / `icon.png` next to `build.py` to brand the binary.

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
go back to 8N1 instead of showing a raw exception.

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
webapp/
  modbus_app.py           desktop entry point: starts the server on a random loopback port
                          and shows it in a native WebView (pywebview)
  modbus_dashboard.py     HTTP server + JSON API (stdlib http.server); also a CLI for
                          headless/LAN use
  modbus_core.py          protocol: CRC16, RTU and MBAP framing, serial/TCP transports,
                          R4DCB08 decoding, Write Single Register with echo check
  network_discovery.py    HF/Elfin UDP broadcast + concurrent TCP port sweep
  build.py                PyInstaller one-file build
  static/                 single-page UI (vanilla HTML/CSS/JS, no build step)
    i18n.js               EN/PL dictionaries; English is the default
  tests/                  unit tests (protocol) + integration tests (API security)
ModbusCommon.ps1 …        PowerShell port of the same protocol layer + CLI scripts
```

No web framework, no JavaScript bundler: the only third-party dependencies are `pyserial`
(serial ports) and `pywebview` (native window). The frontend talks to the backend through a
small JSON API; long operations (scan, discovery) run as background jobs the UI polls.

### JSON API

All request bodies use one `connection` object:

```json
{
  "transport": "serial" | "tcp",
  "port": "COM3", "baud": 9600, "dataBits": 8, "parity": "None",
  "stopBits": "One" | "OnePointFive" | "Two",
  "flowControl": "None" | "RtsCts" | "DsrDtr" | "XonXoff",
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
| `GET /api/network-discovery/default-subnet` | `{ "subnet": "192.168.1.0/24" }` |
| `POST /api/network-discovery` | `{subnet, ports?, timeoutMs?, hfBroadcast?}` → `{ "jobId" }` |
| `GET /api/network-discovery/{jobId}` | as for scan; `found:[{ip,method,mac,model,port}]` |
| `POST /api/network-discovery/{jobId}/cancel` | `{ "ok": true }` |

Errors are `{ "error": "…" }` with 4xx status. `POST` bodies must be `application/json`.
Backend error messages are always English regardless of the UI language.

---

## Security model (summary)

This is a **single-user, local** tool. The server binds to `127.0.0.1` by default and the
desktop app uses a random port. The API validates the `Host` and `Origin` headers (blocks
DNS-rebinding and cross-site requests), requires `application/json` on `POST`, caps request
size and concurrent jobs, and serves static files only from its own directory.

There is **no authentication**. If you start `modbus_dashboard.py --address 0.0.0.0`, anyone
who can reach that port can reconfigure your Modbus devices and run port sweeps from your
machine. Only do that on a network you trust. Details and reporting: [SECURITY.md](SECURITY.md).

---

## Tests

```bash
cd webapp
python -m unittest discover -s tests -v
```

Protocol tests use canned frames (no hardware). API tests start the real server on an
ephemeral port and exercise the security controls.

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
