//! Step 3 behavior tests for explicit, reusable Claude terminal sync
//! (`20261011-ai-gateway-anthropic-messages`, REQ-006 / REQ-007;
//! AC-012 / AC-013 / AC-014 / AC-016).
//!
//! Boundary asserted here (the contract the Step 3 Backend must satisfy):
//!
//! - The existing terminal-sync pipeline (`apply_terminal_sync_with`, the
//!   injectable upsert seam behind `ai_gateway_configure_terminal` and
//!   `ai_gateway_sync_terminal`) accepts the `claude` target and, for it, builds
//!   a gateway-owned Claude *service-provider* payload instead of the
//!   OpenCode/Codex npm provider payload.
//! - That payload carries the tool-specific origin `http://127.0.0.1:<port>`
//!   WITHOUT a trailing `/v1`, the resolved default local key as `api_key`, the
//!   persisted default choice as `model`, the three family choices as
//!   `claude_model_mappings`, a UUID-derived `code`, and the profile gateway
//!   marker under `tool_config`. It never carries `active`/`is_active`.
//! - A later sync reads the saved selection back from the canonical marked
//!   Claude provider in the terminal service-provider list and resends it whole,
//!   so the existing app-store upsert (which replaces the record) cannot clear
//!   omitted choices. The ledger records the origin (no `/v1`) as
//!   `synced_base_url`.
//! - Ownership is the same-tool, same-profile marker rule already used for
//!   OpenCode/Codex: an unmarked user Claude provider is never claimed and its
//!   provider id is never reused.
//! - `terminal_targets_from` exposes a Claude target whose model-only selection
//!   snapshot carries the four model choices and NEVER a credential value; a
//!   current origin/key/selection is not pending.
//! - OpenCode keeps its existing Chat adapter for any Anthropic-servable local
//!   model (the Chat entry converts); only a genuinely Responses-only model gets
//!   the `@ai-sdk/openai` override. Codex configuration is unchanged.
//!
//! The tests deliberately use the existing seam signatures and raw JSON
//! fixtures so that, before Step 3 lands, they fail behaviorally (Claude is
//! rejected as an unsupported tool and no Claude target/selection exists)
//! rather than by a compiler error. No private collaborator is mocked and no
//! same-implementation algorithm is used as the expected value.

use super::{
    capture_terminal_sync, gateway_config, isolated_temp_home, mapping, target_for,
    upstream_provider,
};
use crate::ai_gateway::commands::{
    apply_terminal_sync_with, apply_terminal_sync_with_selection, build_gateway_provider,
    terminal_targets_from, TerminalSyncProfile, UpsertFuture,
};
use crate::ai_gateway::storage::{read_config, write_config};
use crate::ai_gateway::{ClaudeModelSelection, TerminalSyncRecord, UpstreamProtocol};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

const LOCAL_KEY: &str = "local-key-123";
const ORIGIN: &str = "http://127.0.0.1:19000";

/// A canonical, already-synced gateway-owned Claude service provider exactly as
/// it appears in `service_providers_list()`: the same-tool, same-profile gateway
/// marker plus the persisted `model` / `claude_model_mappings` selection. The
/// four model values are local mapping ids (the ids Claude Code sends), not
/// vendor ids.
fn saved_claude_provider(
    id: &str,
    marker: Value,
    default_model: &str,
    haiku: &str,
    sonnet: &str,
    opus: &str,
    code: &str,
) -> Value {
    json!({
        "id": id,
        "tool": "claude",
        "name": "AI Gateway",
        "base_url": ORIGIN,
        "api_key": "previous-local-key",
        "model": default_model,
        "claude_default_model": default_model,
        "code": code,
        "claude_api_format": "anthropic_messages",
        "claude_connection_mode": "native_anthropic",
        "claude_auth_env_key": "ANTHROPIC_API_KEY",
        "claude_model_mappings": [
            { "family": "haiku", "display_name": "Haiku", "upstream_model": haiku },
            { "family": "sonnet", "display_name": "Sonnet", "upstream_model": sonnet },
            { "family": "opus", "display_name": "Opus", "upstream_model": opus },
        ],
        "tool_config": { "ai_gateway_gateway": marker },
    })
}

