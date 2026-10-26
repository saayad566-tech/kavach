#!/usr/bin/env python3
"""
Check that the UI only calls commands the host actually implements, with the argument
names the shell passes through.

The UI is plain JavaScript talking to Tauri, so a typo in a command name would only appear
when a user clicked the button. This reads `ui/app.js`, extracts every `call('name', {...})`
and compares the names against `kavach-host::COMMAND_NAMES`.
"""

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
UI = ROOT / "apps" / "desktop" / "ui" / "app.js"
HOST_LIB = ROOT / "crates" / "kavach-host" / "src" / "lib.rs"
SHELL_COMMANDS = ROOT / "apps" / "desktop" / "src-tauri" / "src" / "commands.rs"


def main() -> int:
    ui = UI.read_text()
    host_section = HOST_LIB.read_text().split("COMMAND_NAMES")[1].split("];")[0]
    host_names = set(re.findall(r'"(\w+)"', host_section))
    shell_names = set(re.findall(r"pub fn (\w+)", SHELL_COMMANDS.read_text()))

    used = re.findall(r"call\(\s*'([a-z_]+)'", ui)
    problems = []
    for name in sorted(set(used)):
        if name not in host_names and name not in shell_names:
            problems.append(f"UI calls '{name}', which no command implements")

    # Argument names the UI passes, checked against the shell's parameter list.
    for match in re.finditer(r"call\(\s*'([a-z_]+)'\s*,\s*\{([^}]*)\}", ui):
        command, raw = match.group(1), match.group(2)
        params = set(re.findall(r"([A-Za-z_]+):", raw))
        # camelCase in JS maps to snake_case in Rust.
        def snake(name: str) -> str:
            return re.sub(r"(?<!^)(?=[A-Z])", "_", name).lower()

        expected = {snake(p) for p in params}
        signature = re.search(rf"pub fn {command}\(([^)]*)\)", SHELL_COMMANDS.read_text())
        if signature:
            accepted = set()
            for part in signature.group(1).split(","):
                bits = part.split(":")
                if len(bits) >= 2:
                    accepted.add(bits[0].strip())
            for arg in expected:
                if arg not in accepted:
                    problems.append(f"UI passes '{arg}' to {command}, which does not accept it")

    print("UI ↔ command contract")
    print(f"  commands called by the UI: {len(set(used))} ({', '.join(sorted(set(used)))})")
    if problems:
        print("\nPROBLEMS")
        for problem in problems:
            print(f"  - {problem}")
        return 1
    print("\nRESULT: PASS — every UI call resolves to an implemented command")
    return 0


if __name__ == "__main__":
    sys.exit(main())
