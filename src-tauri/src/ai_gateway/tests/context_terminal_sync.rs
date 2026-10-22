//! Behavior coverage for carrying synchronized declared context windows into
//! the supported native terminal configuration.
//!
//! The tests drive the existing terminal-sync composition and the real
//! configuration storage; they do not mock private algorithms. A per-target
//! `context` JSON summary is the intended UI-facing contract:
//!
//! - `mode`: `per_model`, `selected_model` or `shared_compaction`.
//! - `scope`: `generated_models`, `activation` or
//!   `global_and_isolated_settings`.
//! - `coverage`: `{ total, known, unknown }` over the applicable public models
//!   or deduplicated Claude choices.
//! - `selected_model`, `raw_context` and `applied_context` where they apply.
//! - `reasons`: stable codes such as `unknown_context`, `below_minimum`,
//!   `capped_at_1000000` and `incomplete_selection`.
//!
//! Coverage traceability: REQ-004/REQ-005, AC-005 through AC-008 and the
//! terminal continuation of AC-003. No live provider request or user
//! configuration is touched.

use super::{
    gateway_config, isolated_temp_home, mapping, target_for, upstream_provider,
};
use crate::ai_gateway::commands::{
    apply_provider_save_with_terminal_refresh, apply_template_save_with_terminal_refresh,
    apply_terminal_sync_with_selection, apply_template_sync_with_terminal_refresh,
    terminal_targets_from, TerminalSyncProfile, UpsertFuture,
};
use crate::ai_gateway::storage::{local_base_url, read_config, write_config};
use crate::ai_gateway::{
    ClaudeModelSelection, GatewayConfig, ModelMapping, ModelPool,
    ModelPoolCandidate, ProviderTemplate, ProviderTemplateModel, TerminalSyncRecord,
    UpstreamProtocol,
};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

const PORT: u16 = 19000;

fn context_mapping(
    local_model: &str,
    upstream_model: &str,
    display_name: Option<&str>,
    context_window: Option<u32>,
) -> ModelMapping {
    let mut row = mapping(local_model, upstream_model, display_name);
    row.context_window = context_window;
    row
}

fn pool_candidate(
    provider_id: &str,
    local_model: &str,
    upstream_model: &str,
    protocol: UpstreamProtocol,
) -> ModelPoolCandidate {
    ModelPoolCandidate {
        provider_id: provider_id.to_string(),
        local_model: local_model.to_string(),
        upstream_model: upstream_model.to_string(),
        protocol,
        weight: 1,
        ..ModelPoolCandidate::default()
    }
}

fn enabled_pool() -> ModelPool {
    ModelPool {
        id: "pool-free".to_string(),
        name: "Free Model".to_string(),
        model: "free-model".to_string(),
        enabled: true,
        candidates: vec![pool_candidate(
            "g1",
            "gpt-4o",
            "gpt-4o-2024",
            UpstreamProtocol::ChatCompletions,
        )],
        ..ModelPool::default()
    }
}

async fn capture_with_selection(
    providers_data: &Value,
    tools: Vec<String>,
    profile: TerminalSyncProfile,
    selection: Option<ClaudeModelSelection>,
) -> Vec<Value> {
    let captured: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = captured.clone();
    apply_terminal_sync_with_selection(
        providers_data,
        move |value| -> UpsertFuture {
            let sink = sink.clone();
            Box::pin(async move {
                sink.lock().expect("capture lock").push(value);
                Ok(())
            })
        },
        tools,
        profile,
        selection,
    )
    .await
    .expect("terminal sync must succeed");
    let payloads = captured.lock().unwrap().clone();
    payloads
}

fn payload_for<'a>(payloads: &'a [Value], tool: &str) -> &'a Value {
    payloads
        .iter()
        .find(|value| value["tool"] == tool)
        .unwrap_or_else(|| panic!("missing {tool} payload: {payloads:?}"))
}

fn opencode_model<'a>(payload: &'a Value, local_model: &str) -> &'a Value {
    &payload["tool_config"]["models"][local_model]
}

fn context_summary(target: &crate::ai_gateway::TerminalTarget) -> Value {
    let serialized = serde_json::to_value(target).expect("serialize terminal target");
    serialized
        .get("context")
        .cloned()
        .unwrap_or_else(|| panic!("terminal target has no observable context summary: {serialized}"))
}

fn stored_opencode(config: &GatewayConfig) -> Value {
    json!({
        "id": "gw-open",
        "tool": "opencode",
        "name": "AI Gateway",
        "base_url": local_base_url(config.port),
        "api_key": "previous-local-key",
        "provider_key": "gateway",
        "tool_config": {
            "ai_gateway_gateway": true,
            "models": {}
        }
    })
}

fn opencode_ledger(config: &GatewayConfig) -> TerminalSyncRecord {
    TerminalSyncRecord {
        provider_id: "gw-open".to_string(),
        tool: "opencode".to_string(),
        synced_key_id: "k1".to_string(),
        synced_base_url: local_base_url(config.port),
        synced_at: 1,
    }
}

fn claude_mapping(family: &str, local_model: &str) -> Value {
    json!({
        "family": family,
        "display_name": family,
        "upstream_model": local_model,
    })
}

fn stored_claude_provider(selection: &ClaudeModelSelection, threshold: Option<u32>) -> Value {
    json!({
        "id": "gw-claude",
        "tool": "claude",
        "name": "AI Gateway",
        "base_url": "http://127.0.0.1:19000",
        "api_key": "previous-local-key",
        "model": selection.default_model,
        "claude_default_model": selection.default_model,
        "code": "gateway-claude-code",
        "claude_api_format": "anthropic_messages",
        "claude_connection_mode": "native_anthropic",
        "claude_auth_env_key": "ANTHROPIC_API_KEY",
        "claude_model_mappings": [
            claude_mapping("haiku", &selection.haiku),
            claude_mapping("sonnet", &selection.sonnet),
            claude_mapping("opus", &selection.opus),
        ],
        "tool_config": {
            "ai_gateway_gateway": true,
            "claude_auto_compact_window": threshold,
        }
    })
}

