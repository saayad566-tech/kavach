//! Agents, skills, commands and rules — adapted from the Claude-Code style plugin layout to
//! run against a local model.
//!
//! The original layout is a directory of Markdown files: `agents/*.md` with YAML frontmatter,
//! `skills/*/SKILL.md`, `commands/*.md` and `rules/*.md`. That part is portable and worth
//! keeping — it is just text, and it makes the behaviour inspectable and diffable.
//!
//! What is *not* portable is everything the original runtime supplied around that text:
//!
//!   * `model: opus` names a hosted model. Here a model is chosen from the local registry.
//!   * `tools: Read, Grep, Glob, Bash` name another product's tool surface. Here they map to
//!     Kavach's allow-listed operations, and an unmapped tool is an error, not a guess.
//!   * Tool use is a native API feature in the hosted runtime. Most local models have no such
//!     thing, so this module defines an explicit text protocol and a repair path for when the
//!     model gets it wrong — which it will.
//!
//! Nothing here talks to a model. This module is the parsing, matching and prompt-composition
//! half; `runtime.rs` does the talking.

use std::path::{Path, PathBuf};

/// Which kind of file a definition came from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DefinitionKind {
    Agent,
    Skill,
    Command,
    Rule,
}

impl DefinitionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            DefinitionKind::Agent => "agent",
            DefinitionKind::Skill => "skill",
            DefinitionKind::Command => "command",
            DefinitionKind::Rule => "rule",
        }
    }
}

/// The frontmatter of a definition. Only the keys the port needs are parsed; anything else is
/// kept in `extra` so a file written for the hosted runtime still loads.
#[derive(Default, Debug, Clone)]
pub struct Frontmatter {
    pub name: String,
    pub description: String,
    pub tools: Vec<String>,
    /// The original `model:` value, e.g. "opus". Mapped by `resolve_model_class`.
    pub model: String,
    pub extra: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct Definition {
    pub kind: DefinitionKind,
    pub frontmatter: Frontmatter,
    pub body: String,
    /// Where it came from, for error messages that point at a file rather than a concept.
    pub path: String,
}

impl Definition {
    pub fn name(&self) -> &str {
        if self.frontmatter.name.is_empty() {
            // Fall back to the file stem, so a file without frontmatter is still usable.
            self.path
                .rsplit('/')
                .next()
                .unwrap_or("unnamed")
                .trim_end_matches(".md")
        } else {
            &self.frontmatter.name
        }
    }
}

/// Parse a Markdown file with optional `---` frontmatter.
///
/// Deliberately hand-rolled: the host crate has no dependencies, and the subset of YAML these
/// files use is a handful of `key: value` lines. Lists are written inline (`a, b, c`) or as
/// `- item` lines; nested maps are kept verbatim in `extra` rather than half-understood.
pub fn parse_definition(text: &str, kind: DefinitionKind, path: &str) -> Result<Definition, String> {
    let normalised = text.replace("\r\n", "\n");
    let mut frontmatter = Frontmatter::default();
    let body;

    if let Some(rest) = normalised.strip_prefix("---\n") {
        let Some(end) = rest.find("\n---") else {
            return Err(format!("{path}: frontmatter starts but never closes with ---"));
        };
        let raw = &rest[..end];
        body = rest[end + 4..].trim_start_matches('\n').to_string();
        parse_frontmatter(raw, &mut frontmatter, path)?;
    } else {
        body = normalised;
    }

    if body.trim().is_empty() {
        return Err(format!("{path}: the body is empty, so there is nothing to instruct the model with"));
    }

    Ok(Definition {
        kind,
        frontmatter,
        body,
        path: path.to_string(),
    })
}

fn parse_frontmatter(raw: &str, out: &mut Frontmatter, path: &str) -> Result<(), String> {
    let mut pending_list_key: Option<String> = None;

    for line in raw.lines() {
        let line = line.trim_end();
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }

        // A `- item` line belongs to the key above it.
        if let Some(item) = line.trim_start().strip_prefix("- ") {
            if let Some(key) = &pending_list_key {
                if key == "tools" {
                    out.tools.push(item.trim().to_string());
                }
            }
            continue;
        }

        let Some((key, value)) = line.split_once(':') else {
            return Err(format!("{path}: frontmatter line is not `key: value`: {line}"));
        };
        let key = key.trim().to_string();
        let value = value.trim().trim_matches('"').trim_matches('\'').to_string();
        pending_list_key = None;

        match key.as_str() {
            "name" => out.name = value,
            "description" => out.description = value,
            "model" => out.model = value,
            "tools" => {
                if value.is_empty() {
                    // The list follows as `- item` lines.
                    pending_list_key = Some(key);
                } else {
                    out.tools = value
                        .split(',')
                        .map(|t| t.trim().to_string())
                        .filter(|t| !t.is_empty())
                        .collect();
                }
            }
            _ => out.extra.push((key, value)),
        }
    }
    Ok(())
}

