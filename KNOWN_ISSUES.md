# Known issues — the honest list

Anything here is a defect or a gap. Nothing here is "planned" hand-waving; each has a next
step. If an item is fixed, delete it rather than marking it done.

## Blockers for a public release

| # | Issue | Impact | Next step |
|---|---|---|---|
| K-01 | **No installer matches the current commit.** The last proven Windows build predates the last ~15 commits. | A user cannot install this build. | Run `.github/workflows/build-self-hosted.yml` on a Windows machine (free, no CI minutes), or `scripts/build-windows.ps1`. |
| K-02 | **The Windows smoke test has never run.** `scripts/smoke-test-windows.ps1` is written and unexecuted. | A build that compiles is not a build that works. | Same run as K-01, then `smoke-test-windows.ps1 -Installer <path>`. |
| K-03 | **Binaries are unsigned.** | SmartScreen warns on every install. | Free self-signed route exists for internal use (`scripts/make-self-signed-cert.ps1`); public distribution needs a paid certificate — a business decision. |

## Environmental limits of this build environment

| # | Issue | Impact |
|---|---|---|
| K-04 | `webkit2gtk` is absent and cannot be installed (no root). | The Tauri shell cannot be compiled here. Host logic and all tests still run. |
| K-05 | The `x86_64-pc-windows-msvc` target cannot be installed (`static.rust-lang.org` blocked). | No cross-compilation to Windows from here. |
| K-06 | Line coverage is not measured; the gate is structural. | A module can have three shallow tests and pass. |

## Product gaps

| # | Issue | Impact | Next step |
|---|---|---|---|
| K-07 | The agent loop is not built. Definitions parse, map and compose; nothing executes a tool call yet. | Model-driven agents do not run. | Implement `agents::run` — executor, iteration cap, repair path. `docs/ECC-LOCAL-PORT.md` has the sequence. |
| K-08 | The network guard is a preload, not a kernel boundary. | A determined test could reload a module past it. | Node 22's permission model, when the target runtime moves. |
| K-10 | No authentication in generated applications. | A generated app is single-user, local only. | Out of scope unless the product changes. |

## Fixed — keep these fixed

- ~~`install_model` never recorded what it installed~~ (2.0.0) — the registry stayed empty.
- ~~CSV headers are lowercased, so camelCase fields read as null~~ (2.1.0) — normalise at the boundary.
- ~~Piotroski scored 8/9 when prior gross profit was absent~~ (2.1.0) — derive it from prior revenue less COGS.
- ~~`map_tool` rejected the local tool names it advertises~~ (2.0.0).
- ~~The tree was replaced by a single-file snapshot~~ (1.7.0) — recovered from the parent commit.
- ~~The inspector was hidden below 1200px~~ (2.5.0) — now a drawer, opened from a button.
- ~~The platform could not run without a system webview~~ (3.0.0) — `kavach-server` serves the
  same UI and command surface over loopback.
- ~~`crates.io` recorded as blocked~~ — only the website is; the sparse index and crate files download.
