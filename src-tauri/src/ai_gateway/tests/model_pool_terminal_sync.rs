//! Step 3 behavior tests for model-pool discovery and terminal exposure
//! (`20261013-free-model-pool`, REQ-008/REQ-009; AC-011 / AC-013 / AC-016).
//!
//! Boundary asserted here (the contract the Step 3 Backend leaf must satisfy):
//!
//! - `runtime_http::local_model_names` / `models_payload` advertise an enabled
//!   pool alias exactly once, never a disabled pool and never a retired alias,
//!   while ordinary mapping rows keep their existing behavior.
//! - The existing terminal-sync seam (`apply_terminal_sync_with_selection`)
//!   exposes an enabled pool alias to OpenCode as a Chat-compatible model with
//!   no guessed reasoning variants, and never a disabled pool.
//! - Codex uses an enabled pool alias as the generated fallback only when no
//!   ordinary model is configured, retains an already selected enabled pool on
//!   resync, and never replaces an ordinary default.
//! - A native Claude selection rejects an OpenAI pool alias before writing.
//! - A pool membership change marks the OpenCode terminal target pending while
//!   the profile ledger stays current.
//!
//! Every test drives only already-compiled entry points and typed raw fixtures,
//! so a RED is a compiled observable assertion failure rather than a missing
//! Rust symbol. No real CLI file, terminal projection or user credential is
//! read or written.

use super::{gateway_config, isolated_temp_home};
use crate::ai_gateway::commands::{
    apply_terminal_sync_with_selection, terminal_targets_from, TerminalSyncProfile, UpsertFuture,
};
use crate::ai_gateway::runtime_http::{local_model_names, models_payload};
use crate::ai_gateway::storage::{local_base_url, read_config, write_config};
use crate::ai_gateway::{
    ClaudeModelSelection, ModelPool, ModelPoolCandidate, TerminalSyncRecord, UpstreamProtocol,
};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

/// The provider/local/upstream triple `gateway_config(port)` actually serves.
const ORDINARY_LOCAL: &str = "gpt-4o";
const ORDINARY_UPSTREAM: &str = "gpt-4o-2024";

fn pool_candidate(
    provider_id: &str,
    local_model: &str,
    upstream_model: &str,
    protocol: UpstreamProtocol,
    weight: u32,
) -> ModelPoolCandidate {
    ModelPoolCandidate {
        provider_id: provider_id.to_string(),
        local_model: local_model.to_string(),
        upstream_model: upstream_model.to_string(),
        protocol,
        weight,
        ..ModelPoolCandidate::default()
    }
}

/// An enabled pool whose single candidate references the ordinary `g1` mapping
/// served by `gateway_config`, so the candidate resolves against the fixture.
fn enabled_pool() -> ModelPool {
    ModelPool {
        id: "pool-free".to_string(),
        name: "Free Modle".to_string(),
        model: "free-model".to_string(),
        enabled: true,
        candidates: vec![pool_candidate(
            "g1",
            ORDINARY_LOCAL,
            ORDINARY_UPSTREAM,
            UpstreamProtocol::ChatCompletions,
            1,
        )],
        ..ModelPool::default()
    }
}

/// Run the existing terminal-sync seam against `providers_data` and return the
/// captured submitted payloads, one per requested tool.
async fn capture_payloads(providers_data: &Value, tools: Vec<String>) -> Vec<Value> {
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
        TerminalSyncProfile::RELEASE,
        None,
    )
    .await
    .expect("terminal sync must succeed");
    let submitted = captured.lock().unwrap().clone();
    submitted
}

fn payload_for<'a>(payloads: &'a [Value], tool: &str) -> &'a Value {
    payloads
        .iter()
        .find(|value| value["tool"] == tool)
        .unwrap_or_else(|| panic!("missing terminal payload for {tool}: {payloads:?}"))
}

// ---------------------------------------------------------------------------
// REQ-008 / AC-011: discovery advertises exactly one enabled public ID
// ---------------------------------------------------------------------------

/// AC-011: an enabled configured pool appears exactly once in the local model
/// union and in the `/v1/models` payload, a disabled pool appears in neither,
/// and the ordinary mapping rows are unchanged.
#[test]
fn discovery_advertises_enabled_pool_once_and_excludes_disabled() {
    let mut config = gateway_config(19000);
    config.model_pools.push(enabled_pool());
    let mut disabled = enabled_pool();
    disabled.id = "pool-off".to_string();
    disabled.model = "off-model".to_string();
    disabled.enabled = false;
    config.model_pools.push(disabled);

    let names = local_model_names(&config);
    assert_eq!(
        names.iter().filter(|name| name.as_str() == "free-model").count(),
        1,
        "an enabled pool alias must be advertised exactly once: {names:?}"
    );
    assert!(
        !names.iter().any(|name| name == "off-model"),
        "a disabled pool alias must not be advertised: {names:?}"
    );
    assert!(
        names.iter().any(|name| name == ORDINARY_LOCAL),
        "ordinary mapping rows must keep their existing discovery behavior: {names:?}"
    );

    let payload = models_payload(&config);
    let ids: Vec<&str> = payload["data"]
        .as_array()
        .unwrap_or_else(|| panic!("models payload must carry a data array: {payload}"))
        .iter()
        .filter_map(|row| row["id"].as_str())
        .collect();
    assert_eq!(
        ids.iter().filter(|id| **id == "free-model").count(),
        1,
        "the enabled pool alias must appear exactly once in /v1/models: {ids:?}"
    );
    assert!(
        !ids.contains(&"off-model"),
        "a disabled pool alias must never be advertised: {ids:?}"
    );
}

