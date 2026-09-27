# Contributing

## Development setup

```bash
cd webapp
python -m venv .venv
. .venv/bin/activate            # Windows: .venv\Scripts\activate
pip install -r requirements.txt -r requirements-build.txt
python modbus_dashboard.py      # browser mode is quickest for UI work
```

Run the tests before opening a pull request:

```bash
python -m unittest discover -s tests -v
```

## Ground rules

- **Keep it dependency-light.** The backend is Python stdlib + `pyserial` (+ `pywebview`
  for the window). The frontend is plain HTML/CSS/JS with no build step. Please don't add a
  framework or bundler for a small feature.
- **Every user-facing string goes through `static/i18n.js`** with both `en` and `pl`
  entries. English is the default. Backend error messages are English only.
- **Protocol changes need a test** in `tests/test_core.py` using canned frames; API/security
  changes need one in `tests/test_api_security.py`.
- **New discovery protocols** must be verified against a real device or a public,
  independently confirmed specification — include the source in the PR. Don't guess bytes.
- The PowerShell scripts mirror `modbus_core.py`; if you change framing or serial
  parameters in one, change the other or say why not.
- Don't commit `webapp/dist/`, `webapp/build/` or `*.spec` (see `.gitignore`); binaries go
  to Releases.

## Reporting hardware findings

Issues that include the device model, the exact serial settings or converter mode, and the
raw request/response hex (the app shows it) are the most useful. For undocumented discovery
protocols, a Wireshark capture of the vendor tool's *Search* is what unblocks support.