/// Load every `.md` file in a directory. A directory that does not exist is empty, not an
/// error: a project may use agents but no skills.
pub fn load_dir(dir: &Path, kind: DefinitionKind) -> Result<Vec<Definition>, String> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("cannot read {}: {e}", dir.display())),
    };

    let mut out = Vec::new();
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    paths.sort();

    for path in paths {
        if path.is_dir() {
            // Skills are `skills/<name>/SKILL.md`; take the inner file.
            let inner = path.join("SKILL.md");
            if inner.is_file() {
                let text = std::fs::read_to_string(&inner)
                    .map_err(|e| format!("cannot read {}: {e}", inner.display()))?;
                out.push(parse_definition(&text, kind, &inner.to_string_lossy())?);
            }
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        out.push(parse_definition(&text, kind, &path.to_string_lossy())?);
    }
    Ok(out)
}

pub fn find<'a>(definitions: &'a [Definition], name: &str) -> Option<&'a Definition> {
    definitions.iter().find(|d| d.name().eq_ignore_ascii_case(name))
}

/// Parse the definitions embedded in the binary.
///
/// A malformed file must not take the whole catalogue down, so errors are returned alongside
/// the definitions that did parse. A shipped application that refuses to start because one
/// Markdown file has a stray `---` would be worse than one that reports it.
pub fn builtin() -> (Vec<Definition>, Vec<String>) {
    let mut definitions = Vec::new();
    let mut errors = Vec::new();
    for (kind, path, text) in crate::agents_builtin::BUILTIN {
        let kind = match *kind {
            "agent" => DefinitionKind::Agent,
            "skill" => DefinitionKind::Skill,
            "command" => DefinitionKind::Command,
            _ => DefinitionKind::Rule,
        };
        match parse_definition(text, kind, path) {
            Ok(definition) => definitions.push(definition),
            Err(e) => errors.push(e),
        }
    }
    (definitions, errors)
}

/// The embedded definitions of one kind.
pub fn builtin_of(kind: DefinitionKind) -> Vec<Definition> {
    let (definitions, _) = builtin();
    definitions.into_iter().filter(|d| d.kind == kind).collect()
}