/// One native Anthropic-Messages provider serving the four Claude choices with
/// the supplied declared windows.
fn claude_config(windows: [Option<u32>; 4]) -> GatewayConfig {
    let mut config = gateway_config(PORT);
    let mut anthropic = upstream_provider(
        "g-claude",
        "Anthropic Fixture",
        "https://api.anthropic.example/v1",
        "sk-anthropic",
        None,
    );
    anthropic.protocol = UpstreamProtocol::AnthropicMessages;
    anthropic.mappings = vec![
        context_mapping("claude-default", "vendor/default", Some("Default"), windows[0]),
        context_mapping("claude-haiku", "vendor/haiku", Some("Haiku"), windows[1]),
        context_mapping("claude-sonnet", "vendor/sonnet", Some("Sonnet"), windows[2]),
        context_mapping("claude-opus", "vendor/opus", Some("Opus"), windows[3]),
    ];
    config.providers.push(anthropic);
    config
}

fn claude_selection() -> ClaudeModelSelection {
    ClaudeModelSelection {
        default_model: "claude-default".to_string(),
        haiku: "claude-haiku".to_string(),
        sonnet: "claude-sonnet".to_string(),
        opus: "claude-opus".to_string(),
    }
}

async fn claude_payload(config: GatewayConfig) -> Value {
    write_config(&config).expect("write gateway config");
    let payloads = capture_with_selection(
        &json!({ "providers": [] }),
        vec!["claude".to_string()],
        TerminalSyncProfile::RELEASE,
        Some(claude_selection()),
    )
    .await;
    payload_for(&payloads, "claude").clone()
}

// ---------------------------------------------------------------------------
// OpenCode: per-model limits, output zero, omission and preserved metadata
// ---------------------------------------------------------------------------

#[tokio::test]
async fn opencode_known_ordinary_and_pool_models_render_numeric_limits() {
    let _home = isolated_temp_home("ctx-opencode-known");
    let mut config = gateway_config(PORT);
    config.providers[0].mappings = vec![
        context_mapping("gpt-4o", "gpt-4o-2024", Some("GPT-4o"), Some(128000)),
        context_mapping("ds", "deepseek-chat", None, None),
    ];
    config.model_pools.push(enabled_pool());
    write_config(&config).expect("write gateway config");

    let payloads = capture_with_selection(
        &json!({ "providers": [] }),
        vec!["opencode".to_string()],
        TerminalSyncProfile::RELEASE,
        None,
    )
    .await;
    let opencode = payload_for(&payloads, "opencode");

    let ordinary = opencode_model(opencode, "gpt-4o");
    assert_eq!(
        ordinary["limit"]["context"], 128000,
        "a known ordinary model must carry its numeric context window: {opencode}"
    );
    assert_eq!(
        ordinary["limit"]["output"], 0,
        "zero keeps the native unspecified-output behavior: {opencode}"
    );

    let pool = opencode_model(opencode, "free-model");
    assert_eq!(
        pool["limit"]["context"], 128000,
        "a known pool model must carry the conservative numeric window: {opencode}"
    );
    assert_eq!(pool["limit"]["output"], 0, "pool output stays zero: {opencode}");

    assert!(
        opencode_model(opencode, "ds").get("limit").is_none(),
        "an unknown context must omit the whole limit object: {opencode}"
    );
}

#[tokio::test]
async fn opencode_cleared_context_omits_the_limit_on_the_next_sync() {
    let _home = isolated_temp_home("ctx-opencode-cleared");
    let mut config = gateway_config(PORT);
    config.providers[0].mappings =
        vec![context_mapping("gpt-4o", "gpt-4o-2024", Some("GPT-4o"), Some(128000))];
    write_config(&config).expect("write gateway config");

    let first = capture_with_selection(
        &json!({ "providers": [] }),
        vec!["opencode".to_string()],
        TerminalSyncProfile::RELEASE,
        None,
    )
    .await;
    assert_eq!(
        payload_for(&first, "opencode")["tool_config"]["models"]["gpt-4o"]["limit"]["context"],
        128000
    );

    config.providers[0].mappings[0].context_window = None;
    write_config(&config).expect("write cleared config");
    let second = capture_with_selection(
        &json!({ "providers": [] }),
        vec!["opencode".to_string()],
        TerminalSyncProfile::RELEASE,
        None,
    )
    .await;
    assert!(
        payload_for(&second, "opencode")["tool_config"]["models"]["gpt-4o"]
            .get("limit")
            .is_none(),
        "clearing the declared value must omit the limit rather than retain it: {second:?}"
    );
}

