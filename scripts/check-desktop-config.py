#!/usr/bin/env python3
"""
Offline verification of the desktop shell and the Windows packaging setup.

Nothing here compiles the Tauri shell — that needs crates.io and a system webview. What it
does do is check every fact about the build that can be checked without one: that the icons
are real icons of the right sizes, that the versions agree across three files, that every
registered command exists, that the capability files are referenced and least-privileged,
that the workflows are valid YAML that would actually build and upload an installer, and
that no placeholder text or Tauri 1 leftover remains.

Run: python3 scripts/check-desktop-config.py [--verbose]
"""

from __future__ import annotations

import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
DESKTOP = ROOT / "apps" / "desktop" / "src-tauri"
UI = ROOT / "apps" / "desktop" / "ui"
HOST = ROOT / "crates" / "kavach-host"

# Commands that live in the shell because only it can answer them.
SHELL_ONLY = {"workspace_root"}

problems: list[str] = []
notes: list[str] = []
verbose = "--verbose" in sys.argv


def fail(message: str) -> None:
    problems.append(message)


def note(message: str) -> None:
    notes.append(message)


def load_toml(path: pathlib.Path) -> dict:
    """Read the handful of keys we need without a TOML dependency."""
    out: dict[str, object] = {}
    section = ""
    for raw in path.read_text().splitlines():
        line = raw.split("#", 1)[0].rstrip()
        if not line:
            continue
        if line.startswith("[") and line.endswith("]"):
            section = line.strip("[]").strip()
            out.setdefault(section, {})
            continue
        match = re.match(r'\s*([A-Za-z0-9_.\-]+)\s*=\s*(.+)', line)
        if not match:
            continue
        key, value = match.group(1), match.group(2).strip()
        target = out if section == "" else out.setdefault(section, {})
        if isinstance(target, dict):
            target[key] = value.strip('"')
    return out


def semver_ok(value: str) -> bool:
    return re.fullmatch(r"\d+\.\d+\.\d+", value) is not None


# ---------------------------------------------------------------- configuration

conf_path = DESKTOP / "tauri.conf.json"
try:
    conf = json.loads(conf_path.read_text())
except Exception as exc:  # noqa: BLE001
    print(f"cannot parse {conf_path}: {exc}")
    sys.exit(1)

for key in ("productName", "version", "identifier", "build", "app", "bundle"):
    if key not in conf:
        fail(f"tauri.conf.json is missing required key {key!r}")

version = str(conf.get("version", ""))
if not semver_ok(version):
    fail(f"tauri.conf.json version {version!r} is not a plain semver")

identifier = str(conf.get("identifier", ""))
if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9.\-]*", identifier):
    fail(f"identifier {identifier!r} is not a valid bundle identifier")
if identifier.endswith(".app"):
    fail(f"identifier {identifier!r} ends with '.app', which breaks macOS bundling")
if identifier == "com.tauri.dev":
    fail("identifier is still the Tauri default")

security = conf.get("app", {}).get("security", {})
if not security.get("csp"):
    fail("no content security policy is set")
if security.get("capabilities") is None:
    fail("app.security.capabilities is not set, so the window would get the default set")

if conf.get("app", {}).get("withGlobalTauri") is not True:
    note("withGlobalTauri is not enabled; the static UI uses window.__TAURI__.invoke and would fail")

targets = conf.get("bundle", {}).get("targets", [])
if not isinstance(targets, list):
    fail("bundle.targets must be a list")
else:
    for required in ("nsis", "msi"):
        if required not in targets:
            fail(f"bundle.targets does not include {required!r}, so no Windows installer would be produced")

if conf.get("bundle", {}).get("active") is not True:
    fail("bundle.active is not true, so no installers would be produced")

# Version drift between the three files that carry it.
desktop_cargo = load_toml(DESKTOP / "Cargo.toml")
host_cargo = load_toml(HOST / "Cargo.toml")
for label, cargo in (("apps/desktop/src-tauri/Cargo.toml", desktop_cargo), ("crates/kavach-host/Cargo.toml", host_cargo)):
    cargo_version = str(cargo.get("package", {}).get("version", ""))
    if cargo_version != version:
        fail(f"version drift: tauri.conf.json says {version}, {label} says {cargo_version}")

# ---------------------------------------------------------------- frontend

dist = (DESKTOP / conf.get("build", {}).get("frontendDist", "")).resolve()
if not (dist / "index.html").exists():
    fail(f"frontendDist has no index.html ({dist})")
else:
    note(f"frontend: {dist.relative_to(ROOT)}/index.html")
    for asset in re.findall(r'(?:src|href)="([^":]+)"', (dist / "index.html").read_text()):
        if not (dist / asset).exists():
            fail(f"index.html references {asset}, which is not in frontendDist")
    if (DESKTOP / conf.get("build", {}).get("beforeBuildCommand", "") or "x").name == "":
        pass
    if conf.get("build", {}).get("beforeBuildCommand"):
        note("beforeBuildCommand is set; the desktop build expects a static UI, so this should normally be empty")

