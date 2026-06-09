use super::{
    error_summary, forward_request, generate_token, now_ts, prune_calls, read_config, read_stats,
    resolve_model, resolve_runtime_route, route_id_for_claude_provider, run_server, state_lock,
    status_from_config, summarize_calls, usage_from_value, validate_config, write_config,
    ProtocolRouterCallRecord, ProtocolRouterConfig, ProtocolRouterConnectionTestInput,
    ProtocolRouterStatsSummary, ProtocolRouterStatus, RunningServer, StatsQuery, UpstreamResult,
};
use serde_json::json;
use std::time::Instant;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

#[tauri::command]
pub fn protocol_router_get_config() -> Result<ProtocolRouterConfig, String> {
    read_config()
}

pub async fn protocol_router_autostart() -> Result<ProtocolRouterStatus, String> {
    let config = read_config()?;
    if config.enabled {
        protocol_router_start().await
    } else {
        Ok(status_from_config(&config, false))
    }
}

#[tauri::command]
pub async fn protocol_router_save_config(
    _app: tauri::AppHandle,
    config: ProtocolRouterConfig,
) -> Result<ProtocolRouterConfig, String> {
    validate_config(&config)?;
    write_config(&config)?;
    if config.enabled {
        protocol_router_start().await?;
    } else {
        protocol_router_stop().await?;
    }
    read_config()
}

#[tauri::command]
pub async fn protocol_router_start() -> Result<ProtocolRouterStatus, String> {
    let config = read_config()?;
    validate_config(&config)?;
    let already_running = {
        let guard = state_lock()
            .lock()
            .map_err(|_| "router state lock poisoned".to_string())?;
        guard.as_ref().map(|s| s.port) == Some(config.port)
    };
    if already_running {
        return Ok(status_from_config(&config, true));
    }
    let listener = TcpListener::bind(("127.0.0.1", config.port))
        .await
        .map_err(|e| format!("failed to bind protocol router port {}: {e}", config.port))?;
    let mut guard = state_lock()
        .lock()
        .map_err(|_| "router state lock poisoned".to_string())?;
    if let Some(mut running) = guard.take() {
        if let Some(tx) = running.shutdown.take() {
            let _ = tx.send(());
        }
    }
    let (tx, rx) = oneshot::channel();
    let port = config.port;
    tauri::async_runtime::spawn(run_server(listener, rx));
    *guard = Some(RunningServer {
        port,
        shutdown: Some(tx),
    });
    Ok(status_from_config(&config, true))
}

#[tauri::command]
pub async fn protocol_router_stop() -> Result<ProtocolRouterStatus, String> {
    let config = read_config()?;
    let mut guard = state_lock()
        .lock()
        .map_err(|_| "router state lock poisoned".to_string())?;
    if let Some(mut running) = guard.take() {
        if let Some(tx) = running.shutdown.take() {
            let _ = tx.send(());
        }
    }
    Ok(status_from_config(&config, false))
}

#[tauri::command]
pub fn protocol_router_status() -> Result<ProtocolRouterStatus, String> {
    let config = read_config()?;
    let running = state_lock().lock().map(|g| g.is_some()).unwrap_or(false);
    Ok(status_from_config(&config, running))
}

#[tauri::command]
pub fn protocol_router_rotate_token() -> Result<ProtocolRouterConfig, String> {
    let mut config = read_config()?;
    config.token = generate_token();
    write_config(&config)?;
    read_config()
}

#[tauri::command]
pub async fn protocol_router_test_connection(
    input: ProtocolRouterConnectionTestInput,
) -> Result<ProtocolRouterCallRecord, String> {
    let route_id = if !input.claude_provider_id.trim().is_empty() {
        route_id_for_claude_provider(&input.claude_provider_id)
    } else {
        input.route_id.clone()
    };
    let route = resolve_runtime_route(&route_id)?;
    let requested_model = input
        .model
        .as_deref()
        .filter(|model| !model.trim().is_empty())
        .or(route.default_model.as_deref());
    let model = resolve_model(&route, requested_model);
    if model.trim().is_empty() {
        return Err("route model is required".to_string());
    }
    let started = Instant::now();
    let body = json!({
        "model": model,
        "messages": [{ "role": "user", "content": "ping" }],
        "max_tokens": 8
    });
    let result = forward_request(&route, &body, &model).await;
    let latency_ms = started.elapsed().as_millis();
    match result {
        Ok(UpstreamResult::Json { status, body }) => {
            let (input_tokens, output_tokens, total_tokens) = usage_from_value(&body);
            Ok(ProtocolRouterCallRecord {
                ts: now_ts(),
                route_id: route.id,
                provider: route.upstream_provider_name,
                model,
                endpoint: "/v1/messages".to_string(),
                wire_api: route.wire_api,
                status,
                latency_ms,
                input_tokens,
                output_tokens,
                total_tokens,
                error_summary: if status >= 400 {
                    Some(error_summary(&body))
                } else {
                    None
                },
            })
        }
        Ok(UpstreamResult::Stream { .. }) => {
            Err("test connection does not use streaming".to_string())
        }
        Err(error) => Err(error),
    }
}

#[tauri::command]
pub fn protocol_router_stats(
    query: Option<StatsQuery>,
) -> Result<ProtocolRouterStatsSummary, String> {
    let config = read_config()?;
    let mut stats = read_stats()?;
    prune_calls(&mut stats.calls, config.retention_days);
    let days = query.and_then(|q| q.days).unwrap_or(config.retention_days);
    let cutoff = now_ts().saturating_sub(days * 24 * 60 * 60);
    let calls = stats
        .calls
        .into_iter()
        .filter(|call| call.ts >= cutoff)
        .collect::<Vec<_>>();
    Ok(summarize_calls(calls))
}
