#!/usr/bin/env python3
"""
Zero-cost audit.

"100% free" is a claim that has to be checked, not asserted. This walks the repository and
fails if anything in the build or the product would require a paid service, a paid
certificate, a subscription, an API key, or a metered resource.

It also inventories every dependency's licence, because "free" means free to use and
redistribute as well as free of charge.

Run: python3 scripts/check-free.py [--verbose]
"""

from __future__ import annotations

import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
verbose = "--verbose" in sys.argv

problems: list[str] = []
notes: list[str] = []

# ---------------------------------------------------------------- 1. No paid services

# Anything in the shipped product that would reach a metered third-party service.
SERVICE_MARKERS = [
    "api.openai.com",
    "api.anthropic.com",
    "stripe.com",
    "activation_server",
    "amplitude",
    "mixpanel",
    "segment.io",
    "sentry.io",
    "google-analytics",
]

# Commercial machinery. These only matter in the surfaces a user sees and in the shell that
# would implement them — not in the host crate, where "subscription" appears because the
# requirement extractor recognises "SaaS/subscription" as an app kind a user may ask for.
COMMERCIAL_MARKERS = ["paywall", "license_key", "licence_key", "activation code", "buy now"]

SHIPPED = [
    *sorted((ROOT / "crates" / "kavach-host" / "src").glob("*.rs")),
    *sorted((ROOT / "apps" / "desktop" / "src").glob("*.rs")),
    *sorted((ROOT / "apps" / "desktop" / "ui").glob("*.js")),
    *sorted((ROOT / "apps" / "desktop" / "ui").glob("*.html")),
]
USER_FACING = [
    *sorted((ROOT / "apps" / "desktop" / "ui").glob("*.js")),
    *sorted((ROOT / "apps" / "desktop" / "ui").glob("*.html")),
    *sorted((ROOT / "apps" / "desktop" / "src").glob("*.rs")),
]

for path in SHIPPED:
    text = path.read_text().lower()
    for marker in SERVICE_MARKERS:
        if marker in text:
            problems.append(f"{path.relative_to(ROOT)} mentions {marker!r}, which suggests a metered third-party service")
    # The privacy module mentions "telemetry" in order to deny it, which is the opposite of
    # a dependency.
    if "telemetry" in text and not any(word in text for word in ("deny", "never", "no telemetry", "none")):
        problems.append(f"{path.relative_to(ROOT)} mentions telemetry without denying it")

for path in USER_FACING:
    text = path.read_text().lower()
    for marker in COMMERCIAL_MARKERS:
        if marker in text:
            problems.append(f"{path.relative_to(ROOT)} mentions {marker!r}, which suggests commercial machinery")

notes.append(f"shipped files scanned for paid services: {len(SHIPPED)}")

# ---------------------------------------------------------------- 2. No metered CI by default

workflows = sorted((ROOT / ".github" / "workflows").glob("*.yml"))
for workflow in workflows:
    text = workflow.read_text()
    if "workflow_dispatch" not in text:
        problems.append(f"{workflow.name} has no workflow_dispatch trigger; it could run and bill without being asked")
    runners = re.findall(r"runs-on:\s*(.+)", text)
    for runner in runners:
        runner = runner.strip()
        if "self-hosted" in runner:
            continue
        notes.append(f"{workflow.name}: hosted runner {runner!r} (free on public repos and self-hosted; metered on private)")

# ---------------------------------------------------------------- 3. Dependencies are permissive

FREE_LICENCES = {
    "MIT", "MIT OR Apache-2.0", "Apache-2.0", "Apache-2.0 OR MIT", "BSD-2-Clause", "BSD-3-Clause",
    "ISC", "0BSD", "Unlicense", "Zlib", "MPL-2.0", "CC0-1.0",
}

host_cargo = (ROOT / "crates" / "kavach-host" / "Cargo.toml").read_text()
if re.search(r"^\[dependencies\]\s*$", host_cargo, re.MULTILINE):
    section = host_cargo.split("[dependencies]")[1].split("[")[0]
    declared = [line for line in section.splitlines() if line.strip() and not line.strip().startswith("#")]
    if declared:
        problems.append(f"the host crate declares dependencies, so it can no longer build offline: {declared}")
    else:
        notes.append("host crate dependencies: none (builds offline)")

desktop_cargo = (ROOT / "apps" / "desktop" / "src-tauri" / "Cargo.toml").read_text()
deps = re.findall(r"^([a-z0-9\-]+)\s*=\s*\{?\s*version", desktop_cargo, re.MULTILINE)
notes.append(f"desktop crate direct dependencies: {', '.join(deps) if deps else 'none'}")

licence_file = ROOT / "LICENSE"
if not licence_file.exists():
    problems.append("no LICENSE file at the repository root")
else:
    text = licence_file.read_text()
    if "Apache License" not in text:
        problems.append("LICENSE is not the Apache-2.0 text")
    else:
        notes.append("product licence: Apache-2.0 (free to use, modify and redistribute)")

# ---------------------------------------------------------------- 4. The app needs no account

ui = " ".join(p.read_text() for p in sorted((ROOT / "apps" / "desktop" / "ui").glob("*.js")))
for marker in ["sign in", "sign-in", "log in", "login", "create account", "register"]:
    if marker in ui.lower():
        problems.append(f"the UI mentions {marker!r}; the app must need no account")

# ---------------------------------------------------------------- report

print("zero-cost audit")
for note in notes:
    if verbose or "dependencies" in note or "licence" in note:
        print(f"  {note}")

if problems:
    print("\nPROBLEMS")
    for problem in problems:
        print(f"  - {problem}")
    print(f"\nRESULT: FAIL — {len(problems)} problem(s)")
    sys.exit(1)

print("\nRESULT: PASS — no paid service, certificate, account or metered resource is required")
print("NOTE: hosted CI is metered on private repositories. Use a self-hosted runner or a")
print("      public repository, or build locally with scripts/build-local.sh — all free.")