/// AC-011: a retired (reserved-but-unowned) alias stays guarded and is never
/// advertised as a callable model.
#[test]
fn discovery_does_not_advertise_a_retired_pool_alias() {
    let mut config = gateway_config(19000);
    config
        .model_pool_reserved_models
        .push("old-model".to_string());

    let names = local_model_names(&config);
    assert!(
        !names.iter().any(|name| name == "old-model"),
        "a retired pool alias is reserved, never advertised: {names:?}"
    );
    let payload = models_payload(&config);
    assert!(
        !payload["data"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or(&[])
            .iter()
            .any(|row| row["id"].as_str() == Some("old-model")),
        "a retired pool alias must not be callable: {payload}"
    );
}

// ---------------------------------------------------------------------------
// REQ-008 / AC-011: terminal exposure
// ---------------------------------------------------------------------------

/// AC-011: OpenCode manual sync exposes an enabled pool alias as a
/// Chat-compatible model. The pool advertises no reasoning variants it did not
/// get from a real source mapping.
#[tokio::test]
async fn opencode_payload_includes_enabled_pool_as_chat_model_without_guessed_variants() {
    let _home = isolated_temp_home("pool-terminal-opencode");
    let mut config = gateway_config(19000);
    config.model_pools.push(enabled_pool());
    write_config(&config).expect("write gateway config");

    let payloads = capture_payloads(&json!({ "providers": [] }), vec!["opencode".to_string()]).await;
    let opencode = payload_for(&payloads, "opencode");
    let model = &opencode["tool_config"]["models"]["free-model"];

    assert!(
        !model.is_null(),
        "an enabled pool alias must be exposed to OpenCode: {opencode}"
    );
    assert!(
        model.get("provider").is_none(),
        "a Chat-compatible pool must keep the Chat adapter and never the Responses npm override: {model}"
    );
    assert!(
        model.get("variants").is_none(),
        "the pool must not guess reasoning variants from its source model: {model}"
    );
    assert!(
        !opencode["tool_config"]["models"]["gpt-4o"].is_null(),
        "the ordinary mapping rows must stay exposed: {opencode}"
    );
}

/// AC-011: a disabled pool is never exposed to OpenCode.
#[tokio::test]
async fn opencode_payload_excludes_disabled_pool() {
    let _home = isolated_temp_home("pool-terminal-opencode-disabled");
    let mut config = gateway_config(19000);
    let mut disabled = enabled_pool();
    disabled.model = "off-model".to_string();
    disabled.enabled = false;
    config.model_pools.push(disabled);
    write_config(&config).expect("write gateway config");

    let payloads = capture_payloads(&json!({ "providers": [] }), vec!["opencode".to_string()]).await;
    let models = &payload_for(&payloads, "opencode")["tool_config"]["models"];
    assert!(
        models.get("off-model").is_none(),
        "a disabled pool must not be exposed to OpenCode: {models}"
    );
}

/// AC-011: Codex uses an enabled pool alias as the generated fallback when no
/// ordinary model is configured, because the only ordinary mapping is disabled.
#[tokio::test]
async fn codex_payload_uses_pool_as_fallback_when_no_ordinary_model() {
    let _home = isolated_temp_home("pool-terminal-codex-fallback");
    let mut config = gateway_config(19000);
    for mapping in &mut config.providers[0].mappings {
        mapping.enabled = false;
    }
    config.model_pools.push(enabled_pool());
    write_config(&config).expect("write gateway config");

    let payloads = capture_payloads(&json!({ "providers": [] }), vec!["codex".to_string()]).await;
    let codex = payload_for(&payloads, "codex");
    assert_eq!(
        codex["model"], "free-model",
        "a pool-only Codex configuration must fall back to the enabled pool alias: {codex}"
    );
}

/// AC-011: a resync retains an already selected enabled pool alias instead of
/// dropping it, even though the config exposes no ordinary model.
#[tokio::test]
async fn codex_resync_retains_already_selected_enabled_pool() {
    let _home = isolated_temp_home("pool-terminal-codex-retain");
    let mut config = gateway_config(19000);
    for mapping in &mut config.providers[0].mappings {
        mapping.enabled = false;
    }
    config.model_pools.push(enabled_pool());
    write_config(&config).expect("write gateway config");

    let stored_codex = json!({
        "id": "gw-codex",
        "tool": "codex",
        "name": "AI Gateway",
        "base_url": local_base_url(config.port),
        "model": "free-model",
        "tool_config": { "ai_gateway_gateway": true },
    });
    let payloads = capture_payloads(
        &json!({ "providers": [stored_codex] }),
        vec!["codex".to_string()],
    )
    .await;
    let codex = payload_for(&payloads, "codex");
    assert_eq!(
        codex["id"], "gw-codex",
        "the owned Codex record must be reused: {codex}"
    );
    assert_eq!(
        codex["model"], "free-model",
        "a previously selected enabled pool alias must survive resync: {codex}"
    );
}

/// AC-011: an ordinary Codex default is not automatically replaced by a pool.
#[tokio::test]
async fn codex_payload_does_not_replace_ordinary_model_with_pool() {
    let _home = isolated_temp_home("pool-terminal-codex-ordinary");
    let mut config = gateway_config(19000);
    config.model_pools.push(enabled_pool());
    write_config(&config).expect("write gateway config");

    let payloads = capture_payloads(&json!({ "providers": [] }), vec!["codex".to_string()]).await;
    let codex = payload_for(&payloads, "codex");
    assert_eq!(
        codex["model"], ORDINARY_LOCAL,
        "an ordinary mapping must keep serving Codex: {codex}"
    );
}

/// AC-011: a native Claude selection rejects an OpenAI pool alias and writes
/// nothing, so the pool never enters native Claude choices.
#[tokio::test]
async fn claude_native_selection_rejects_openai_pool_alias_without_writing() {
    let _home = isolated_temp_home("pool-terminal-claude-reject");
    let mut config = gateway_config(19000);
    config.model_pools.push(enabled_pool());
    write_config(&config).expect("write gateway config");

    let calls = Arc::new(Mutex::new(0usize));
    let calls_for_upsert = calls.clone();
    let selection = ClaudeModelSelection {
        default_model: "free-model".to_string(),
        haiku: "free-model".to_string(),
        sonnet: "free-model".to_string(),
        opus: "free-model".to_string(),
    };
    let result = apply_terminal_sync_with_selection(
        &json!({ "providers": [] }),
        move |_value| -> UpsertFuture {
            let calls = calls_for_upsert.clone();
            Box::pin(async move {
                *calls.lock().expect("call counter") += 1;
                Ok(())
            })
        },
        vec!["claude".to_string()],
        TerminalSyncProfile::RELEASE,
        Some(selection),
    )
    .await;

    assert!(
        result.is_err(),
        "an OpenAI pool alias must not be accepted as a native Claude selection: {result:?}"
    );
    assert_eq!(
        *calls.lock().unwrap(),
        0,
        "nothing may be written for a rejected Claude selection"
    );
    let stored = read_config().expect("read gateway config");
    assert!(
        !stored.terminal_syncs.iter().any(|record| record.tool == "claude"),
        "no Claude ledger may be written: {:?}",
        stored.terminal_syncs
    );
}

/// AC-011 / AC-016: a pool membership change is part of the pending-sync
/// comparison; the stored OpenCode models (provider-only) drift once the pool
/// alias is added, so the target is reported pending.
#[test]
fn terminal_targets_mark_opencode_pending_when_pool_membership_changes() {
    let mut config = gateway_config(19000);
    config.default_key_id = Some("k1".to_string());
    config.model_pools.push(enabled_pool());
    config.terminal_syncs.push(TerminalSyncRecord {
        provider_id: "gw-open".to_string(),
        tool: "opencode".to_string(),
        synced_key_id: "k1".to_string(),
        synced_base_url: local_base_url(config.port),
        synced_at: 1,
    });

    // The stored provider carries exactly the provider-only model catalog the
    // pre-pool pipeline generated; only the pool alias makes it drift.
    let stored_opencode = json!({
        "id": "gw-open",
        "tool": "opencode",
        "name": "AI Gateway",
        "base_url": local_base_url(config.port),
        "provider_key": "gateway",
        "tool_config": {
            "ai_gateway_gateway": true,
            "models": {
                "gpt-4o": { "name": "GPT-4o" },
                "ds": { "name": "deepseek-chat" },
            },
        },
    });

    let targets = terminal_targets_from(
        &config,
        &json!({ "providers": [stored_opencode] }),
        TerminalSyncProfile::RELEASE,
    );
    let opencode = targets
        .iter()
        .find(|target| target.tool == "opencode")
        .expect("an opencode target must exist");
    assert!(
        opencode.pending_sync,
        "a pool membership change must mark the OpenCode target pending: {opencode:?}"
    );
}
