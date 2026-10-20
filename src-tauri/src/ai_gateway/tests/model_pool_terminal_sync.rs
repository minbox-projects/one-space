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
use crate::ai_gateway::model_pools::cool_pool_candidate;
use crate::ai_gateway::runtime_http::{local_model_names, models_payload};
use crate::ai_gateway::storage::{local_base_url, read_config, write_config};
use crate::ai_gateway::{
    ClaudeModelSelection, ModelPool, ModelPoolCandidate, PoolCandidateIdentity,
    PoolCandidateReasoning, PoolDefaultReasoningEffort, PoolReasoningAction, PoolReasoningEffort,
    PoolReasoningMapping, PoolReasoningProfile, TerminalSyncRecord, UpstreamProtocol,
};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::Duration;

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

// ---------------------------------------------------------------------------
// Step 3 of `20261015-pool-only-reasoning` (REQ-005 / REQ-007, AC-009 / AC-012):
// pool reasoning capability catalogs
// ---------------------------------------------------------------------------
//
// Boundary asserted here (the contract the Step 3 Backend leaf must satisfy):
//
// - `/v1/models` pool rows and OpenCode manual-sync pool rows expose the pool's
//   *static* configured reasoning capability: the supported canonical
//   preferences in canonical order (`low`, `medium`, `high`, `xhigh`, `none`),
//   the pool default, and a five-row coverage table. The support set derives
//   from mappings that resolve against the current source identity; `none`
//   requires an explicit disable mapping. Cooldown, key availability, health and
//   source enable state never redefine the static configuration count.
// - A pool with no reasoning declarations keeps its pre-feature shape (no
//   OpenCode variants, no advertised support).
// - OpenCode pool variants use the canonical preference as both the variant name
//   and `reasoningEffort`; no `auto` variant, native mapped value, borrowed
//   Responses npm override or fabricated price/credential field is exposed.
// - Ordinary OpenCode entries stay identical across pool policy edits, and a
//   pool policy edit only marks the target pending for the existing manual sync
//   rather than projecting automatically.
//
// Metadata shape (identical keys on discovery/status/terminal pool entries):
//   `reasoning_efforts`: [canonical]              (support, canonical order)
//   `default_reasoning_effort`: canonical | auto
//   `reasoning_coverage`: [{effort, mapped_candidates, total_candidates}] x5
//
// Red before Step 3: the current `pool_discovery_entry` and OpenCode pool row
// carry only the pre-existing `name`/`sources` shape, so each assertion below is
// an observable JSON mismatch after successful compilation (no absent import or
// type is required).

const CANONICAL_EFFORTS: [&str; 6] = ["low", "medium", "high", "xhigh", "max", "none"];

fn enum_action(value: &str) -> PoolReasoningAction {
    PoolReasoningAction::Enum {
        value: value.to_string(),
    }
}

/// One OpenAI-effort pool declaration whose declared native value set is derived
/// from its enum actions, so a catalog derivation may re-check value membership.
fn reasoning_with(
    mappings: &[(PoolReasoningEffort, PoolReasoningAction)],
    supports_disable: bool,
) -> PoolCandidateReasoning {
    let values = mappings
        .iter()
        .filter_map(|(_, action)| match action {
            PoolReasoningAction::Enum { value } => Some(value.clone()),
            _ => None,
        })
        .collect();
    PoolCandidateReasoning {
        profile: PoolReasoningProfile::OpenaiEffort,
        values,
        budget: None,
        supports_disable,
        mappings: mappings
            .iter()
            .map(|(effort, action)| PoolReasoningMapping {
                effort: *effort,
                action: action.clone(),
            })
            .collect(),
    }
}

fn reasoning_candidate(
    local: &str,
    upstream: &str,
    reasoning: Option<PoolCandidateReasoning>,
) -> ModelPoolCandidate {
    ModelPoolCandidate {
        provider_id: "g1".to_string(),
        local_model: local.to_string(),
        upstream_model: upstream.to_string(),
        protocol: UpstreamProtocol::ChatCompletions,
        weight: 1,
        reasoning,
    }
}

