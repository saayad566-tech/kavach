# Windows build evidence

**Status: the Windows build succeeded. Both installers were produced.**
This is the first successful Windows build of Kavach.

## Run

| Field | Value |
|---|---|
| Repository | `saayad566-tech/kavach`, branch `kavach-desktop` |
| Commit | `9347d6f055cbc298e12758b25e25e0881dad393d` |
| Workflow run | 40316051982 (`workflow_dispatch`) |
| Runner | `windows-latest`, image `windows-2025-vs2026` 20261021.278.1 (Windows Server 2025, 10.0.26100) |
| Host job | 11:34:21Z → 11:35:14Z, **success**, 14/14 steps |
| Windows job | 11:35:18Z → 11:41:20Z, **success**, 16/16 steps |
| Release compile | 4 min 36 s |

## Artefacts

| File | Size | SHA-256 |
|---|---|---|
| `Kavach_0.5.0_x64-setup.exe` (NSIS) | 1.12 MiB | `8c0cd24f14529b2a79f7b971fedfc292c15a71a9641a299c3f58eeca31504c91` |
| `Kavach_0.5.0_x64_en-US.msi` (WiX) | 1.61 MiB | `7ce128749f35edbf27fbf8a8f8482948333a8018d15baaa48b7e8d5829fada3d` |

The checksums above are the build machine's own `sha256sum` output, from the step
"Checksum and list what was produced".

Uploaded artefact bundle (both installers + `installer-checksums.txt`):
`kavach-windows-installers.zip`, 2,641,463 bytes,
SHA-256 `fe84cef93fb3efc6d16e2e3aace49238c1393099708cb4e9a8e182ad3d2288bb`,
artefact ID 12646714063,
download: `https://github.com/saayad566-tech/kavach/actions/runs/40316051982/artifacts/12646714063`

Generated lockfile (406 crates pinned), `desktop-cargo-lock.zip`, 24,413 bytes,
SHA-256 `0b7d9873ee95a1991a83b80630449f2576b1018a17ce4dac4db01138f5cd5db9`,
artefact ID 12647008291.

## Toolchain actually used

```
rustc 1.99.1 (58cf8f9c3 2026-10-13)   x86_64-pc-windows-msvc
cargo 1.99.1 (5f94df478 2026-08-27)
node  v20.20.2 / npm 10.8.2
tauri 2.12.5, tauri-build 2.7.4, tauri-runtime-wry 2.12.3, wry 0.57.1
NSIS 3.13, nsis_tauri_utils 0.5.3, WiX 3.14.1 (downloaded and hash-validated by the CLI)
406 crates locked to highest compatible versions
```

## What the log shows

- `cargo test --offline --workspace` on Windows: **39 passed, 0 failed** — the whole host
  suite runs on Windows, not just on Linux.
- The desktop crate compiled cleanly: `Compiling kavach v0.5.0`, `Finished release profile
  [optimized] target(s) in 4m 36s`.
- `Built application at: .../release/kavach.exe`.
- NSIS: `Running makensis to produce ... Kavach_0.5.0_x64-setup.exe`.
- WiX: `Running light to produce ... Kavach_0.5.0_x64_en-US.msi`.
- `Finished 2 bundles`.

## Warnings, none fatal

| Warning | Where | Note |
|---|---|---|
| unused import `std::path::Path` | `probe.rs:9` | Linux-only use; `cfg`-gate it |
| `read_trimmed` never used | `probe.rs:44` | Only used by the Linux GPU path |
| `command_names` never used | shell `commands.rs:114` | Kept for the command-list test |
| unreachable statement / unused `root`, `outside` | `path.rs:158-165` | **Real gap**: `rejects_a_symlink_that_escapes_the_root` returns early on non-Unix, so symlink behaviour is untested on Windows |

## Not done

- **The installers are unsigned.** They will trigger SmartScreen. See `WINDOWS-BUILD.md` §7.
- **No smoke test has been run on Windows.** The installers have not been installed and
  launched. Building is not the same as working, and this file does not claim otherwise.
- The generated `Cargo.lock` has not been committed back, because the artefact could not be
  fetched into the working environment. It should be added from artefact 12647008291.
- Local model inference remains unimplemented.
