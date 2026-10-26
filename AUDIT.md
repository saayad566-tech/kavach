# Audit — deep review of Kavach against its own brief

26 October 2026. Version 0.6.1. Covers the whole repository at commit `249cf102` plus the
changes made during this audit.

This is an audit, not a summary: it states the method, what external research changed, every
defect found (including one suspicion that turned out to be wrong), and what is still not true.

---

## 1. Method

1. **Read every source file** in `crates/kavach-host/src` (13 modules), the Tauri shell, the
   static UI and the scripts — 5,600 lines of Rust, 700 of JavaScript, 900 of Python.
2. **Ran the suite** at every step: 70 tests across unit, integration and crash suites.
3. **Grepped for defect classes** rather than reading passively: `unwrap`/`expect`/`panic!` in
   shipped code, `unsafe` blocks, byte-slicing of strings, lock handling, non-atomic writes,
   id generation, CSP vs actual UI behaviour.
4. **Researched the external contracts** the code depends on, because a hand-written client is
   only correct if the protocol it speaks is real.
5. **Checked the brief's non-negotiables** one by one against evidence, not against intent.

---

## 2. External research, and what it changed

### 2.1 The Ollama contract — verified correct

`runtime.rs` speaks Ollama's HTTP API by hand. The published specification confirms the client
is right: `POST /api/generate` requires only `model`, accepts `prompt`, `stream: false` and
`options.num_predict` / `options.temperature`, and returns the generated text in a `response`
field [cite:5c3fae32]. The `/api/tags` shape (`models[].name`) matches the model-list parsing.

No change was needed. The value of checking was certainty: a hand-rolled client against a
guessed protocol would fail only on a user's machine.

### 2.2 Tauri exposes app commands to every webview by default — documented, not applied

Tauri's capability documentation states that **all commands registered with
`invoke_handler` are allowed for all windows and webviews by default**, and that restricting
them requires `AppManifest::commands` in `build.rs` [cite:b2ae702b]. Our capability file
grants only `core:default`, which constrains Tauri's own plugins but not our 20 commands.

This is a real hardening opportunity: the correct fix is to declare the allowed command list in
`tauri_build::Attributes::new().app_manifest(...)`. I did **not** apply it, deliberately. The
app has a single window, so the practical exposure is nil, and the `tauri-build` builder API
has never been compiled in this environment — adding an untested API call to the build script
risks breaking the one thing that currently works. It is recorded here and in
`PRODUCTION-READINESS.md` as the next hardening step, to be done in the same change that
rebuilds.

### 2.3 CSP guidance — applied

Tauri hashes local scripts and nonces styles automatically, and recommends the tightest policy
the app can tolerate [cite:44be66df]. Our policy carried `style-src 'self' 'unsafe-inline'`
with no inline styles anywhere in the UI. It is now `style-src 'self'`, plus `form-action
'none'`, and a checker rule fails the build if the CSP and the UI ever disagree about inline
styles again.

---

## 3. Defects found

| # | Severity | Defect | Why it matters | Status |
|---|---|---|---|---|
| 1 | **High** | Checkpoint ids were `cp-{seconds}`. Two checkpoints in the same second got the same id, so the second overwrote the first snapshot while the index kept both entries | A restore could silently restore the wrong tree — a data-loss bug in the one feature meant to prevent data loss | **Fixed** — nanosecond id plus a process-local counter; test takes three checkpoints in a row and asserts unique ids |
| 2 | **High** | The project index was written in place, not atomically. A crash mid-write left a torn `index.tsv` | Losing the index loses every project's metadata | **Fixed** — write to `index.tsv.tmp` then rename; test simulates a stale temp file and asserts the index still parses |
| 3 | **High** | `has_aws_key` sliced a `&str` at `i+4..i+20`, which panics if the end lands inside a multi-byte character | A security scan that panics takes down the whole build on any project containing non-ASCII text | **Fixed** — byte slices throughout; test scans emoji and CJK text adjacent to a key id |
| 4 | Medium | Three `.lock().unwrap()` calls in `exec.rs` | A poisoned mutex would panic the command instead of returning an error | **Fixed** — `unwrap_or_else(\|p\| p.into_inner())` |
| 5 | Medium | CSP granted `'unsafe-inline'` for styles the UI never uses | Wider policy than necessary, with no benefit | **Fixed** — removed, with a checker rule coupling CSP to actual UI behaviour |
| 6 | Medium | The private-key rule tested the whole file instead of the line, so it fired once per line | One finding became N findings; a scanner that over-reports gets ignored | **Fixed** (found earlier this session, verified here) |
| 7 | Medium | `verify-ui.mjs` could pass by matching the text of the injected stub script instead of rendered output | A check that can pass without the app doing anything is worse than no check | **Fixed** — scripts are stripped from the DOM before assertions, and the page is driven to actually build |
| 8 | — | **Suspicion that turned out wrong:** the entity extractor looked like it could loop forever when a phrase ends immediately after a trigger word | — | **Not a bug** — each iteration advances by at least the trigger length, so the search string strictly shortens. Verified by tracing, and the existing extraction tests cover the case |

