mod catalog_parse;
mod commands;
mod diff;
mod installed_scan;
mod paths_state;
mod repository;
mod sync_apply;
#[cfg(test)]
mod tests;
mod types;

pub(in crate::skills) use catalog_parse::*;
pub(in crate::skills) use commands::*;
pub(in crate::skills) use diff::*;
pub(in crate::skills) use installed_scan::*;
pub(in crate::skills) use paths_state::*;
pub(in crate::skills) use repository::*;
pub(in crate::skills) use sync_apply::*;
pub(in crate::skills) use types::*;

pub use commands::*;
pub use types::*;

/// Returns the supported CLI compatibility contract for the canonical Skills directory.
pub fn compatibility_matrix() -> Vec<CompatibilityResult> {
    vec![
        CompatibilityResult {
            tool: "claude".to_string(),
            kind: CompatibilityKind::CompatibilityPath,
            evidence: vec![
                "Claude reads ~/.claude/skills; OneSpace projects the unified Skills directory there.".to_string(),
            ],
        },
        CompatibilityResult {
            tool: "opencode".to_string(),
            kind: CompatibilityKind::CompatibilityPath,
            evidence: vec![
                "OpenCode reads ~/.config/opencode/skills and <project>/.opencode/skills, so it uses a tool-specific projection.".to_string(),
            ],
        },
        CompatibilityResult {
            tool: "codex".to_string(),
            kind: CompatibilityKind::DirectUnified,
            evidence: vec![
                "Codex project Skills resolve to <project>/.agents/skills, the unified project directory.".to_string(),
                "OneSpace's canonical unified directory is ~/.agents/skills for the Codex-compatible layout.".to_string(),
            ],
        },
        CompatibilityResult {
            tool: "antigravity".to_string(),
            kind: CompatibilityKind::CompatibilityPath,
            evidence: vec![
                "Antigravity projects resolve to <project>/.agents/skills, while its global directory is ~/.gemini/config/skills.".to_string(),
            ],
        },
    ]
}