# ---------------------------------------------------------------- commands

lib = (DESKTOP / "src" / "lib.rs").read_text()
commands_rs = (DESKTOP / "src" / "commands.rs").read_text()
registered = re.findall(r"commands::(\w+)", lib)
defined = set(re.findall(r"pub fn (\w+)", commands_rs))
for cmd in registered:
    if cmd not in defined:
        fail(f"lib.rs registers commands::{cmd}, which commands.rs does not define")

host_section = (HOST / "src" / "lib.rs").read_text().split("COMMAND_NAMES")[1].split("];")[0]
host_names = set(re.findall(r'"(\w+)"', host_section))
for cmd in registered:
    if cmd in SHELL_ONLY:
        continue
    if cmd not in host_names:
        fail(f"command {cmd} is registered but absent from kavach-host::COMMAND_NAMES")
for name in host_names:
    if name not in registered and name not in SHELL_ONLY:
        fail(f"kavach-host advertises {name!r} but the shell never registers it")
note(f"commands: {len(registered)} registered, {len(host_names)} in the host list")

handler_block = lib.split("generate_handler!")[1].split("])")[0] if "generate_handler!" in lib else ""
for cmd in defined:
    if cmd in SHELL_ONLY or cmd in host_names:
        if f"commands::{cmd}" not in handler_block:
            fail(f"{cmd} is defined but missing from generate_handler!, so the UI cannot call it")

if not re.search(r"pub fn run\(\)", lib):
    fail("lib.rs does not define pub fn run(), which main.rs calls")
if "kavach_lib::run()" not in (DESKTOP / "src" / "main.rs").read_text():
    fail("main.rs does not call kavach_lib::run()")

# ---------------------------------------------------------------- capabilities

declared = security.get("capabilities", [])
files: dict[str, pathlib.Path] = {}
for path in (DESKTOP / "capabilities").glob("*.json"):
    ident = json.loads(path.read_text()).get("identifier", "")
    files[ident] = path
    if ident != path.stem:
        fail(f"{path.name} declares identifier {ident!r}; the two must match")
for ident in declared:
    if ident not in files:
        fail(f'capability "{ident}" is declared but no file provides it')
for ident, path in files.items():
    if ident not in declared:
        fail(f"capability file {path.name} is never referenced")
    body = json.loads(path.read_text())
    if not body.get("windows"):
        fail(f"{path.name} targets no window")
    for permission in body.get("permissions", []):
        if permission.startswith(("fs:", "shell:", "http:")):
            fail(f"{path.name} grants {permission}; the window must hold no filesystem, shell or HTTP rights")
note(f"capabilities: {sorted(declared)}")

window_labels = {w.get("label") for w in conf.get("app", {}).get("windows", [])}
for path in files.values():
    for label in json.loads(path.read_text()).get("windows", []):
        if label not in window_labels:
            fail(f"{path.name} targets window {label!r}, which the config does not define")
if None in window_labels:
    fail("a window is declared without a label, so capabilities cannot target it")

# ---------------------------------------------------------------- icons

icons = conf.get("bundle", {}).get("icon", [])
if not icons:
    fail("no bundle icons are listed")
try:
    from PIL import Image
except Exception:  # noqa: BLE001
    Image = None  # type: ignore[assignment]
    note("Pillow is not installed: icon dimensions were not verified")

for icon in icons:
    path = DESKTOP / icon
    if not path.exists():
        fail(f"missing icon: {icon}")
        continue
    if path.stat().st_size < 200:
        fail(f"icon looks empty ({path.stat().st_size} bytes): {icon}")
        continue
    if Image is None:
        continue
    suffix = path.suffix.lower()
    if suffix == ".ico":
        with Image.open(path) as img:
            sizes = sorted(getattr(img, "info", {}).get("sizes", []) or [img.size])
        need = {(16, 16), (32, 32), (48, 48), (256, 256)}
        missing = need - set(sizes)
        if missing:
            fail(f"{icon} is missing sizes {sorted(missing)} (Windows uses 16, 32, 48 and 256)")
        note(f"icon.ico sizes: {sizes}")
    elif suffix == ".icns":
        if path.read_bytes()[:4] != b"icns":
            fail(f"{icon} does not start with the 'icns' magic bytes")
    elif suffix == ".png":
        with Image.open(path) as img:
            width, height = img.size
        expect_double = "@2x" in path.name
        base = int(re.match(r"(\d+)x\d+", path.name).group(1)) if re.match(r"(\d+)x\d+", path.name) else None
        if base and expect_double and (width, height) != (base * 2, base * 2):
            fail(f"{icon} is {width}x{height}; @2x means it should be {base * 2}x{base * 2}")
        if base and not expect_double and (width, height) != (base, base):
            fail(f"{icon} is {width}x{height}, but its name says {base}x{base}")
note(f"icons: {len(icons)} referenced and present")

# ---------------------------------------------------------------- Rust hygiene

