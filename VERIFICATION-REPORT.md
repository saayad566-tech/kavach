# Kavach — Verification and Build Report (0.5.1)

**Milestone: offline verification of the deliverables, Tauri configuration and Windows
packaging setup · 26 October 2026**

---

## 1. What this turn did

You asked for the three deliverables to be inspected, every offline verification performed,
remaining issues fixed, the Tauri configuration and Windows packaging setup verified, and a
reproducible Windows build procedure prepared — with no paid CI used and no installer claimed.

| Asked | Done |
|---|---|
| Inspect the three deliverables | **Partly — see §2.** The sandbox was recycled again; the deliverables were gone. The source was restored from GitHub and re-verified. |
| Every verification possible locally/offline | **Yes** — 39 Rust tests + 3 static/UI checks, all passing; one command runs them (`scripts/verify-all.sh`) |
| Fix remaining issues | **Yes** — five fixed, listed in §4 |
| Verify Tauri configuration | **Yes** — substantially strengthened checker, §3 |
| Verify Windows packaging setup | **Yes** — bundle targets, icons, workflow semantics, §3 |
| Reproducible Windows build procedure | **Yes** — `WINDOWS-BUILD.md` |
| Local inference marked unimplemented | **Yes** — in the UI, in every delivery report, and in the README status table |
| No paid CI minutes used | **Not quite — see §2a.** A push-triggered run started and was cancelled after 49 s (~1 Linux minute). CI is now dispatch-only |
| No installer claimed | **Yes** — no installer exists |

---

## 2. About the three deliverables

At the start of this turn `/scratch/work`, `/scratch/saved` and `/scratch/output` were all
empty — the pod had been recycled again, so `kavach-source.zip`, `CI-EVIDENCE.md` and
`Kavach-Verification-Report.md` were not present to inspect. What I did instead:

1. Restored all **35 files** from `saayad566-tech/kavach`, branch `kavach-desktop`, commit
   `d2771954`, tree `1beaea25`, through the Git Data API — verified file-by-file by size.
2. Re-ran every check against that restored tree. Results in §3 — they reproduce exactly.
3. Rebuilt the deliverables from the restored tree at the end of this turn.

That is a real limitation of the inspection: I verified the *repository contents*, not the
bytes of the zip you downloaded. The zip was built from that same tree, and the checksums are
recomputed and reported in §6.

---

## 2a. A run started, and it corrects my previous diagnosis

Pushing this revision triggered `build.yml` (which had a `push` trigger), and the run actually
started — contrary to my previous report, which concluded that the account's Actions allowance
was exhausted. I cancelled it 49 seconds in, before the Windows job began. What that run
proved, at a cost of about one Linux runner minute:

| Step | Result |
|---|---|
| Host tests (offline, zero dependencies) | **success** |
| Desktop configuration and packaging setup | **success** |
| UI only calls commands that exist | **success** |
| UI renders against a stubbed IPC | **success** |

So the entire offline suite passes on a clean GitHub runner, not just in this container. The
earlier four failures (zero steps, no logs) remain unexplained and my allowance explanation
should not be relied on. `build.yml` now triggers on `workflow_dispatch` only, so no push can
spend minutes again.

**No Windows build has been attempted at any point.**

---

## 3. Verification results

```
$ ./scripts/verify-all.sh
=== host tests (offline) ===              test result: ok. 39 passed; 0 failed
=== desktop configuration and packaging === RESULT: PASS
=== UI to command contract ===             RESULT: PASS
=== UI renders against a stubbed IPC ===   7/7 checks passed
RESULT: PASS — all offline checks passed
NOT PROVEN: the Tauri shell compiles, a window renders, or an installer exists.
```

### The configuration checker now verifies

| Area | Checks |
|---|---|
| Config structure | required keys present; semver version; valid bundle identifier; not the Tauri default; CSP set; `capabilities` set; `withGlobalTauri` on |
| Versions | `tauri.conf.json` == desktop `Cargo.toml` == host `Cargo.toml` (drift would produce a mislabelled installer) |
| Frontend | `frontendDist` exists; every `src`/`href` in `index.html` resolves inside it |
| Commands | registered ⊆ defined ⊆ `generate_handler!`; every shell command (except the documented `workspace_root`) exists in `kavach-host::COMMAND_NAMES`; no advertised-but-unregistered command |
| Capabilities | every declared capability has a file, every file is referenced, identifiers match filenames, targets exist, **no `fs:`/`shell:`/`http:` permission** |
| Icons | all five present and non-empty; `icon.ico` contains 16/32/48/256 (it has 16/24/32/48/64/128/256); `.icns` has the `icns` magic; PNG dimensions match their `@2x` names |
| Windows packaging | `bundle.active`, and `nsis` + `msi` both in `bundle.targets` |
| Cargo | no `[features]` table; no `tauri/custom-protocol` in code; `kavach-host` path dependency resolves; `build.rs` calls `tauri_build::build()` |
| Rust hygiene | no `todo!`/`unimplemented!`; no TODO/FIXME comments; `GetDiskFreeSpaceExW` declared once and exercised by tests |
| Workflows | both parse as YAML; a `windows-latest` job exists; it invokes the Tauri CLI, requests bundles, uploads an artefact; every action has a version pin |

