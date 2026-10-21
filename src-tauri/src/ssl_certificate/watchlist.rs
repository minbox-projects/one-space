use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use uuid::Uuid;

use super::inspector::inspect_ssl;
use super::types::{SslStatus, SslWatchItem, SslWatchItemInput};

static WATCHLIST_LOCK: Mutex<()> = Mutex::new(());

fn get_watchlist_path() -> Result<PathBuf, String> {
    let data_dir = crate::get_data_dir()?;
    let dir = data_dir.join("data");
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create data dir: {}", e))?;
    Ok(dir.join("ssl_watchlist.json"))
}

pub fn load_watchlist() -> Result<Vec<SslWatchItem>, String> {
    let _guard = WATCHLIST_LOCK.lock().map_err(|e| e.to_string())?;
    let path = get_watchlist_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(path).map_err(|e| format!("Failed to read watchlist: {}", e))?;
    if content.trim().is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(&content).map_err(|e| format!("Failed to parse watchlist json: {}", e))
}

pub fn save_watchlist(items: &[SslWatchItem]) -> Result<(), String> {
    let _guard = WATCHLIST_LOCK.lock().map_err(|e| e.to_string())?;
    let path = get_watchlist_path()?;
    let json_bytes = serde_json::to_vec_pretty(items)
        .map_err(|e| format!("Failed to serialize watchlist: {}", e))?;

    let tmp_path = path.with_extension("tmp");
    fs::write(&tmp_path, json_bytes)
        .map_err(|e| format!("Failed to write tmp watchlist file: {}", e))?;
    fs::rename(&tmp_path, &path)
        .map_err(|e| format!("Failed to rename watchlist file: {}", e))?;
    Ok(())
}

pub fn add_watch_item(input: SslWatchItemInput) -> Result<SslWatchItem, String> {
    let mut items = load_watchlist()?;
    let clean_domain = input.domain.trim().to_lowercase();
    if clean_domain.is_empty() {
        return Err("Domain cannot be empty".to_string());
    }

    if let Some(existing) = items.iter_mut().find(|it| it.domain == clean_domain && it.port == input.port) {
        if let Some(alias) = input.alias {
            existing.alias = Some(alias);
        }
        existing.alert_days_threshold = input.alert_days_threshold;
        existing.enabled = true;
        let updated = existing.clone();
        save_watchlist(&items)?;
        return Ok(updated);
    }

    let now_ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let new_item = SslWatchItem {
        id: Uuid::new_v4().to_string(),
        domain: clean_domain,
        port: input.port,
        alias: input.alias,
        enabled: true,
        alert_days_threshold: input.alert_days_threshold,
        last_check_at: None,
        last_days_remaining: None,
        last_status: None,
        last_error: None,
        issuer_cn: None,
        created_at: now_ts,
    };

    items.push(new_item.clone());
    save_watchlist(&items)?;
    Ok(new_item)
}

pub fn remove_watch_item(id: &str) -> Result<(), String> {
    let mut items = load_watchlist()?;
    items.retain(|it| it.id != id);
    save_watchlist(&items)
}

pub async fn check_watch_item(item: &mut SslWatchItem) {
    let now_ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    item.last_check_at = Some(now_ts);

    match inspect_ssl(&item.domain, item.port).await {
        Ok(res) => {
            item.last_days_remaining = Some(res.days_remaining);
            item.issuer_cn = res.certificates.first().and_then(|c| c.issuer_cn.clone());
            item.last_error = res.validation_error.clone();

            let status = if res.is_expired {
                SslStatus::Expired
            } else if res.days_remaining <= item.alert_days_threshold as i64 {
                SslStatus::ExpiringSoon
            } else if !res.is_valid {
                SslStatus::Error
            } else {
                SslStatus::Valid
            };
            item.last_status = Some(status);

            // Trigger notification if expiring soon or expired
            if item.enabled && (res.is_expired || res.days_remaining <= item.alert_days_threshold as i64) {
                notify_expiration(item, res.days_remaining, res.is_expired);
            }
        }
        Err(e) => {
            item.last_status = Some(SslStatus::Error);
            item.last_error = Some(e);
        }
    }
}

pub async fn check_all_watchlist() -> Result<Vec<SslWatchItem>, String> {
    let mut items = load_watchlist()?;
    for item in &mut items {
        check_watch_item(item).await;
    }
    save_watchlist(&items)?;
    Ok(items)
}

fn notify_expiration(item: &SslWatchItem, days_remaining: i64, is_expired: bool) {
    let severity = if is_expired || days_remaining <= 7 {
        "error"
    } else {
        "warning"
    };

    let title = if is_expired {
        format!("SSL 证书已过期: {}", item.domain)
    } else {
        format!("SSL 证书即将到期: {}", item.domain)
    };

    let summary = if is_expired {
        format!(
            "域名 {}:{} 的 SSL 证书已过期，请立即续签并部署！",
            item.domain, item.port
        )
    } else {
        format!(
            "域名 {}:{} 的 SSL 证书仅剩 {} 天到期，请及时续签。",
            item.domain, item.port, days_remaining
        )
    };

    let dedupe_key = format!(
        "ssl_expire:{}:{}:{}",
        item.domain,
        item.port,
        if is_expired { 0 } else { days_remaining / 3 }
    );

    let input = crate::messages::MessageCreateInput {
        source: "ssl_certificate".to_string(),
        category: "ssl_certificate".to_string(),
        severity: severity.to_string(),
        title,
        summary: Some(summary),
        detail: None,
        dedupe_key: Some(dedupe_key),
        target: Some(crate::messages::MessageTarget {
            tab: "more-tools".to_string(),
            section: Some("ssl-certificate".to_string()),
            entity_id: Some(item.id.clone()),
        }),
        metadata: None,
    };

    let _ = crate::messages::create_message_headless(input);
}