/// A user-owned Claude provider with no AI Gateway marker. The stale-ledger and
/// ownership rules must never claim or overwrite it.
fn unmarked_user_claude_provider(id: &str, code: &str) -> Value {
    json!({
        "id": id,
        "tool": "claude",
        "name": "My Claude",
        "base_url": "https://user.example.com",
        "api_key": "user-secret-key",
        "code": code,
        "tool_config": { "env_managed": true }
    })
}

fn family_upstream(payload: &Value, family: &str) -> String {
    payload["claude_model_mappings"]
        .as_array()
        .unwrap_or_else(|| panic!("payload has no claude_model_mappings: {payload}"))
        .iter()
        .find(|row| row["family"] == family)
        .and_then(|row| row["upstream_model"].as_str())
        .unwrap_or_else(|| panic!("payload has no {family} mapping: {payload}"))
        .to_string()
}

/// AC-012 / AC-013 / AC-014: a Claude sync writes one gateway-owned Claude
/// service-provider payload carrying the origin (no `/v1`), the local key, the
/// saved default and family choices, the profile marker and a UUID-derived code,
/// and never an `active`/`is_active` flag.
#[tokio::test]
async fn claude_sync_writes_origin_model_selection_and_never_appends_v1() {
    let _home = isolated_temp_home("claude-sync-origin-selection");
    let config = claude_servable_gateway_config(19000);
    write_config(&config).expect("write gateway config");
    assert_eq!(super::super::storage::local_base_url(config.port), format!("{ORIGIN}/v1"));

    let providers_data = json!({ "providers": [saved_claude_provider(
        "gateway-claude",
        json!(true),
        "gpt-4o",
        "claude-haiku-local",
        "claude-sonnet-local",
        "claude-opus-local",
        "gateway-claude-code",
    )] });

    let (submitted, records) =
        capture_terminal_sync(&providers_data, vec!["claude".to_string()], TerminalSyncProfile::RELEASE)
            .await;

    assert_eq!(submitted.len(), 1, "one upsert for the claude target: {submitted:?}");
    let claude = &submitted[0];

    assert_eq!(claude["tool"], "claude");
    assert_eq!(claude["name"], "AI Gateway");
    assert_eq!(claude["base_url"], ORIGIN, "claude must use the bare origin: {claude}");
    assert!(
        !claude["base_url"].as_str().unwrap_or_default().contains("/v1"),
        "the claude origin must never carry a trailing /v1: {claude}"
    );
    assert_eq!(claude["api_key"], LOCAL_KEY);

    assert_eq!(claude["model"], "gpt-4o", "the saved default choice must be resent");
    assert_eq!(family_upstream(claude, "haiku"), "claude-haiku-local");
    assert_eq!(family_upstream(claude, "sonnet"), "claude-sonnet-local");
    assert_eq!(family_upstream(claude, "opus"), "claude-opus-local");

    assert_eq!(claude["code"], "gateway-claude-code", "the owned id's code must be reused");
    assert_eq!(claude["tool_config"]["ai_gateway_gateway"], true);
    assert!(claude.get("active").is_none(), "must never auto-activate: {claude}");
    assert!(claude.get("is_active").is_none(), "must never auto-activate: {claude}");

    assert_eq!(records.len(), 1);
    assert_eq!(records[0].tool, "claude");
    assert_eq!(records[0].provider_id, "gateway-claude", "the marked record id is reused");
    assert_eq!(records[0].synced_key_id, "k1");
    assert_eq!(
        records[0].synced_base_url, ORIGIN,
        "the claude ledger stores the bare origin: {records:?}"
    );
}

