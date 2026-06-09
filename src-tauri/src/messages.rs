use crate::config;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::Emitter;
use uuid::Uuid;

const MESSAGE_SCHEMA_VERSION: u32 = 1;
const MESSAGES_UPDATED_EVENT: &str = "messages-updated";
const DEDUPE_WINDOW_SECONDS: i64 = 60 * 60;
const MAX_MESSAGES: usize = 5000;
const DEFAULT_RETENTION_DAYS: u64 = 30;
const MAX_RETENTION_DAYS: u64 = 365;

static STORE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct MessageTarget {
    pub tab: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct MessageRecord {
    pub id: String,
    pub source: String,
    pub category: String,
    pub severity: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub created_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dedupe_key: Option<String>,
    #[serde(default = "default_occurrences")]
    pub occurrences: u32,
    pub last_seen_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<MessageTarget>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct MessageCreateInput {
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub severity: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub dedupe_key: Option<String>,
    #[serde(default)]
    pub target: Option<MessageTarget>,
    #[serde(default)]
    pub metadata: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct MessageStore {
    pub schema_version: u32,
    #[serde(default)]
    pub messages: Vec<MessageRecord>,
}

impl Default for MessageStore {
    fn default() -> Self {
        Self {
            schema_version: MESSAGE_SCHEMA_VERSION,
            messages: Vec::new(),
        }
    }
}

#[derive(Debug, Serialize, Clone)]
struct MessagesUpdatedPayload {
    unread_count: usize,
}

fn default_occurrences() -> u32 {
    1
}

fn store_lock() -> &'static Mutex<()> {
    STORE_LOCK.get_or_init(|| Mutex::new(()))
}

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn messages_file_path(app_dir: &Path) -> PathBuf {
    app_dir.join("messages").join("messages.json")
}

fn messages_base_dir() -> Result<PathBuf, String> {
    #[cfg(target_os = "windows")]
    {
        if let Some(dir) = dirs::config_dir() {
            let app_dir = dir.join("onespace");
            fs::create_dir_all(&app_dir).map_err(|e| e.to_string())?;
            return Ok(app_dir);
        }
    }

    config::get_app_dir()
}

fn messages_path() -> Result<PathBuf, String> {
    Ok(messages_file_path(&messages_base_dir()?))
}

fn current_retention_days() -> u64 {
    config::get_config()
        .ok()
        .and_then(|cfg| cfg.message_retention_days)
        .unwrap_or(DEFAULT_RETENTION_DAYS)
        .clamp(1, MAX_RETENTION_DAYS)
}

pub fn current_language_is_zh() -> bool {
    config::get_config()
        .ok()
        .and_then(|cfg| cfg.language)
        .map(|language| language.to_ascii_lowercase().starts_with("zh"))
        .unwrap_or(true)
}

pub fn localized(zh: &str, en: &str) -> String {
    if current_language_is_zh() {
        zh.to_string()
    } else {
        en.to_string()
    }
}

fn read_store(path: &Path) -> Result<MessageStore, String> {
    if !path.exists() {
        return Ok(MessageStore::default());
    }

    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    if content.trim().is_empty() {
        return Ok(MessageStore::default());
    }

    if let Ok(store) = serde_json::from_str::<MessageStore>(&content) {
        return Ok(store);
    }

    if let Ok(messages) = serde_json::from_str::<Vec<MessageRecord>>(&content) {
        return Ok(MessageStore {
            schema_version: MESSAGE_SCHEMA_VERSION,
            messages,
        });
    }

    Err("Failed to parse messages store".to_string())
}

fn write_store(path: &Path, store: &MessageStore) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let content = serde_json::to_string_pretty(store).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, content).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

fn sort_messages(messages: &mut [MessageRecord]) {
    messages.sort_by(|a, b| {
        b.last_seen_at
            .cmp(&a.last_seen_at)
            .then_with(|| b.created_at.cmp(&a.created_at))
            .then_with(|| b.id.cmp(&a.id))
    });
}

fn cleanup_store(store: &mut MessageStore, retention_days: u64, now: i64) -> bool {
    let before = store.messages.len();
    let retention_seconds = retention_days.clamp(1, MAX_RETENTION_DAYS) as i64 * 24 * 60 * 60;
    let cutoff = now.saturating_sub(retention_seconds);

    store.messages.retain(|message| {
        let last_seen = if message.last_seen_at > 0 {
            message.last_seen_at
        } else {
            message.created_at
        };
        last_seen >= cutoff
    });

    sort_messages(&mut store.messages);
    if store.messages.len() > MAX_MESSAGES {
        store.messages.truncate(MAX_MESSAGES);
    }

    before != store.messages.len()
}

fn unread_count(messages: &[MessageRecord]) -> usize {
    messages
        .iter()
        .filter(|message| message.read_at.is_none())
        .count()
}

fn emit_messages_updated(app: &tauri::AppHandle, messages: &[MessageRecord]) {
    let _ = app.emit(
        MESSAGES_UPDATED_EVENT,
        MessagesUpdatedPayload {
            unread_count: unread_count(messages),
        },
    );
}

fn truncate_chars(input: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (idx, ch) in input.chars().enumerate() {
        if idx >= max_chars {
            out.push_str("\n...[truncated]");
            return out;
        }
        out.push(ch);
    }
    out
}

fn redact_text(input: &str, max_chars: usize) -> String {
    let mut out = input.to_string();
    if let Ok(re) = Regex::new(
        r"(?i)\b(password|passwd|token|secret|api[_-]?key|authorization|cookie)(\s*[:=]\s*)([^\s,;]+)",
    ) {
        out = re.replace_all(&out, "$1$2[redacted]").to_string();
    }
    if let Ok(re) = Regex::new(r"(?i)\bbearer\s+[A-Za-z0-9._~+/=-]+") {
        out = re.replace_all(&out, "Bearer [redacted]").to_string();
    }
    truncate_chars(&out, max_chars)
}

fn sensitive_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    [
        "password",
        "passwd",
        "token",
        "secret",
        "api_key",
        "apikey",
        "authorization",
        "cookie",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn redact_json(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut redacted = Map::new();
            for (key, value) in map {
                if sensitive_key(&key) {
                    redacted.insert(key, Value::String("[redacted]".to_string()));
                } else {
                    redacted.insert(key, redact_json(value));
                }
            }
            Value::Object(redacted)
        }
        Value::Array(items) => Value::Array(items.into_iter().map(redact_json).collect()),
        Value::String(text) => Value::String(redact_text(&text, 1000)),
        other => other,
    }
}