#[tokio::test]
async fn opencode_limits_preserve_protocol_npm_reasoning_and_profile_identity() {
    let _home = isolated_temp_home("ctx-opencode-preserve");
    let mut config = gateway_config(PORT);
    config.providers[0].mappings = vec![
        {
            let mut row = context_mapping("gpt-4o", "gpt-4o-2024", Some("GPT-4o"), Some(128000));
            row.reasoning_efforts = vec!["low".to_string(), "high".to_string()];
            row
        },
        context_mapping("ds", "deepseek-chat", None, None),
    ];
    config.model_pools.push(enabled_pool());
    let mut responses = upstream_provider(
        "g-resp",
        "Responses Fixture",
        "https://responses.example/v1",
        "sk-responses",
        None,
    );
    let mut responses_row = context_mapping(
        "resp-only",
        "vendor/responses",
        Some("Responses Only"),
        Some(64000),
    );
    responses_row.protocol = Some(UpstreamProtocol::Responses);
    responses.mappings = vec![responses_row];
    config.providers.push(responses);
    write_config(&config).expect("write gateway config");

    let release = capture_with_selection(
        &json!({ "providers": [] }),
        vec!["opencode".to_string()],
        TerminalSyncProfile::RELEASE,
        None,
    )
    .await;
    let opencode = payload_for(&release, "opencode");
    assert_eq!(opencode["provider_key"], "gateway");
    assert_eq!(
        opencode_model(opencode, "gpt-4o")["variants"]["low"]["reasoningEffort"],
        "low",
        "declared reasoning variants must survive limit generation: {opencode}"
    );
    assert_eq!(
        opencode_model(opencode, "resp-only")["provider"]["npm"],
        "@ai-sdk/openai",
        "a Responses-only model keeps its npm override: {opencode}"
    );
    assert_eq!(
        opencode_model(opencode, "resp-only")["limit"]["context"], 64000,
        "the npm override and the context limit must coexist: {opencode}"
    );
    assert_eq!(
        opencode_model(opencode, "free-model")["interleaved"]["field"],
        "reasoning_content",
        "the pool row keeps its existing interleaved metadata: {opencode}"
    );

    let dev = capture_with_selection(
        &json!({ "providers": [] }),
        vec!["opencode".to_string()],
        TerminalSyncProfile::DEV,
        None,
    )
    .await;
    let dev_opencode = payload_for(&dev, "opencode");
    assert_eq!(dev_opencode["provider_key"], "gateway-dev");
    assert_eq!(dev_opencode["tool_config"]["ai_gateway_gateway"], "dev");
    assert_eq!(
        opencode_model(dev_opencode, "gpt-4o")["limit"]["context"],
        128000,
        "the dev profile carries the same declared value under its own identity: {dev:?}"
    );
}

// ---------------------------------------------------------------------------
// Codex: selected-model registration, explicit null, activation boundary
// ---------------------------------------------------------------------------

#[tokio::test]
async fn codex_sync_records_only_the_selected_models_window_without_activating() {
    let _home = isolated_temp_home("ctx-codex-selected");
    let mut config = gateway_config(PORT);
    config.providers[0].mappings = vec![
        context_mapping("gpt-4o", "gpt-4o-2024", Some("GPT-4o"), Some(256000)),
        context_mapping("ds", "deepseek-chat", None, Some(128000)),
    ];
    write_config(&config).expect("write gateway config");

    let payloads = capture_with_selection(
        &json!({ "providers": [] }),
        vec!["codex".to_string()],
        TerminalSyncProfile::RELEASE,
        None,
    )
    .await;
    let codex = payload_for(&payloads, "codex");

    assert_eq!(codex["model"], "gpt-4o");
    assert_eq!(
        codex["tool_config"]["model_context_window"], 256000,
        "the selected model's effective window is synchronized: {codex}"
    );
    assert!(
        codex.get("active").is_none() && codex.get("is_active").is_none(),
        "synchronization must not activate the Codex provider: {codex}"
    );
    assert!(
        codex["tool_config"].get("models").is_none(),
        "Codex must not receive a per-model catalog: {codex}"
    );
}

#[tokio::test]
async fn codex_unknown_selected_model_syncs_an_explicit_null() {
    let _home = isolated_temp_home("ctx-codex-unknown-null");
    let mut config = gateway_config(PORT);
    config.providers[0].mappings =
        vec![context_mapping("gpt-4o", "gpt-4o-2024", Some("GPT-4o"), None)];
    write_config(&config).expect("write gateway config");

    let payloads = capture_with_selection(
        &json!({ "providers": [] }),
        vec!["codex".to_string()],
        TerminalSyncProfile::RELEASE,
        None,
    )
    .await;
    let codex = payload_for(&payloads, "codex");
    assert!(
        codex["tool_config"]
            .get("model_context_window")
            .is_some(),
        "an unknown selected window must still write the managed key: {codex}"
    );
    assert_eq!(
        codex["tool_config"]["model_context_window"], Value::Null,
        "an unknown selected window must be recorded as an explicit null so a merged store can clear it: {codex}"
    );
}

// ---------------------------------------------------------------------------
// Claude: deduplicated choice minimum, thresholds, explicit clearing
// ---------------------------------------------------------------------------

#[tokio::test]
async fn claude_sync_uses_the_deduplicated_minimum_of_all_four_choices() {
    let _home = isolated_temp_home("ctx-claude-min");
    let config = claude_config([Some(128000), Some(128000), Some(256000), Some(200000)]);
    let claude = claude_payload(config).await;

    assert_eq!(
        claude["tool_config"]["claude_auto_compact_window"], 128000,
        "the shared value is the minimum of the deduplicated four choices: {claude}"
    );
}

#[tokio::test]
async fn claude_thresholds_follow_the_supported_boundaries_and_clear_unknown() {
    let _home = isolated_temp_home("ctx-claude-thresholds");
    for (window, expected) in [
        (Some(99999), Value::Null),
        (Some(100000), json!(100000)),
        (Some(1000000), json!(1000000)),
        (Some(1050000), json!(1000000)),
        (None, Value::Null),
    ] {
        let claude = claude_payload(claude_config([window; 4])).await;
        assert_eq!(
            claude["tool_config"]["claude_auto_compact_window"], expected,
            "a choice minimum of {window:?} must produce {expected}: {claude}"
        );
        assert!(
            claude["tool_config"]
                .get("claude_auto_compact_window")
                .is_some(),
            "the managed key must be present so explicit clearing can travel: {claude}"
        );
    }
}

