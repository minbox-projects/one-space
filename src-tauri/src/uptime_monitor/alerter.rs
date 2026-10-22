use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use serde_json::json;

use super::types::MonitorTarget;
use super::state_machine::StateTransition;

/// 同一监控项在同一种状态下的最小告警静默保护间隔（秒），例如 5 分钟（300 秒）
pub const ALERT_COOLDOWN_SECONDS: i64 = 300;

static ALERT_THROTTLE: OnceLock<Mutex<HashMap<String, i64>>> = OnceLock::new();

pub fn can_dispatch_alert(monitor_id: &str, status_kind: &str, now: i64) -> bool {
    let key = format!("{}:{}", monitor_id, status_kind);
    let mut throttle = ALERT_THROTTLE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap();
    if let Some(&last_time) = throttle.get(&key) {
        if now.saturating_sub(last_time) < ALERT_COOLDOWN_SECONDS {
            return false;
        }
    }
    throttle.insert(key, now);
    true
}

#[cfg(test)]
pub fn reset_alert_throttle() {
    if let Some(throttle) = ALERT_THROTTLE.get() {
        throttle.lock().unwrap().clear();
    }
}

pub async fn dispatch_transition_alert(
    target: &MonitorTarget,
    transition: &StateTransition,
    duration_sec: Option<i64>,
) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    match transition {
        StateTransition::BecameDown { cause, .. } => {
            if !can_dispatch_alert(&target.id, "down", now) {
                log::warn!(
                    "[Uptime Alert Suppressed] Down alert for monitor {} is within cooldown period",
                    target.name
                );
                return;
            }
            dispatch_alert(
                target,
                &format!("服务异常: {}", target.name),
                &format!("目标 [{}] 发生故障: {}", target.target, cause),
                "down",
            )
            .await;
        }
        StateTransition::BecameUp { .. } => {
            if !can_dispatch_alert(&target.id, "up", now) {
                log::warn!(
                    "[Uptime Alert Suppressed] Up alert for monitor {} is within cooldown period",
                    target.name
                );
                return;
            }
            let duration_text = if let Some(dur) = duration_sec {
                if dur > 60 {
                    format!(" (故障持续 {} 分 {} 秒)", dur / 60, dur % 60)
                } else {
                    format!(" (故障持续 {} 秒)", dur)
                }
            } else {
                String::new()
            };

            dispatch_alert(
                target,
                &format!("服务恢复: {}", target.name),
                &format!("目标 [{}] 已恢复正常{}", target.target, duration_text),
                "up",
            )
            .await;
        }
    }
}

async fn dispatch_alert(
    target: &MonitorTarget,
    title: &str,
    body: &str,
    status_kind: &str,
) {
    let alert_cfg = &target.alert_config;

    // 1. 系统通知与消息中心
    if alert_cfg.system_notification {
        log::info!("[Uptime Alert] {} - {}", title, body);
        let input = crate::messages::MessageCreateInput {
            source: "uptime_monitor".to_string(),
            category: "system".to_string(),
            severity: match status_kind {
                "down" => "error",
                "up" => "success",
                _ => "info",
            }.to_string(),
            title: title.to_string(),
            summary: Some(body.to_string()),
            detail: Some(format!(
                "监控项: {}\n目标地址: {}\n探针协议: {:?}\n当前状态: {}\n事件时间: {}",
                target.name,
                target.target,
                target.probe_type,
                if status_kind == "down" { "故障异常 (Down)" } else { "已恢复正常 (Up)" },
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
            )),
            dedupe_key: Some(format!("uptime_{}_{}", target.id, status_kind)),
            target: Some(crate::messages::MessageTarget {
                tab: "more-tools".to_string(),
                section: Some("uptime-monitor".to_string()),
                entity_id: Some(target.id.clone()),
            }),
            metadata: None,
        };

        if let Some(ref app) = super::scheduler::captured_app_handle() {
            let _ = crate::messages::create_message_with_app(app, input);
            use tauri::Emitter;
            let _ = app.emit(
                "uptime-monitor-alert",
                serde_json::json!({
                    "monitor_id": target.id,
                    "name": target.name,
                    "target": target.target,
                    "status_kind": status_kind,
                    "title": title,
                    "body": body,
                }),
            );
        } else {
            let _ = crate::messages::create_message_headless(input);
        }
    }

    // 2. 外部 Webhook 发送
    if let Some(ref webhook_url) = alert_cfg.webhook_url {
        if !webhook_url.trim().is_empty() {
            send_webhook(webhook_url, title, body, status_kind, target).await;
        }
    }
}

async fn send_webhook(
    url: &str,
    title: &str,
    body: &str,
    status_kind: &str,
    target: &MonitorTarget,
) {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(_) => return,
    };

    let payload = if url.contains("open.feishu.cn") {
        json!({
            "msg_type": "post",
            "content": {
                "post": {
                    "zh_cn": {
                        "title": title,
                        "content": [
                            [
                                {
                                    "tag": "text",
                                    "text": format!("{}\n监控类型: {:?}\n目标: {}", body, target.probe_type, target.target)
                                }
                            ]
                        ]
                    }
                }
            }
        })
    } else if url.contains("dingtalk.com") {
        json!({
            "msgtype": "markdown",
            "markdown": {
                "title": title,
                "text": format!("### {}\n\n{}\n\n- **目标**: `{}`\n- **时间**: 当前系统时间", title, body, target.target)
            }
        })
    } else {
        json!({
            "event": if status_kind == "down" { "service_down" } else { "service_up" },
            "monitorId": target.id,
            "monitorName": target.name,
            "target": target.target,
            "probeType": format!("{:?}", target.probe_type).to_lowercase(),
            "title": title,
            "message": body,
            "timestamp": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        })
    };

    let _ = client.post(url).json(&payload).send().await;
}
