# CI evidence — `saayad566-tech/kavach`

Recorded 26 October 2026. Branch `kavach-desktop`.

## Runs

| Run | Workflow | Event | Head | Conclusion | Steps executed | Notes |
|---|---|---|---|---|---|---|
| 40262828033 | build | push | bb51e9e9 | failure | **0** | job log absent (`BlobNotFound`) |
| 40262831721 | build | workflow_dispatch | bb51e9e9 | failure | **0** | same |
| 40262971751 | probe (`echo`) | push | d2771954 | failure | **0** | same |
| 40262971822 | build | push | d2771954 | failure | **0** | same |
| 40287981728 | build | push | 86d36be8 | **cancelled** | host: 14/14 steps green; windows: 0 | cancelled by the assistant 49 s after start |

## What changed, and a correction

The first four runs failed in about two seconds with `steps: []` and no job logs. I read that
as an exhausted Actions allowance. **That reading is now contradicted by evidence:** run
40287981728 started normally and its `host` job executed every step successfully on a real
runner. So Actions does work on this repository; the earlier failures remain unexplained
(transient platform condition or account state that has since cleared). The allowance
arithmetic in the previous version of this file was suggestive, not causal, and should not be
relied on.

## The host job passed on CI

Run 40287981728, job "Offline verification (no crates.io, no compiler for the shell)", 49 s:

| Step | Result |
|---|---|
| Set up job, checkout, rust-toolchain, setup-node, setup-python | success |
| Python tooling for the static checks | success |
| **Host tests (offline, zero dependencies)** | **success** |
| **Desktop configuration and packaging setup** | **success** |
| **UI only calls commands that exist** | **success** |
| **UI renders against a stubbed IPC** | **success** |

That is the offline suite passing on a clean machine that is not the development container —
the strongest verification obtained so far, and it costs nothing to keep proving.

## The Windows job did not run

`needs: host` was satisfied, but the run was cancelled immediately afterwards, so the
"Windows installers (.exe / .msi)" job shows `steps: 0`. **No Windows build has ever been
attempted.** Cost of the cancelled run: one Linux runner minute (49 s, rounded up).

## Consequence for this repository

`build.yml` now triggers on `workflow_dispatch` only. A push cannot start a run, so no
Actions minutes are spent unless someone asks for them explicitly.

## To attempt the Windows build (needs the owner's approval)

```bash
gh workflow run build.yml --ref kavach-desktop   # or: Actions tab -> build -> Run workflow
```

Budget: one Linux minute for the host job, then roughly 12–20 minutes of wall clock on
`windows-latest`, billed at 2× — call it 24–40 billed minutes.
