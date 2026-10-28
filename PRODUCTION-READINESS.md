# Production readiness

An honest assessment, 26 October 2026. Version 0.6.0.

"Production ready" is not a state a repository reaches by adding a checklist file. This
document states the criteria, what is true today, and what is still missing — including the
things I cannot do from here at all.

---

## Cost

Zero. No backend, no account, no metered API, no telemetry, no bundled weights. Building is
free locally or on a self-hosted runner. The only genuine cost in the project is a public
code-signing certificate. See `FREE.md`; `scripts/check-free.py` verifies it.

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
| 2 | Test suite passes on the target platform | **Yes** | 70 tests (67 unit + 1 crash-safety + 2 integration), green on Linux; 67 green on Windows |
| 3 | Installer exists | **Yes** | NSIS `.exe` and WiX `.msi`, checksums in `BUILD-EVIDENCE.md` |
| 4 | Installer has been installed and exercised | **No** | No Windows machine here. `scripts/smoke-test-windows.ps1` automates 11 checks and prompts for 8 |
| 5 | Binaries are code-signed | **Partly** | Free self-signed route implemented (`scripts/make-self-signed-cert.ps1`) for internal use; public distribution still needs a paid certificate, which is a business decision |
| 6 | Generated code is sandboxed | **Yes** | Tests run inside Node's permission model: filesystem reads and writes confined to the project and the OS temp directory, child processes denied. Network access is denied by a preload that blocks net, tls, dns, http, https, dgram and fetch — Node 20 has no switch for it. On top of that: an allow-list, wall-clock timeout, process-group kill, kernel-enforced CPU-time and per-file-write limits, and a path guard. The preload is a preload, not a kernel boundary |
| 7 | Secrets are not committed or generated | **Yes** | `security.rs` scans every generated project; a high-severity finding fails verification |
| 8 | No telemetry, no silent network egress | **Yes** | No HTTP client in the host except a loopback socket to the user's own model server; the policy engine denies the telemetry category outright |
| 9 | Local model support | **Yes, bounded** | Discovery, generation and error handling tested against real sockets. The model writes notes and may add code; model code is kept only if the project's own tests still pass, and rolled back if they do not |
| 10a | A user can reopen previous work | **Yes** | Project rows are clickable: reopening restores the project directory, checkpoints and file tree. Before this, only the project just built was reachable |
| 10b | Checkpoints on demand | **Yes** | `create_checkpoint` command + button; builds still checkpoint automatically |
| 10d | Install and verify a local model | **Yes** | Model panel: licence acceptance and SHA-256 are enforced in Rust before a file is adopted; disk space is reported before installing. Nothing is downloaded |
| 10c | Inspect and re-scan a project | **Yes** | File reader (`read_project_file`, path-guarded) and a "re-scan for secrets" button |
| 10 | Accessibility | **Partly** | Skip link, landmarks, labelled controls, `aria-live` status, visible focus, reduced-motion support, all asserted in `verify-ui.mjs`. No screen-reader or high-contrast testing |
| 11 | Crash safety | **Yes** | Index written temp-then-rename, the previous copy is kept as `index.tsv.bak`, a corrupt primary falls back to it automatically, and the format carries a version header. `tests/crash_safety.rs` SIGKILLs the writer six times and asserts coherence |
| 12 | Upgrade path | **No, deliberately** | No updater. An auto-updater phones home, which contradicts the product's core promise; the supported path is a manual install of a newer release over the old one, with data in the app-data directory untouched. Tauri's updater keys are free (ed25519) if you decide a network check is acceptable |
| 13 | Documentation for operators | **Yes** | `WINDOWS-BUILD.md`, `BUILD-EVIDENCE.md`, `CI-EVIDENCE.md`, `README.md`, this file |
| 14 | Licence compliance for bundled weights | **N/A yet** | No weights are bundled or downloaded by the host; the licence gate exists and is tested for when they are |
| 15 | Privacy policy / terms | **No** | A product decision and a legal one, not something to invent here |

---

## What changed in this pass

### The sandbox depended on a version nobody checked

Generated tests run under Node's permission model. That model arrived in Node 20 — and nothing
verified which Node was installed. On Node 18 the flag is rejected and every test fails, which
is safe but unhelpful; the real risk is a runtime that accepts the flag and ignores it.

The builder now reads `node --version` before running anything and refuses to verify a project
on an older runtime, naming the problem. Generated `package.json` declares
`"engines": { "node": ">=20" }`, so the requirement travels with the project.

### The toolchain is declared, not assumed

