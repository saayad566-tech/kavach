# Building Kavach for Windows

Reproducible procedure for producing the Windows installers. **This procedure has not been
executed yet** — the CI runner that would run it cannot start (see "Why this has not run",
below). It is written so that the first person or machine to run it gets a build or a
precise error, not a mystery.

---

## 1. What is being built

| Artefact | Produced by | Typical path |
|---|---|---|
| `Kavach_0.5.0_x64-setup.exe` | NSIS | `apps/desktop/src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/` |
| `Kavach_0.5.0_x64_en-US.msi` | WiX | `apps/desktop/src-tauri/target/x86_64-pc-windows-msvc/release/bundle/msi/` |
| `kavach.exe` (unbundled) | cargo | `apps/desktop/src-tauri/target/x86_64-pc-windows-msvc/release/` |

The application is a single native binary plus a static UI. There is no Node runtime, no npm
install, and no bundler step in the build.

---

## 2. Prerequisites

On Windows 10 or 11, x64:

1. **Visual Studio Build Tools 2022** with the *Desktop development with C++* workload.
   This supplies MSVC, the linker and the Windows SDK. Nothing builds without it.
   ```
   winget install --id Microsoft.VisualStudio.2022.BuildTools -e
   ```
   Then in the installer, tick *Desktop development with C++*.

2. **Rust stable, MSVC toolchain**:
   ```
   winget install --id Rustlang.Rustup -e
   rustup default stable-x86_64-pc-windows-msvc
   rustup target add x86_64-pc-windows-msvc
   ```

3. **Node.js 20+** (only to run the Tauri CLI through `npx`):
   ```
   winget install --id OpenJS.NodeJS.LTS -e
   ```

4. **WebView2 runtime.** Preinstalled on Windows 11 and on updated Windows 10. The bundle
   is configured with `webviewInstallMode: downloadBootstrapper`, so the installer fetches it
   if it is missing — that needs network access on the target machine at install time.

5. **NSIS and WiX** are downloaded by the Tauri CLI on first use. They need network access
   during the build; nothing else does.

Nothing in the build contacts a Kavach server, because there is no Kavach server.

---

## 3. Build

```powershell
git clone https://github.com/saayad566-tech/kavach.git
cd kavach
git checkout kavach-desktop

# 1. Everything that can be verified without a compiler for the shell
cargo test --offline --workspace
python -m pip install --quiet pyyaml pillow
python scripts/check-desktop-config.py --verbose
python scripts/check-ui-contract.py
node scripts/verify-ui.mjs

# 2. The build itself
cd apps/desktop/src-tauri
npx --yes @tauri-apps/cli@2 build --target x86_64-pc-windows-msvc --bundles nsis,msi
```

Expect roughly 12–20 minutes on a first build (compiling the Tauri dependency graph) and
about 2 minutes afterwards.

### Record what you built with

Reproducibility depends on knowing the inputs. Capture:

```powershell
rustc --version; cargo --version; node --version
npx --yes @tauri-apps/cli@2 --version
Get-FileHash apps\desktop\src-tauri\target\x86_64-pc-windows-msvc\release\bundle\nsis\*.exe -Algorithm SHA256
```

After a first successful build, commit `apps/desktop/src-tauri/Cargo.lock`. That file pins
every transitive crate version, which is the difference between "builds today" and
"builds reproducibly". The CI job uploads it as the `desktop-cargo-lock` artefact for
exactly this reason.

---

## 4. Smoke test, once an installer exists

Do not call the build done until these pass on a clean machine or VM:

1. Run the NSIS installer. It installs per-user, so no administrator prompt is expected.
2. Launch Kavach from the Start menu. The window should open at 1180×820.
3. The badge reads **"desktop shell · local only"**. If it reads "not running in the desktop
   shell", the `withGlobalTauri` setting or the `invoke` bridge is broken.
4. The **This device** card shows real values for CPU, cores, RAM and disk, and lists what it
   could not measure. Disk free must be non-zero — that path uses `GetDiskFreeSpaceExW`, which
   is the one piece of Windows-only code in the crate.
5. The yellow notice states that local inference is not implemented. It must be there.
6. Type a prompt, press **Build it**. Expect: five stages, `verify` green, 5 tasks verified and
   2 unverified, and a file list including `src/store.js` and `test/store.test.js`.
7. Open a terminal in the generated project (`%APPDATA%\app.kavach.builder\projects\<id>`)
   and run `npm test`. It should pass, independently of Kavach.
8. **Checkpoints → Restore** on the `before build` entry should report files restored and
   files removed.
9. Close the window. No console window should linger (release builds set
   `windows_subsystem = "windows"`).
10. Uninstall from Settings → Apps. The workspace under `%APPDATA%` is intentionally left
    behind; note that in the release notes rather than surprising the user.

---

## 5. Why this has not run yet

The workflow is committed and dispatched, but **GitHub refuses to start any job on this
account** — including a one-step `echo` workflow. Four runs completed as `failure` in about
two seconds with zero steps executed and no job logs, which is the signature of an exhausted
Actions allowance rather than a code or YAML problem. October usage was ~8,100 billed minutes
against the 2,000 included with a free plan for private repositories.

Resolving that is a billing decision, so it is not something to do without the owner's
approval. Options, in order of preference:

1. Raise the Actions spending limit (Settings → Billing → Plans and usage). One Windows build
   costs roughly 24–40 billed minutes at the 2× Windows rate.
2. Wait for the monthly allowance to reset.
3. Run section 3 on any Windows machine — the procedure above is the same one CI runs.

---

## 6. Likely first-build failures, and what they mean

These are the things a first compile of a Tauri 2 shell usually surfaces. None of them
indicate a design problem:

| Symptom | Cause | Fix |
|---|---|---|
| `failed to select a version for the requirement 'tauri'` | No `Cargo.lock` yet and a registry hiccup | Re-run; the lock is written on success |
| `error: linker 'link.exe' not found` | Build Tools installed without the C++ workload | Re-run the VS installer and tick *Desktop development with C++* |
| `permission 'opener:allow-open-path' not found` | Plugin version does not expose that permission | Drop it from `capabilities/default.json`; the app does not need it |
| `tauri.conf.json: unknown field` | A Tauri 1 key survived | The offline checker flags most of these before the build |
| NSIS download fails | No network, or a proxy | Install NSIS manually, or build `--bundles msi` only |
| `icons/icon.ico` rejected | Icon lacks a 256×256 frame | Regenerate; the checker verifies 16/32/48/256 are present |
| App opens a blank window | `frontendDist` wrong, or the UI threw | Open the WebView2 devtools with a debug build; `verify-ui.mjs` catches UI-side errors offline |

---

## 7. Signing

Unsigned installers trigger SmartScreen. Before distributing outside the team:

1. Obtain an Authenticode code-signing certificate (an EV certificate avoids the worst of the
   reputation delay).
2. Add the certificate and password as repository secrets, and pass them to the Tauri CLI as
   `TAURI_SIGNING_PRIVATE_KEY` / the `certificateThumbprint` + `digestAlgorithm` + `timestampUrl`
   settings under `bundle.windows`.
3. Re-run the build and verify the signature with `signtool verify /pa /v <installer>`.

Do not distribute an unsigned build to users and call it a release.