/// Score how well a skill's description matches a task.
///
/// The hosted runtime let the model choose a skill by reading its description. A local model
/// can do that too, but only if you put every description in the prompt — which is exactly the
/// cost the original avoided. Keyword scoring picks the few worth including, so the prompt
/// stays small enough for a 7B model's context.
///
/// Deliberately simple and explainable: shared words, with the name and description weighted
/// differently, and stop words ignored. No embeddings, no model call.
pub fn score_match(definition: &Definition, task: &str) -> u32 {
    const STOP: &[&str] = &[
        "the", "a", "an", "and", "or", "for", "to", "of", "in", "on", "with", "when", "use",
        "this", "that", "it", "is", "are", "be", "you", "your", "all", "any", "can", "should",
    ];

    let words = |text: &str| -> Vec<String> {
        text.to_lowercase()
            .split(|c: char| !c.is_alphanumeric() && c != '-')
            .filter(|w| w.len() > 2 && !STOP.contains(w))
            .map(|w| w.to_string())
            .collect()
    };

    let task_words = words(task);
    if task_words.is_empty() {
        return 0;
    }

    let name_words = words(definition.name());
    let desc_words = words(&definition.frontmatter.description);

    let mut score = 0u32;
    for word in &task_words {
        // A hit in the name is worth more than a hit in the description.
        if name_words.iter().any(|n| n == word || n.starts_with(word.as_str())) {
            score += 5;
        } else if desc_words.iter().any(|d| d == word) {
            score += 2;
        } else if desc_words.iter().any(|d| d.starts_with(word.as_str()) && word.len() > 4) {
            score += 1;
        }
    }
    score
}

/// The skills worth putting in the prompt for this task, best first.
pub fn rank_skills<'a>(skills: &'a [Definition], task: &str, limit: usize) -> Vec<(&'a Definition, u32)> {
    let mut scored: Vec<(&Definition, u32)> = skills
        .iter()
        .map(|s| (s, score_match(s, task)))
        .filter(|(_, score)| *score > 0)
        .collect();
    scored.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.name().cmp(b.0.name())));
    scored.truncate(limit.max(1));
    scored
}

/// What a hosted `model:` value means when there is no cloud.
///
/// "opus" asks for the most capable model available; that is a *requirement on the machine*,
/// not a model name. The registry then picks the largest installed model that fits.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ModelClass {
    /// Most capable local model (the "opus" role).
    Reasoning,
    /// Balanced (the "sonnet" role).
    Balanced,
    /// Smallest usable (the "haiku" role) — cheap classification and summarising.
    Fast,
}

impl ModelClass {
    pub fn as_str(self) -> &'static str {
        match self {
            ModelClass::Reasoning => "reasoning",
            ModelClass::Balanced => "balanced",
            ModelClass::Fast => "fast",
        }
    }

    /// The smallest parameter count that can plausibly do this job. Below this, a model will
    /// follow the format but not the reasoning, and silently produce confident nonsense.
    pub fn min_params_b(self) -> f32 {
        match self {
            ModelClass::Reasoning => 14.0,
            ModelClass::Balanced => 7.0,
            ModelClass::Fast => 1.0,
        }
    }
}

/// Map the frontmatter's `model:` value onto a local class. Unknown values become Balanced,
/// which is the safe middle: not so small that the job fails, not so large that it will not fit.
pub fn resolve_model_class(model: &str) -> ModelClass {
    match model.trim().to_lowercase().as_str() {
        "opus" | "reasoning" | "large" => ModelClass::Reasoning,
        "haiku" | "fast" | "small" => ModelClass::Fast,
        _ => ModelClass::Balanced,
    }
}

/// A tool a local agent may call. Each maps to an operation the host already guards.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LocalTool {
    ReadFile,
    WriteFile,
    ListFiles,
    Search,
    RunCommand,
}

impl LocalTool {
    pub fn name(self) -> &'static str {
        match self {
            LocalTool::ReadFile => "read_file",
            LocalTool::WriteFile => "write_file",
            LocalTool::ListFiles => "list_files",
            LocalTool::Search => "search",
            LocalTool::RunCommand => "run_command",
        }
    }

    pub fn describe(self) -> &'static str {
        match self {
            LocalTool::ReadFile => "read_file {\"path\":\"src/server.js\"} — read a file in the project",
            LocalTool::WriteFile => "write_file {\"path\":\"src/a.js\",\"content\":\"...\"} — write a file in the project",
            LocalTool::ListFiles => "list_files {\"dir\":\"src\"} — list files under a directory",
            LocalTool::Search => "search {\"pattern\":\"createServer\"} — search file contents",
            LocalTool::RunCommand => "run_command {\"command\":\"node\",\"args\":[\"--test\"]} — run an allow-listed command",
        }
    }
}