// ---------------------------------------------------------------------------
// Per-target summaries and context-only drift
// ---------------------------------------------------------------------------

#[test]
fn opencode_target_reports_context_coverage_and_pending_drift_without_secrets() {
    let mut config = gateway_config(PORT);
    config.default_key_id = Some("k1".to_string());
    config.providers[0].mappings = vec![
        context_mapping("gpt-4o", "gpt-4o-2024", Some("GPT-4o"), Some(128000)),
        context_mapping("ds", "deepseek-chat", None, None),
    ];
    config.terminal_syncs.push(opencode_ledger(&config));

    let stale = json!({
        "id": "gw-open",
        "tool": "opencode",
        "name": "AI Gateway",
        "base_url": local_base_url(config.port),
        "api_key": "previous-local-key",
        "provider_key": "gateway",
        "tool_config": {
            "ai_gateway_gateway": true,
            "models": {
                "gpt-4o": { "name": "GPT-4o" },
                "ds": { "name": "deepseek-chat" }
            }
        }
    });
    let targets = terminal_targets_from(
        &config,
        &json!({ "providers": [stale] }),
        TerminalSyncProfile::RELEASE,
    );
    let opencode = target_for(&targets, "opencode");
    assert!(
        opencode.pending_sync,
        "a missing context limit must make the OpenCode target pending: {opencode:?}"
    );

    let summary = context_summary(opencode);
    assert_eq!(summary["mode"], "per_model");
    assert_eq!(summary["scope"], "generated_models");
    assert_eq!(summary["coverage"]["total"], 2);
    assert_eq!(summary["coverage"]["known"], 1);
    assert_eq!(summary["coverage"]["unknown"], 1);
    assert_eq!(summary["reasons"], json!(["unknown_context"]));

    let serialized = serde_json::to_string(opencode).expect("serialize target");
    assert!(
        !serialized.contains("previous-local-key"),
        "the target summary must never expose a credential: {serialized}"
    );

    let current = json!({
        "id": "gw-open",
        "tool": "opencode",
        "name": "AI Gateway",
        "base_url": local_base_url(config.port),
        "api_key": "previous-local-key",
        "provider_key": "gateway",
        "tool_config": {
            "ai_gateway_gateway": true,
            "models": {
                "gpt-4o": {
                    "name": "GPT-4o",
                    "limit": { "context": 128000, "output": 0 }
                },
                "ds": { "name": "deepseek-chat" }
            }
        }
    });
    let current_targets = terminal_targets_from(
        &config,
        &json!({ "providers": [current] }),
        TerminalSyncProfile::RELEASE,
    );
    assert!(
        !target_for(&current_targets, "opencode").pending_sync,
        "a matching generated limit must clear the context-only drift: {current_targets:?}"
    );
}

#[test]
fn codex_target_reports_selected_window_and_pending_drift() {
    let mut config = gateway_config(PORT);
    config.default_key_id = Some("k1".to_string());
    config.providers[0].mappings = vec![
        context_mapping("gpt-4o", "gpt-4o-2024", Some("GPT-4o"), Some(256000)),
        context_mapping("ds", "deepseek-chat", None, Some(128000)),
    ];
    config.terminal_syncs.push(TerminalSyncRecord {
        provider_id: "gw-codex".to_string(),
        tool: "codex".to_string(),
        synced_key_id: "k1".to_string(),
        synced_base_url: local_base_url(config.port),
        synced_at: 1,
    });
    let stored = |window: Value| {
        json!({
            "id": "gw-codex",
            "tool": "codex",
            "name": "AI Gateway",
            "base_url": local_base_url(config.port),
            "api_key": "previous-local-key",
            "model": "gpt-4o",
            "tool_config": {
                "ai_gateway_gateway": true,
                "model_context_window": window
            }
        })
    };

    let stale_targets = terminal_targets_from(
        &config,
        &json!({ "providers": [stored(json!(128000))] }),
        TerminalSyncProfile::RELEASE,
    );
    let codex = target_for(&stale_targets, "codex");
    assert!(
        codex.pending_sync,
        "a stale selected-model scalar must mark Codex pending: {codex:?}"
    );
    let summary = context_summary(codex);
    assert_eq!(summary["mode"], "selected_model");
    assert_eq!(summary["scope"], "activation");
    assert_eq!(summary["selected_model"], "gpt-4o");
    assert_eq!(summary["raw_context"], 256000);
    assert_eq!(summary["applied_context"], 256000);
    assert_eq!(summary["coverage"]["total"], 1);
    assert_eq!(summary["coverage"]["known"], 1);
    assert_eq!(summary["coverage"]["unknown"], 0);

    let current_targets = terminal_targets_from(
        &config,
        &json!({ "providers": [stored(json!(256000))] }),
        TerminalSyncProfile::RELEASE,
    );
    assert!(
        !target_for(&current_targets, "codex").pending_sync,
        "a matching selected-model scalar must clear Codex drift: {current_targets:?}"
    );
}

