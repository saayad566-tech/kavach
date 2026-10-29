# Windows build evidence

**Status: the Windows build succeeded. Installers were produced and uploaded.**
This is the first successful Windows build of Kavach.

Every value below was read from the GitHub API or from the job log. Where a value could not
be read back, it says so rather than being estimated.

## Run

| Field | Value |
|---|---|
| Repository | `saayad566-tech/kavach`, branch `kavach-desktop` |
| Commit | `9347d6f055cbc298e12758b25e25e0881dad393d` |
| Workflow run | 40316051982 (`workflow_dispatch`) |
| Run status | `completed` / **`success`** |
| Run window | 11:34:17Z → 11:41:20Z (7 min 3 s total) |
| Runner | `windows-latest` — Windows Server 2025 (10.0.26100), image `windows-2025-vs2026` 20261021.278.1 |

### Jobs

| Job | Window | Result | Steps |
|---|---|---|---|
| Offline verification (no crates.io, no compiler for the shell) | 11:34:21Z → 11:35:14Z | **success** | 14/14 |
| Windows installers (.exe / .msi) | 11:35:18Z → 11:41:20Z | **success** | 16/16 |

Windows job steps, all `success`: set up job · checkout · rust-toolchain · setup-node ·
Rust cache · record the toolchain · **host tests on Windows** · **build the application and
installers** · **checksum and list what was produced** · **upload installers and their
checksums** · upload the generated lockfile · post steps · complete job. The only step not run
was "Upload build output on failure", which is `if: failure()` and correctly skipped.

## Artefacts

Downloaded from the run and verified locally. The per-file hashes below were recomputed in
this environment from the downloaded binaries and **match** the values the build wrote into
`installer-checksums.txt`.

| File | SHA-256 (recomputed) | Type |
|---|---|---|
| `Kavach_0.5.0_x64_en-US.msi` | `7ce128749f35edbf27fbf8a8f8482948333a8018d15baaa48b7e8d5829fada3d` | MSI, Windows Installer XML Toolset 3.14.1.8722, x64 |
| `Kavach_0.5.0_x64-setup.exe` | `8c0cd24f14529b2a79f7b971fedfc292c15a71a9641a299c3f58eeca31504c91` | PE32 GUI, NSIS self-extracting |

Artefact bundle `kavach-windows-installers.zip`: 2,641,463 bytes,
`sha256:fe84cef93fb3efc6d16e2e3aace49238c1393099708cb4e9a8e182ad3d2288bb` — downloaded and
digest-checked against the GitHub API value.

Lockfile artefact `desktop-cargo-lock.zip`: 24,413 bytes,
`sha256:0b7d9873ee95a1991a83b80630449f2576b1018a17ce4dac4db01138f5cd5db9` — downloaded,
digest-checked, and its `Cargo.lock` (103,828 bytes, **407 pinned packages**) is now committed
at `apps/desktop/src-tauri/Cargo.lock`.

Run page: `https://github.com/saayad566-tech/kavach/actions/runs/40316051982`

## What the run proves, and what it does not

Proven:

- The Tauri shell **compiles on Windows**. The build step succeeded, and it would have failed
  the job otherwise.
- `cargo test --offline --workspace` **passed on Windows** — the step ran and returned success,
  so the whole host suite (39 tests) works on a Windows runner, not only on Linux.
- At least one installer was produced, because the checksum step ends with
  `test -s installer-checksums.txt || exit 1` and the upload step uses
  `if-no-files-found: error`. Both succeeded.
- The generated `Cargo.lock` exists (24 KB, uploaded as its own artefact).

Not proven:

- **No installer has been installed or launched.** A successful build is not a smoke test. The
  ten-point checklist in `WINDOWS-BUILD.md` §4 has not been performed.
- The installers are **unsigned** — see `WINDOWS-BUILD.md` §7.
- Local model inference remains unimplemented; the app states this on screen.

## Warnings seen during the build

Reported by the compiler in the log, none fatal:

| Warning | Where | Note |
|---|---|---|
| unused import `std::path::Path` | `crates/kavach-host/src/probe.rs` | used only by the Linux GPU path |
| `read_trimmed` never used | `crates/kavach-host/src/probe.rs` | same |
| `command_names` never used | `apps/desktop/src-tauri/src/commands.rs` | kept for the command-list test |
| unreachable statement, unused `root`/`outside` | `crates/kavach-host/src/path.rs` | **real gap**: `rejects_a_symlink_that_escapes_the_root` returns early on non-Unix, so symlink behaviour is not exercised on Windows |

These are cosmetic except the last, which means symlink confinement is untested on the platform
we ship first. Fixing it needs a Windows-privileged symlink creation or a junction test, and a
new build to verify.

## Follow-ups from this run

| # | Follow-up | Status |
|---|---|---|
| 1 | Commit `Cargo.lock` so the build is reproducible | **Done** — 407 packages pinned, digest verified |
| 2 | Clear the three unused-item warnings | **Done** — `Path` and `read_trimmed` gated to Linux, `command_names` to test; `cargo test` emits no warnings |
| 3 | Exercise the link-confinement test on Windows | **Code done, unverified on Windows** — the test now creates a directory symlink or junction and fails loudly if it cannot, instead of returning early |
| 4 | Install and smoke-test the installer | **Script ready, not run** — `scripts/smoke-test-windows.ps1` automates 11 checks and walks a human through 8 more |
| 5 | Sign the installers | **Blocked on a certificate** — configuration documented in `WINDOWS-BUILD.md` §7, not applied |

Items 1 and 2 changed source after the build, so the released binaries correspond to commit
`9347d6f0`, one commit earlier. The changes are a `cfg` gate on two Linux-only items and a
`#[cfg(test)]` on a test helper: no behaviour changes. A rebuild is the only way to prove that
rather than assert it, and it costs another run.
