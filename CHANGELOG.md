# Changelog

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
