# Origin and licence of these definitions

The Markdown definitions in `agents/`, `skills/`, `commands/` and `rules/` are adapted from
**everything-claude-code** (https://github.com/affaan-m/everything-claude-code), whose plugin
metadata declares the MIT licence and names Affaan Mustafa as the author.

They are kept here because the *format* — Markdown with YAML frontmatter — is the portable
part of that project, and because the prompts are worth reading. What changed is how they are
run: `crates/kavach-host/src/agents.rs` maps their hosted-model and hosted-tool references onto
a local model and this platform's allow-listed operations.

Two notes, stated rather than glossed over:

1. The upstream archive contained no LICENSE file, only `"license": "MIT"` in
   `.claude-plugin/plugin.json`. If you redistribute these files, confirm the licence with the
   upstream author first.
2. `model: opus` is still in the frontmatter of every agent. It is not a model name here — it
   is read as "the most capable local model that fits this machine" and resolved against the
   local registry. Nothing calls a hosted API.