/// AC-013: a repeat sync reads the saved selection back from the canonical
/// provider and resends it whole, reusing the same provider id and code, so an
/// upsert that replaces the record cannot clear omitted choices.
#[tokio::test]
async fn claude_resync_preserves_saved_selection_and_reuses_identity() {
    let _home = isolated_temp_home("claude-resync-preserve");
    write_config(&claude_servable_gateway_config(19000)).expect("write gateway config");

    let saved = saved_claude_provider(
        "gateway-claude",
        json!(true),
        "gpt-4o",
        "claude-haiku-local",
        "claude-sonnet-local",
        "claude-opus-local",
        "gateway-claude-code",
    );
    let providers_data = json!({ "providers": [saved.clone()] });

    let (first, first_records) =
        capture_terminal_sync(&providers_data, vec!["claude".to_string()], TerminalSyncProfile::RELEASE)
            .await;
    let (second, second_records) =
        capture_terminal_sync(&providers_data, vec!["claude".to_string()], TerminalSyncProfile::RELEASE)
            .await;

    for payloads in [&first, &second] {
        let claude = &payloads[0];
        assert_eq!(claude["id"], "gateway-claude", "the owned provider id must be reused: {claude}");
        assert_eq!(claude["code"], "gateway-claude-code");
        assert_eq!(claude["model"], "gpt-4o");
        assert_eq!(family_upstream(claude, "haiku"), "claude-haiku-local");
        assert_eq!(family_upstream(claude, "sonnet"), "claude-sonnet-local");
        assert_eq!(family_upstream(claude, "opus"), "claude-opus-local");
    }
    assert_eq!(
        first[0]["claude_model_mappings"], second[0]["claude_model_mappings"],
        "a repeat sync must resend the identical complete selection"
    );
    assert_eq!(first_records[0].provider_id, second_records[0].provider_id);
}

/// AC-013: a ledger id that points at an unmarked user Claude provider is stale.
/// The marked gateway provider is reused instead and the user record is never
/// claimed or overwritten.
#[tokio::test]
async fn claude_sync_never_claims_an_unmarked_user_provider() {
    let _home = isolated_temp_home("claude-sync-ownership");
    let mut config = claude_servable_gateway_config(19000);
    config.terminal_syncs.push(TerminalSyncRecord {
        provider_id: "user-claude".to_string(),
        tool: "claude".to_string(),
        synced_key_id: "k-old".to_string(),
        synced_base_url: ORIGIN.to_string(),
        synced_at: 9,
    });
    write_config(&config).expect("write gateway config");

    let providers_data = json!({
        "providers": [
            unmarked_user_claude_provider("user-claude", "my-work"),
            saved_claude_provider(
                "gateway-claude",
                json!(true),
                "gpt-4o",
                "claude-haiku-local",
                "claude-sonnet-local",
                "claude-opus-local",
                "gateway-claude-code",
            ),
        ]
    });

    let (submitted, records) =
        capture_terminal_sync(&providers_data, vec!["claude".to_string()], TerminalSyncProfile::RELEASE)
            .await;

    assert_eq!(submitted.len(), 1);
    assert_eq!(
        submitted[0]["id"], "gateway-claude",
        "the marked gateway must be reused, never the user record: {submitted:?}"
    );
    assert_ne!(submitted[0]["code"], "my-work", "the user code must not be reused");
    assert_eq!(records[0].provider_id, "gateway-claude");
}

/// AC-012: a Claude sync with no saved/resolvable selection writes nothing
/// rather than persisting an empty provider.
#[tokio::test]
async fn claude_sync_without_any_selection_is_rejected_without_writing() {
    let _home = isolated_temp_home("claude-sync-no-selection");
    write_config(&gateway_config(19000)).expect("write gateway config");

    let calls = Arc::new(Mutex::new(0usize));
    let calls_for_upsert = calls.clone();
    let result = apply_terminal_sync_with(
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
    )
    .await;

    assert!(
        result.is_err(),
        "a Claude sync with no selection must be rejected, not silently written: {result:?}"
    );
    assert_eq!(*calls.lock().unwrap(), 0, "nothing may be upserted");
}

