#!/usr/bin/env python3
"""
Builds a standalone Modbus Dashboard executable (no Python required on the
machine that runs it) - the same packaging approach as ESPHome Device Builder.

IMPORTANT: PyInstaller cannot cross-compile - run this script ON EACH target
OS separately (Windows -> .exe, macOS -> .app, Linux -> ELF binary).

Usage:
    pip install -r requirements.txt -r requirements-build.txt
    python build.py

Output lands in dist/:
    Windows: dist/Modbus Dashboard.exe
    macOS:   dist/Modbus Dashboard.app
    Linux:   dist/Modbus Dashboard
"""

from __future__ import annotations

import platform
from pathlib import Path
from typing import Optional

import PyInstaller.__main__

ROOT = Path(__file__).resolve().parent
APP_NAME = "Modbus Dashboard"


ICON_NAME_BY_SYSTEM = {"Windows": "icon.ico", "Darwin": "icon.icns", "Linux": "icon.png"}


def _icon_path(system: str) -> Optional[Path]:
    name = ICON_NAME_BY_SYSTEM.get(system)
    if not name:
        return None
    icon = ROOT / name
    return icon if icon.is_file() else None


def main() -> None:
    system = platform.system()
    data_sep = ";" if system == "Windows" else ":"

    args = [
        str(ROOT / "modbus_app.py"),
        "--name", APP_NAME,
        "--onefile",
        "--windowed",
        "--add-data", f"{ROOT / 'static'}{data_sep}static",
        "--noconfirm",
        "--clean",
    ]

    icon = _icon_path(system)
    if icon:
        args += ["--icon", str(icon)]
    else:
        hint = ICON_NAME_BY_SYSTEM.get(system, "icon.*")
        print(f"(No icon file for {system} - building with PyInstaller's default icon. Provide your own as: {hint})")

    print(f"Building '{APP_NAME}' for {system}...")
    PyInstaller.__main__.run(args)
    print(f"\nDone. Output is in: {ROOT / 'dist'}")


if __name__ == "__main__":
    main()