fn normalize_non_empty(value: String, fallback: &str, max_chars: usize) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        redact_text(trimmed, max_chars)
    }
}

fn normalize_optional(value: Option<String>, max_chars: usize) -> Option<String> {
    value.and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(redact_text(trimmed, max_chars))
        }
    })
}

fn normalize_severity(value: String) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "success" => "success".to_string(),
        "warning" | "warn" => "warning".to_string(),
        "error" | "danger" | "critical" => "error".to_string(),
        _ => "info".to_string(),
    }
}

fn normalize_dedupe_key(value: Option<String>) -> Option<String> {
    value.and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(truncate_chars(trimmed, 240))
        }
    })
}

fn normalize_target(target: Option<MessageTarget>) -> Option<MessageTarget> {
    target.and_then(|target| {
        let tab = target.tab.trim();
        if tab.is_empty() {
            None
        } else {
            Some(MessageTarget {
                tab: truncate_chars(tab, 120),
                section: target.section.and_then(|value| {
                    let trimmed = value.trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some(truncate_chars(trimmed, 120))
                    }
                }),
                entity_id: target.entity_id.and_then(|value| {
                    let trimmed = value.trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some(truncate_chars(trimmed, 240))
                    }
                }),
            })
        }
    })
}

fn normalize_input(input: MessageCreateInput) -> MessageCreateInput {
    MessageCreateInput {
        source: normalize_non_empty(input.source, "system", 80),
        category: normalize_non_empty(input.category, "general", 80),
        severity: normalize_severity(input.severity),
        title: normalize_non_empty(input.title, "Untitled message", 160),
        summary: normalize_optional(input.summary, 500),
        detail: normalize_optional(input.detail, 10000),
        dedupe_key: normalize_dedupe_key(input.dedupe_key),
        target: normalize_target(input.target),
        metadata: input.metadata.map(redact_json),
    }
}

