use crate::{atomic_write_string, config, messages};
use chrono::{DateTime, Utc};
use quick_xml::escape::resolve_predefined_entity;
use quick_xml::events::Event;
use quick_xml::Reader;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct AiNewsItem {
    pub id: String,
    pub provider: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub url: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub language: String,
    pub published_at: u64,
    pub fetched_at: u64,
    #[serde(default)]
    pub rank: f64,
    #[serde(default)]
    pub is_new: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct AiNewsProviderSyncState {
    pub provider: String,
    pub status: String,
    pub fetched_count: usize,
    pub added_count: usize,
    #[serde(default)]
    pub last_error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct AiNewsSyncState {
    pub status: String,
    #[serde(default)]
    pub last_error: Option<String>,
    pub last_sync_at: Option<u64>,
    pub added_count: usize,
    #[serde(default)]
    pub provider_states: Vec<AiNewsProviderSyncState>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ApiMeta {
    pub revision: u64,
    pub ts: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ApiOk<T> {
    pub ok: bool,
    pub data: T,
    pub meta: ApiMeta,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct AiNewsStore {
    pub revision: u64,
    #[serde(default)]
    pub items: Vec<AiNewsItem>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct AiNewsSyncStore {
    pub revision: u64,
    #[serde(default)]
    pub status: AiNewsSyncState,
}

#[derive(Debug, Clone, Default)]
struct RssEntry {
    title: String,
    link: String,
    description: String,
    pub_date: String,
    guid: String,
    categories: Vec<String>,
}

fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn api_ok<T: Serialize>(data: T, revision: u64) -> Result<ApiOk<T>, String> {
    Ok(ApiOk {
        ok: true,
        data,
        meta: ApiMeta {
            revision,
            ts: now_ts(),
        },
    })
}

fn news_root() -> Result<PathBuf, String> {
    let dir = crate::get_data_dir()?.join("data").join("news");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

pub fn ai_news_local_path() -> Result<PathBuf, String> {
    Ok(news_root()?.join("ai_news.json"))
}

fn sync_state_path() -> Result<PathBuf, String> {
    Ok(news_root()?.join("sync_state.json"))
}

fn read_json_or_default<T: for<'de> Deserialize<'de> + Default>(
    path: &PathBuf,
) -> Result<T, String> {
    if !path.exists() {
        return Ok(T::default());
    }
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    if content.trim().is_empty() {
        return Ok(T::default());
    }
    serde_json::from_str(&content).map_err(|e| e.to_string())
}

fn write_json<T: Serialize>(path: &PathBuf, value: &T) -> Result<(), String> {
    let content = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    atomic_write_string(path, &content)
}

fn load_news_store() -> Result<AiNewsStore, String> {
    read_json_or_default(&ai_news_local_path()?)
}

pub fn ai_news_count_fast() -> Result<usize, String> {
    load_news_store().map(|store| store.items.len())
}

fn save_news_store(mut store: AiNewsStore) -> Result<AiNewsStore, String> {
    store.revision = store.revision.saturating_add(1);
    write_json(&ai_news_local_path()?, &store)?;
    Ok(store)
}

fn load_sync_store() -> Result<AiNewsSyncStore, String> {
    read_json_or_default(&sync_state_path()?)
}

fn save_sync_store(mut store: AiNewsSyncStore) -> Result<AiNewsSyncStore, String> {
    store.revision = store.revision.saturating_add(1);
    write_json(&sync_state_path()?, &store)?;
    Ok(store)
}

fn canonicalize_url(url: &str) -> String {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    trimmed.trim_end_matches('/').to_lowercase()
}

fn dedupe_key(item: &AiNewsItem) -> String {
    let canon = canonicalize_url(&item.url);
    if !canon.is_empty() {
        return canon;
    }
    let source = item.source.trim().to_lowercase();
    let title = item.title.trim().to_lowercase();
    format!("{}|{}|{}", source, title, item.published_at)
}

fn make_item_id(provider: &str, url: &str, title: &str, published_at: u64) -> String {
    let mut hasher = Sha256::new();
    hasher.update(provider.as_bytes());
    hasher.update(b"|");
    hasher.update(url.as_bytes());
    hasher.update(b"|");
    hasher.update(title.as_bytes());
    hasher.update(b"|");
    hasher.update(published_at.to_string().as_bytes());
    let digest = hasher.finalize();
    let hex = format!("{:x}", digest);
    hex.chars().take(32).collect()
}

fn item_rank(item: &AiNewsItem, now: u64) -> f64 {
    let age_hours = now.saturating_sub(item.published_at) as f64 / 3600.0;
    let freshness = (72.0 - age_hours).max(0.0);
    freshness + 1.0
}

fn dedupe_keep_latest(items: Vec<AiNewsItem>) -> Vec<AiNewsItem> {
    let mut map: HashMap<String, AiNewsItem> = HashMap::new();
    for item in items {
        let key = dedupe_key(&item);
        match map.get(&key) {
            None => {
                map.insert(key, item);
            }
            Some(existing) => {
                let replace = item.published_at > existing.published_at
                    || (item.published_at == existing.published_at
                        && item.fetched_at >= existing.fetched_at);
                if replace {
                    map.insert(key, item);
                }
            }
        }
    }
    map.into_values().collect()
}

fn apply_retention(
    mut items: Vec<AiNewsItem>,
    retention_days: u64,
    retention_max_items: usize,
) -> Vec<AiNewsItem> {
    let now = now_ts();
    let cutoff = now.saturating_sub(retention_days.saturating_mul(24 * 60 * 60));
    items.retain(|item| item.published_at >= cutoff);
    items.sort_by(|a, b| {
        b.published_at
            .cmp(&a.published_at)
            .then_with(|| b.fetched_at.cmp(&a.fetched_at))
    });
    items.truncate(retention_max_items);
    items
}

fn build_client() -> Result<Client, String> {
    if let Some(proxy_mgr) = crate::proxy::PROXY_MANAGER.get() {
        return proxy_mgr.get_client();
    }
    Ok(Client::new())
}

fn split_news_keywords(custom_keywords: Option<&str>) -> Vec<String> {
    let raw = custom_keywords.unwrap_or("").trim();
    if raw.is_empty() {
        return Vec::new();
    }
    raw.split(|c| matches!(c, ',' | '\n' | '\r' | ';' | '，' | '；'))
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(|token| token.to_lowercase())
        .collect()
}

fn keyword_matches(haystack: &str, keyword: &str) -> bool {
    if keyword.is_empty() {
        return false;
    }
    if keyword.chars().all(|ch| ch.is_ascii_alphanumeric()) {
        let mut start = 0usize;
        while let Some(offset) = haystack[start..].find(keyword) {
            let match_start = start + offset;
            let match_end = match_start + keyword.len();
            let prev = haystack[..match_start].chars().next_back();
            let next = haystack[match_end..].chars().next();
            let prev_ok = prev.is_none_or(|ch| !ch.is_ascii_alphanumeric());
            let next_ok = next.is_none_or(|ch| !ch.is_ascii_alphanumeric());
            if prev_ok && next_ok {
                return true;
            }
            start = match_end;
        }
        return false;
    }
    haystack.contains(keyword)
}

fn rss_entry_matches_keywords(entry: &RssEntry, source_name: &str, keywords: &[String]) -> bool {
    if keywords.is_empty() {
        return true;
    }
    let haystack = format!(
        "{}\n{}\n{}\n{}",
        entry.title,
        entry.description,
        source_name,
        entry.categories.join("\n")
    )
    .to_lowercase();
    keywords
        .iter()
        .filter(|token| !token.is_empty())
        .any(|keyword| keyword_matches(&haystack, keyword))
}

fn local_name(name: &[u8]) -> String {
    let decoded = String::from_utf8_lossy(name);
    decoded
        .rsplit(':')
        .next()
        .unwrap_or(&decoded)
        .to_ascii_lowercase()
}

fn parse_rss_entries(xml: &str) -> Result<Vec<RssEntry>, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut entries = Vec::new();
    let mut current: Option<RssEntry> = None;
    let mut current_field: Option<String> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = local_name(e.name().as_ref());
                if name == "item" {
                    current = Some(RssEntry::default());
                    current_field = None;
                } else if current.is_some()
                    && matches!(
                        name.as_str(),
                        "title" | "link" | "description" | "pubdate" | "guid" | "category"
                    )
                {
                    current_field = Some(name);
                }
            }
            Ok(Event::Text(e)) => {
                if let (Some(entry), Some(field)) = (current.as_mut(), current_field.as_deref()) {
                    let text = e.xml_content().map_err(|err| err.to_string())?.to_string();
                    if text.is_empty() {
                        continue;
                    }
                    match field {
                        "title" => append_xml_text(&mut entry.title, &text),
                        "link" => append_xml_text(&mut entry.link, &text),
                        "description" => append_xml_text(&mut entry.description, &text),
                        "pubdate" => append_xml_text(&mut entry.pub_date, &text),
                        "guid" => append_xml_text(&mut entry.guid, &text),
                        "category" => entry.categories.push(text.trim().to_string()),
                        _ => {}
                    }
                }
            }
            Ok(Event::CData(e)) => {
                if let (Some(entry), Some(field)) = (current.as_mut(), current_field.as_deref()) {
                    let text = e.xml_content().map_err(|err| err.to_string())?.to_string();
                    if text.is_empty() {
                        continue;
                    }
                    match field {
                        "title" => append_xml_text(&mut entry.title, &text),
                        "link" => append_xml_text(&mut entry.link, &text),
                        "description" => append_xml_text(&mut entry.description, &text),
                        "pubdate" => append_xml_text(&mut entry.pub_date, &text),
                        "guid" => append_xml_text(&mut entry.guid, &text),
                        "category" => entry.categories.push(text.trim().to_string()),
                        _ => {}
                    }
                }
            }
            Ok(Event::GeneralRef(e)) => {
                if let (Some(entry), Some(field)) = (current.as_mut(), current_field.as_deref()) {
                    let text =
                        if let Some(ch) = e.resolve_char_ref().map_err(|err| err.to_string())? {
                            ch.to_string()
                        } else {
                            let entity = e.decode().map_err(|err| err.to_string())?;
                            resolve_predefined_entity(&entity)
                                .map(str::to_string)
                                .unwrap_or_else(|| format!("&{};", entity))
                        };
                    match field {
                        "title" => append_xml_text(&mut entry.title, &text),
                        "link" => append_xml_text(&mut entry.link, &text),
                        "description" => append_xml_text(&mut entry.description, &text),
                        "pubdate" => append_xml_text(&mut entry.pub_date, &text),
                        "guid" => append_xml_text(&mut entry.guid, &text),
                        "category" => entry.categories.push(text),
                        _ => {}
                    }
                }
            }
            Ok(Event::End(e)) => {
                let name = local_name(e.name().as_ref());
                if name == "item" {
                    if let Some(mut entry) = current.take() {
                        trim_rss_entry(&mut entry);
                        entries.push(entry);
                    }
                    current_field = None;
                } else if current_field.as_deref() == Some(name.as_str()) {
                    current_field = None;
                }
            }
            Ok(Event::Eof) => break,
            Err(err) => return Err(err.to_string()),
            _ => {}
        }
    }

    Ok(entries)
}

fn append_xml_text(target: &mut String, text: &str) {
    target.push_str(text);
}

fn trim_rss_entry(entry: &mut RssEntry) {
    entry.title = entry.title.trim().to_string();
    entry.link = entry.link.trim().to_string();
    entry.description = entry.description.trim().to_string();
    entry.pub_date = entry.pub_date.trim().to_string();
    entry.guid = entry.guid.trim().to_string();
    entry.categories = entry
        .categories
        .iter()
        .map(|category| category.trim().to_string())
        .filter(|category| !category.is_empty())
        .collect();
}

fn parse_published_ts(input: &str) -> u64 {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return now_ts();
    }
    DateTime::parse_from_rfc3339(trimmed)
        .or_else(|_| DateTime::parse_from_rfc2822(trimmed))
        .or_else(|_| DateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M:%S %z"))
        .or_else(|_| DateTime::parse_from_str(trimmed, "%Y/%m/%d %H:%M:%S %z"))
        .map(|dt| dt.with_timezone(&Utc).timestamp().max(0) as u64)
        .unwrap_or_else(|_| now_ts())
}

fn rss_entry_to_item(
    source: &config::AiNewsRssSource,
    entry: RssEntry,
    fetched_at: u64,
) -> Option<AiNewsItem> {
    let title = entry.title.trim();
    let url = entry.link.trim();
    if title.is_empty() || url.is_empty() {
        return None;
    }
    let published_at = parse_published_ts(&entry.pub_date);
    Some(AiNewsItem {
        id: make_item_id(&source.id, url, title, published_at),
        provider: source.id.clone(),
        title: title.to_string(),
        description: entry.description.trim().to_string(),
        url: url.to_string(),
        source: source.name.trim().to_string(),
        language: String::new(),
        published_at,
        fetched_at,
        rank: 0.0,
        is_new: false,
    })
}

async fn fetch_rss_source(
    client: &Client,
    source: &config::AiNewsRssSource,
    keywords: &[String],
    max_results: usize,
) -> Result<Vec<AiNewsItem>, String> {
    let response = client
        .get(source.url.trim())
        .timeout(Duration::from_secs(20))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(format!(
            "{} HTTP {}: {}",
            source.name,
            status.as_u16(),
            text
        ));
    }

    let xml = response.text().await.map_err(|e| e.to_string())?;
    let entries = parse_rss_entries(&xml)?;
    let fetched_at = now_ts();
    let mut out = Vec::new();
    for entry in entries {
        if !rss_entry_matches_keywords(&entry, &source.name, keywords) {
            continue;
        }
        if let Some(item) = rss_entry_to_item(source, entry, fetched_at) {
            out.push(item);
            if out.len() >= max_results {
                break;
            }
        }
    }
    Ok(out)
}

#[tauri::command]
pub fn ai_news_read() -> Result<ApiOk<Vec<AiNewsItem>>, String> {
    let mut store = load_news_store()?;
    store.items.sort_by(|a, b| {
        b.published_at
            .cmp(&a.published_at)
            .then_with(|| b.fetched_at.cmp(&a.fetched_at))
    });
    api_ok(store.items, store.revision)
}

#[tauri::command]
pub fn ai_news_sync_status_get() -> Result<ApiOk<AiNewsSyncState>, String> {
    let store = load_sync_store()?;
    api_ok(store.status, store.revision)
}

#[tauri::command]
pub async fn ai_news_sync_now(app: tauri::AppHandle) -> Result<ApiOk<AiNewsSyncState>, String> {
    let cfg = config::get_storage_config()?;
    let enabled = cfg.ai_news_enabled.unwrap_or(false);
    if !enabled {
        let mut sync_store = load_sync_store()?;
        sync_store.status = AiNewsSyncState {
            status: "skipped_disabled".to_string(),
            last_error: None,
            last_sync_at: Some(now_ts()),
            added_count: 0,
            provider_states: vec![],
        };
        sync_store = save_sync_store(sync_store)?;
        return api_ok(sync_store.status, sync_store.revision);
    }

    let retention_days = cfg.ai_news_retention_days.unwrap_or(90).clamp(1, 3650);
    let retention_max_items = cfg
        .ai_news_retention_max_items
        .unwrap_or(1000)
        .clamp(10, 100000) as usize;
    let keywords = split_news_keywords(cfg.ai_news_keywords.as_deref());
    let max_results = 20usize;
    let client = build_client()?;

    let mut provider_states: Vec<AiNewsProviderSyncState> = Vec::new();
    let mut fetched: Vec<AiNewsItem> = Vec::new();

    for source in cfg
        .ai_news_rss_sources
        .iter()
        .filter(|source| source.enabled && !source.url.trim().is_empty())
    {
        match fetch_rss_source(&client, source, &keywords, max_results).await {
            Ok(items) => {
                provider_states.push(AiNewsProviderSyncState {
                    provider: source.id.clone(),
                    status: "done".to_string(),
                    fetched_count: items.len(),
                    added_count: 0,
                    last_error: None,
                });
                fetched.extend(items);
            }
            Err(err) => {
                provider_states.push(AiNewsProviderSyncState {
                    provider: source.id.clone(),
                    status: "error".to_string(),
                    fetched_count: 0,
                    added_count: 0,
                    last_error: Some(err),
                });
            }
        }
    }

    if provider_states.is_empty() {
        provider_states.push(AiNewsProviderSyncState {
            provider: "rss".to_string(),
            status: "skipped_no_source".to_string(),
            fetched_count: 0,
            added_count: 0,
            last_error: None,
        });
    }

    let mut news_store = load_news_store()?;
    for item in &mut news_store.items {
        item.is_new = false;
    }
    let existing_keys: HashSet<String> = news_store.items.iter().map(dedupe_key).collect();

    let mut added_count = 0usize;
    for item in &mut fetched {
        let key = dedupe_key(item);
        if !existing_keys.contains(&key) {
            item.is_new = true;
            added_count += 1;
        }
    }

    let mut per_provider_added: HashMap<String, usize> = HashMap::new();
    for item in &fetched {
        if item.is_new {
            let entry = per_provider_added.entry(item.provider.clone()).or_insert(0);
            *entry += 1;
        }
    }

    news_store.items.extend(fetched);
    let mut merged = dedupe_keep_latest(news_store.items);
    let now = now_ts();
    for item in &mut merged {
        item.rank = item_rank(item, now);
    }
    merged = apply_retention(merged, retention_days, retention_max_items);
    news_store.items = merged;
    news_store = save_news_store(news_store)?;

    for provider_state in &mut provider_states {
        provider_state.added_count = per_provider_added
            .get(&provider_state.provider)
            .copied()
            .unwrap_or(0);
    }

    let mut status = AiNewsSyncState {
        status: "done".to_string(),
        last_error: None,
        last_sync_at: Some(now_ts()),
        added_count,
        provider_states,
    };

    let should_sync_now = added_count > 0 && cfg.sync_policy.ai_news;
    if should_sync_now {
        if let Err(err) = crate::app_store::sync_run_now(app.clone()).await {
            status.status = "error".to_string();
            status.last_error = Some(format!(
                "news-added sync failed: {} ({})",
                err.message, err.code
            ));
        }
    }

    if added_count > 0 {
        messages::record_message_silent(
            &app,
            messages::MessageCreateInput {
                source: "ai_news".to_string(),
                category: "background_fetch".to_string(),
                severity: "success".to_string(),
                title: messages::localized("AI News 抓取完成", "AI News fetch completed"),
                summary: Some(if messages::current_language_is_zh() {
                    format!("新增 {} 条 AI 资讯", added_count)
                } else {
                    format!("Added {} AI news item(s)", added_count)
                }),
                detail: Some(
                    status
                        .provider_states
                        .iter()
                        .map(|state| {
                            format!(
                                "{}: status={}, fetched={}, added={}",
                                state.provider,
                                state.status,
                                state.fetched_count,
                                state.added_count
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n"),
                ),
                dedupe_key: Some("ai-news:added".to_string()),
                target: Some(messages::MessageTarget {
                    tab: "ai-news".to_string(),
                    section: None,
                    entity_id: None,
                }),
                metadata: Some(json!({
                    "added_count": added_count,
                    "providers": status.provider_states,
                })),
            },
        );
    }

    let provider_errors: Vec<_> = status
        .provider_states
        .iter()
        .filter(|state| state.status == "error" && state.last_error.is_some())
        .collect();
    if !provider_errors.is_empty() {
        let detail = provider_errors
            .iter()
            .map(|state| {
                format!(
                    "{}: {}",
                    state.provider,
                    state.last_error.as_deref().unwrap_or("Unknown error")
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        messages::record_message_silent(
            &app,
            messages::MessageCreateInput {
                source: "ai_news".to_string(),
                category: "background_fetch".to_string(),
                severity: "error".to_string(),
                title: messages::localized("AI News 抓取失败", "AI News fetch failed"),
                summary: Some(
                    detail
                        .lines()
                        .next()
                        .unwrap_or("News provider failed")
                        .to_string(),
                ),
                detail: Some(detail),
                dedupe_key: Some("ai-news:provider-error".to_string()),
                target: Some(messages::MessageTarget {
                    tab: "ai-news".to_string(),
                    section: None,
                    entity_id: None,
                }),
                metadata: Some(json!({
                    "providers": provider_errors
                        .iter()
                        .map(|state| state.provider.clone())
                        .collect::<Vec<_>>(),
                })),
            },
        );
    }

    if let Some(sync_error) = status.last_error.clone() {
        messages::record_message_silent(
            &app,
            messages::MessageCreateInput {
                source: "ai_news".to_string(),
                category: "sync".to_string(),
                severity: "error".to_string(),
                title: messages::localized("AI News 同步失败", "AI News sync failed"),
                summary: Some(sync_error.clone()),
                detail: Some(sync_error),
                dedupe_key: Some("ai-news:sync-error".to_string()),
                target: Some(messages::MessageTarget {
                    tab: "ai-news".to_string(),
                    section: None,
                    entity_id: None,
                }),
                metadata: Some(json!({ "added_count": added_count })),
            },
        );
    }

    let mut sync_store = load_sync_store()?;
    sync_store.status = status;
    sync_store = save_sync_store(sync_store)?;
    api_ok(
        sync_store.status,
        news_store.revision.max(sync_store.revision),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(id: &str, name: &str) -> config::AiNewsRssSource {
        config::AiNewsRssSource {
            id: id.to_string(),
            name: name.to_string(),
            url: "https://example.com/feed.xml".to_string(),
            enabled: true,
        }
    }

    #[test]
    fn parses_36kr_style_rss_date() {
        let xml = r#"
            <rss><channel>
              <item>
                <title>AI product launch</title>
                <link>https://www.36kr.com/p/1</link>
                <description>OpenAI related news</description>
                <pubDate>2026-06-10 08:08:16 +0800</pubDate>
                <guid>36kr-1</guid>
                <category>AI</category>
              </item>
            </channel></rss>
        "#;

        let entries = parse_rss_entries(xml).expect("rss should parse");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].description, "OpenAI related news");
        assert_eq!(parse_published_ts(&entries[0].pub_date), 1_781_050_096);
    }

    #[test]
    fn parses_rss_text_entities() {
        let xml = r#"
            <rss><channel>
              <item>
                <title>OpenAI&amp;Anthropic</title>
                <link>https://example.com/1</link>
                <description><![CDATA[AI <strong>summary</strong>]]></description>
              </item>
            </channel></rss>
        "#;

        let entries = parse_rss_entries(xml).expect("rss should parse");
        assert_eq!(entries[0].title, "OpenAI&Anthropic");
        assert_eq!(entries[0].description, "AI <strong>summary</strong>");
    }

    #[test]
    fn parses_oschina_rfc2822_rss_date() {
        let xml = r#"
            <rss><channel>
              <item>
                <title>开源模型更新</title>
                <link>https://www.oschina.net/news/1</link>
                <description>大模型发布</description>
                <pubDate>Wed, 10 Jun 2026 08:08:16 +0800</pubDate>
                <guid>oschina-1</guid>
              </item>
            </channel></rss>
        "#;

        let entries = parse_rss_entries(xml).expect("rss should parse");
        assert_eq!(entries.len(), 1);
        assert_eq!(parse_published_ts(&entries[0].pub_date), 1_781_050_096);
    }

    #[test]
    fn rss_keyword_filter_matches_title_description_or_source() {
        let title_match = RssEntry {
            title: "OpenAI releases a model".to_string(),
            ..RssEntry::default()
        };
        let description_match = RssEntry {
            description: "A new Gemini feature shipped".to_string(),
            ..RssEntry::default()
        };
        let source_match = RssEntry::default();
        let no_match = RssEntry {
            title: "Cloud storage update".to_string(),
            description: "No model news here".to_string(),
            ..RssEntry::default()
        };
        let keywords = split_news_keywords(Some("openai; Gemini\n开源中国"));

        assert!(rss_entry_matches_keywords(&title_match, "36Kr", &keywords));
        assert!(rss_entry_matches_keywords(
            &description_match,
            "36Kr",
            &keywords
        ));
        assert!(rss_entry_matches_keywords(
            &source_match,
            "开源中国",
            &keywords
        ));
        assert!(!rss_entry_matches_keywords(&no_match, "36Kr", &keywords));
    }

    #[test]
    fn rss_keyword_filter_matches_ai_as_a_word() {
        let entry = RssEntry {
            title: "美团 AI 浏览器 Tabbit 1.0 上线".to_string(),
            ..RssEntry::default()
        };

        assert!(rss_entry_matches_keywords(
            &entry,
            "开源中国",
            &["ai".to_string()]
        ));
    }

    #[test]
    fn rss_keyword_filter_does_not_match_ai_inside_plain_word() {
        let entry = RssEntry {
            title: "Paid plans updated".to_string(),
            ..RssEntry::default()
        };

        assert!(!rss_entry_matches_keywords(
            &entry,
            "36Kr",
            &["ai".to_string()]
        ));
    }

    #[test]
    fn rss_entry_to_item_uses_source_id_as_provider() {
        let entry = RssEntry {
            title: "AI news".to_string(),
            link: "https://example.com/news".to_string(),
            description: "A summary".to_string(),
            pub_date: "Wed, 10 Jun 2026 08:08:16 +0800".to_string(),
            guid: "guid".to_string(),
            categories: vec![],
        };

        let item = rss_entry_to_item(&source("oschina", "开源中国"), entry, 123)
            .expect("entry should become item");
        assert_eq!(item.provider, "oschina");
        assert_eq!(item.source, "开源中国");
        assert_eq!(item.published_at, 1_781_050_096);
    }
}
