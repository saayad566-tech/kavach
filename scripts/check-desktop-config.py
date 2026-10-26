#!/usr/bin/env python3
"""
Static checks on the desktop shell configuration.

These catch what would otherwise surface 12 minutes into a Windows run: a missing icon, a
capability nobody references, a registered command that does not exist, a Tauri 1 leftover,
a placeholder in shipped Rust. They do not replace compiling the app.
"""

import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
DESKTOP = ROOT / "apps" / "desktop" / "src-tauri"

# Commands that live in the shell because only it can answer them.
SHELL_ONLY = {"workspace_root"}


def main() -> int:
    problems: list[str] = []
    notes: list[str] = []

    conf = json.loads((DESKTOP / "tauri.conf.json").read_text())

    for icon in conf.get("bundle", {}).get("icon", []):
        path = DESKTOP / icon
        if not path.exists():
            problems.append(f"missing icon: {icon}")
        elif path.stat().st_size < 200:
            problems.append(f"icon looks empty ({path.stat().st_size} bytes): {icon}")
    notes.append(f"icons: {len(conf.get('bundle', {}).get('icon', []))} referenced")

    declared = conf.get("app", {}).get("security", {}).get("capabilities", [])
    files = {}
    for path in (DESKTOP / "capabilities").glob("*.json"):
        files[json.loads(path.read_text()).get("identifier", "")] = path.name
    for ident in declared:
        if ident not in files:
            problems.append(f'capability "{ident}" declared but no file declares it')
    for ident, name in files.items():
        if ident not in declared:
            problems.append(f"capability file {name} is never referenced")
    notes.append(f"capabilities: {declared}")

    dist = (DESKTOP / conf.get("build", {}).get("frontendDist", "")).resolve()
    if not (dist / "index.html").exists():
        problems.append(f"frontendDist has no index.html ({dist})")
    else:
        notes.append("frontend: present")

    lib = (DESKTOP / "src" / "lib.rs").read_text()
    registered = re.findall(r"commands::(\w+)", lib)
    defined = set(re.findall(r"pub fn (\w+)", (DESKTOP / "src" / "commands.rs").read_text()))
    for cmd in registered:
        if cmd not in defined:
            problems.append(f"lib.rs registers commands::{cmd}, which commands.rs does not define")

    host_names = set(
        re.findall(r'"(\w+)"', (ROOT / "crates" / "kavach-host" / "src" / "lib.rs").read_text().split("COMMAND_NAMES")[1].split("];")[0])
    )
    for cmd in registered:
        if cmd in SHELL_ONLY:
            continue
        if cmd not in host_names:
            problems.append(f"command {cmd} is registered but absent from kavach-host::COMMAND_NAMES")
    notes.append(f"commands registered: {len(registered)}")

    cargo = (DESKTOP / "Cargo.toml").read_text()
    if re.search(r"^\[features\]", cargo, re.MULTILINE):
        problems.append("Cargo.toml declares a [features] table; Tauri 2 needs none")

    for rs in (DESKTOP / "src").glob("*.rs"):
        text = rs.read_text()
        for marker in ["todo!(", "unimplemented!(", "TODO:", "FIXME"]:
            if marker in text:
                problems.append(f"{rs.name} contains {marker}")

    labels = {w.get("label") for w in conf.get("app", {}).get("windows", [])}
    for name in files.values():
        caps = json.loads((DESKTOP / "capabilities" / name).read_text())
        for label in caps.get("windows", []):
            if label not in labels:
                problems.append(f"{name} targets window {label!r}, which the config does not define")
        for permission in caps.get("permissions", []):
            if permission.startswith(("fs:", "shell:", "http:")):
                problems.append(f"{name} grants {permission}; the window must not hold filesystem, shell or HTTP rights")

    print("desktop shell configuration")
    for note in notes:
        print(f"  {note}")
    if problems:
        print("\nPROBLEMS")
        for problem in problems:
            print(f"  - {problem}")
        return 1
    print("\nRESULT: PASS — configuration is internally consistent")
    return 0


if __name__ == "__main__":
    sys.exit(main())