fn reasoning_pool(
    id: &str,
    model: &str,
    default: PoolDefaultReasoningEffort,
    candidates: Vec<ModelPoolCandidate>,
) -> ModelPool {
    ModelPool {
        id: id.to_string(),
        name: format!("Pool {id}"),
        model: model.to_string(),
        enabled: true,
        candidates,
        default_reasoning_effort: default,
    }
}

fn discovered_pool<'a>(payload: &'a Value, alias: &str) -> &'a Value {
    payload["data"]
        .as_array()
        .unwrap_or_else(|| panic!("models payload must carry a data array: {payload}"))
        .iter()
        .find(|row| row["id"].as_str() == Some(alias))
        .unwrap_or_else(|| panic!("missing pool discovery row '{alias}': {payload}"))
}

/// Extract `(reasoning_efforts, default_reasoning_effort, coverage)` from one
/// catalog entry, panicking when the metadata is absent so the RED is explicit.
fn reasoning_metadata(entry: &Value) -> (Vec<String>, String, Vec<(String, u64, u64)>) {
    let efforts = entry["reasoning_efforts"]
        .as_array()
        .unwrap_or_else(|| panic!("pool entry must expose reasoning_efforts: {entry}"))
        .iter()
        .map(|effort| {
            effort
                .as_str()
                .expect("reasoning_efforts entries must be strings")
                .to_string()
        })
        .collect();
    let default = entry["default_reasoning_effort"]
        .as_str()
        .unwrap_or_else(|| {
            panic!("pool entry must expose default_reasoning_effort as a string: {entry}")
        })
        .to_string();
    let coverage = entry["reasoning_coverage"]
        .as_array()
        .unwrap_or_else(|| panic!("pool entry must expose reasoning_coverage: {entry}"))
        .iter()
        .map(|row| {
            (
                row["effort"].as_str().expect("coverage effort").to_string(),
                row["mapped_candidates"]
                    .as_u64()
                    .unwrap_or_else(|| panic!("mapped_candidates must be a uint: {row}")),
                row["total_candidates"]
                    .as_u64()
                    .unwrap_or_else(|| panic!("total_candidates must be a uint: {row}")),
            )
        })
        .collect();
    (efforts, default, coverage)
}

fn coverage_for(coverage: &[(String, u64, u64)], effort: &str) -> (u64, u64) {
    coverage
        .iter()
        .find(|(name, _, _)| name == effort)
        .map(|(_, mapped, total)| (*mapped, *total))
        .unwrap_or_else(|| panic!("missing coverage row for '{effort}': {coverage:?}"))
}

