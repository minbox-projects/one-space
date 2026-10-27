use crate::ai_sessions::SourceFileMeta;
use std::path::Path;

/// A stable metadata discriminator for history parse-cache keys. It is built
/// from a file's byte length and mtime (never a hash), so a changed shared
/// index or transcript forces only its dependent parses to re-run.
pub(super) fn history_dependency(path: &Path) -> String {
    SourceFileMeta::for_path(path)
        .map(|meta| format!("{}:{}", meta.len, meta.modified_ms))
        .unwrap_or_else(|| "none".to_string())
}
