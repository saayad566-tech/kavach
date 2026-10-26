# CI evidence — `saayad566-tech/kavach`

Recorded 26 October 2026, 08:00 UTC. Branch `kavach-desktop`.

## Runs

| Run | Workflow | Event | Conclusion | Jobs | Steps executed | Job logs |
|---|---|---|---|---|---|---|
| 40262828033 | build | push | failure | host (failed) + windows (skipped) | **0** | none (`BlobNotFound`) |
| 40262831721 | build | workflow_dispatch | failure | host (failed) + windows (skipped) | **0** | none |
| 40262971751 | probe | push | failure | hello (failed) | **0** | none |
| 40262971822 | build | push | failure | host (failed) + windows (skipped) | **0** | none |

Total runs on the repository: 4. All four completed as `failure` in roughly two seconds.

## Interpretation

A one-step workflow whose only step is `run: echo "actions can run on this repository"` failed
in exactly the same way as the full build — `completed/failure`, `steps: []`, and the job-log
blob does not exist. No shell command ever ran, so:

- this is **not** a YAML error: the workflow was parsed and a run was created;
- this is **not** a compiler error: `cargo` was never invoked;
- the runner never picked the job up.

## Supporting facts

| Fact | Value | Source |
|---|---|---|
| Repository Actions enabled | `{"allowed_actions":"all","enabled":true}` | Actions permissions API |
| October Actions Linux minutes | ~6,781 | Billing usage report |
| October Actions Windows minutes | ~661 (billed at 2× → ~1,322) | Billing usage report |
| Total billed minutes in October | **~8,100** | sum of the above |
| Included allowance, free plan, private repositories | **2,000 minutes/month** | GitHub pricing |
| Private-repository jobs that did run | 15–18 October (`god-console`, `ai-work-stream`, `analyzerx-1.4.0-private`) | Billing usage report |

## Conclusion

The account's Actions allowance is exhausted, so GitHub refuses to start any job — including a
one-step `echo`. This is a billing condition, not a defect in this repository, and no code
change will make the Windows job start until the allowance resets or the spending limit is
raised. Raising it is a decision for the account owner.

## How to confirm independently

1. Actions tab → run 40262971751 (`probe`). It has one step and it did not run.
2. Settings → Billing and plans → Plans and usage → Actions minutes used this month.
3. Re-dispatch `probe` after the allowance resets; a single green `echo` proves the runner
   path works and the `build` job can then be attempted.