#[test]
fn codex_target_summary_reports_the_retained_enabled_pool_alias_on_resync() {
    let mut config = gateway_config(PORT);
    config.default_key_id = Some("k1".to_string());
    config.providers[0].mappings = vec![
        context_mapping("gpt-4o", "gpt-4o-2024", Some("GPT-4o"), Some(128000)),
        context_mapping(
            "pool-upstream",
            "pool-upstream-2024",
            Some("Pool Upstream"),
            Some(256000),
        ),
    ];
    config.model_pools.push(ModelPool {
        id: "pool-free".to_string(),
        name: "Free Model".to_string(),
        model: "free-model".to_string(),
        enabled: true,
        candidates: vec![pool_candidate(
            "g1",
            "pool-upstream",
            "pool-upstream-2024",
            UpstreamProtocol::ChatCompletions,
        )],
        ..ModelPool::default()
    });
    config.terminal_syncs.push(TerminalSyncRecord {
        provider_id: "gw-codex".to_string(),
        tool: "codex".to_string(),
        synced_key_id: "k1".to_string(),
        synced_base_url: local_base_url(config.port),
        synced_at: 1,
    });

    // The previously synchronized managed record selected an enabled pool
    // alias, so a resync retains it instead of falling back to the ordinary
    // default model.
    let stored = json!({
        "id": "gw-codex",
        "tool": "codex",
        "name": "AI Gateway",
        "base_url": local_base_url(config.port),
        "api_key": "previous-local-key",
        "model": "free-model",
        "tool_config": {
            "ai_gateway_gateway": true,
            "model_context_window": 256000
        }
    });
    let targets = terminal_targets_from(
        &config,
        &json!({ "providers": [stored] }),
        TerminalSyncProfile::RELEASE,
    );
    let summary = context_summary(target_for(&targets, "codex"));

    assert_eq!(
        summary["selected_model"], "free-model",
        "the summary must report the retained pool alias, not the ordinary default: {summary}"
    );
    assert_eq!(
        summary["raw_context"], 256000,
        "the retained alias must report its conservative pool context: {summary}"
    );
    assert_eq!(summary["applied_context"], 256000);
    assert_eq!(summary["coverage"]["total"], 1);
    assert_eq!(summary["coverage"]["known"], 1);
    assert_eq!(summary["coverage"]["unknown"], 0);
    assert_eq!(summary["reasons"], json!([]));
}

#[test]
fn claude_target_reports_shared_minimum_and_pending_drift() {
    let selection = claude_selection();
    let mut config = claude_config([Some(128000), Some(128000), Some(256000), Some(200000)]);
    config.default_key_id = Some("k1".to_string());
    config.terminal_syncs.push(TerminalSyncRecord {
        provider_id: "gw-claude".to_string(),
        tool: "claude".to_string(),
        synced_key_id: "k1".to_string(),
        synced_base_url: "http://127.0.0.1:19000".to_string(),
        synced_at: 1,
    });

    let stale_targets = terminal_targets_from(
        &config,
        &json!({ "providers": [stored_claude_provider(&selection, Some(256000))] }),
        TerminalSyncProfile::RELEASE,
    );
    let claude = target_for(&stale_targets, "claude");
    assert!(
        claude.pending_sync,
        "a stale shared compaction scalar must mark Claude pending: {claude:?}"
    );
    let summary = context_summary(claude);
    assert_eq!(summary["mode"], "shared_compaction");
    assert_eq!(summary["scope"], "global_and_isolated_settings");
    assert_eq!(summary["coverage"]["total"], 3, "duplicate choices count once");
    assert_eq!(summary["coverage"]["known"], 3);
    assert_eq!(summary["applied_context"], 128000);
    assert_eq!(summary["reasons"], json!([]));

    let current_targets = terminal_targets_from(
        &config,
        &json!({ "providers": [stored_claude_provider(&selection, Some(128000))] }),
        TerminalSyncProfile::RELEASE,
    );
    assert!(
        !target_for(&current_targets, "claude").pending_sync,
        "a matching shared scalar must clear Claude drift: {current_targets:?}"
    );
}

#[test]
fn claude_target_explains_below_minimum_and_unknown_choices() {
    let selection = claude_selection();
    for (windows, reason) in [
        ([Some(99999); 4], "below_minimum"),
        ([None, Some(128000), Some(128000), Some(128000)], "unknown_context"),
    ] {
        let mut config = claude_config(windows);
        config.terminal_syncs.push(TerminalSyncRecord {
            provider_id: "gw-claude".to_string(),
            tool: "claude".to_string(),
            synced_key_id: "k1".to_string(),
            synced_base_url: "http://127.0.0.1:19000".to_string(),
            synced_at: 1,
        });
        let targets = terminal_targets_from(
            &config,
            &json!({ "providers": [stored_claude_provider(&selection, None)] }),
            TerminalSyncProfile::RELEASE,
        );
        let summary = context_summary(target_for(&targets, "claude"));
        assert_eq!(summary["applied_context"], Value::Null);
        assert!(
            summary["reasons"]
                .as_array()
                .expect("reasons array")
                .iter()
                .any(|value| value == reason),
            "the summary must explain {reason}: {summary}"
        );
    }
}

// ---------------------------------------------------------------------------
// Injected remote/template refresh through the production terminal pipeline
// ---------------------------------------------------------------------------

fn bound_refresh_config() -> GatewayConfig {
    let mut config = gateway_config(PORT);
    config.providers[0].template_id = Some("commandcode".to_string());
    config.providers[0].mappings = vec![
        context_mapping("gpt-4o", "gpt-4o-2024", Some("GPT-4o"), Some(128000)),
        context_mapping("ds", "deepseek-chat", None, Some(128000)),
    ];
    config.terminal_syncs.push(opencode_ledger(&config));
    config
}

const REFRESH_BODY: &str = r#"{
  "data": [
    {
      "id": "gpt-4o-2024",
      "name": "GPT-4o",
      "context_length": 256000,
      "supported_endpoints": ["/chat/completions"]
    },
    {
      "id": "deepseek-chat",
      "name": "DeepSeek Chat",
      "context_length": 64000,
      "supported_endpoints": ["/chat/completions"]
    }
  ]
}"#;

