#!/usr/bin/env python3
"""Regenerate crates/kavach-host/src/agents_builtin.rs from the Markdown definitions.

Run after editing anything in agents/, skills/, commands/ or rules/.
"""
import pathlib

TEST = """
#[cfg(test)]
mod tests {
    use super::BUILTIN;

    #[test]
    fn every_embedded_definition_is_present_and_non_empty() {
        // This file is generated, so these tests are about the generator's output. A
        // generator bug would otherwise ship as a silently missing agent.
        assert!(BUILTIN.len() >= 40, "only {} definitions were embedded", BUILTIN.len());
        for (_, path, text) in BUILTIN {
            assert!(!text.trim().is_empty(), "{path} is empty");
            assert!(text.len() > 40, "{path} does not look like a definition");
        }
    }

    #[test]
    fn every_kind_is_represented_and_recognised() {
        for kind in ["agent", "skill", "command", "rule"] {
            assert!(BUILTIN.iter().any(|(k, _, _)| *k == kind), "no {kind} was embedded");
        }
        for (kind, path, _) in BUILTIN {
            assert!(
                ["agent", "skill", "command", "rule"].contains(kind),
                "unknown kind {kind} for {path}"
            );
        }
    }

    #[test]
    fn every_path_points_at_a_markdown_file_in_its_kind_directory() {
        for (kind, path, _) in BUILTIN {
            assert!(path.ends_with(".md"), "{path} is not Markdown");
            let expected_dir = match *kind {
                "agent" => "agents/",
                "skill" => "skills/",
                "command" => "commands/",
                _ => "rules/",
            };
            assert!(path.starts_with(expected_dir), "{path} is filed under {kind} but not in {expected_dir}");
            assert!(!path.contains("NOTICE"), "{path} is the notice, not a definition");
        }
    }
}
"""

GROUPS = [("agent", "agents"), ("skill", "skills"), ("command", "commands"), ("rule", "rules")]
ROOT = pathlib.Path(__file__).resolve().parent.parent

entries = []
for kind, folder in GROUPS:
    for path in sorted((ROOT / folder).rglob("*.md")):
        if path.name == "NOTICE.md":
            continue
        entries.append((kind, path.relative_to(ROOT).as_posix()))

lines = [
    "//! The bundled agent, skill, command and rule definitions.",
    "//!",
    "//! Generated from the Markdown files in the repository root by `scripts/embed-agents.py`.",
    "//! They are embedded rather than read from disk so a shipped application behaves the same",
    "//! wherever it is installed — an app that silently loses its agents when moved is worse",
    "//! than one that carries them.",
    "",
    "/// (kind, path, text)",
    "pub static BUILTIN: &[(&str, &str, &str)] = &[",
]
for kind, rel in entries:
    lines.append(f'    ("{kind}", "{rel}", include_str!("../../../{rel}")),')
lines.append("];")
lines.append("")
lines.append(TEST)

(ROOT / "crates/kavach-host/src/agents_builtin.rs").write_text("\n".join(lines) + "\n")
print(f"embedded {len(entries)} definitions")
