# Decisions — what was chosen, and what it cost

Newest first. Each entry says the decision, why, and what it rules out. A decision without a
cost was not a decision.

## D-011 The desktop UI is a conversation, not a dashboard
Chat-shaped shell: project rail, message column, composer, inspector rail.
**Cost:** three columns at 1200px; the inspector is hidden below that, so the panels are
unreachable on a narrow window until a drawer is built.

## D-010 Sandbox generated tests with Node's permission model, not Wasmtime
Generated code is JavaScript; Wasmtime runs WebAssembly. Node 20's permission model confines
the filesystem and denies child processes; a preload denies the network.
**Cost:** Node 20 becomes a hard requirement. `--allow-child-process` is forbidden — it defeats
the model, proven by a test that escaped through `execSync`.

## D-009 The host crate has zero dependencies
`cargo build --offline` works anywhere, and the code can be audited without reading a
dependency tree.
**Cost:** a hand-written JSON writer and parser, SHA-256, and a loopback HTTP client.

## D-008 Generated applications are domain-neutral
No field names from one domain in the templates; the analysis panel discovers its fields from
the inferred schema.
**Cost:** the sample dataset is a placeholder, so the first run looks generic.

## D-007 Evidence-gated verification
`Store::upsert_task` refuses `verified` with empty evidence; the build writes the ledger as it
runs.
**Cost:** every stage must produce a detail line, so a silent stage is a build failure.

## D-006 Model install requires licence acceptance and a SHA-256
Enforced in Rust before the file is adopted; a refused install writes nothing.
**Cost:** the user must have the digest. Kavach never downloads, so it cannot compute it for you.

## D-005 No updater
An auto-updater phones home, which contradicts the product.
**Cost:** upgrades are manual. Revisit only if a user opts in explicitly.

## D-004 No data feeds, scrapers or market integrations
The product's promise is that nothing leaves the machine.
**Cost:** the app cannot tell you a live price; it analyses what you give it.

## D-003 Tree commits must be complete
A GitHub tree is a snapshot. Building a tree with one entry and committing it deleted every
other file. **Cost of getting it wrong:** one commit of data loss, recovered from the parent.

## D-002 Verify artefacts, not working directories
`scripts/verify-zip.sh` extracts and builds elsewhere.
**Cost:** slower releases; the only way "ready to use" means anything.

## D-001 Local-first over hosted convenience
No account, no metered API, no telemetry.
**Cost:** no cloud sync, no shared workspaces, no server-side compute.