Two of these caught false positives in the checker itself on first run — a comment mentioning
`custom-protocol`, and the placeholder-marker strings that `builder.rs` legitimately contains.
Both checks were tightened rather than the code changed.

---

## 4. Issues found and fixed

| # | Issue | Why it mattered | Fix |
|---|---|---|---|
| 1 | **Windows disk usage shelled out to PowerShell and parsed text** with a fragile `Get-PSDrive` one-liner | Untestable here, and a plausible first-build or first-run failure | Replaced with `GetDiskFreeSpaceExW` behind a function pointer. The marshalling and the `ULARGE_INTEGER` split are now plain Rust, tested on Linux with a stub callable: 4 GiB + 4096 bytes and `u64::MAX` cases |
| 2 | The UI never told the user that inference is unimplemented — only the post-build report did | The brief requires it to be clearly marked | Persistent yellow notice above the build button, plus a render check that fails if it disappears |
| 3 | `probe.yml` ran on every push | Would spend runner minutes for no reason | Now `workflow_dispatch` only, documented as an allowance canary |
| 4 | Workflow did not install `pyyaml`/`pillow`, so the checker would silently skip YAML and icon checks | The strongest new checks would not have run in CI | Added to the host job; the checker also says when a check was skipped |
| 5 | Workflow would report success without an installer, and did not capture the lockfile | A green build with no artefact is the worst outcome | `test -s installer-checksums.txt` fails the job if nothing was produced; lockfile uploaded as an artefact for later pinning |

---

## 5. What is verified, and what is not

| Claim | Proven? | By what |
|---|---|---|
| Host compiles and 39 tests pass | **Yes** | `cargo test --offline` |
| The builder produces a project that passes its own tests | **Yes** | builder + commands end-to-end tests |
| Commands are allow-listed, time-bounded, path-guarded, symlink-safe | **Yes** | Rust tests |
| Model install requires licence acceptance and a matching hash | **Yes** | `install.rs` tests |
| Windows disk-space marshalling is correct | **Yes**, against a stub | `windows_disk_marshalling_is_correct` |
| Desktop config and packaging setup are internally consistent | **Yes** | `check-desktop-config.py` (30+ rules) |
| The UI renders and its calls resolve to real commands | **Yes**, with a stubbed IPC | `verify-ui.mjs`, `check-ui-contract.py` |
| The Tauri shell compiles | **No** | never through `rustc` with the Tauri macros |
| A window renders the real UI | **No** | needs a build |
| A Windows installer exists | **No** | no runner |
| Local model inference works | **No** | not implemented; stated in the UI, the report and the README |

---

## 6. Deliverables

| File | Contents |
|---|---|
| `kavach-source.zip` | The repository at this revision: host crate, Tauri shell, static UI, scripts, workflows, `WINDOWS-BUILD.md`, this report, CI evidence |
| `Kavach-Verification-Report.md` | This document |
| `CI-EVIDENCE.md` | The four failed runs, with timings, step counts and the billing arithmetic |
| `WINDOWS-BUILD.md` | Prerequisites, exact commands, expected artefacts, smoke-test checklist, likely first-build failures, signing steps |

Checksums are given in the chat reply.

---

## 7. Exact remaining blockers

**Blocker 1 — approval to spend CI minutes, not a technical blocker.** The previous diagnosis
(allowance exhausted) is wrong: a run started normally on 26 Oct and the host job passed all
14 steps. The earlier zero-step failures are unexplained. What is needed now is your go-ahead
to run the Windows job, roughly 24–40 billed minutes.

**Blocker 2 — first-compile risk in the Tauri shell (expected, not yet characterised).** The
shell has never been through `rustc` with the Tauri macros. The commands delegate correctly
and the config passes 30+ static rules, but `generate_handler!`, the capability permission
names and the NSIS/WiX step are unexercised. Expect one round of mundane fixes;
`WINDOWS-BUILD.md` §6 lists the likely ones.

**Blocker 3 — local inference (deliberately open).** Not implemented. The app says so on
screen, every delivery report says so, and the README status table says so.

**Non-blocker worth noting:** macOS and Linux bundles are configured but equally unbuilt.

---

## 8. Next step, pending your approval

Say the word and I will dispatch `build.yml` on `kavach-desktop` — one Windows job, roughly
24–40 billed minutes — then iterate on the compiler output until either the installers build
and their SHA-256 values are recorded here, or I report the precise error. Until then no CI
minutes are spent and no installer is claimed.
