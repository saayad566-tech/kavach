#!/usr/bin/env python3
"""
Command-surface audit.

A command that exists in Rust but cannot be reached from the UI is either dead surface or an
unfinished feature. This walks the list the desktop shell registers, checks each one against
the UI, and requires that any command the UI does not call is named here with a reason.

That way "we'll wire it up later" cannot be quietly forgotten: adding a command without
either a caller or an entry below fails the build.

Run: python3 scripts/check-command-surface.py [--verbose]
"""

from __future__ import annotations

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
VERBOSE = "--verbose" in sys.argv

# Commands the UI must never call, each with the reason. Anything not here and not called by
# the UI is a gap.
INTERNAL = {
    "run_command": "runs an allow-listed child process; exposing it to the webview would hand "
                   "the page an execution primitive, which is the opposite of least privilege",
    "write_project_file": "writes into a project tree; the build pipeline and the model-code "
                          "stage use it. A UI editor would be a separate feature with its own "
                          "undo and validation, not a raw write",
    "delete_project_file": "deletes from a project tree; reachable only through restore, which "
                           "applies a whole tree atomically",
}

def read(path: pathlib.Path) -> str:
    return path.read_text() if path.exists() else ""

host = read(ROOT / "crates/kavach-host/src/lib.rs")
shell = read(ROOT / "apps/desktop/src-tauri/src/lib.rs")
ui = read(ROOT / "apps/desktop/ui/app.js")

# The command names the host publishes.
block = host.split("COMMAND_NAMES", 1)[1] if "COMMAND_NAMES" in host else ""
host_names = set(re.findall(r'"([a-z_]+)"', block.split("]", 1)[0]))

# The shell adds the one command only it can serve.
shell_only = set(re.findall(r'commands::([a-z_]+)', shell)) - host_names
registered = host_names | shell_only

called = set(re.findall(r"call\('([a-z_]+)'", ui))

problems: list[str] = []

missing = registered - called - set(INTERNAL)
for name in sorted(missing):
    problems.append(f"{name} is registered but nothing calls it and it is not declared internal")

stale_internal = set(INTERNAL) - registered
for name in sorted(stale_internal):
    problems.append(f"{name} is declared internal but is no longer a registered command")

now_called = set(INTERNAL) & called
for name in sorted(now_called):
    problems.append(f"{name} is declared internal but the UI calls it — remove the exemption")

unknown = called - registered
for name in sorted(unknown):
    problems.append(f"the UI calls {name}, which is not a registered command")

print("command surface")
print(f"  registered: {len(registered)}   called by the UI: {len(called)}   internal: {len(INTERNAL)}")
if VERBOSE:
    for name in sorted(called):
        print(f"    UI        {name}")
    for name in sorted(INTERNAL):
        print(f"    internal  {name}")

if problems:
    print("\nPROBLEMS")
    for problem in problems:
        print(f"  - {problem}")
    print(f"\nRESULT: FAIL — {len(problems)} problem(s)")
    sys.exit(1)

print("\nRESULT: PASS — every registered command is either reachable from the UI or declared "
      "internal with a reason")
