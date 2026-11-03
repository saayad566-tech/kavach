# Checklist — current state

Tick only what a command proved. Unticked items are in `KNOWN_ISSUES.md`.

## Engine

- [x] Host crate compiles with `--offline` and has zero dependencies
- [x] Path guard on every write; a traversal attempt is refused (tested)
- [x] Child processes: allow-list, timeout, process-group kill, CPU and file-size rlimits
- [x] SHA-256 implemented without crates and matching known vectors
- [x] Store: temp-then-rename, `.bak`, recovery from a corrupt primary, version header
- [x] Task ledger: eight states, evidence required for `verified`
- [x] Model catalogue: 12 models, 5 quants, device-aware recommendation
- [x] Model install: licence gate and hash enforced before adoption; refusal writes nothing

## Builder

- [x] Requirements → plan → scaffold → security scan → verify → report
- [x] Generated app passes its own suite (71 tests)
- [x] Model-authored files kept only if the project still passes; rolled back otherwise
- [x] High-severity security finding fails the build
- [x] Generated tests run under Node's permission model, child processes and network denied
- [x] Node older than 20 refused rather than run unsandboxed

## Desktop

- [x] 24 of 27 commands reachable from the UI; 3 internal by documented decision
- [x] 45 UI checks: render, labels, focus, live regions, tabs, drawer, no error banner
- [x] ChatGPT-style shell: project rail, thread, composer, inspector
- [x] Inspector reachable below 1200px — drawer opened from a button

## Full stack

- [x] `kavach-server` serves the UI and every command over loopback
- [x] All 27 commands dispatch over HTTP (enforced by a test that walks `COMMAND_NAMES`)
- [x] Unknown command → 404, bad JSON → 400, traversal → 404 (tested)
- [x] The UI runs in a browser with no Tauri, verified by rendering it against the real server

## Delivery

- [x] `scripts/verify-all.sh` — 10 checks green
- [x] `scripts/verify-zip.sh` — builds and tests from a clean extraction
- [x] `scripts/bootstrap.sh` — one command to set a machine up
- [x] `scripts/check-toolchain.py` — names the install line for anything missing
- [ ] Installer built from the current commit (K-01)
- [ ] Windows smoke test executed (K-02)
- [ ] Binaries signed (K-03)

## Not started

- [ ] Agent execution loop (K-07)
- [ ] Line coverage measurement (K-06)