`rust-toolchain.toml` pins the channel. `rust-version` states the minimum. And
`scripts/check-toolchain.py` checks every tool the build actually uses — rustc, cargo, node,
python, plus npx, chromium and webkit2gtk as optional — printing the install line for anything
missing. Both build scripts and `verify-all.sh` run it first, so a fresh machine gets a list of
what to install instead of a linker error forty seconds in.


### The sandbox is real, and it is not Wasmtime

Earlier passes recorded "needs Wasmtime" as the fix for unsandboxed generated code. That was
the wrong tool: generated applications are JavaScript, and Wasmtime cannot run JavaScript.
The right mechanism was already on the machine.

`node --experimental-permission` confines a process to an explicit filesystem allow-list.
Each generated test file now runs under it, with reads and writes limited to the project and
the OS temp directory. Three tests prove it rather than assert it: a test that reads
`/etc/hostname` fails, a test that shells out fails, and an ordinary test that writes its own
scratch files still passes.

One finding is worth recording. The obvious implementation — `node --test` with
`--allow-child-process`, because the runner spawns a child per file — is **not a sandbox**.
Node's own warning says the flag "could invalidate the permission model", and it does: with
it, a test spawned `cat /etc/hostname` and read the result. Running each test file directly
keeps everything in one process, which is why the grant stays meaningful.

The last hole was network access: Node 20's permission model has no switch for it, so a
generated test could still open a socket. That is now closed by a preload
(`assets/app/test-guard/network-guard.mjs`) that blocks `net`, `tls`, `dns`, `http`, `https`,
`dgram` and the global `fetch` before the test file loads. Two tests prove it: a test calling
`net.connect` fails, and a test calling `fetch` fails.

It is a preload, not a kernel boundary — a determined test could reload a module and get past
it. It stops ordinary code, which is what a generated test is, and the report says which of
those two things it is.


### Every registered command now has a destination

Seven commands had no caller. Four now drive a model panel (install, verify, hash, disk
space); the remaining three are declared internal with a reason and enforced by
`scripts/check-command-surface.py`, which fails the build if a command is added without either
a caller or an exemption. "Wire it up later" can no longer be forgotten.

The model panel is the completion of a claim the product was making without the means to
support it: Kavach said it could run local models, but there was no way to install one. The
panel takes a file you already have, refuses it without an accepted licence, refuses it on a
hash mismatch, and writes nothing in either case — all three asserted at the command boundary.


### Features that existed but nothing could reach

Ten host commands were implemented and tested with no way to invoke them from the UI. Four
now have one: on-demand checkpoints, a security re-scan, a file reader, and a file listing.
The others (`run_command`, the raw file writers) stay unexposed deliberately — a UI that can
run arbitrary commands is the opposite of least privilege.

The bigger gap was that a user could only act on the project they had *just* built: restart
the app and all previous work became unreachable. Project rows are now clickable and restore
the directory, checkpoints and file tree.

### Generated applications

Every generated app now links to its dataset as CSV and shows a trend line for its numeric
field, so the analysis the pipeline computes is visible rather than only available over HTTP.


### Resource limits on generated code

Every command a generated project runs now gets two kernel-enforced limits, set in
`pre_exec` so they apply to the whole process tree: a CPU-time cap slightly above the
wall-clock budget, and a per-file write cap of 512 MiB. The first stops a runaway loop even if
the poll loop is delayed; the second stops a build filling the disk. Both are tested — a
`while :; do :; done` loop is killed in about a second, and a 5 MB write is refused at 4 KiB.

This is not a sandbox. It is containment of the two failure modes that actually destroy a
user's machine, and it is honest about the difference.

### The store can lose a file without losing your work

The index now carries a version header, keeps the previous good copy as `index.tsv.bak`, and
falls back to it when the primary is unreadable. A file written by an earlier version still
loads. Tested by corrupting the primary and asserting the projects come back.


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
- **The model writes notes, and may contribute code — under a rule.** A proposal is parsed,
  validated (safe relative path, allowed extension, no placeholders, no secrets, no overwriting
  generated files, size and count limits) and written. Then the project's own `node --test`
  runs. If it passes, the model's files stay; if it fails, they are deleted, the tests are
  re-run, and the delivery reports the rollback. A model that breaks the build cannot ship
  broken code — which is the only way this feature is safe with a small model.

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
5. **Line coverage.** `cargo-llvm-cov` needs llvm-tools and an instrumented rebuild, so the gate is structural instead
   (every module tested, required suites present). The crash harness now exists and runs.

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
