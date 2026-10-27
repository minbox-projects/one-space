use serde_json::Value;

pub(in crate::ai_sessions) fn trim_history_text(input: &str) -> Option<String> {
    let compact = input.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = compact.trim();
    if trimmed.is_empty() {
        return None;
    }
    let clipped: String = trimmed.chars().take(140).collect();
    Some(clipped)
}

pub(in crate::ai_sessions) fn value_as_text(value: &Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        return trim_history_text(text);
    }
    if let Some(array) = value.as_array() {
        let parts = array
            .iter()
            .filter_map(|item| {
                item.get("text")
                    .and_then(|text| text.as_str())
                    .or_else(|| item.get("content").and_then(|content| content.as_str()))
                    .and_then(trim_history_text)
            })
            .collect::<Vec<_>>();
        if !parts.is_empty() {
            return trim_history_text(&parts.join(" "));
        }
    }
    if let Some(object) = value.as_object() {
        if let Some(text) = object.get("text").and_then(|text| text.as_str()) {
            return trim_history_text(text);
        }
        if let Some(text) = object.get("content").and_then(|text| text.as_str()) {
            return trim_history_text(text);
        }
    }
    None
}

pub(in crate::ai_sessions) fn fallback_history_title(tool: &str, session_id: &str) -> String {
    let suffix: String = session_id.chars().take(8).collect();
    format!("{} {}", tool.to_uppercase(), suffix)
}