#[tokio::test]
async fn template_refresh_carries_fetched_context_into_the_terminal_payload_once() {
    let _home = isolated_temp_home("ctx-template-refresh-chain");
    let mut config = bound_refresh_config();
    write_config(&config).expect("write initial config");
    let stored = stored_opencode(&config);
    let providers_data = json!({ "providers": [stored] });

    let calls = Arc::new(AtomicUsize::new(0));
    let captured: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
    let calls_for_sync = calls.clone();
    let captured_for_sync = captured.clone();
    let providers_for_sync = providers_data.clone();
    let view = apply_template_sync_with_terminal_refresh(
        &mut config,
        "commandcode",
        |_| Ok(REFRESH_BODY.to_string()),
        |next| write_config(next).map_err(|error| error.to_string()),
        &providers_data,
        move |tools| {
            calls_for_sync.fetch_add(1, Ordering::SeqCst);
            let providers = providers_for_sync.clone();
            let sink = captured_for_sync.clone();
            async move {
                apply_terminal_sync_with_selection(
                    &providers,
                    move |value| -> UpsertFuture {
                        let sink = sink.clone();
                        Box::pin(async move {
                            sink.lock().expect("capture lock").push(value);
                            Ok(())
                        })
                    },
                    tools,
                    TerminalSyncProfile::RELEASE,
                    None,
                )
                .await
            }
        },
    )
    .await
    .expect("the injected refresh must succeed");

    assert_eq!(calls.load(Ordering::SeqCst), 1, "exactly one refresh call");
    assert!(view.synced_at.is_some());
    assert_eq!(
        config.providers[0].mappings[0].context_window,
        Some(256000),
        "the fetched value must reach the bound mapping"
    );
    let payloads = captured.lock().unwrap().clone();
    let opencode = payload_for(&payloads, "opencode");
    assert_eq!(
        opencode_model(opencode, "gpt-4o")["limit"]["context"], 256000,
        "the refreshed value must reach the native terminal payload: {opencode}"
    );
}

#[tokio::test]
async fn failed_or_invalid_template_refresh_performs_no_terminal_work() {
    let _home = isolated_temp_home("ctx-template-refresh-failures");
    for (fetch, persist, label) in [
        (
            Box::new(|_: &ProviderTemplate| Err("network down".to_string()))
                as Box<dyn FnOnce(&ProviderTemplate) -> Result<String, String>>,
            Box::new(|_: &GatewayConfig| Ok(()))
                as Box<dyn FnOnce(&GatewayConfig) -> Result<(), String>>,
            "fetch failure",
        ),
        (
            Box::new(|_: &ProviderTemplate| {
                Ok(r#"{"data":[{"id":"gpt-4o-2024","context_length":0}]}"#.to_string())
            }),
            Box::new(|_: &GatewayConfig| Ok(())),
            "invalid metadata",
        ),
    ] {
        let mut config = bound_refresh_config();
        let providers_data = json!({ "providers": [stored_opencode(&config)] });
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_for_sync = calls.clone();
        let result = apply_template_sync_with_terminal_refresh(
            &mut config,
            "commandcode",
            fetch,
            persist,
            &providers_data,
            move |_tools| {
                calls_for_sync.fetch_add(1, Ordering::SeqCst);
                async { Ok::<Vec<TerminalSyncRecord>, String>(Vec::new()) }
            },
        )
        .await;
        assert!(result.is_err(), "{label} must fail the sync");
        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "{label} must perform zero terminal calls"
        );
        assert_eq!(
            config.providers[0].mappings[0].context_window,
            Some(128000),
            "{label} must leave the original value unchanged"
        );
    }

    let mut config = bound_refresh_config();
    let providers_data = json!({ "providers": [stored_opencode(&config)] });
    let calls = Arc::new(AtomicUsize::new(0));
    let calls_for_sync = calls.clone();
    let result = apply_template_sync_with_terminal_refresh(
        &mut config,
        "commandcode",
        |_| Ok(REFRESH_BODY.to_string()),
        |_| Err("persist failed".to_string()),
        &providers_data,
        move |_tools| {
            calls_for_sync.fetch_add(1, Ordering::SeqCst);
            async { Ok::<Vec<TerminalSyncRecord>, String>(Vec::new()) }
        },
    )
    .await;
    assert!(result.is_err(), "a persistence failure must fail the sync");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(config.providers[0].mappings[0].context_window, Some(128000));
}

#[tokio::test]
async fn terminal_failure_during_refresh_keeps_the_persisted_gateway_change() {
    let _home = isolated_temp_home("ctx-template-refresh-terminal-error");
    let mut config = bound_refresh_config();
    write_config(&config).expect("write initial config");
    let providers_data = json!({ "providers": [stored_opencode(&config)] });
    let calls = Arc::new(AtomicUsize::new(0));
    let calls_for_sync = calls.clone();
    let view = apply_template_sync_with_terminal_refresh(
        &mut config,
        "commandcode",
        |_| Ok(REFRESH_BODY.to_string()),
        |next| write_config(next).map_err(|error| error.to_string()),
        &providers_data,
        move |_tools| {
            calls_for_sync.fetch_add(1, Ordering::SeqCst);
            async { Err::<Vec<TerminalSyncRecord>, String>("terminal upsert failed".to_string()) }
        },
    )
    .await
    .expect("terminal refresh is best-effort and must not fail the template sync");

    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(view.synced_at.is_some());
    let persisted = read_config().expect("read persisted config");
    assert_eq!(
        persisted.providers[0].mappings[0].context_window,
        Some(256000),
        "the gateway change must stand even when the terminal refresh fails"
    );
}