cargo_text = (DESKTOP / "Cargo.toml").read_text()
if re.search(r"^\[features\]", cargo_text, re.MULTILINE):
    fail("Cargo.toml declares a [features] table; Tauri 2 needs none")
# Only an actual dependency feature counts; a comment explaining why it is absent does not.
code_lines = [line for line in cargo_text.splitlines() if not line.lstrip().startswith("#")]
if any("tauri/custom-protocol" in line for line in code_lines):
    fail("Cargo.toml still asks for the Tauri 1 custom-protocol feature")
if "kavach-host" not in cargo_text:
    fail("the desktop crate does not depend on kavach-host, so the commands would not compile")

host_dep = re.search(r'kavach-host\s*=\s*\{\s*path\s*=\s*"([^"]+)"', cargo_text)
if host_dep:
    resolved = (DESKTOP / host_dep.group(1)).resolve()
    if not (resolved / "Cargo.toml").exists():
        fail(f"kavach-host path dependency does not resolve: {host_dep.group(1)}")
    else:
        note(f"kavach-host resolves to {resolved.relative_to(ROOT)}")

for rs in sorted((DESKTOP / "src").glob("*.rs")) + sorted((HOST / "src").glob("*.rs")):
    text = rs.read_text()
    # Real incompleteness: an unimplemented macro call, anywhere.
    for marker in ("todo!(", "unimplemented!("):
        if marker in text:
            fail(f"{rs.name} contains {marker}")
    # A note-to-self only counts in a comment. builder.rs legitimately holds the strings
    # "TODO" and "FIXME" because it uses them to detect placeholders in generated code.
    for line in text.splitlines():
        stripped = line.lstrip()
        if stripped.startswith("//") and re.search(r"\b(TODO|FIXME)\b", stripped):
            fail(f"{rs.name} has a {stripped[:60]!r} comment")

if (DESKTOP / "build.rs").read_text().strip() != "fn main() {\n    tauri_build::build()\n}":
    fail("build.rs does not call tauri_build::build(), which Tauri requires")

# Windows-only code cannot be compiled here; at least confirm the FFI is declared once and used once.
probe = (HOST / "src" / "probe.rs").read_text()
if probe.count("fn GetDiskFreeSpaceExW") != 1:
    fail("GetDiskFreeSpaceExW should be declared exactly once in probe.rs")
if probe.count("disk_from_ffi(") < 2:
    fail("disk_from_ffi is declared but never called, or never tested")
if 'cfg(target_os = "windows")' not in probe:
    fail("no Windows branch found in probe.rs")

# ---------------------------------------------------------------- workflows

try:
    import yaml
except Exception:  # noqa: BLE001
    yaml = None  # type: ignore[assignment]
    note("PyYAML is not installed: workflow YAML was not parsed")

if yaml is not None:
    for wf in sorted((ROOT / ".github" / "workflows").glob("*.yml")):
        try:
            body = yaml.safe_load(wf.read_text())
        except Exception as exc:  # noqa: BLE001
            fail(f"{wf.name} is not valid YAML: {exc}")
            continue
        if not isinstance(body, dict) or "jobs" not in body:
            fail(f"{wf.name} has no jobs")
            continue
        note(f"{wf.name}: {len(body['jobs'])} job(s)")

    build_wf = ROOT / ".github" / "workflows" / "build.yml"
    if build_wf.exists():
        body = yaml.safe_load(build_wf.read_text())
        jobs = body.get("jobs", {})
        windows_jobs = {
            name: job for name, job in jobs.items() if "windows" in str(job.get("runs-on", "")).lower()
        }
        if not windows_jobs:
            fail("build.yml has no job that runs on a Windows runner")
        else:
            for name, job in windows_jobs.items():
                steps = " ".join(str(step) for step in job.get("steps", []))
                if "tauri-apps/cli" not in steps and "tauri-action" not in steps:
                    fail(f"job {name!r} never invokes the Tauri CLI")
                if "--bundles" not in steps and "tauri-action" not in steps:
                    fail(f"job {name!r} does not ask for bundles, so no installer would be produced")
                if "upload-artifact" not in steps:
                    fail(f"job {name!r} uploads nothing, so a passing build would leave no installer")
                if not job.get("needs"):
                    note(f"job {name!r} does not depend on the host job; it would build even if tests failed")
        for name, job in jobs.items():
            for step in job.get("steps", []):
                for action in [step.get("uses", "")] if step.get("uses") else []:
                    if "@" not in action:
                        fail(f"{name}: action {action!r} has no version pin")

# ---------------------------------------------------------------- report

print("desktop shell and packaging")
for item in notes:
    print(f"  {item}")
if verbose:
    print(f"  version {version}, identifier {identifier}")

if problems:
    print("\nPROBLEMS")
    for problem in problems:
        print(f"  - {problem}")
    print(f"\nRESULT: FAIL — {len(problems)} problem(s)")
    sys.exit(1)

print("\nRESULT: PASS — configuration and packaging setup are internally consistent")
print("NOTE: this does not prove the shell compiles. Only a real Tauri build does that.")
