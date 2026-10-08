# Changelog

## 2.0.0 — 2026-10-08

Rewritten in Rust: one executable with the UI built in, no Python or PyInstaller.

### Changed
- Native window via wry/tao (WebKitGTK / WebView2 / WKWebView); `--serve` replaces
  `modbus_dashboard.py` (same `--address`, `--port`, `--no-browser`).
- Serial ports via the `serialport` crate: Mark/Space parity, 1.5 stop bits and DSR/DTR flow
  control are no longer offered.

### Fixed
- Serial responses were cut after 5 ms without a new byte, so at 1200 baud (≈9 ms per byte)
  every frame failed its CRC, and USB adapters that deliver data in 16 ms chunks could fail
  too. Responses are now read until the frame is complete or 3.5 characters (≥20 ms) of
  silence; TCP likewise.
- Device addresses were masked to a byte, so e.g. 256 became 0 (broadcast) and "Change
  address" reprogrammed every device on the bus. Addresses are now validated (1–247;
  `oldAddress` 0 = broadcast on purpose) and out-of-range values are rejected.
- A scan whose connection could not be opened (busy port, permission, wrong IP) reported
  "no devices found"; it now ends with the error.
- The language and theme were never remembered in the window: it uses a new random port on
  every launch and localStorage is per origin (port included). Preferences are now saved by the
  app in `~/.config/modbus-dashboard/prefs.json` (`%APPDATA%` on Windows).
- DiscoverAddress scans now show progress; a converter found by both discovery methods is
  listed once; a truncated MBAP coil response gives a readable error.

## 1.0.0 — 2026-09-11

First public release.

### Dashboard (`webapp/`)
- Native-window desktop app (pywebview) with a stdlib HTTP backend; one-file PyInstaller build.
- Serial (RS485) and network transports; RTU-over-TCP and native Modbus TCP (MBAP) framing.
- Configurable serial parameters: data bits 5–8, parity, stop bits 1/1.5/2, flow control,
  with a clear message when the adapter's driver rejects a combination.
- Tabs: network discovery (HF/Elfin UDP broadcast + TCP port sweep with retry), address/baud
  scan with progress and cancel, R4DCB08 temperature read (single/continuous), coil/relay
  control (toggle one, all ON/OFF, configurable start and count, read-back after write),
  change address, change baud rate.
- English default UI with Polish translation; light/dark theme with manual override.
- Security hardening of the local API: Host/Origin validation (DNS rebinding, CSRF),
  JSON-only POST, body-size and concurrent-job limits, static path containment.
- Test suite: protocol unit tests and API security integration tests.

### PowerShell scripts
- `Scan-Modbus`, `Read-Temperature`, `Set-ModbusAddress`, `Set-ModbusBaudRate` sharing
  `ModbusCommon.ps1`; serial or network transport; full serial parameter set.
- `ModbusGUI.ps1` WinForms front-end (Windows only).