#[tokio::test]
async fn template_refresh_never_enrolls_an_unsynchronized_tool() {
    let _home = isolated_temp_home("ctx-template-refresh-unsynced");
    let mut config = bound_refresh_config();
    config.terminal_syncs.clear();
    write_config(&config).expect("write initial config");
    let calls = Arc::new(AtomicUsize::new(0));
    let calls_for_sync = calls.clone();
    apply_template_sync_with_terminal_refresh(
        &mut config,
        "commandcode",
        |_| Ok(REFRESH_BODY.to_string()),
        |next| write_config(next).map_err(|error| error.to_string()),
        &json!({ "providers": [] }),
        move |_tools| {
            calls_for_sync.fetch_add(1, Ordering::SeqCst);
            async { Ok::<Vec<TerminalSyncRecord>, String>(Vec::new()) }
        },
    )
    .await
    .expect("template sync succeeds without enrollment");

    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "a tool without a managed marker must never be enrolled by a refresh"
    );
}

#[tokio::test]
async fn explicit_terminal_sync_surfaces_an_upsert_error_and_writes_no_ledger() {
    let _home = isolated_temp_home("ctx-explicit-error");
    write_config(&gateway_config(PORT)).expect("write gateway config");
    let result = apply_terminal_sync_with_selection(
        &json!({ "providers": [] }),
        |_| -> UpsertFuture {
            Box::pin(async { Err("upsert failed".to_string()) })
        },
        vec!["opencode".to_string()],
        TerminalSyncProfile::RELEASE,
        None,
    )
    .await;

    assert!(result.is_err(), "an explicit sync error must surface");
    let stored = read_config().expect("read config");
    assert!(
        !stored
            .terminal_syncs
            .iter()
            .any(|record| record.tool == "opencode"),
        "a failed explicit sync must not claim a ledger record: {:?}",
        stored.terminal_syncs
    );
}

// ---------------------------------------------------------------------------
// Save-triggered refresh contract
//
// The two seams below are the intended in-memory composition points behind the
// terminal-save wrappers: they persist the gateway change first, then call the
// injected terminal pipeline at most once for the tools that were previously
// synchronized. They are deliberately proposed by these behavior tests rather
// than by naming arbitrary internal helpers.
// ---------------------------------------------------------------------------

fn template_bound_config() -> GatewayConfig {
    let mut config = gateway_config(PORT);
    config.providers[0].template_id = Some("tpl-ctx".to_string());
    config.providers[0].mappings =
        vec![context_mapping("gpt-4o", "model-a", Some("Model A"), Some(128000))];
    config.terminal_syncs.push(opencode_ledger(&config));
    config
}

fn template_with_context(context_window: Option<u32>) -> ProviderTemplate {
    ProviderTemplate {
        id: "tpl-ctx".to_string(),
        name: "Context Template".to_string(),
        base_url: "https://upstream.example/v1".to_string(),
        models: vec![ProviderTemplateModel {
            upstream_model: "model-a".to_string(),
            local_model: Some("gpt-4o".to_string()),
            display_name: Some("Model A".to_string()),
            context_window,
            ..ProviderTemplateModel::default()
        }],
        ..ProviderTemplate::default()
    }
}

#[tokio::test]
async fn provider_save_refreshes_previously_synced_tools_once_after_persistence() {
    let _home = isolated_temp_home("ctx-provider-save-refresh");
    let config = gateway_config(PORT);
    write_config(&config).expect("write initial config");
    let providers_data = json!({ "providers": [stored_opencode(&config)] });

    let mut updated = config.providers[0].clone();
    updated.mappings[0].context_window = Some(256000);

    let calls = Arc::new(AtomicUsize::new(0));
    let persisted = Arc::new(Mutex::new(None));
    let calls_for_sync = calls.clone();
    let persisted_for_sync = persisted.clone();
    let saved: GatewayConfig = apply_provider_save_with_terminal_refresh(
        updated,
        None,
        &providers_data,
        move |tools| {
            calls_for_sync.fetch_add(1, Ordering::SeqCst);
            let on_disk = read_config().expect("the save must persist before the refresh");
            let persisted = persisted_for_sync.clone();
            async move {
                assert_eq!(tools, vec!["opencode".to_string()]);
                *persisted.lock().unwrap() = on_disk.providers[0].mappings[0].context_window;
                Ok::<Vec<TerminalSyncRecord>, String>(Vec::new())
            }
        },
    )
    .await
    .expect("the provider save must succeed");

    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        *persisted.lock().unwrap(),
        Some(256000),
        "the injected refresh must observe the already-persisted value"
    );
    assert_eq!(saved.providers[0].mappings[0].context_window, Some(256000));
}

#[tokio::test]
async fn provider_save_without_a_context_change_or_for_unsynced_tools_does_not_refresh() {
    let _home = isolated_temp_home("ctx-provider-save-no-refresh");

    let mut config = gateway_config(PORT);
    config.providers[0].mappings[0].context_window = Some(128000);
    write_config(&config).expect("write initial config");
    let providers_data = json!({ "providers": [stored_opencode(&config)] });
    let calls = Arc::new(AtomicUsize::new(0));
    let calls_for_sync = calls.clone();
    apply_provider_save_with_terminal_refresh(
        config.providers[0].clone(),
        None,
        &providers_data,
        move |_tools| {
            calls_for_sync.fetch_add(1, Ordering::SeqCst);
            async { Ok::<Vec<TerminalSyncRecord>, String>(Vec::new()) }
        },
    )
    .await
    .expect("an unchanged provider save succeeds");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "a save that does not change effective context must not refresh"
    );

    let mut invalid = config.providers[0].clone();
    invalid.mappings[0].context_window = Some(0);
    let calls = Arc::new(AtomicUsize::new(0));
    let calls_for_sync = calls.clone();
    let result: Result<GatewayConfig, String> = apply_provider_save_with_terminal_refresh(
        invalid,
        None,
        &providers_data,
        move |_tools| {
            calls_for_sync.fetch_add(1, Ordering::SeqCst);
            async { Ok::<Vec<TerminalSyncRecord>, String>(Vec::new()) }
        },
    )
    .await;
    assert!(result.is_err(), "an invalid context save must be rejected");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        read_config().unwrap().providers[0].mappings[0].context_window,
        Some(128000),
        "the invalid save must not publish"
    );

    let mut unsynced = config.clone();
    unsynced.terminal_syncs.clear();
    write_config(&unsynced).expect("write unsynced config");
    let mut changed = unsynced.providers[0].clone();
    changed.mappings[0].context_window = Some(256000);
    let calls = Arc::new(AtomicUsize::new(0));
    let calls_for_sync = calls.clone();
    apply_provider_save_with_terminal_refresh(
        changed,
        None,
        &json!({ "providers": [] }),
        move |_tools| {
            calls_for_sync.fetch_add(1, Ordering::SeqCst);
            async { Ok::<Vec<TerminalSyncRecord>, String>(Vec::new()) }
        },
    )
    .await
    .expect("an unsynced provider save succeeds");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "a context save must never enroll an unsynchronized tool"
    );
}