fn create_message_at_path(
    path: &Path,
    retention_days: u64,
    input: MessageCreateInput,
    now: i64,
) -> Result<MessageRecord, String> {
    let mut store = read_store(path)?;
    cleanup_store(&mut store, retention_days, now);
    let input = normalize_input(input);

    if let Some(ref dedupe_key) = input.dedupe_key {
        if let Some(existing) = store.messages.iter_mut().find(|message| {
            message.dedupe_key.as_deref() == Some(dedupe_key.as_str())
                && now.saturating_sub(message.last_seen_at) <= DEDUPE_WINDOW_SECONDS
        }) {
            existing.source = input.source;
            existing.category = input.category;
            existing.severity = input.severity;
            existing.title = input.title;
            existing.summary = input.summary;
            existing.detail = input.detail;
            existing.target = input.target;
            existing.metadata = input.metadata;
            existing.occurrences = existing.occurrences.saturating_add(1).max(1);
            existing.last_seen_at = now;
            existing.read_at = None;
            let record = existing.clone();
            sort_messages(&mut store.messages);
            write_store(path, &store)?;
            return Ok(record);
        }
    }

    let record = MessageRecord {
        id: Uuid::new_v4().to_string(),
        source: input.source,
        category: input.category,
        severity: input.severity,
        title: input.title,
        summary: input.summary,
        detail: input.detail,
        created_at: now,
        read_at: None,
        dedupe_key: input.dedupe_key,
        occurrences: 1,
        last_seen_at: now,
        target: input.target,
        metadata: input.metadata,
    };
    store.messages.push(record.clone());
    sort_messages(&mut store.messages);
    if store.messages.len() > MAX_MESSAGES {
        store.messages.truncate(MAX_MESSAGES);
    }
    write_store(path, &store)?;
    Ok(record)
}

fn list_messages_at_path(
    path: &Path,
    retention_days: u64,
    now: i64,
) -> Result<(Vec<MessageRecord>, bool), String> {
    let mut store = read_store(path)?;
    let changed = cleanup_store(&mut store, retention_days, now);
    if changed {
        write_store(path, &store)?;
    }
    Ok((store.messages, changed))
}

fn mark_read_at_path(path: &Path, retention_days: u64, id: &str, now: i64) -> Result<bool, String> {
    let mut store = read_store(path)?;
    let mut changed = cleanup_store(&mut store, retention_days, now);
    if let Some(message) = store.messages.iter_mut().find(|message| message.id == id) {
        if message.read_at.is_none() {
            message.read_at = Some(now);
            changed = true;
        }
    }
    if changed {
        write_store(path, &store)?;
    }
    Ok(changed)
}

fn mark_all_read_at_path(path: &Path, retention_days: u64, now: i64) -> Result<bool, String> {
    let mut store = read_store(path)?;
    let mut changed = cleanup_store(&mut store, retention_days, now);
    for message in &mut store.messages {
        if message.read_at.is_none() {
            message.read_at = Some(now);
            changed = true;
        }
    }
    if changed {
        write_store(path, &store)?;
    }
    Ok(changed)
}

fn cleanup_current_path() -> Result<(Vec<MessageRecord>, bool), String> {
    let path = messages_path()?;
    let now = now_ts();
    list_messages_at_path(&path, current_retention_days(), now)
}

pub fn record_message_silent(app: &tauri::AppHandle, input: MessageCreateInput) {
    if let Err(err) = messages_create(app.clone(), input) {
        eprintln!("messages_create failed: {}", err);
    }
}