/// AC-012: no enabled local key rejects before any Claude write.
#[tokio::test]
async fn claude_sync_requires_an_enabled_local_key_before_writing() {
    let _home = isolated_temp_home("claude-sync-no-key");
    let mut config = gateway_config(19000);
    config.keys[0].enabled = false;
    write_config(&config).expect("write gateway config");

    let calls = Arc::new(Mutex::new(0usize));
    let calls_for_upsert = calls.clone();
    let error = apply_terminal_sync_with(
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
    )
    .await
    .unwrap_err();

    assert!(error.contains("local API key"), "error: {error}");
    assert_eq!(*calls.lock().unwrap(), 0, "nothing may be upserted");
}

/// AC-013 / AC-014: `terminal_targets_from` exposes a Claude target whose
/// model-only selection snapshot carries all four choices and never a credential
/// value; a current origin/key/selection is not pending.
#[test]
fn claude_target_snapshot_exposes_choices_and_never_credentials() {
    let mut config = gateway_config(19000);
    config.default_key_id = Some("k1".to_string());
    config.terminal_syncs.push(TerminalSyncRecord {
        provider_id: "gateway-claude".to_string(),
        tool: "claude".to_string(),
        synced_key_id: "k1".to_string(),
        synced_base_url: ORIGIN.to_string(),
        synced_at: 10,
    });
    let providers_data = json!({ "providers": [saved_claude_provider(
        "gateway-claude",
        json!(true),
        "gpt-4o",
        "claude-haiku-local",
        "claude-sonnet-local",
        "claude-opus-local",
        "gateway-claude-code",
    )] });

    let targets = terminal_targets_from(&config, &providers_data, TerminalSyncProfile::RELEASE);
    let claude = target_for(&targets, "claude");

    assert_eq!(claude.provider_id.as_deref(), Some("gateway-claude"));
    assert!(claude.synced, "the marked claude record is synced: {claude:?}");
    assert_eq!(claude.base_url.as_deref(), Some(ORIGIN));
    assert!(
        !claude.pending_sync,
        "a current claude origin/key/selection must not be pending: {claude:?}"
    );

    let serialized = serde_json::to_string(claude).expect("serialize claude target");
    for choice in [
        "gpt-4o",
        "claude-haiku-local",
        "claude-sonnet-local",
        "claude-opus-local",
    ] {
        assert!(
            serialized.contains(choice),
            "the model-only snapshot must carry choice {choice}: {serialized}"
        );
    }
    for secret in ["previous-local-key", "user-secret-key", "api_key"] {
        assert!(
            !serialized.contains(secret),
            "the target snapshot must never carry a credential ({secret}): {serialized}"
        );
    }
}

/// AC-013 / AC-014: the Claude target obeys the release/dev profile marker. A
/// dev-marked record is invisible to release and vice versa.
#[test]
fn claude_target_respects_the_profile_marker() {
    let config = gateway_config(19000);
    let providers_data = json!({ "providers": [saved_claude_provider(
        "claude-dev",
        json!("dev"),
        "gpt-4o",
        "claude-haiku-local",
        "claude-sonnet-local",
        "claude-opus-local",
        "gateway-dev-code",
    )] });

    let release = terminal_targets_from(&config, &providers_data, TerminalSyncProfile::RELEASE);
    let release_claude = target_for(&release, "claude");
    assert!(
        !release_claude.synced,
        "release must not claim a dev-marked claude record: {release_claude:?}"
    );

    let dev = terminal_targets_from(&config, &providers_data, TerminalSyncProfile::DEV);
    let dev_claude = target_for(&dev, "claude");
    assert!(dev_claude.synced, "dev must claim its own marked claude record: {dev_claude:?}");
    assert_eq!(dev_claude.provider_id.as_deref(), Some("claude-dev"));
}