/// Map a hosted tool name onto a local one.
///
/// Returns `None` for anything not mapped. The caller must refuse those rather than dropping
/// them silently: an agent told it has `Bash` when it does not will keep trying to use it.
pub fn map_tool(name: &str) -> Option<LocalTool> {
    match name.trim() {
        // The names this port advertises to the model. These must be accepted, or the protocol
        // it was told to follow produces calls the parser rejects.
        "read_file" => Some(LocalTool::ReadFile),
        "write_file" => Some(LocalTool::WriteFile),
        "list_files" => Some(LocalTool::ListFiles),
        "search" => Some(LocalTool::Search),
        "run_command" => Some(LocalTool::RunCommand),
        // The names the hosted runtime used, so an unmodified definition still maps.
        "Read" | "read" | "ReadFile" => Some(LocalTool::ReadFile),
        "Write" | "write" | "Edit" | "edit" | "MultiEdit" => Some(LocalTool::WriteFile),
        "Glob" | "glob" | "LS" | "List" => Some(LocalTool::ListFiles),
        "Grep" | "grep" | "Search" | "GlobSearch" => Some(LocalTool::Search),
        "Bash" | "bash" | "Shell" => Some(LocalTool::RunCommand),
        _ => None,
    }
}

/// Resolve a definition's declared tools, reporting the ones that have no local equivalent.
pub fn resolve_tools(declared: &[String]) -> (Vec<LocalTool>, Vec<String>) {
    let mut mapped = Vec::new();
    let mut unmapped = Vec::new();
    for name in declared {
        match map_tool(name) {
            Some(tool) => {
                if !mapped.contains(&tool) {
                    mapped.push(tool);
                }
            }
            None => unmapped.push(name.clone()),
        }
    }
    (mapped, unmapped)
}

/// Build the system prompt for a local agent.
///
/// Three things differ from the hosted original and are worth stating in the prompt itself,
/// because a local model has no runtime enforcing them:
///   1. the tool protocol is text, not an API;
///   2. only the listed tools exist;
///   3. the model must say when it is unsure rather than inventing a file or a command result.
pub fn compose_system_prompt(
    agent: &Definition,
    skills: &[&Definition],
    rules: &[&Definition],
    tools: &[LocalTool],
) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", agent.name()));
    out.push_str(agent.body.trim());
    out.push_str("\n\n");

    if !tools.is_empty() {
        out.push_str("## Tools you may call\n\n");
        out.push_str(
            "These are the only tools that exist. To call one, reply with a line beginning \
             `TOOL:` followed by the tool name, then a line beginning `ARGS:` with a JSON \
             object. Stop after that; the result is returned to you in the next message.\n\n",
        );
        for tool in tools {
            out.push_str(&format!("- {}\n", tool.describe()));
        }
        out.push_str(
            "\nTo finish, reply with a line beginning `DONE:` and a short summary. \
             Do not invent tool output, file contents or command results.\n\n",
        );
    }

    if !rules.is_empty() {
        out.push_str("## Rules that always apply\n\n");
        for rule in rules {
            out.push_str(&format!("### {}\n{}\n\n", rule.name(), rule.body.trim()));
        }
    }

    if !skills.is_empty() {
        out.push_str("## Relevant guidance\n\n");
        for skill in skills {
            out.push_str(&format!("### {}\n{}\n\n", skill.name(), skill.body.trim()));
        }
    }
    out.trim_end().to_string()
}

/// One parsed tool call.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCall {
    pub tool: LocalTool,
    pub args: String,
}

/// What the model said.
#[derive(Debug, Clone, PartialEq)]
pub enum AgentTurn {
    Call(ToolCall),
    Done(String),
    /// Neither a tool call nor a finished answer. The caller sends `repair_message` back.
    Unclear(String),
}