pub fn cleanup_for_current_retention(app: tauri::AppHandle) -> Result<(), String> {
    let _guard = store_lock().lock().map_err(|e| e.to_string())?;
    let (messages, changed) = cleanup_current_path()?;
    if changed {
        emit_messages_updated(&app, &messages);
    }
    Ok(())
}

#[tauri::command]
pub fn messages_list(app: tauri::AppHandle) -> Result<Vec<MessageRecord>, String> {
    let _guard = store_lock().lock().map_err(|e| e.to_string())?;
    let (messages, changed) = cleanup_current_path()?;
    if changed {
        emit_messages_updated(&app, &messages);
    }
    Ok(messages)
}

#[tauri::command]
pub fn messages_unread_count(app: tauri::AppHandle) -> Result<usize, String> {
    let _guard = store_lock().lock().map_err(|e| e.to_string())?;
    let (messages, changed) = cleanup_current_path()?;
    if changed {
        emit_messages_updated(&app, &messages);
    }
    Ok(unread_count(&messages))
}

#[tauri::command]
pub fn messages_create(
    app: tauri::AppHandle,
    input: MessageCreateInput,
) -> Result<MessageRecord, String> {
    let _guard = store_lock().lock().map_err(|e| e.to_string())?;
    let path = messages_path()?;
    let record = create_message_at_path(&path, current_retention_days(), input, now_ts())?;
    let store = read_store(&path)?;
    emit_messages_updated(&app, &store.messages);
    Ok(record)
}

#[tauri::command]
pub fn messages_mark_read(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let _guard = store_lock().lock().map_err(|e| e.to_string())?;
    let path = messages_path()?;
    let changed = mark_read_at_path(&path, current_retention_days(), &id, now_ts())?;
    if changed {
        let store = read_store(&path)?;
        emit_messages_updated(&app, &store.messages);
    }
    Ok(())
}