#[tokio::test]
async fn provider_save_refresh_failure_keeps_the_saved_gateway_change() {
    let _home = isolated_temp_home("ctx-provider-save-best-effort");
    let config = gateway_config(PORT);
    write_config(&config).expect("write initial config");
    let providers_data = json!({ "providers": [stored_opencode(&config)] });
    let mut updated = config.providers[0].clone();
    updated.mappings[0].context_window = Some(256000);

    let calls = Arc::new(AtomicUsize::new(0));
    let calls_for_sync = calls.clone();
    let saved: GatewayConfig = apply_provider_save_with_terminal_refresh(
        updated,
        None,
        &providers_data,
        move |_tools| {
            calls_for_sync.fetch_add(1, Ordering::SeqCst);
            async { Err::<Vec<TerminalSyncRecord>, String>("terminal failed".to_string()) }
        },
    )
    .await
    .expect("a best-effort terminal failure must not undo the provider save");

    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(saved.providers[0].mappings[0].context_window, Some(256000));
    assert_eq!(
        read_config().unwrap().providers[0].mappings[0].context_window,
        Some(256000)
    );
}

#[tokio::test]
async fn template_save_refreshes_once_after_persisting_changed_inherited_context() {
    let _home = isolated_temp_home("ctx-template-save-refresh");
    let config = template_bound_config();
    write_config(&config).expect("write initial config");
    let providers_data = json!({ "providers": [stored_opencode(&config)] });

    let calls = Arc::new(AtomicUsize::new(0));
    let persisted = Arc::new(Mutex::new(None));
    let calls_for_sync = calls.clone();
    let persisted_for_sync = persisted.clone();
    let views: Vec<crate::ai_gateway::templates::ProviderTemplateView> = apply_template_save_with_terminal_refresh(
        template_with_context(Some(256000)),
        &providers_data,
        move |tools| {
            calls_for_sync.fetch_add(1, Ordering::SeqCst);
            let on_disk = read_config().expect("the template save must persist first");
            let persisted = persisted_for_sync.clone();
            async move {
                assert_eq!(tools, vec!["opencode".to_string()]);
                *persisted.lock().unwrap() = on_disk.providers[0].mappings[0].context_window;
                Ok::<Vec<TerminalSyncRecord>, String>(Vec::new())
            }
        },
    )
    .await
    .expect("the template save must succeed");

    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(*persisted.lock().unwrap(), Some(256000));
    assert_eq!(
        views
            .iter()
            .find(|view| view.template.id == "tpl-ctx")
            .and_then(|view| view.template.models[0].context_window),
        Some(256000)
    );
    assert_eq!(
        read_config().unwrap().providers[0].mappings[0].context_window,
        Some(256000)
    );
}

#[tokio::test]
async fn template_save_without_a_context_change_does_not_refresh_and_failure_is_best_effort() {
    let _home = isolated_temp_home("ctx-template-save-no-refresh");
    let config = template_bound_config();
    write_config(&config).expect("write initial config");
    let providers_data = json!({ "providers": [stored_opencode(&config)] });

    let calls = Arc::new(AtomicUsize::new(0));
    let calls_for_sync = calls.clone();
    apply_template_save_with_terminal_refresh(
        template_with_context(Some(128000)),
        &providers_data,
        move |_tools| {
            calls_for_sync.fetch_add(1, Ordering::SeqCst);
            async { Ok::<Vec<TerminalSyncRecord>, String>(Vec::new()) }
        },
    )
    .await
    .expect("a non-context template edit succeeds");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "a template save without an inherited context change must not refresh"
    );

    let calls = Arc::new(AtomicUsize::new(0));
    let calls_for_sync = calls.clone();
    let result: Result<Vec<crate::ai_gateway::templates::ProviderTemplateView>, String> = apply_template_save_with_terminal_refresh(
        template_with_context(Some(256000)),
        &providers_data,
        move |_tools| {
            calls_for_sync.fetch_add(1, Ordering::SeqCst);
            async { Err::<Vec<TerminalSyncRecord>, String>("terminal failed".to_string()) }
        },
    )
    .await;
    assert!(
        result.is_ok(),
        "a best-effort terminal failure must not undo a template save"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        read_config().unwrap().providers[0].mappings[0].context_window,
        Some(256000)
    );

    let calls = Arc::new(AtomicUsize::new(0));
    let calls_for_sync = calls.clone();
    let result: Result<Vec<crate::ai_gateway::templates::ProviderTemplateView>, String> = apply_template_save_with_terminal_refresh(
        template_with_context(Some(0)),
        &providers_data,
        move |_tools| {
            calls_for_sync.fetch_add(1, Ordering::SeqCst);
            async { Ok::<Vec<TerminalSyncRecord>, String>(Vec::new()) }
        },
    )
    .await;
    assert!(result.is_err(), "an invalid template context must be rejected");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
