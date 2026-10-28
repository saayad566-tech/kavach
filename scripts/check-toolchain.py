#!/usr/bin/env python3
"""
Toolchain preflight.

Run this before a build on a machine that has never built Kavach. It checks each tool the
build actually uses, reports the version it found, and prints the install line when something
is missing — rather than letting a build fail forty seconds in with a linker error.

    python3 scripts/check-toolchain.py

Exit code 0 means every required tool is present and new enough.
"""

from __future__ import annotations

import re
import shutil
import subprocess
import sys

# (name, minimum, why it is needed, how to install it)
REQUIRED = [
    ("rustc", (1, 75), "compiles the host crate",
     "https://rustup.rs  (or: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh)"),
    ("cargo", (1, 75), "builds and tests the workspace", "installed with rustc via rustup"),
    ("node", (20, 0), "runs generated applications, and hosts the sandbox for generated tests",
     "https://nodejs.org  (Node 20 or newer is required: the permission model arrived in 20)"),
    ("python3", (3, 9), "runs the offline verification scripts", "https://www.python.org/downloads/"),
]

# Needed only for a desktop build or the UI render check.
OPTIONAL = [
    ("npx", "installs the Tauri CLI for a desktop build", "installed with Node"),
    ("chromium", "renders the desktop UI in the offline check (scripts/verify-ui.mjs)",
     "apt install chromium  /  brew install --cask chromium"),
]

problems: list[str] = []
notes: list[str] = []


def version_of(tool: str) -> str:
    try:
        out = subprocess.run(
            [tool, "--version"], capture_output=True, text=True, timeout=20, check=False
        )
        return (out.stdout or out.stderr).strip().splitlines()[0]
    except Exception as exc:  # noqa: BLE001 - the point is to report anything that goes wrong
        return f"<could not run: {exc}>"


def major_minor(text: str) -> tuple[int, int]:
    match = re.search(r"(\d+)\.(\d+)", text)
    return (int(match.group(1)), int(match.group(2))) if match else (0, 0)


print("toolchain preflight\n")

for name, minimum, why, install in REQUIRED:
    path = shutil.which(name)
    if not path:
        problems.append(f"{name} is missing — needed to {why}. Install: {install}")
        continue
    raw = version_of(name)
    found = major_minor(raw)
    if found < minimum:
        problems.append(
            f"{name} {found[0]}.{found[1]} is too old (need {minimum[0]}.{minimum[1]}+) — "
            f"needed to {why}. Install: {install}"
        )
    else:
        print(f"  ok      {name:<9} {raw.split(' ', 1)[-1][:40]:<40} {why}")

for name, why, install in OPTIONAL:
    path = shutil.which(name)
    if path:
        print(f"  ok      {name:<9} {version_of(name).split(' ', 1)[-1][:40]:<40} {why}")
    else:
        notes.append(f"{name} not found — only needed to {why}. Install: {install}")

# The desktop shell needs a system webview; a host-only build does not.
if sys.platform.startswith("linux"):
    has_webkit = shutil.which("pkg-config") is not None and subprocess.run(
        ["pkg-config", "--exists", "webkit2gtk-4.1"], check=False
    ).returncode == 0
    if has_webkit:
        print("  ok      webkit2gtk-4.1                          desktop shell can be built here")
    else:
        notes.append(
            "webkit2gtk-4.1 development files not found — the desktop shell cannot be built on "
            "this machine. Host-only builds and all tests still work. Install: "
            "apt install libwebkit2gtk-4.1-dev build-essential"
        )

print()
for note in notes:
    print(f"  note    {note}")

if problems:
    print("\nPROBLEMS")
    for problem in problems:
        print(f"  - {problem}")
    print(f"\nRESULT: FAIL — {len(problems)} required tool(s) missing or too old")
    sys.exit(1)

print("\nRESULT: PASS — every required tool is present and new enough")
