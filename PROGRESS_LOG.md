# Progress log

Newest first. One entry per session: what changed, what it cost, what is next.

## 2026-11-03 — Full stack: the platform runs in a browser

The desktop shell needs a system webview, which this machine does not have — so until now the
product could not be run at all. Wrote a PRD, then built `kavach-server`: one process serving
the UI and all 27 commands over loopback, zero dependencies, one thread per connection.

- `httpd.rs` — request parsing, routing, dispatch, static files; 12 tests
- every command dispatches over HTTP, enforced by a test that walks `COMMAND_NAMES`
- the UI switches transport in one `call()`: Tauri IPC when embedded, fetch when served
- verified end to end: server started, project created, catalogue and agents returned, UI
  rendered in a browser showing real hardware from the Rust probe

**Two bugs the tests caught:** the parser accepted a request line with no HTTP version, and my
own traversal test was wrong — `//etc/passwd` resolves *inside* the UI root, which is safe; the
dangerous case, `/../../etc/passwd`, is refused.

## 2026-11-03 — Every panel reachable; frontend wired to the whole backend

The UI showed six model recommendations but not the 12-model catalogue, and agents and skills
but not commands or rules — the backend returned data nothing rendered.

- inspector split into four tabs (Device, Models, Agents, Memory); only one panel is shown
- below 1200px the inspector is a drawer opened from a button, instead of being hidden (K-09)
- full catalogue table with best quant per model, plus the quantisation levels
- commands and rules listed; `agents_catalog` now returns them
- 45 UI checks (was 38), including that the drawer actually opens

**Two defects found by rendering:** the drawer button sat on top of the Build button, and
`wireInspector()` was never called because the patch anchored on the wrong line — the DOM
checks caught the second, the screenshot caught the first.

## 2026-11-03 — APEX-X constitution adopted; memory system created

Loaded `master-agent-constitution` and `capability-evolution`. Created the seven memory files
this project has been missing, so a session restart no longer costs rediscovery.

- `PROJECT_SPEC.md`, `MASTER_BLUEPRINT.md`, `DESIGN_SYSTEM.md`, `DECISIONS.md`,
  `KNOWN_ISSUES.md`, `CHECKLIST.md`, `PROGRESS_LOG.md`
- Prior turns recorded the corrections that had been costing time: crates.io reachable,
  Wasmtime the wrong tool, tree commits are snapshots.

**Next:** the agent execution loop (K-07) is the largest remaining product gap that does not
need a Windows machine.

## 2026-11-03 — Conversational desktop UI

Rebuilt the window as a chat-shaped shell. Two defects were found by rendering, not by the DOM
checks: device facts stacked label over value, and a blanket `nowrap` overflowed the model
tables. Both fixed. 142 Rust tests, 83 node tests, 38 UI checks.

## 2026-11-03 — Focus restored

The stock work had drifted into being the product. Moved to `examples/stocklab/` and labelled
an example. Nothing deleted.

## 2026-11-02 — Fundamental analysis engine

39 ratios and composites in every generated app. Two real bugs found while wiring: lowercased
CSV headers nulled every camelCase field, and Piotroski scored 8/9 without prior gross profit.

## 2026-11-01 — Local model manager

Catalogue, device-aware recommendation, registry with one active model, GGUF validation. The
surface checker caught a command with no caller.

## 2026-10-31 — Sandbox completed

Network denial via a preload; Node's permission model for the filesystem and child processes.
Recorded that `--allow-child-process` defeats the model.

## 2026-10-30 — Task ledger

Eight states, acceptance criteria, evidence-gated verification, notes.

## Earlier

Data pipeline and analysis library in generated apps; local inference; crash safety; Windows
build (run 40316051982, commit 9347d6f0); the repository rebuilt as a Rust-only desktop app.
