use crate::{config, secrets};
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const GNEWS_KEY_SECRET: &str = "onespace_ai_news_gnews_apikey";
const NEWSAPI_KEY_SECRET: &str = "onespace_ai_news_newsapi_apikey";
const NEWS_QUERY: &str =
    "\"artificial intelligence\" OR \"generative AI\" OR LLM OR \"large language model\" OR OpenAI OR Anthropic OR Gemini";

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

#[derive(Debug, Deserialize)]
struct GNewsResponse {
    #[serde(default)]
    articles: Vec<GNewsArticle>,
}

#[derive(Debug, Deserialize)]
struct GNewsArticle {
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    url: String,
    #[serde(rename = "publishedAt")]
    #[serde(default)]
    published_at: String,
    #[serde(default)]
    source: Option<GNewsSource>,
}

#[derive(Debug, Deserialize)]
struct GNewsSource {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Deserialize)]
struct NewsApiResponse {
    #[serde(default)]
    articles: Vec<NewsApiArticle>,
}

#[derive(Debug, Deserialize)]
struct NewsApiArticle {
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    url: String,
    #[serde(rename = "publishedAt")]
    #[serde(default)]
    published_at: String,
    #[serde(default)]
    source: Option<NewsApiSource>,
}

#[derive(Debug, Deserialize)]
struct NewsApiSource {
    #[serde(default)]
    name: String,
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

fn read_json_or_default<T: for<'de> Deserialize<'de> + Default>(path: &PathBuf) -> Result<T, String> {
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
    fs::write(path, content).map_err(|e| e.to_string())
}

fn load_news_store() -> Result<AiNewsStore, String> {
    read_json_or_default(&ai_news_local_path()?)
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

fn parse_published_ts(input: &str) -> u64 {
    DateTime::parse_from_rfc3339(input)
        .map(|dt| dt.with_timezone(&Utc).timestamp().max(0) as u64)
        .unwrap_or_else(|_| now_ts())
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

async fn fetch_gnews(client: &Client, api_key: &str, max_results: usize) -> Result<Vec<AiNewsItem>, String> {
    let response = client
        .get("https://gnews.io/api/v4/search")
        .query(&[
            ("q", NEWS_QUERY),
            ("lang", "en"),
            ("max", &max_results.min(100).to_string()),
            ("sortby", "publishedAt"),
            ("apikey", api_key),
        ])
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(format!("GNews HTTP {}: {}", status.as_u16(), text));
    }

    let payload: GNewsResponse = response.json().await.map_err(|e| e.to_string())?;
    let fetched_at = now_ts();
    let mut out = Vec::new();
    for article in payload.articles {
        if article.title.trim().is_empty() || article.url.trim().is_empty() {
            continue;
        }
        let published_at = parse_published_ts(&article.published_at);
        let source = article.source.map(|s| s.name).unwrap_or_default();
        out.push(AiNewsItem {
            id: make_item_id("gnews", &article.url, &article.title, published_at),
            provider: "gnews".to_string(),
            title: article.title.trim().to_string(),
            description: article.description.trim().to_string(),
            url: article.url.trim().to_string(),
            source: source.trim().to_string(),
            language: "en".to_string(),
            published_at,
            fetched_at,
            rank: 0.0,
            is_new: false,
        });
    }
    Ok(out)
}

async fn fetch_newsapi(
    client: &Client,
    api_key: &str,
    max_results: usize,
) -> Result<Vec<AiNewsItem>, String> {
    let response = client
        .get("https://newsapi.org/v2/everything")
        .query(&[
            ("q", NEWS_QUERY),
            ("language", "en"),
            ("sortBy", "publishedAt"),
            ("pageSize", &max_results.min(100).to_string()),
            ("apiKey", api_key),
        ])
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(format!("NewsAPI HTTP {}: {}", status.as_u16(), text));
    }

    let payload: NewsApiResponse = response.json().await.map_err(|e| e.to_string())?;
    let fetched_at = now_ts();
    let mut out = Vec::new();
    for article in payload.articles {
        if article.title.trim().is_empty() || article.url.trim().is_empty() {
            continue;
        }
        let published_at = parse_published_ts(&article.published_at);
        let source = article.source.map(|s| s.name).unwrap_or_default();
        out.push(AiNewsItem {
            id: make_item_id("newsapi", &article.url, &article.title, published_at),
            provider: "newsapi".to_string(),
            title: article.title.trim().to_string(),
            description: article.description.trim().to_string(),
            url: article.url.trim().to_string(),
            source: source.trim().to_string(),
            language: "en".to_string(),
            published_at,
            fetched_at,
            rank: 0.0,
            is_new: false,
        });
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
    let max_results = 20usize;
    let client = build_client()?;

    let mut provider_states: Vec<AiNewsProviderSyncState> = Vec::new();
    let mut fetched: Vec<AiNewsItem> = Vec::new();

    let gnews_key = secrets::get_secret(GNEWS_KEY_SECRET)?;
    if let Some(key) = gnews_key.filter(|v| !v.trim().is_empty()) {
        match fetch_gnews(&client, key.trim(), max_results).await {
            Ok(items) => {
                provider_states.push(AiNewsProviderSyncState {
                    provider: "gnews".to_string(),
                    status: "done".to_string(),
                    fetched_count: items.len(),
                    added_count: 0,
                    last_error: None,
                });
                fetched.extend(items);
            }
            Err(err) => {
                provider_states.push(AiNewsProviderSyncState {
                    provider: "gnews".to_string(),
                    status: "error".to_string(),
                    fetched_count: 0,
                    added_count: 0,
                    last_error: Some(err),
                });
            }
        }
    } else {
        provider_states.push(AiNewsProviderSyncState {
            provider: "gnews".to_string(),
            status: "skipped_no_key".to_string(),
            fetched_count: 0,
            added_count: 0,
            last_error: None,
        });
    }

    let newsapi_key = secrets::get_secret(NEWSAPI_KEY_SECRET)?;
    if let Some(key) = newsapi_key.filter(|v| !v.trim().is_empty()) {
        match fetch_newsapi(&client, key.trim(), max_results).await {
            Ok(items) => {
                provider_states.push(AiNewsProviderSyncState {
                    provider: "newsapi".to_string(),
                    status: "done".to_string(),
                    fetched_count: items.len(),
                    added_count: 0,
                    last_error: None,
                });
                fetched.extend(items);
            }
            Err(err) => {
                provider_states.push(AiNewsProviderSyncState {
                    provider: "newsapi".to_string(),
                    status: "error".to_string(),
                    fetched_count: 0,
                    added_count: 0,
                    last_error: Some(err),
                });
            }
        }
    } else {
        provider_states.push(AiNewsProviderSyncState {
            provider: "newsapi".to_string(),
            status: "skipped_no_key".to_string(),
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
        if let Err(err) = crate::app_store::sync_run_now(app).await {
            status.status = "error".to_string();
            status.last_error = Some(format!(
                "news-added sync failed: {} ({})",
                err.message, err.code
            ));
        }
    }

    let mut sync_store = load_sync_store()?;
    sync_store.status = status;
    sync_store = save_sync_store(sync_store)?;
    api_ok(sync_store.status, news_store.revision.max(sync_store.revision))
}