/// AC-009 / AC-012: two valid `high` mappings plus one dangling selected source
/// advertise `high` with a static 2/3 coverage, all five canonical rows present
/// in canonical order, and a coverage that cooldown, source enable state, key
/// availability and mapping auto-disable never redefine.
#[test]
fn pool_discovery_reports_static_support_and_coverage() {
    let mut config = gateway_config(19000);
    config.model_pools.push(reasoning_pool(
        "pool-cov",
        "free-model",
        PoolDefaultReasoningEffort::Auto,
        vec![
            reasoning_candidate(
                ORDINARY_LOCAL,
                ORDINARY_UPSTREAM,
                Some(reasoning_with(
                    &[(PoolReasoningEffort::High, enum_action("high"))],
                    false,
                )),
            ),
            reasoning_candidate(
                "ds",
                "deepseek-chat",
                Some(reasoning_with(
                    &[(PoolReasoningEffort::High, enum_action("high"))],
                    false,
                )),
            ),
            reasoning_candidate(
                "gone-local",
                "gone-upstream",
                Some(reasoning_with(
                    &[(PoolReasoningEffort::High, enum_action("high"))],
                    false,
                )),
            ),
        ],
    ));

    let baseline = models_payload(&config);
    let (efforts, default, coverage) =
        reasoning_metadata(discovered_pool(&baseline, "free-model"));
    assert_eq!(
        efforts,
        vec!["high"],
        "only the two valid high mappings are supported: {coverage:?}"
    );
    assert_eq!(default, "auto", "a default-less pool reads as auto");
    assert_eq!(
        coverage
            .iter()
            .map(|(effort, _, _)| effort.as_str())
            .collect::<Vec<_>>(),
        CANONICAL_EFFORTS.to_vec(),
        "coverage must carry all five rows in canonical order: {coverage:?}"
    );
    assert_eq!(
        coverage_for(&coverage, "high"),
        (2, 3),
        "two of the three selected candidates resolve against the current source"
    );
    assert_eq!(
        coverage_for(&coverage, "none"),
        (0, 3),
        "no disable mapping means none is unsupported"
    );

    // A candidate cooldown is runtime readiness, never a configuration count.
    cool_pool_candidate(
        &PoolCandidateIdentity {
            pool_id: "pool-cov".to_string(),
            provider_id: "g1".to_string(),
            local_model: ORDINARY_LOCAL.to_string(),
            upstream_model: ORDINARY_UPSTREAM.to_string(),
            protocol: UpstreamProtocol::ChatCompletions,
            reasoning: None,
        },
        Some(Duration::from_secs(60)),
    );
    let after_cooldown = models_payload(&config);
    assert_eq!(
        reasoning_metadata(discovered_pool(&after_cooldown, "free-model")),
        (efforts.clone(), default.clone(), coverage.clone()),
        "cooldown must not change the advertised static coverage"
    );

    // Disabling the source provider, its mappings and its keys leaves the
    // identity-based static count unchanged.
    let mut disabled = config.clone();
    for provider in &mut disabled.providers {
        provider.enabled = false;
        for mapping in &mut provider.mappings {
            mapping.enabled = false;
            mapping.auto_disabled = true;
        }
        for key in &mut provider.keys {
            key.enabled = false;
        }
    }
    assert_eq!(
        reasoning_metadata(discovered_pool(&models_payload(&disabled), "free-model")),
        (efforts, default, coverage),
        "source enable/health/key state must not redefine the static coverage"
    );
}

/// AC-009: the support set follows the canonical order rather than declaration
/// order, and `none` appears only once an explicit disable mapping is declared.
#[test]
fn pool_discovery_support_order_is_canonical_and_none_requires_disable() {
    let declared_positive = [
        (PoolReasoningEffort::High, enum_action("high")),
        (PoolReasoningEffort::Low, enum_action("low")),
        (PoolReasoningEffort::Xhigh, enum_action("xhigh")),
        (PoolReasoningEffort::Medium, enum_action("medium")),
    ];
    let mut config = gateway_config(19000);
    config.model_pools.push(reasoning_pool(
        "pool-order",
        "order-model",
        PoolDefaultReasoningEffort::Auto,
        vec![reasoning_candidate(
            ORDINARY_LOCAL,
            ORDINARY_UPSTREAM,
            Some(reasoning_with(&declared_positive, false)),
        )],
    ));

    let (efforts, _, coverage) =
        reasoning_metadata(discovered_pool(&models_payload(&config), "order-model"));
    assert_eq!(
        efforts,
        vec!["low", "medium", "high", "xhigh"],
        "support must follow the canonical order, not declaration order"
    );
    assert_eq!(
        coverage_for(&coverage, "none"),
        (0, 1),
        "none is absent without an explicit disable"
    );

    // Declaring a supported disable mapping appends `none` at the canonical end.
    let mut with_disable = config.clone();
    let reasoning = with_disable.model_pools[0].candidates[0]
        .reasoning
        .as_mut()
        .expect("reasoning declaration");
    reasoning.supports_disable = true;
    reasoning.mappings.push(PoolReasoningMapping {
        effort: PoolReasoningEffort::None,
        action: PoolReasoningAction::Disable,
    });
    let (efforts, _, coverage) =
        reasoning_metadata(discovered_pool(&models_payload(&with_disable), "order-model"));
    assert_eq!(efforts, vec!["low", "medium", "high", "xhigh", "none"]);
    assert_eq!(coverage_for(&coverage, "none"), (1, 1));
}