/// AC-016: OpenCode keeps its existing Chat adapter for any Anthropic-servable
/// local model, and only a genuinely Responses-only model receives the
/// `@ai-sdk/openai` override.
#[test]
fn opencode_uses_chat_adapter_for_anthropic_servable_models() {
    let mut anthropic = upstream_provider(
        "g-anth",
        "Anthropic",
        "https://api.anthropic.example",
        "sk",
        None,
    );
    let mut anthropic_row = mapping("claude-sonnet-5-5", "commandcode/sonnet-5-5", Some("Sonnet"));
    anthropic_row.protocol = Some(UpstreamProtocol::AnthropicMessages);
    anthropic.mappings = vec![anthropic_row];

    let value = build_gateway_provider(
        "oc",
        "opencode",
        "http://127.0.0.1:19000",
        LOCAL_KEY,
        &[anthropic],
        TerminalSyncProfile::RELEASE,
    )
    .expect("opencode payload must build");
    let model = &value["tool_config"]["models"]["claude-sonnet-5-5"];
    assert!(
        model.get("provider").is_none(),
        "an Anthropic-servable model must keep the Chat adapter (no npm override): {model}"
    );

    let mut responses = upstream_provider(
        "g-resp",
        "Responses",
        "https://api.responses.example",
        "sk",
        None,
    );
    let mut responses_row = mapping("resp-only", "vendor-responses", Some("RespOnly"));
    responses_row.protocol = Some(UpstreamProtocol::Responses);
    responses.mappings = vec![responses_row];

    let value = build_gateway_provider(
        "oc",
        "opencode",
        "http://127.0.0.1:19000",
        LOCAL_KEY,
        &[responses],
        TerminalSyncProfile::RELEASE,
    )
    .expect("opencode payload must build");
    assert_eq!(
        value["tool_config"]["models"]["resp-only"]["provider"]["npm"], "@ai-sdk/openai",
        "a genuinely Responses-only model must keep the Responses override: {value}"
    );
}

/// REQ-006: Claude coexists with the existing terminal targets in one sync,
/// which leaves the OpenCode/Codex payload shapes unchanged.
#[tokio::test]
async fn claude_coexists_with_opencode_and_codex_in_one_sync() {
    let _home = isolated_temp_home("claude-sync-coexist");
    write_config(&claude_servable_gateway_config(19000)).expect("write gateway config");

    let providers_data = json!({ "providers": [saved_claude_provider(
        "gateway-claude",
        json!(true),
        "gpt-4o",
        "claude-haiku-local",
        "claude-sonnet-local",
        "claude-opus-local",
        "gateway-claude-code",
    )] });

    let (submitted, records) = capture_terminal_sync(
        &providers_data,
        vec!["opencode".to_string(), "codex".to_string(), "claude".to_string()],
        TerminalSyncProfile::RELEASE,
    )
    .await;

    assert_eq!(submitted.len(), 3, "one payload per tool: {submitted:?}");
    let by_tool = |tool: &str| {
        submitted
            .iter()
            .find(|value| value["tool"] == tool)
            .unwrap_or_else(|| panic!("missing payload for {tool}"))
    };
    assert_eq!(by_tool("opencode")["provider_key"], "gateway");
    assert_eq!(by_tool("codex")["tool_config"]["wire_api"], "chat");
    assert_eq!(by_tool("claude")["base_url"], ORIGIN);

    let mut tools: Vec<&str> = records.iter().map(|record| record.tool.as_str()).collect();
    tools.sort();
    assert_eq!(tools, vec!["claude", "codex", "opencode"]);
}

