# Kavach

Privacy-first, local-first application builder. Describe an application in plain English;
Kavach extracts requirements, plans it, generates a working project, runs that project's
own tests, and reports honestly what it verified and what it did not.

The device is the default execution environment. No account, no telemetry, no backend, no
cloud fallback.

## Shape of the product

```
crates/kavach-host        Native host: probe, exec, path guard, SHA-256, model install,
                          the builder itself, project store. ZERO dependencies.
apps/desktop/src-tauri    Tauri 2 shell. Thin delegations to kavach-host.
apps/desktop/ui           Static HTML + JS. No bundler, no npm in the desktop build.
scripts/                  Static checks and the UI render test.
```

Deliberately no Node, no npm and no bundler in the desktop build: the window loads a static
page, and everything privileged is a Rust command. That removes the most common causes of a
Windows build failing.

## Build and test

```bash
cargo test --offline --workspace          # 37 tests, no crates.io access needed
python3 scripts/check-desktop-config.py   # icons, capabilities, commands, no Tauri 1 leftovers
python3 scripts/check-ui-contract.py      # every UI call resolves to an implemented command
node scripts/verify-ui.mjs                # renders the UI in headless Chromium (6 checks)

cd apps/desktop/src-tauri && npx @tauri-apps/cli@^2 build   # installers for this platform
```

## What the app does

1. **Probe** — real RAM, cores, disk, GPU and CPU flags, with a list of what it could not
   measure.
2. **Understand** — the prompt becomes app kind, entities, features with acceptance
   criteria, assumptions, and a dependency-ordered plan.
3. **Build** — a complete, dependency-free Node application: JSON-backed store with
   validation, HTTP server, browser UI, and a real test suite.
4. **Verify** — `node --test` is executed in the generated project; the exit code is the
   evidence. A task is verified only if a command produced evidence for it.
5. **Checkpoint** — the tree is snapshotted before the build, and can be restored exactly.

## Security posture

- The window holds no `fs`, `shell` or `http` permission; it can only call the 17 registered
  commands.
- Commands are allow-listed (`node`, `npm`, `python3`, `cargo`, `git`, …). A model-authored
  `rm`, `curl` or `sh` is refused before it is spawned.
- Timeouts are clamped to 1–600 s and kill the whole process tree.
- File writes reject absolute paths, `..` traversal, and symlinks that lead outside the
  project.
- Model installation requires an accepted licence and a matching SHA-256, verified before
  and after the copy.

## Licence

Apache-2.0 for Kavach's code.
