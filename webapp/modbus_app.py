#!/usr/bin/env python3
"""
Desktop shell for Modbus Dashboard: starts the local HTTP server (the same
code as modbus_dashboard.py) in the background and shows it in a native
window (WebView2 on Windows, WKWebView on macOS, GTK WebKit on Linux) instead
of a browser tab - it behaves like a standalone application, much like
ESPHome Device Builder (which does the same: bundled Python + native WebView).

Development mode (requires an installed Python):
    pip install -r requirements.txt
    python modbus_app.py

To build a standalone executable (no Python needed on the end user's machine)
see build.py in this folder.
"""

from __future__ import annotations

import socket
import threading
from http.server import ThreadingHTTPServer

import webview

from modbus_dashboard import Handler

APP_TITLE = "Modbus RTU Dashboard"
HOST = "127.0.0.1"


def _find_free_port(host: str) -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind((host, 0))
        return s.getsockname()[1]


def main() -> None:
    port = _find_free_port(HOST)
    server = ThreadingHTTPServer((HOST, port), Handler)
    server_thread = threading.Thread(target=server.serve_forever, daemon=True)
    server_thread.start()

    url = f"http://{HOST}:{port}/"
    webview.create_window(APP_TITLE, url, width=1000, height=800, min_size=(760, 600))

    try:
        webview.start()
    finally:
        server.shutdown()
        server.server_close()


if __name__ == "__main__":
    main()