/// The dev profile's payload identity is distinct from release; a dev sync must
/// not overwrite or claim the release-owned record.
#[tokio::test]
async fn claude_dev_sync_uses_the_dev_marker_and_name() {
    let _home = isolated_temp_home("claude-sync-dev-identity");
    write_config(&claude_servable_gateway_config(19000)).expect("write gateway config");

    let providers_data = json!({ "providers": [saved_claude_provider(
        "claude-dev",
        json!("dev"),
        "gpt-4o",
        "claude-haiku-local",
        "claude-sonnet-local",
        "claude-opus-local",
        "gateway-dev-code",
    )] });

    let (submitted, _) =
        capture_terminal_sync(&providers_data, vec!["claude".to_string()], TerminalSyncProfile::DEV)
            .await;

    let claude = &submitted[0];
    assert_eq!(claude["name"], "AI Gateway (Dev)");
    assert_eq!(claude["tool_config"]["ai_gateway_gateway"], "dev");
    assert_eq!(claude["id"], "claude-dev");
}

// ---------------------------------------------------------------------------
// Review-repair round: R1-backend / R8 / R12
// ---------------------------------------------------------------------------

/// The four local mapping ids the saved Claude provider fixtures persist as
/// their selection (default + haiku/sonnet/opus), matching the arguments the
/// five read-back tests pass to `saved_claude_provider`.
const CLAUDE_FIXTURE_LOCAL_IDS: [&str; 4] = [
    "gpt-4o",
    "claude-haiku-local",
    "claude-sonnet-local",
    "claude-opus-local",
];

/// `gateway_config(port)` plus one enabled native Anthropic Messages upstream
/// serving `CLAUDE_FIXTURE_LOCAL_IDS`. The shared `gateway_config` stays
/// Chat-only, so the two counterweight fixtures (`gpt-4o`, `ds`) remain
/// unservable; only the five saved-selection read-back tests opt in here so the
/// `None` read-back servability validation accepts a complete-but-servable
/// saved selection.
fn claude_servable_gateway_config(port: u16) -> crate::ai_gateway::GatewayConfig {
    let mut config = gateway_config(port);
    let mut anthropic = upstream_provider(
        "g-anth-fixture",
        "Anthropic Fixture",
        "https://api.anthropic.example/v1",
        "sk-anthropic",
        None,
    );
    anthropic.protocol = UpstreamProtocol::AnthropicMessages;
    anthropic.mappings = CLAUDE_FIXTURE_LOCAL_IDS
        .iter()
        .map(|local_model| mapping(*local_model, &format!("vendor/{local_model}"), None))
        .collect();
    config.providers.push(anthropic);
    config
}

/// A native Anthropic Messages upstream that can serve the local mapping id
/// `claude-sonnet-local`.
fn anthropic_servable_gateway() -> crate::ai_gateway::GatewayUpstreamProvider {
    let mut anthropic = upstream_provider(
        "g-anth",
        "Anthropic",
        "https://api.anthropic.example/v1",
        "sk-anthropic",
        None,
    );
    anthropic.protocol = UpstreamProtocol::AnthropicMessages;
    let mut row = mapping("claude-sonnet-local", "commandcode/sonnet-5-5", Some("Sonnet"));
    row.protocol = Some(UpstreamProtocol::AnthropicMessages);
    anthropic.mappings = vec![row];
    anthropic
}

/// Run an explicit-selection Claude sync against the current stored config and
/// report `(rejected, upsert_call_count)`.
async fn claude_explicit_selection_outcome(
    selection: ClaudeModelSelection,
) -> (bool, usize) {
    let calls = Arc::new(Mutex::new(0usize));
    let calls_for_upsert = calls.clone();
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
    let call_count = *calls.lock().expect("call counter");
    (result.is_err(), call_count)
}

