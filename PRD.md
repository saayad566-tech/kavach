# PRD — Kavach, full-stack local application builder

**Status:** approved by the builder (self-decided, per the session instruction)
**Version:** 1.0 · 2026-11-03

## 1. Problem

A person who wants a small working application has three options, all bad: pay a hosted
generator that sees their prompt and data, wire a stack together themselves, or give up and use
a spreadsheet. The privacy-first version of this tool exists in pieces — an engine, a desktop
shell — but the shell cannot be built without a system webview, so **the platform cannot
actually run anywhere today**. That is the gap this release closes.

## 2. Goal

One command starts Kavach, and it works in a browser on the machine in front of you, with no
Tauri, no account, and no network egress.

```bash
cargo run --bin kavach-server --release
# Kavach on http://127.0.0.1:4780
```

## 3. Users

| Persona | Need | What they get |
|---|---|---|
| Solo developer | A working project skeleton, not a blank page | Generated app with server, store, tests, passing |
| Non-developer with a real problem | Something that runs, without learning a stack | Same, plus a report saying what works and what does not |
| Privacy-sensitive operator | Nothing leaves the machine | Loopback-only server, no telemetry, no hosted model required |

## 4. Scope — this release

### 4.1 In scope

1. **HTTP server binary** (`kavach-server`), zero dependencies, loopback only.
2. **Full command surface over HTTP** — `POST /api/<command>` with a JSON body, returning the
   same JSON envelope the Tauri shell returns. All 27 commands, not a subset.
3. **Static UI** served from the same binary, so one process is the whole application.
4. **Transport fallback in the UI** — the same `app.js` talks to Tauri IPC when embedded and to
   `fetch` when served over HTTP. One codebase, two hosts.
5. **A PRD, and tests for the new code.**

### 4.2 Out of scope, with reasons

| Not building | Why |
|---|---|
| Authentication | Loopback-only, single user. Adding auth would imply it is safe to expose publicly, which it is not. |
| TLS | Same reason; a self-signed cert would add ceremony without adding safety on loopback. |
| A database server | The store is a file with atomic writes and a backup. A daemon adds a dependency and a failure mode for one user's data. |
| Cloud sync | Contradicts the product's reason to exist. |
| Windows installers | Needs a Windows machine (K-01/K-02). The server makes the product runnable without one. |

## 5. Requirements

| ID | Requirement | Acceptance |
|---|---|---|
| R-01 | Server binds loopback only | A connection from a non-loopback address is impossible; the bind address is `127.0.0.1`. |
| R-02 | Every registered command is reachable over HTTP | A test enumerates `COMMAND_NAMES` and asserts each dispatches. |
| R-03 | Unknown command returns 404 with a JSON error | Not a 500, not an empty body. |
| R-04 | Malformed JSON returns 400 with a JSON error | Never a panic, never a dropped connection. |
| R-05 | Static files cannot escape the UI directory | `GET /../../etc/passwd` returns 404. |
| R-06 | The UI works over HTTP exactly as over IPC | The same 45 UI checks pass with `fetch` as the transport. |
| R-07 | The server is testable without a browser | Request parsing, routing and dispatch are library functions with unit tests. |
| R-08 | One command starts everything | `cargo run --bin kavach-server` serves UI and API on one port. |

## 6. Non-functional

- **Zero dependencies.** The server is `std` only; the host crate stays audit-able.
- **Concurrency.** One thread per connection, bounded by the OS. No async runtime.
- **Failure.** A bad request never kills the process; a handler panic is caught and returned as
  a 500 rather than taking the server down.
- **Startup.** Prints the URL and the workspace path. Exits non-zero if the port is taken.

## 7. Success criteria

1. `cargo run --bin kavach-server` starts and serves the UI in a browser.
2. A build can be started from the browser and its report renders.
3. `scripts/verify-all.sh` green, including the new server tests.
4. `KNOWN_ISSUES.md` loses the "cannot run without Tauri" limitation and gains nothing false.

## 8. Risks

| Risk | Mitigation |
|---|---|
| Hand-written HTTP parser has a bug | Parse defensively, reject oversized/malformed input, unit-test the parser directly. |
| Exposing a build engine over HTTP widens the attack surface | Loopback bind, no CORS, no auth by design — and the same path guard and command allow-list the desktop app uses. |
| The UI diverges between the two hosts | One `call()` with a transport switch; the same 45 checks run against the HTTP host. |
