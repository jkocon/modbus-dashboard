# Contributing

## Development setup

```bash
cargo run -- --serve            # browser mode is quickest for UI work
cargo run                       # native window
```

The UI files in `static/` are compiled into the binary, so rebuild after editing them.

Run the tests before opening a pull request:

```bash
cargo test
cargo clippy --all-targets
```

## Ground rules

- **Keep it dependency-light.** The backend is `serialport`, `tiny_http`, `serde_json` and
  `clap` (+ `wry`/`tao` for the window). The frontend is plain HTML/CSS/JS with no build step. Please don't add a
  framework or bundler for a small feature.
- **Every user-facing string goes through `static/i18n.js`** with both `en` and `pl`
  entries. English is the default. Backend error messages are English only.
- **Protocol changes need a test** in `src/core.rs` using canned frames; API/security
  changes need one in `src/server.rs`.
- **New discovery protocols** must be verified against a real device or a public,
  independently confirmed specification — include the source in the PR. Don't guess bytes.
- The PowerShell scripts mirror `src/core.rs`; if you change framing or serial
  parameters in one, change the other or say why not.
- Don't commit `target/` (see `.gitignore`); binaries go to Releases.

## Reporting hardware findings

Issues that include the device model, the exact serial settings or converter mode, and the
raw request/response hex (the app shows it) are the most useful. For undocumented discovery
protocols, a Wireshark capture of the vendor tool's *Search* is what unblocks support.