/// R1-backend / REQ-006: the `None` read-back path must validate servability.
/// A complete-but-unservable saved selection (its local ids are not enabled
/// native-Anthropic mappings) is rejected and writes no Claude provider or
/// ledger, exactly like an explicitly submitted invalid selection.
#[tokio::test]
async fn claude_none_readback_rejects_unservable_saved_selection_before_writing() {
    let _home = isolated_temp_home("claude-none-unservable");
    write_config(&gateway_config(19000)).expect("write gateway config");

    let providers_data = json!({ "providers": [saved_claude_provider(
        "gateway-claude",
        json!(true),
        "gpt-4o",
        "removed-haiku",
        "removed-sonnet",
        "removed-opus",
        "gateway-claude-code",
    )] });

    let calls = Arc::new(Mutex::new(0usize));
    let calls_for_upsert = calls.clone();
    let result = apply_terminal_sync_with_selection(
        &providers_data,
        move |_value| -> UpsertFuture {
            let calls = calls_for_upsert.clone();
            Box::pin(async move {
                *calls.lock().expect("call counter") += 1;
                Ok(())
            })
        },
        vec!["claude".to_string()],
        TerminalSyncProfile::RELEASE,
        None,
    )
    .await;

    assert!(
        result.is_err(),
        "an unservable saved selection must be rejected on the None read-back path: {result:?}"
    );
    assert_eq!(
        *calls.lock().unwrap(),
        0,
        "no Claude provider may be upserted for an unservable saved selection"
    );
    let stored = read_config().expect("read gateway config");
    assert!(
        !stored.terminal_syncs.iter().any(|record| record.tool == "claude"),
        "no Claude ledger may be written for an unservable saved selection: {:?}",
        stored.terminal_syncs
    );
}

/// R8 / REQ-006: a best-effort template-refresh fan-out over opencode, codex and
/// claude must not abort when the saved Claude selection became unservable. Only
/// Claude is skipped; opencode and codex are still upserted with no error.
#[tokio::test]
async fn template_refresh_skips_invalid_claude_and_still_upserts_opencode_codex() {
    let _home = isolated_temp_home("claude-fanout-skip");
    write_config(&gateway_config(19000)).expect("write gateway config");

    let providers_data = json!({ "providers": [saved_claude_provider(
        "gateway-claude",
        json!(true),
        "gpt-4o",
        "removed-haiku",
        "removed-sonnet",
        "removed-opus",
        "gateway-claude-code",
    )] });

    let captured: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = captured.clone();
    let records = apply_terminal_sync_with_selection(
        &providers_data,
        move |value| -> UpsertFuture {
            let sink = sink.clone();
            Box::pin(async move {
                sink.lock().expect("capture lock").push(value);
                Ok(())
            })
        },
        vec![
            "opencode".to_string(),
            "codex".to_string(),
            "claude".to_string(),
        ],
        TerminalSyncProfile::RELEASE,
        None,
    )
    .await
    .expect("a fan-out refresh must not abort because the saved Claude selection is unservable");

    let submitted = captured.lock().unwrap().clone();
    let tools: Vec<String> = submitted
        .iter()
        .filter_map(|value| value["tool"].as_str().map(str::to_string))
        .collect();
    assert!(
        tools.contains(&"opencode".to_string()),
        "opencode must still be upserted while Claude is skipped: {tools:?}"
    );
    assert!(
        tools.contains(&"codex".to_string()),
        "codex must still be upserted while Claude is skipped: {tools:?}"
    );
    assert!(
        !tools.contains(&"claude".to_string()),
        "the unservable Claude must be skipped, not written: {tools:?}"
    );
    assert!(
        !records.iter().any(|record| record.tool == "claude"),
        "no Claude ledger record may be written when Claude is skipped: {records:?}"
    );
}

