//! The bundled agent, skill, command and rule definitions.
//!
//! Generated from the Markdown files in the repository root by `scripts/embed-agents.py`.
//! They are embedded rather than read from disk so a shipped application behaves the same
//! wherever it is installed — an app that silently loses its agents when moved is worse
//! than one that carries them.

/// (kind, path, text)
pub static BUILTIN: &[(&str, &str, &str)] = &[
    ("agent", "agents/architect.md", include_str!("../../../agents/architect.md")),
    ("agent", "agents/build-error-resolver.md", include_str!("../../../agents/build-error-resolver.md")),
    ("agent", "agents/code-reviewer.md", include_str!("../../../agents/code-reviewer.md")),
    ("agent", "agents/doc-updater.md", include_str!("../../../agents/doc-updater.md")),
    ("agent", "agents/e2e-runner.md", include_str!("../../../agents/e2e-runner.md")),
    ("agent", "agents/planner.md", include_str!("../../../agents/planner.md")),
    ("agent", "agents/refactor-cleaner.md", include_str!("../../../agents/refactor-cleaner.md")),
    ("agent", "agents/security-reviewer.md", include_str!("../../../agents/security-reviewer.md")),
    ("agent", "agents/tdd-guide.md", include_str!("../../../agents/tdd-guide.md")),
    ("skill", "skills/backend-patterns/SKILL.md", include_str!("../../../skills/backend-patterns/SKILL.md")),
    ("skill", "skills/clickhouse-io/SKILL.md", include_str!("../../../skills/clickhouse-io/SKILL.md")),
    ("skill", "skills/coding-standards/SKILL.md", include_str!("../../../skills/coding-standards/SKILL.md")),
    ("skill", "skills/continuous-learning/SKILL.md", include_str!("../../../skills/continuous-learning/SKILL.md")),
    ("skill", "skills/eval-harness/SKILL.md", include_str!("../../../skills/eval-harness/SKILL.md")),
    ("skill", "skills/frontend-patterns/SKILL.md", include_str!("../../../skills/frontend-patterns/SKILL.md")),
    ("skill", "skills/project-guidelines-example/SKILL.md", include_str!("../../../skills/project-guidelines-example/SKILL.md")),
    ("skill", "skills/security-review/SKILL.md", include_str!("../../../skills/security-review/SKILL.md")),
    ("skill", "skills/strategic-compact/SKILL.md", include_str!("../../../skills/strategic-compact/SKILL.md")),
    ("skill", "skills/tdd-workflow/SKILL.md", include_str!("../../../skills/tdd-workflow/SKILL.md")),
    ("skill", "skills/verification-loop/SKILL.md", include_str!("../../../skills/verification-loop/SKILL.md")),
    ("command", "commands/build-fix.md", include_str!("../../../commands/build-fix.md")),
    ("command", "commands/checkpoint.md", include_str!("../../../commands/checkpoint.md")),
    ("command", "commands/code-review.md", include_str!("../../../commands/code-review.md")),
    ("command", "commands/e2e.md", include_str!("../../../commands/e2e.md")),
    ("command", "commands/eval.md", include_str!("../../../commands/eval.md")),
    ("command", "commands/learn.md", include_str!("../../../commands/learn.md")),
    ("command", "commands/orchestrate.md", include_str!("../../../commands/orchestrate.md")),
    ("command", "commands/plan.md", include_str!("../../../commands/plan.md")),
    ("command", "commands/refactor-clean.md", include_str!("../../../commands/refactor-clean.md")),
    ("command", "commands/setup-pm.md", include_str!("../../../commands/setup-pm.md")),
    ("command", "commands/tdd.md", include_str!("../../../commands/tdd.md")),
    ("command", "commands/test-coverage.md", include_str!("../../../commands/test-coverage.md")),
    ("command", "commands/update-codemaps.md", include_str!("../../../commands/update-codemaps.md")),
    ("command", "commands/update-docs.md", include_str!("../../../commands/update-docs.md")),
    ("command", "commands/verify.md", include_str!("../../../commands/verify.md")),
    ("rule", "rules/agents.md", include_str!("../../../rules/agents.md")),
    ("rule", "rules/coding-style.md", include_str!("../../../rules/coding-style.md")),
    ("rule", "rules/git-workflow.md", include_str!("../../../rules/git-workflow.md")),
    ("rule", "rules/hooks.md", include_str!("../../../rules/hooks.md")),
    ("rule", "rules/patterns.md", include_str!("../../../rules/patterns.md")),
    ("rule", "rules/performance.md", include_str!("../../../rules/performance.md")),
    ("rule", "rules/security.md", include_str!("../../../rules/security.md")),
    ("rule", "rules/testing.md", include_str!("../../../rules/testing.md")),
];


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