/// AC-009: removing the final `high` declaration under an `auto` default stops
/// advertising `high` and drops its coverage to zero.
#[test]
fn pool_discovery_stops_advertising_removed_last_high_under_auto_default() {
    let mut before = gateway_config(19000);
    before.model_pools.push(reasoning_pool(
        "pool-drop",
        "drop-model",
        PoolDefaultReasoningEffort::Auto,
        vec![reasoning_candidate(
            ORDINARY_LOCAL,
            ORDINARY_UPSTREAM,
            Some(reasoning_with(
                &[(PoolReasoningEffort::High, enum_action("high"))],
                false,
            )),
        )],
    ));
    let (efforts, _, coverage) =
        reasoning_metadata(discovered_pool(&models_payload(&before), "drop-model"));
    assert_eq!(efforts, vec!["high"]);
    assert_eq!(coverage_for(&coverage, "high"), (1, 1));

    let mut after = gateway_config(19000);
    after.model_pools.push(reasoning_pool(
        "pool-drop",
        "drop-model",
        PoolDefaultReasoningEffort::Auto,
        vec![reasoning_candidate(
            ORDINARY_LOCAL,
            ORDINARY_UPSTREAM,
            Some(reasoning_with(&[], false)),
        )],
    ));
    let (efforts, _, coverage) =
        reasoning_metadata(discovered_pool(&models_payload(&after), "drop-model"));
    assert!(
        efforts.is_empty(),
        "no effort remains supported after the final declaration is removed: {efforts:?}"
    );
    assert_eq!(
        coverage_for(&coverage, "high"),
        (0, 1),
        "the removed declaration is not counted"
    );
}

/// AC-009: the OpenCode manual-sync pool row exposes canonical variants (the
/// preference as both the variant name and `reasoningEffort`). It never exposes
/// an `auto` variant, a native mapped value, a borrowed Responses npm override
/// or a fabricated price/credential field; an unmapped legacy pool keeps the
/// pre-feature shape with no variants.
#[tokio::test]
async fn opencode_pool_row_uses_canonical_variants_without_guesses() {
    let _home = isolated_temp_home("pool-opencode-reasoning");
    let mut config = gateway_config(19000);
    config.model_pools.push(reasoning_pool(
        "pool-open",
        "free-model",
        PoolDefaultReasoningEffort::High,
        vec![reasoning_candidate(
            ORDINARY_LOCAL,
            ORDINARY_UPSTREAM,
            Some(reasoning_with(
                &[
                    // A provider-specific native identifier must never become the
                    // public variant name or `reasoningEffort` value.
                    (PoolReasoningEffort::Low, enum_action("minimal")),
                    (PoolReasoningEffort::High, enum_action("high")),
                    (PoolReasoningEffort::None, PoolReasoningAction::Disable),
                ],
                true,
            )),
        )],
    ));
    config.model_pools.push(reasoning_pool(
        "pool-legacy",
        "legacy-model",
        PoolDefaultReasoningEffort::Auto,
        vec![pool_candidate(
            "g1",
            ORDINARY_LOCAL,
            ORDINARY_UPSTREAM,
            UpstreamProtocol::ChatCompletions,
            1,
        )],
    ));
    write_config(&config).expect("write gateway config");

    let payloads = capture_payloads(&json!({ "providers": [] }), vec!["opencode".to_string()]).await;
    let models = &payload_for(&payloads, "opencode")["tool_config"]["models"];

    let model = &models["free-model"];
    assert!(
        !model.is_null(),
        "the enabled reasoning pool must be exposed: {models}"
    );
    assert_eq!(
        model["variants"],
        json!({
            "low": { "reasoningEffort": "low" },
            "high": { "reasoningEffort": "high" },
            "none": { "reasoningEffort": "none" },
        }),
        "pool variants must use the canonical preference as name and reasoningEffort: {model}"
    );
    assert!(
        model["variants"].get("auto").is_none(),
        "auto is never a variant: {model}"
    );
    assert!(
        !model.to_string().contains("minimal"),
        "a native mapped value must never leak into the public variants: {model}"
    );
    assert!(
        model.get("provider").is_none(),
        "a Chat pool must not borrow the Responses npm override: {model}"
    );
    for forbidden in ["cost", "pricing", "apiKey", "balance"] {
        assert!(
            model.get(forbidden).is_none(),
            "a pool row must not fabricate '{forbidden}': {model}"
        );
    }

    let legacy = &models["legacy-model"];
    assert!(
        !legacy.is_null(),
        "a legacy pool stays exposed to OpenCode: {models}"
    );
    assert!(
        legacy.get("variants").is_none(),
        "an unmapped legacy pool keeps no reasoning variants: {legacy}"
    );
}

