# Production readiness

An honest assessment, 26 October 2026. Version 0.6.0.

"Production ready" is not a state a repository reaches by adding a checklist file. This
document states the criteria, what is true today, and what is still missing — including the
things I cannot do from here at all.

---

## Verdict

**Not ready to sell. Ready for internal use and technical preview.**

The application builds on Windows, produces installers, generates working projects, verifies
them by execution, scans them for secrets, and can talk to a local model. What is missing is
everything that surrounds software rather than being software: a signed binary, a smoke test
on a real Windows machine, an update path, a support process, and a security review of the
one thing that genuinely worries me — generated code running as an ordinary child process.

---

## Criteria

| # | Criterion | Status | Evidence or blocker |
|---|---|---|---|
| 1 | Builds reproducibly on the target platform | **Yes** | Run 40316051982, `windows-latest`, installers produced; `Cargo.lock` now committed (407 packages) |
| 2 | Test suite passes on the target platform | **Yes** | 66 tests (64 unit + 2 integration), green on Windows and Linux |
| 3 | Installer exists | **Yes** | NSIS `.exe` and WiX `.msi`, checksums in `BUILD-EVIDENCE.md` |
| 4 | Installer has been installed and exercised | **No** | No Windows machine here. `scripts/smoke-test-windows.ps1` automates 11 checks and prompts for 8 |
| 5 | Binaries are code-signed | **No** | Needs an Authenticode certificate the owner must buy; config documented in `WINDOWS-BUILD.md` §7 |
| 6 | Generated code is sandboxed | **No** | Runs as a child process with an allow-list, timeout and path guard. No WASM or container isolation. Largest security gap |
| 7 | Secrets are not committed or generated | **Yes** | `security.rs` scans every generated project; a high-severity finding fails verification |
| 8 | No telemetry, no silent network egress | **Yes** | No HTTP client in the host except a loopback socket to the user's own model server; the policy engine denies the telemetry category outright |
| 9 | Local model support | **Partly** | Discovery, connection, generation and error handling are implemented and tested against real sockets. The model writes project notes; it does not write application code |
| 10 | Accessibility | **Partly** | Skip link, landmarks, labelled controls, `aria-live` status, visible focus, reduced-motion support, all asserted in `verify-ui.mjs`. No screen-reader or high-contrast testing |
| 11 | Crash safety | **Partly** | The store writes via a temp-then-rename path; no kill-loop harness yet (B-22) |
| 12 | Upgrade path | **No** | No updater configured; `createUpdaterArtifacts` is off |
| 13 | Documentation for operators | **Yes** | `WINDOWS-BUILD.md`, `BUILD-EVIDENCE.md`, `CI-EVIDENCE.md`, `README.md`, this file |
| 14 | Licence compliance for bundled weights | **N/A yet** | No weights are bundled or downloaded by the host; the licence gate exists and is tested for when they are |
| 15 | Privacy policy / terms | **No** | A product decision and a legal one, not something to invent here |

---

## What changed in this pass

### Local inference is now implemented, tested, and honestly bounded

The headline feature was unimplemented. It is now real:

- `runtime.rs` speaks HTTP/1.1 over a loopback TCP socket to a model server the user is
  already running — Ollama on 11434, or a llama.cpp server on 8080. No HTTP client crate, no
  TLS, no network.
- A non-loopback host is refused **before** a socket is opened, so "local" cannot quietly
  become "remote".
- Discovery never starts a server. If nothing is listening, that is what the app reports.
- Responses are parsed by a hand-written JSON parser (nested objects, arrays, escapes,
  `\u` sequences), capped at 4 MiB, with chunked encoding rejected rather than mis-parsed.
- **The model writes the project notes, not the application code.** A 7B model can write four
  good bullet points; emitting a valid multi-file manifest needs a larger model, and
  pretending otherwise would produce a builder that fails unpredictably. The UI states exactly
  which of the two happened.

Tested against real sockets, not mocks. Unit tests cover Ollama's `/api/generate`, OpenAI's
`/v1/chat/completions`, an HTTP 500, a chunked response, malformed JSON, and discovery finding
nothing versus finding a server. An integration test (`crates/kavach-host/tests/local_runtime.rs`)
then binds the real port 11434, and asserts that the real builder discovers the runtime, gets a
completion, writes `docs/MODEL-NOTES.md` into the project, still passes the project's own
`node --test`, and passes the security scan. A second integration test asserts that with no
runtime the delivery says so in its limitations rather than implying a model was used.

### The security task is now provable

Before, the plan contained "Run security checks" and the ledger could never evidence it, so
every delivery reported it as unverified. `security.rs` now scans generated files for private
keys, AWS key ids, hardcoded secrets (with placeholder detection to avoid crying wolf),
`eval`/`Function`/`child_process` sinks, plaintext remote URLs and long opaque tokens. A
high-severity finding **fails the verify stage**, so a delivery cannot claim success with a
committed key in it.

### Export

A builder that traps your work is not a builder. `export_project` copies the tree to a
directory outside the project, excluding Kavach's own state, refusing a destination inside the
project and refusing a relative path.

### Accessibility

Skip link, `<main>` landmark, `role="status"` + `aria-live` on the badge and busy indicator,
`role="alert"` on errors, labels on every control, `:focus-visible` outlines and
`prefers-reduced-motion` support — each asserted by a check that fails if it regresses.

---

## What I could not do here, and why

1. **Sign the installers.** Requires a certificate you purchase. Unsigned installers show
   SmartScreen warnings.
2. **Install and run the app on Windows.** No Windows machine in this environment. A build
   that compiles is not a build that works; the smoke-test script exists precisely because
   that step is still owed.
3. **Sandbox generated code.** Needs Wasmtime or a similar runtime, which needs crates.io,
   which is blocked here. This is the gap I would prioritise next, and it is a design change
   rather than a patch.
4. **Prove the current commit builds.** Code changed after the successful build (runtime,
   security, export, UI). The installers correspond to commit `9347d6f0`; a rebuild is needed
   to attach them to `HEAD`, and it costs another CI run.
5. **Coverage measurement and crash harness.** `cargo-llvm-cov` needs crates.io; the kill-loop
   harness is straightforward but needs a rebuild to mean anything.

---

## The path to shippable

Ordered by risk removed per unit of effort:

1. **Rebuild and smoke-test.** One CI run plus a Windows machine. Turns "compiles" into "works".
2. **Sign.** Buy the certificate, wire the secrets, verify with `signtool`.
3. **Sandbox generated code** with Wasmtime and a capability-limited filesystem view.
4. **Updater.** Tauri's updater plus a signed manifest, so users get fixes without reinstalling.
5. **Crash harness** (B-22) and a coverage gate (B-21).
6. **Screen-reader pass** and a high-contrast check, which no automated check substitutes for.
7. **Privacy policy and terms**, written by someone qualified to write them.

Until 1 and 2 are done, treat this as a technical preview: it will build, run, and generate
working software, and Windows will warn anyone who installs it.