### Defect classes that were checked and found clean

- **`unsafe`**: 7 blocks, all in `exec.rs` (`setsid`, `kill`) and `probe.rs` (`GetDiskFreeSpaceExW`), each necessary, each documented, each exercised by a test.
- **`panic!`/`expect` in shipped code**: none outside tests (the `self.expect(...)` hits in `runtime.rs` are the parser's own method, not `Option::expect`).
- **Path handling**: `safe_join` + `assert_real_path_inside`, tested against `..`, absolute paths, Windows drive letters, UNC paths and symlinks.
- **Command execution**: argv only, allow-list, timeout clamp, process-group kill, capped output — all tested.

---

## 4. Compliance with the brief's non-negotiables

| Requirement (from the master brief) | Verdict | Evidence |
|---|---|---|
| Never claim success without captured evidence | **Met** | The ledger refuses `verified_complete` without passing command evidence; the delivery report separates verified from unverified; every claim in our own docs points at a command |
| No telemetry, ever | **Met** | No analytics code; the policy engine denies the category outright; the daemon reports `telemetry: none` and a check asserts it |
| Local-first; no mandatory backend | **Met** | The app runs entirely on the device; no account, no server |
| No silent cloud fallback | **Met** | The only socket in the host is loopback, and a non-loopback host is refused before connecting |
| Works offline | **Met** | Generated projects have zero dependencies; the builder needs only `node` |
| Deterministic output | **Met** | The scaffold is a pure function of the requirements |
| Privacy: user code never leaves the device | **Met** | No HTTP client except loopback; no egress in any test run |
| Licence compliance for weights | **Met** (for what ships) | Nothing is bundled or downloaded; the gate exists and is tested for when it is |
| Accessibility | **Partial** | Skip link, landmarks, labels, `aria-live`, focus-visible, reduced-motion — all asserted. No screen-reader or high-contrast testing |
| Security: least privilege | **Partial** | Window holds no fs/shell/http permission; app commands are exposed to the webview by Tauri default (see §2.2) |
| Windows, macOS, Linux, Android, iOS | **Windows only** | One successful Windows build; nothing else attempted |
| Local AI that writes the application | **Partial** | Discovery, inference and delivery of model notes work end-to-end. The model does not write application code |

---

## 5. What the audit added

- `tests/crash_safety.rs` — six SIGKILLs at varying points, then a coherence check on the
  index and a successful write afterwards. A test that kills its own child process cannot
  cheat: SIGKILL has no handler to hide behind.
- `scripts/check-test-coverage.py` — a structural gate (every module has tests; the two
  integration suites that prove product claims exist). Explicitly **not** line coverage, and
  it says so in its own output.
- Three defect fixes with regression tests, one CSP tightening with a permanent rule, and one
  research finding recorded rather than silently applied.

## 6. Test totals

```
cargo test --offline --workspace
  72 unit tests           (13 modules)
   1 crash-safety test    (6 kills)
   2 integration tests    (local runtime, with and without a model)
   4 model-code tests     (accepted / not-JSON / rolled back / traversal)
  = 79 tests, 0 failed, 0 warnings
```

---

## 7. What is still not true

1. **No installer corresponds to this commit.** The binaries on record were built from
   `9347d6f0`; this audit changed six files. A rebuild is needed, and it costs a CI run.
2. **No Windows smoke test.** The script exists; nobody has run it.
3. **Unsigned.** Needs a certificate.
4. **Generated code is not sandboxed.** Allow-list, timeout and path guard, but no WASM or
   container isolation. This is the largest remaining security gap and the audit did not
   close it, because it needs a dependency this environment cannot fetch.
5. **App commands are exposed to the webview** by Tauri's default (§2.2), pending a tested
   `AppManifest` change.
6. **Model-authored application code does not exist.** The model writes notes. Claiming
   otherwise would be the exact kind of overstatement this project was built to avoid.
