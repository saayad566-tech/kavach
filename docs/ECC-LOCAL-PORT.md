# Porting the everything-claude-code architecture to a local model

What was ported, what could not be, and why. Written after reading all 81 files of the archive.

## What the original is

A Claude-Code plugin: 9 agents, 11 skills, 15 slash commands, 8 rule files and a hook
configuration, all plain Markdown plus one `hooks.json`.

| Part | Format | Portable? |
|---|---|---|
| `agents/*.md` | YAML frontmatter (`name`, `description`, `tools`, `model`) + a system prompt | **Yes** — the text is the product |
| `skills/*/SKILL.md` | frontmatter + guidance, loaded on demand | **Yes** |
| `commands/*.md` | slash commands, some defining multi-agent workflows | **Yes**, as prompt recipes |
| `rules/*.md` | always-on guidance | **Yes** |
| `hooks/hooks.json` | event matchers (`PreToolUse`, `PostToolUse`, `PreCompact`) running shell commands | **Partly** — the events assume a runtime that emits them |
| `model: opus` | names a hosted model | **No** — remapped |
| `tools: Read, Grep, Glob, Bash` | another product's tool surface | **No** — remapped |
| Native tool use | an API feature | **No** — replaced with a text protocol |

## What was ported, and how

All 43 definitions are in the repository (`agents/`, `skills/`, `commands/`, `rules/`), byte for
byte as written, with a NOTICE recording their origin and licence. They are embedded into the
binary by `scripts/embed-agents.py`, so a shipped application cannot lose them.

`crates/kavach-host/src/agents.rs` does the translation:

1. **`model: opus` → a model class.** `opus` means "the most capable model available", which on
   a local machine is a requirement on the hardware, not a name. It resolves to
   `ModelClass::Reasoning` (≥14B), `Balanced` (≥7B) or `Fast` (≥1B); the model registry then
   picks the largest installed model that fits.
2. **`tools: Read, Grep, Glob, Bash` → local operations.** `Read`→`read_file`,
   `Write`/`Edit`→`write_file`, `Glob`→`list_files`, `Grep`→`search`, `Bash`→`run_command`.
   Anything unmapped (for example `Task`, which spawns a subagent in the hosted runtime) is
   **reported, not dropped** — an agent told it has a tool it does not have will keep trying.
3. **Native tool use → an explicit text protocol.** Local models largely lack structured tool
   calling, so the prompt states:

   ```
   TOOL: read_file
   ARGS: {"path": "src/server.js"}
   ```

   and `DONE: <summary>` to finish. `parse_turn` accepts a call, a completion, or an
   `Unclear` case; the last is not an error but a *repair* — small models get the format wrong
   often enough that the loop must expect it and send `repair_message` back.
4. **Skill selection → keyword scoring.** The hosted runtime let the model read descriptions
   and choose. Doing that locally means putting every description in the prompt, which is the
   cost the original design avoided. `rank_skills` scores name and description words instead,
   so a 7B model's context holds only the two or three skills that matter.

## What still has to be built

The port so far is the definition layer: parsing, mapping, skill selection, prompt composition
and reply parsing — 18 tests, all passing, against the real archive.

| Step | What | Why it is not done yet |
|---|---|---|
| 1 | Tool executor: run a parsed `ToolCall` against the project with the existing allow-list and path guard | needs the loop below to drive it |
| 2 | Agent loop: send prompt → parse → execute → feed result back, with an iteration cap | needs a live local model to be worth testing end to end |
| 3 | Model-class resolution against the registry (pick the best installed model for the class) | `models.rs` has the catalogue; the join is small but untested without models installed |
| 4 | Hook runner: `PreToolUse`/`PostToolUse` matchers as local shell hooks | the events only exist once step 2 emits them |
| 5 | Multi-agent workflows from `commands/orchestrate.md` (planner → tdd-guide → reviewer) | each step is step 2 |
| 6 | Context compaction (the `PreCompact` hook): summarise with the `Fast` class when the prompt grows | needs step 2's token accounting |

## The honest limits

- A 7B model will follow this protocol less reliably than a hosted frontier model. The repair
  path exists because of that, not as a nicety.
- The original's hooks assume a runtime that emits `PreToolUse` with a JSON payload. Kavach is
  the runtime, so hooks become part of step 2 rather than a configuration file.
- `Task` (subagent spawning) has no local equivalent yet; orchestration is a loop in one
  process, not parallel agents. Parallelism across one small GPU is a throughput loss anyway.
