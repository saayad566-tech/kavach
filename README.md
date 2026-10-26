# Kavach

Privacy-first, local-first application builder. Describe an application in plain English;
Kavach extracts requirements, plans it, generates a working project, runs that project's own
tests, and reports honestly what it verified and what it did not.

The device is the default execution environment. No account, no telemetry, no backend, no
cloud fallback.

## Status

| Part | State |
|---|---|
| Native host (probe, exec, path guard, SHA-256, model install, builder, store) | **Working** — 79 tests, zero dependencies |
| Builder pipeline (requirements → plan → scaffold → security scan → verify → report) | **Working** — generates a project that passes its own `node --test`, and refuses to deliver one with a committed secret |
| Security scan of generated code | **Working** — high-severity findings fail verification |
| Local model support | **Working** — discovers Ollama or a llama.cpp server on loopback, never starts one, and writes the project notes |
| Model-authored application code | **Working, with a safety rule** — the model may add files; they are kept only if the project still passes its own tests, and rolled back otherwise |
| Desktop UI (static HTML/JS, 20 commands) | **Working** — 13 render and accessibility checks |
| Export to a plain source tree | **Working** |
| Tauri shell compiles | **Proven on Windows** — run 40316051982 |
| Windows installers | **Built, unsigned, not yet smoke-tested** — see `BUILD-EVIDENCE.md` |
| Production readiness | **Technical preview** — see `PRODUCTION-READINESS.md` and `AUDIT.md` |

Nothing here is claimed to work unless a command in `scripts/verify-all.sh` proves it, and
that script stops short of claiming the shell compiles — only a real build does that.

## Shape

```
crates/kavach-host        Native host + the builder. ZERO dependencies, builds offline.
apps/desktop/src-tauri    Tauri 2 shell: 18 commands, one-line delegations to the host.
apps/desktop/ui           Static HTML/CSS/JS. No bundler, no npm in the desktop build.
scripts/                  Offline verification.
WINDOWS-BUILD.md          Reproducible Windows build procedure.
```

No Node, no npm and no bundler in the desktop build: the window loads a static page and
everything privileged is a Rust command. That removes the most common reasons a Windows build
fails.

## Verify

```bash
./scripts/verify-all.sh          # everything that can be checked offline
```

Individually:

```bash
cargo test --offline --workspace          # 79 tests, no crates.io needed
python3 scripts/check-desktop-config.py   # icons, versions, capabilities, commands, workflows
python3 scripts/check-ui-contract.py      # every UI call resolves to an implemented command
node scripts/verify-ui.mjs                # renders the UI in headless Chromium (13 checks)
python3 scripts/check-test-coverage.py    # structural gate: every module tested
```

## Build

See `WINDOWS-BUILD.md`. Short version, on Windows with Visual Studio Build Tools and Rust
installed:

```powershell
cd apps/desktop/src-tauri
npx --yes @tauri-apps/cli@2 build --target x86_64-pc-windows-msvc --bundles nsis,msi
```

## What the app does

1. **Probe** — real RAM, cores, disk, GPU and CPU flags, with a list of what it could not
   measure.
2. **Understand** — the prompt becomes app kind, entities, features with acceptance criteria,
   assumptions, and a dependency-ordered plan.
3. **Build** — a complete, dependency-free Node application: JSON-backed store with
   validation, HTTP server, browser UI, and a real test suite. If a local model is running it
   also writes the project notes, and may contribute code that is accepted only when the
   project's tests still pass.
4. **Verify** — `node --test` runs inside the generated project; the exit code is the evidence.
   A task is verified only if a command produced evidence for it.
5. **Checkpoint** — the tree is snapshotted before the build and can be restored exactly.

## Security posture

- The window holds no `fs`, `shell` or `http` permission; it can only call the registered
  commands.
- Commands are allow-listed (`node`, `npm`, `python3`, `cargo`, `git`, …). A model-authored
  `rm`, `curl` or `sh` is refused before it is spawned.
- Timeouts are clamped to 1–600 s and kill the whole process tree.
- File writes reject absolute paths, `..` traversal, and symlinks leading outside the project.
- Model installation requires an accepted licence and a matching SHA-256, verified before and
  after the copy.

## Documents

| File | Purpose |
|---|---|
| `AUDIT.md` | Deep audit: method, research findings, 7 defects, compliance matrix |
| `PRODUCTION-READINESS.md` | Criteria, honest status, and the path to shippable |
| `WINDOWS-BUILD.md` | Reproducible build and signing procedure |
| `BUILD-EVIDENCE.md` | The successful build: run, artefacts, checksums |
| `CI-EVIDENCE.md` | What the CI runs showed, including the failures |

## Licence

Apache-2.0 for Kavach's code.