/// R12 / REQ-006 / AC-012: a valid explicit four-choice selection (the same id
/// allowed in every slot) persists the exact local ids into `model` and the
/// three `claude_model_mappings` families plus a ledger record.
#[tokio::test]
async fn claude_explicit_selection_persists_valid_choices_and_allows_duplicates() {
    let _home = isolated_temp_home("claude-explicit-valid");
    let mut config = gateway_config(19000);
    config.providers.push(anthropic_servable_gateway());
    write_config(&config).expect("write gateway config");

    let selection = ClaudeModelSelection {
        default_model: "claude-sonnet-local".to_string(),
        haiku: "claude-sonnet-local".to_string(),
        sonnet: "claude-sonnet-local".to_string(),
        opus: "claude-sonnet-local".to_string(),
    };

    let captured: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = captured.clone();
    let records = apply_terminal_sync_with_selection(
        &json!({ "providers": [] }),
        move |value| -> UpsertFuture {
            let sink = sink.clone();
            Box::pin(async move {
                sink.lock().expect("capture lock").push(value);
                Ok(())
            })
        },
        vec!["claude".to_string()],
        TerminalSyncProfile::RELEASE,
        Some(selection),
    )
    .await
    .expect("a valid explicit selection must persist");

    let submitted = captured.lock().unwrap().clone();
    assert_eq!(submitted.len(), 1, "one Claude upsert: {submitted:?}");
    let claude = &submitted[0];
    assert_eq!(claude["model"], "claude-sonnet-local");
    assert_eq!(family_upstream(claude, "haiku"), "claude-sonnet-local");
    assert_eq!(family_upstream(claude, "sonnet"), "claude-sonnet-local");
    assert_eq!(family_upstream(claude, "opus"), "claude-sonnet-local");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].tool, "claude");
}

/// R12 / REQ-006 / AC-012: blank slots, unservable/disabled local ids and a
/// missing enabled local key all reject before any Claude write.
#[tokio::test]
async fn claude_explicit_selection_rejections_write_nothing() {
    let _home = isolated_temp_home("claude-explicit-reject");
    let mut config = gateway_config(19000);
    config.providers.push(anthropic_servable_gateway());
    write_config(&config).expect("write gateway config");

    let valid = |default_model: &str,
                 haiku: &str,
                 sonnet: &str,
                 opus: &str|
     -> ClaudeModelSelection {
        ClaudeModelSelection {
            default_model: default_model.to_string(),
            haiku: haiku.to_string(),
            sonnet: sonnet.to_string(),
            opus: opus.to_string(),
        }
    };

    // A blank slot (a complete-looking selection with whitespace) is rejected.
    let (rejected, calls) = claude_explicit_selection_outcome(valid(
        "claude-sonnet-local",
        "   ",
        "claude-sonnet-local",
        "claude-sonnet-local",
    ))
    .await;
    assert!(rejected, "a blank slot must be rejected");
    assert_eq!(calls, 0, "nothing may be written for a blank slot");

    // An unservable local id (a Chat-only mapping) is rejected.
    let (rejected, calls) = claude_explicit_selection_outcome(valid(
        "claude-sonnet-local",
        "gpt-4o",
        "claude-sonnet-local",
        "claude-sonnet-local",
    ))
    .await;
    assert!(rejected, "an unservable local id must be rejected");
    assert_eq!(calls, 0, "nothing may be written for an unservable local id");

    // A disabled native row is not servable and is rejected.
    {
        let mut disabled = gateway_config(19000);
        let mut gateway = anthropic_servable_gateway();
        gateway.mappings[0].enabled = false;
        disabled.providers.push(gateway);
        write_config(&disabled).expect("write disabled config");
    }
    let (rejected, calls) = claude_explicit_selection_outcome(valid(
        "claude-sonnet-local",
        "claude-sonnet-local",
        "claude-sonnet-local",
        "claude-sonnet-local",
    ))
    .await;
    assert!(rejected, "a disabled native row must not be servable");
    assert_eq!(calls, 0, "nothing may be written for a disabled row");

    // No enabled local key rejects before any Claude write.
    {
        let mut keyless = gateway_config(19000);
        keyless.keys[0].enabled = false;
        keyless.providers.push(anthropic_servable_gateway());
        write_config(&keyless).expect("write keyless config");
    }
    let (rejected, calls) = claude_explicit_selection_outcome(valid(
        "claude-sonnet-local",
        "claude-sonnet-local",
        "claude-sonnet-local",
        "claude-sonnet-local",
    ))
    .await;
    assert!(rejected, "a missing enabled local key must reject");
    assert_eq!(calls, 0, "nothing may be written without an enabled local key");
}