/// AC-012: adding and editing a pool reasoning policy leaves ordinary OpenCode
/// entries byte-for-byte identical, including native effort identifiers
/// (`max`/`minimal`) that are outside the canonical pool set.
#[tokio::test]
async fn ordinary_opencode_entries_stay_identical_across_pool_policy_edits() {
    let _home = isolated_temp_home("pool-opencode-isolation");

    let mut before = gateway_config(19000);
    before.providers[0].mappings[0].reasoning_efforts =
        vec!["max".to_string(), "minimal".to_string()];
    write_config(&before).expect("write baseline config");
    let baseline_payloads =
        capture_payloads(&json!({ "providers": [] }), vec!["opencode".to_string()]).await;
    let baseline_models =
        payload_for(&baseline_payloads, "opencode")["tool_config"]["models"].clone();
    assert_eq!(
        baseline_models[ORDINARY_LOCAL]["variants"],
        json!({
            "max": { "reasoningEffort": "max" },
            "minimal": { "reasoningEffort": "minimal" },
        }),
        "the ordinary row must keep its native effort identifiers: {baseline_models}"
    );

    let mut after = before.clone();
    after.model_pools.push(reasoning_pool(
        "pool-isolate",
        "free-model",
        PoolDefaultReasoningEffort::High,
        vec![reasoning_candidate(
            ORDINARY_LOCAL,
            ORDINARY_UPSTREAM,
            Some(reasoning_with(
                &[
                    (PoolReasoningEffort::Low, enum_action("minimal")),
                    (PoolReasoningEffort::High, enum_action("high")),
                ],
                false,
            )),
        )],
    ));
    write_config(&after).expect("write pool config");
    let pool_payloads =
        capture_payloads(&json!({ "providers": [] }), vec!["opencode".to_string()]).await;
    let pool_models = &payload_for(&pool_payloads, "opencode")["tool_config"]["models"];

    assert_eq!(
        pool_models[ORDINARY_LOCAL], baseline_models[ORDINARY_LOCAL],
        "an ordinary entry must be identical before and after a pool policy edit"
    );
    assert_eq!(
        pool_models["ds"], baseline_models["ds"],
        "every ordinary entry must be identical after a pool policy edit"
    );
    assert!(
        !pool_models["free-model"].is_null(),
        "the new pool is still exposed: {pool_models}"
    );
}

/// AC-009 / REQ-007: a pool policy edit that changes the generated catalog marks
/// the OpenCode target pending, so it waits for the existing manual sync and is
/// never projected automatically.
#[test]
fn pool_policy_edit_marks_opencode_pending_for_manual_sync() {
    let mut config = gateway_config(19000);
    config.default_key_id = Some("k1".to_string());
    config.model_pools.push(reasoning_pool(
        "pool-pending",
        "free-model",
        PoolDefaultReasoningEffort::High,
        vec![reasoning_candidate(
            ORDINARY_LOCAL,
            ORDINARY_UPSTREAM,
            Some(reasoning_with(
                &[(PoolReasoningEffort::High, enum_action("high"))],
                false,
            )),
        )],
    ));
    config.terminal_syncs.push(TerminalSyncRecord {
        provider_id: "gw-open".to_string(),
        tool: "opencode".to_string(),
        synced_key_id: "k1".to_string(),
        synced_base_url: local_base_url(config.port),
        synced_at: 1,
    });

    // The stored record already carries the pool alias, but with the pre-policy
    // catalog shape (no canonical variants), so only the policy edit drifts.
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
                "free-model": { "name": "Pool pool-pending" },
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
        "a pool reasoning policy edit must require the existing manual sync: {opencode:?}"
    );
}