#[tauri::command]
pub fn messages_mark_all_read(app: tauri::AppHandle) -> Result<(), String> {
    let _guard = store_lock().lock().map_err(|e| e.to_string())?;
    let path = messages_path()?;
    let changed = mark_all_read_at_path(&path, current_retention_days(), now_ts())?;
    if changed {
        let store = read_store(&path)?;
        emit_messages_updated(&app, &store.messages);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        create_message_at_path, list_messages_at_path, mark_all_read_at_path, mark_read_at_path,
        messages_file_path, unread_count, write_store, MessageCreateInput, MessageRecord,
        MessageStore, DEDUPE_WINDOW_SECONDS, MAX_MESSAGES, MESSAGE_SCHEMA_VERSION,
    };
    use std::path::{Path, PathBuf};
    use uuid::Uuid;

    fn temp_messages_path(test_name: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!(
                "onespace-messages-{}-{}",
                test_name,
                Uuid::new_v4()
            ))
            .join("messages.json")
    }

    fn input(title: &str) -> MessageCreateInput {
        MessageCreateInput {
            source: "test".to_string(),
            category: "unit".to_string(),
            severity: "info".to_string(),
            title: title.to_string(),
            summary: None,
            detail: None,
            dedupe_key: None,
            target: None,
            metadata: None,
        }
    }

    #[test]
    fn creates_sorts_and_counts_unread_messages() {
        let path = temp_messages_path("sort-unread");
        let first = create_message_at_path(&path, 30, input("first"), 100).unwrap();
        let second = create_message_at_path(&path, 30, input("second"), 110).unwrap();

        let (messages, changed) = list_messages_at_path(&path, 30, 110).unwrap();
        assert!(!changed);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].id, second.id);
        assert_eq!(messages[1].id, first.id);
        assert_eq!(unread_count(&messages), 2);
    }

    #[test]
    fn marks_single_and_all_messages_read() {
        let path = temp_messages_path("mark-read");
        let first = create_message_at_path(&path, 30, input("first"), 100).unwrap();
        create_message_at_path(&path, 30, input("second"), 101).unwrap();

        assert!(mark_read_at_path(&path, 30, &first.id, 120).unwrap());
        let (messages, _) = list_messages_at_path(&path, 30, 120).unwrap();
        assert_eq!(unread_count(&messages), 1);

        assert!(mark_all_read_at_path(&path, 30, 130).unwrap());
        let (messages, _) = list_messages_at_path(&path, 30, 130).unwrap();
        assert_eq!(unread_count(&messages), 0);
    }

    #[test]
    fn cleans_messages_by_retention_days() {
        let path = temp_messages_path("retention");
        create_message_at_path(&path, 30, input("old"), 10).unwrap();
        create_message_at_path(&path, 30, input("fresh"), 200_000).unwrap();

        let (messages, changed) = list_messages_at_path(&path, 1, 200_000).unwrap();
        assert!(changed);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].title, "fresh");
    }

    #[test]
    fn caps_store_to_latest_five_thousand_messages() {
        let path = temp_messages_path("cap");
        let store = MessageStore {
            schema_version: MESSAGE_SCHEMA_VERSION,
            messages: (0..(MAX_MESSAGES + 5))
                .map(|idx| MessageRecord {
                    id: idx.to_string(),
                    source: "test".to_string(),
                    category: "unit".to_string(),
                    severity: "info".to_string(),
                    title: format!("message {}", idx),
                    summary: None,
                    detail: None,
                    created_at: idx as i64,
                    read_at: None,
                    dedupe_key: None,
                    occurrences: 1,
                    last_seen_at: idx as i64,
                    target: None,
                    metadata: None,
                })
                .collect(),
        };
        write_store(&path, &store).unwrap();

        let (messages, _) = list_messages_at_path(&path, 365, (MAX_MESSAGES + 5) as i64).unwrap();
        assert_eq!(messages.len(), MAX_MESSAGES);
        assert_eq!(messages[0].title, format!("message {}", MAX_MESSAGES + 4));
        assert_eq!(messages[MAX_MESSAGES - 1].title, "message 5");
    }

    #[test]
    fn deduplicates_messages_with_same_key_inside_window() {
        let path = temp_messages_path("dedupe");
        let mut first = input("first");
        first.dedupe_key = Some("same-key".to_string());
        create_message_at_path(&path, 30, first, 100).unwrap();

        let mut second = input("second");
        second.dedupe_key = Some("same-key".to_string());
        second.summary = Some("new summary".to_string());
        let merged =
            create_message_at_path(&path, 30, second, 100 + DEDUPE_WINDOW_SECONDS).unwrap();

        let (messages, _) = list_messages_at_path(&path, 30, 100 + DEDUPE_WINDOW_SECONDS).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(merged.occurrences, 2);
        assert_eq!(messages[0].title, "second");
        assert_eq!(messages[0].summary.as_deref(), Some("new summary"));
        assert_eq!(messages[0].last_seen_at, 100 + DEDUPE_WINDOW_SECONDS);
    }

    #[test]
    fn creates_new_message_for_same_key_after_window() {
        let path = temp_messages_path("dedupe-expired");
        let mut first = input("first");
        first.dedupe_key = Some("same-key".to_string());
        create_message_at_path(&path, 30, first, 100).unwrap();

        let mut second = input("second");
        second.dedupe_key = Some("same-key".to_string());
        create_message_at_path(&path, 30, second, 100 + DEDUPE_WINDOW_SECONDS + 1).unwrap();

        let (messages, _) =
            list_messages_at_path(&path, 30, 100 + DEDUPE_WINDOW_SECONDS + 1).unwrap();
        assert_eq!(messages.len(), 2);
    }

    #[test]
    fn message_store_path_is_outside_shared_sync_roots() {
        let app_dir = Path::new("/Users/test/.config/onespace");
        let path = messages_file_path(app_dir);
        assert_eq!(
            path,
            Path::new("/Users/test/.config/onespace/messages/messages.json")
        );
        assert!(!path.to_string_lossy().contains("/local_data/"));
        assert!(!path.to_string_lossy().contains("/shared/"));
        assert!(!path.to_string_lossy().contains("/git_data/"));
    }
}
