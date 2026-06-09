use super::{
    resolve_claude_session_id_for_existing, resolve_codex_session_id_for_existing,
    resolve_gemini_session_id_for_existing, resolve_gemini_session_id_for_pending_bind,
    resolve_opencode_session_id_for_existing,
};
use std::collections::{HashMap, HashSet};

pub fn resolve_native_session_id_for_existing(
    model_type: &str,
    working_dir: &str,
    env: Option<&HashMap<String, String>>,
    created_at_ms: Option<i64>,
    exclude_ids: Option<&HashSet<String>>,
    allow_pending_bind_fallback: bool,
) -> Option<String> {
    match model_type.to_lowercase().as_str() {
        "claude" => {
            resolve_claude_session_id_for_existing(working_dir, created_at_ms, exclude_ids, env)
        }
        "gemini" => {
            let strict =
                resolve_gemini_session_id_for_existing(working_dir, created_at_ms, exclude_ids);
            if strict.is_some() || !allow_pending_bind_fallback {
                strict
            } else {
                resolve_gemini_session_id_for_pending_bind(working_dir, created_at_ms, exclude_ids)
            }
        }
        "codex" => resolve_codex_session_id_for_existing(working_dir, env),
        "opencode" => resolve_opencode_session_id_for_existing(working_dir),
        _ => None,
    }
}