/// Parse one reply from the model.
///
/// Local models get this wrong often enough that the failure has to be a first-class case
/// rather than an error that aborts the run: a small model will sometimes write
/// `TOOL: read_file` and then put the arguments on the same line, or forget `ARGS:` entirely.
pub fn parse_turn(reply: &str) -> AgentTurn {
    let trimmed = reply.trim();
    if trimmed.is_empty() {
        return AgentTurn::Unclear("the reply was empty".to_string());
    }

    let mut tool: Option<String> = None;
    let mut args: Option<String> = None;

    for line in trimmed.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("TOOL:") {
            tool = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("ARGS:") {
            args = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("DONE:") {
            return AgentTurn::Done(rest.trim().to_string());
        }
    }

    if let Some(name) = tool {
        let Some(tool) = map_tool(&name) else {
            return AgentTurn::Unclear(format!(
                "there is no tool called `{name}`; available tools are read_file, write_file, list_files, search, run_command"
            ));
        };
        let Some(args) = args else {
            return AgentTurn::Unclear(format!(
                "you called {name} but sent no ARGS line; send a line beginning ARGS: with a JSON object"
            ));
        };
        // Validate the JSON shape here rather than in the caller: a malformed call should come
        // back as a repair instruction, not as a parse error five layers down.
        if crate::runtime::parse_json(&args).is_err() {
            return AgentTurn::Unclear(format!(
                "the ARGS for {name} is not valid JSON: {args}"
            ));
        }
        return AgentTurn::Call(ToolCall { tool, args });
    }

    // No protocol markers at all: the model answered in prose. That is fine for a question,
    // but in a tool loop it means the instruction was not followed.
    AgentTurn::Unclear("the reply contained no TOOL:, ARGS: or DONE: line".to_string())
}

/// The message to send back when a reply could not be understood.
pub fn repair_message(reason: &str) -> String {
    format!(
        "That reply could not be used: {reason}.\n\nReply in exactly one of these two shapes:\n\n\
         TOOL: <tool name>\nARGS: <a JSON object>\n\n\
         or\n\n\
         DONE: <a short summary of what you did>\n\n\
         No prose before the first line."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const AGENT: &str = "---\nname: code-reviewer\ndescription: Expert code review specialist for quality and security\ntools: Read, Grep, Glob, Bash\nmodel: opus\n---\n\nYou are a senior code reviewer.\n";

    #[test]
    fn parses_frontmatter_and_body() {
        let d = parse_definition(AGENT, DefinitionKind::Agent, "agents/code-reviewer.md").unwrap();
        assert_eq!(d.name(), "code-reviewer");
        assert_eq!(d.frontmatter.model, "opus");
        assert_eq!(d.frontmatter.tools, vec!["Read", "Grep", "Glob", "Bash"]);
        assert!(d.body.contains("senior code reviewer"));
    }

    #[test]
    fn parses_a_tools_list_written_as_lines() {
        let text = "---\nname: x\ntools:\n- Read\n- Grep\n---\nbody\n";
        let d = parse_definition(text, DefinitionKind::Agent, "a.md").unwrap();
        assert_eq!(d.frontmatter.tools, vec!["Read", "Grep"]);
    }

    #[test]
    fn a_file_without_frontmatter_still_loads() {
        let d = parse_definition("# Orchestrate\n\nRun the agents.\n", DefinitionKind::Command, "commands/orchestrate.md").unwrap();
        assert_eq!(d.name(), "orchestrate");
        assert!(d.frontmatter.tools.is_empty());
        assert!(d.body.contains("Run the agents"));
    }

    #[test]
    fn an_unclosed_frontmatter_is_an_error_not_a_silent_success() {
        let err = parse_definition("---\nname: x\nbody without closing", DefinitionKind::Agent, "a.md").unwrap_err();
        assert!(err.contains("never closes"), "{err}");
    }

    #[test]
    fn an_empty_body_is_rejected() {
        let err = parse_definition("---\nname: x\n---\n\n", DefinitionKind::Agent, "a.md").unwrap_err();
        assert!(err.contains("empty"), "{err}");
    }

    #[test]
    fn unknown_frontmatter_keys_are_kept_not_dropped() {
        let text = "---\nname: x\npermission-mode: acceptEdits\n---\nbody\n";
        let d = parse_definition(text, DefinitionKind::Agent, "a.md").unwrap();
        assert!(d.frontmatter.extra.iter().any(|(k, v)| k == "permission-mode" && v == "acceptEdits"));
    }

    #[test]
    fn model_names_map_to_local_classes() {
        assert_eq!(resolve_model_class("opus"), ModelClass::Reasoning);
        assert_eq!(resolve_model_class("haiku"), ModelClass::Fast);
        assert_eq!(resolve_model_class("sonnet"), ModelClass::Balanced);
        // An unknown name must not silently become the largest model.
        assert_eq!(resolve_model_class("gpt-5-turbo"), ModelClass::Balanced);
        assert!(ModelClass::Reasoning.min_params_b() > ModelClass::Balanced.min_params_b());
    }

    #[test]
    fn hosted_tools_map_and_unmapped_tools_are_reported() {
        let (mapped, unmapped) = resolve_tools(&["Read".into(), "Bash".into(), "Task".into()]);
        assert_eq!(mapped, vec![LocalTool::ReadFile, LocalTool::RunCommand]);
        assert_eq!(unmapped, vec!["Task"], "an unsupported tool must be named, not ignored");
    }

    #[test]
    fn skill_matching_picks_the_relevant_skill() {
        let skills = vec![
            parse_definition("---\nname: tdd-workflow\ndescription: Use when writing features, fixing bugs or refactoring code\n---\nTDD guidance\n", DefinitionKind::Skill, "s1").unwrap(),
            parse_definition("---\nname: clickhouse-io\ndescription: Querying ClickHouse analytics tables\n---\nCH guidance\n", DefinitionKind::Skill, "s2").unwrap(),
            parse_definition("---\nname: security-review\ndescription: Review code for injection and credential leaks\n---\nSec guidance\n", DefinitionKind::Skill, "s3").unwrap(),
        ];
        let ranked = rank_skills(&skills, "write a new feature and fix the failing test", 2);
        assert_eq!(ranked[0].0.name(), "tdd-workflow");

        let ranked = rank_skills(&skills, "check for injection and credential leaks", 1);
        assert_eq!(ranked[0].0.name(), "security-review");

        assert!(rank_skills(&skills, "unrelated words entirely", 3).is_empty());
    }

    #[test]
    fn the_system_prompt_states_the_protocol_and_only_the_real_tools() {
        let agent = parse_definition(AGENT, DefinitionKind::Agent, "a.md").unwrap();
        let skill = parse_definition("---\nname: tdd\ndescription: tests first\n---\nWrite the test first.\n", DefinitionKind::Skill, "s.md").unwrap();
        let rule = parse_definition("---\nname: security\ndescription: rules\n---\nNever log a secret.\n", DefinitionKind::Rule, "r.md").unwrap();
        let prompt = compose_system_prompt(
            &agent,
            &[&skill],
            &[&rule],
            &[LocalTool::ReadFile, LocalTool::Search],
        );
        assert!(prompt.contains("senior code reviewer"));
        assert!(prompt.contains("TOOL:"));
        assert!(prompt.contains("ARGS:"));
        assert!(prompt.contains("DONE:"));
        assert!(prompt.contains("read_file"));
        assert!(!prompt.contains("run_command"), "a tool that was not granted must not be advertised");
        assert!(prompt.contains("Write the test first."));
        assert!(prompt.contains("Never log a secret."));
    }

    #[test]
    fn a_tool_call_is_parsed() {
        let turn = parse_turn("TOOL: read_file\nARGS: {\"path\":\"src/server.js\"}");
        match turn {
            AgentTurn::Call(call) => {
                assert_eq!(call.tool, LocalTool::ReadFile);
                assert!(call.args.contains("server.js"));
            }
            other => panic!("expected a call, got {other:?}"),
        }
    }

    #[test]
    fn a_done_line_is_parsed() {
        assert_eq!(parse_turn("DONE: added the endpoint"), AgentTurn::Done("added the endpoint".to_string()));
    }

    #[test]
    fn malformed_replies_become_repair_cases_not_crashes() {
        // Unknown tool.
        assert!(matches!(parse_turn("TOOL: Task\nARGS: {}"), AgentTurn::Unclear(m) if m.contains("no tool")));
        // Missing ARGS.
        assert!(matches!(parse_turn("TOOL: read_file"), AgentTurn::Unclear(m) if m.contains("no ARGS")));
        // Invalid JSON.
        assert!(matches!(parse_turn("TOOL: read_file\nARGS: {path: src}"), AgentTurn::Unclear(m) if m.contains("not valid JSON")));
        // Prose only.
        assert!(matches!(parse_turn("I will now read the file."), AgentTurn::Unclear(m) if m.contains("no TOOL")));
        // Empty.
        assert!(matches!(parse_turn("   "), AgentTurn::Unclear(_)));
    }

    #[test]
    fn the_repair_message_restates_the_protocol() {
        let message = repair_message("the ARGS for read_file is not valid JSON");
        assert!(message.contains("TOOL:"));
        assert!(message.contains("ARGS:"));
        assert!(message.contains("DONE:"));
    }

    #[test]
    fn every_embedded_definition_parses() {
        // A shipped definition that does not parse is a silent capability loss.
        let (definitions, errors) = builtin();
        assert!(errors.is_empty(), "embedded definitions failed to parse: {errors:?}");
        assert!(definitions.len() >= 40, "only {} definitions", definitions.len());

        let agents = definitions.iter().filter(|d| d.kind == DefinitionKind::Agent).count();
        let skills = definitions.iter().filter(|d| d.kind == DefinitionKind::Skill).count();
        assert!(agents >= 8, "expected the agent set, found {agents}");
        assert!(skills >= 10, "expected the skill set, found {skills}");
    }

    #[test]
    fn embedded_agents_map_onto_local_tools() {
        let agents = builtin_of(DefinitionKind::Agent);
        let reviewer = find(&agents, "code-reviewer").expect("code-reviewer should be embedded");
        let (tools, unmapped) = resolve_tools(&reviewer.frontmatter.tools);
        assert!(tools.contains(&LocalTool::ReadFile));
        assert!(tools.contains(&LocalTool::RunCommand));
        assert!(unmapped.is_empty(), "these agents only use tools that map: {unmapped:?}");
        assert_eq!(resolve_model_class(&reviewer.frontmatter.model), ModelClass::Reasoning);
    }

    #[test]
    fn a_task_selects_a_sensible_agent_and_skills_from_the_embedded_set() {
        let (definitions, _) = builtin();
        let skills: Vec<Definition> = definitions.iter().filter(|d| d.kind == DefinitionKind::Skill).cloned().collect();
        let ranked = rank_skills(&skills, "review this code for security issues and injection", 3);
        assert!(!ranked.is_empty(), "a security task should match at least one skill");
        assert!(
            ranked.iter().any(|(s, _)| s.name().contains("security")),
            "expected a security skill, got {:?}",
            ranked.iter().map(|(s, _)| s.name()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn loading_a_missing_directory_is_empty_not_an_error() {
        let missing = Path::new("/definitely/not/a/directory");
        assert!(load_dir(missing, DefinitionKind::Agent).unwrap().is_empty());
    }
}
