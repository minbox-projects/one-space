use crate::ai_sessions::HistorySessionEntry;
use std::collections::HashMap;

pub(in crate::ai_sessions) fn dedupe_history_sessions(
    items: Vec<HistorySessionEntry>,
) -> Vec<HistorySessionEntry> {
    let mut by_key = HashMap::<(String, String), HistorySessionEntry>::new();
    for item in items {
        let key = (item.tool.clone(), item.tool_session_id.clone());
        match by_key.get(&key) {
            Some(existing) if existing.updated_at_ms >= item.updated_at_ms => {}
            _ => {
                by_key.insert(key, item);
            }
        }
    }
    let mut out = by_key.into_values().collect::<Vec<_>>();
    out.sort_by(|a, b| {
        b.updated_at_ms
            .cmp(&a.updated_at_ms)
            .then_with(|| a.tool.cmp(&b.tool))
    });
    out
}
