//! Step 2 behavior tests: template data model and the in-app snapshot.
//!
//! These tests pin the public boundary of `api_gateway::templates` and the
//! snapshot JSON shape: `parse_template_snapshot` accepts a top-level JSON
//! array of templates, `builtin_templates` embeds `provider_templates.json`,
//! and `find_builtin_template` resolves an id or reports it in the error.
//! They are expected to fail (unresolved module/types/fields) until Step 2
//! lands.

use crate::api_gateway::templates::{
    apply_delete_provider_template, apply_reset_provider_templates, apply_upsert_provider_template,
    builtin_templates, find_builtin_template, parse_template_snapshot,
};
use crate::api_gateway::types_config::{
    GatewayConfig, OffPeakPrice, ProviderTemplateModel, UpstreamProtocol,
};
use std::collections::BTreeSet;

/// AC-001 / REQ-001: both built-in templates ship with models, use only a
/// gateway-servable protocol, and OpenCode Zen carries reasoning efforts.
#[test]
fn builtin_templates_contain_both_templates_with_models() {
    let templates = builtin_templates().expect("built-in templates must parse");

    let opencode = templates
        .iter()
        .find(|template| template.id == "opencode-zen")
        .expect("the opencode-zen template must exist");
    let commandcode = templates
        .iter()
        .find(|template| template.id == "commandcode")
        .expect("the commandcode template must exist");

    assert!(
        !opencode.models.is_empty(),
        "opencode-zen must ship at least one model"
    );
    assert!(
        !commandcode.models.is_empty(),
        "commandcode must ship at least one model"
    );

    // Only `chat_completions` / `responses` are servable; a model without its
    // own protocol inherits the template protocol.
    for template in [opencode, commandcode] {
        assert!(matches!(
            template.protocol,
            UpstreamProtocol::ChatCompletions | UpstreamProtocol::Responses
        ));
        for model in &template.models {
            let effective = model.protocol.unwrap_or(template.protocol);
            assert!(matches!(
                effective,
                UpstreamProtocol::ChatCompletions | UpstreamProtocol::Responses
            ));
        }
    }

    assert!(
        opencode
            .models
            .iter()
            .any(|model| !model.reasoning_efforts.is_empty()),
        "opencode-zen must ship at least one model with reasoning efforts"
    );
}

/// REQ-002 / REQ-008: CommandCode's curated DeepSeek model carries the exact
/// UTC+8 weekday and weekend off-peak windows (set comparison, order-agnostic).
#[test]
fn builtin_commandcode_deepseek_has_weekday_and_weekend_off_peak_windows() {
    let templates = builtin_templates().expect("built-in templates must parse");
    let commandcode = templates
        .iter()
        .find(|template| template.id == "commandcode")
        .expect("the commandcode template must exist");

    let deepseek: Vec<&ProviderTemplateModel> = commandcode
        .models
        .iter()
        .filter(|model| model.upstream_model.contains("deepseek"))
        .collect();
    assert!(
        !deepseek.is_empty(),
        "commandcode must ship at least one deepseek model"
    );
    assert!(
        deepseek
            .iter()
            .any(|model| !model.reasoning_efforts.is_empty()),
        "the deepseek model must carry reasoning efforts"
    );

    let expected: BTreeSet<(Option<Vec<u8>>, String, String)> = [
        (
            Some(vec![1, 2, 3, 4, 5]),
            "00:00".to_string(),
            "09:00".to_string(),
        ),
        (
            Some(vec![1, 2, 3, 4, 5]),
            "12:00".to_string(),
            "14:00".to_string(),
        ),
        (
            Some(vec![1, 2, 3, 4, 5]),
            "18:00".to_string(),
            "00:00".to_string(),
        ),
        (
            Some(vec![0, 6]),
            "00:00".to_string(),
            "09:00".to_string(),
        ),
        (
            Some(vec![0, 6]),
            "09:00".to_string(),
            "18:00".to_string(),
        ),
        (
            Some(vec![0, 6]),
            "18:00".to_string(),
            "00:00".to_string(),
        ),
    ]
    .into_iter()
    .collect();

    let matched = deepseek.iter().any(|model| {
        !model.reasoning_efforts.is_empty() && off_peak_set(&model.off_peaks) == expected
    });
    assert!(
        matched,
        "a deepseek model must ship exactly the weekday [1,2,3,4,5] and weekend [0,6] UTC+8 off-peak windows"
    );
}

fn off_peak_set(off_peaks: &[OffPeakPrice]) -> BTreeSet<(Option<Vec<u8>>, String, String)> {
    off_peaks
        .iter()
        .map(|window| {
            (
                window.days.clone(),
                window.start_time.clone(),
                window.end_time.clone(),
            )
        })
        .collect()
}

/// Numeric boundary: negative and infinite prices are treated as missing (0.0)
/// and must not fail the whole snapshot.
#[test]
fn parse_template_snapshot_normalizes_invalid_prices_to_missing() {
    let raw = r#"[
        {
            "id": "t",
            "name": "T",
            "base_url": "https://example.com/v1",
            "models": [
                { "upstream_model": "m", "input": -3, "output": 1e999 }
            ]
        }
    ]"#;

    let templates =
        parse_template_snapshot(raw).expect("invalid prices must be non-fatal, not an error");
    assert_eq!(templates.len(), 1);
    let model = &templates[0].models[0];
    assert_eq!(model.input, 0.0, "a negative input price is missing");
    assert_eq!(model.output, 0.0, "an infinite output price is missing");
    assert_eq!(model.cache_read, 0.0);
    assert_eq!(model.cache_write, 0.0);
}

/// Snapshot boundary: duplicate `upstream_model` entries are deduplicated
/// keeping the first.
#[test]
fn parse_template_snapshot_deduplicates_models_keeping_first() {
    let raw = r#"[
        {
            "id": "t",
            "name": "T",
            "base_url": "https://example.com/v1",
            "models": [
                { "upstream_model": "m", "display_name": "First" },
                { "upstream_model": "m", "display_name": "Second" }
            ]
        }
    ]"#;

    let templates = parse_template_snapshot(raw).expect("duplicate models must be deduplicated");
    assert_eq!(templates[0].models.len(), 1);
    assert_eq!(
        templates[0].models[0].display_name.as_deref(),
        Some("First"),
        "the first occurrence of an upstream_model must win"
    );
}

/// Counterexample: a model that only supports `/messages` and a model without
/// an identifier are dropped without failing the whole snapshot.
#[test]
fn parse_template_snapshot_drops_models_with_unknown_protocol_and_empty_identifier() {
    let raw = r#"[
        {
            "id": "t",
            "name": "T",
            "base_url": "https://example.com/v1",
            "models": [
                { "upstream_model": "m-messages", "protocol": "messages" },
                { "upstream_model": "", "protocol": "chat_completions" },
                { "upstream_model": "m-ok", "protocol": "responses" }
            ]
        }
    ]"#;

    let templates = parse_template_snapshot(raw)
        .expect("unknown protocols and empty identifiers must be non-fatal");
    assert_eq!(templates.len(), 1);
    let ids: Vec<&str> = templates[0]
        .models
        .iter()
        .map(|model| model.upstream_model.as_str())
        .collect();
    assert_eq!(ids, vec!["m-ok"], "only the servable model must remain");
    assert_eq!(
        templates[0].models[0].protocol,
        Some(UpstreamProtocol::Responses)
    );
}

/// Snapshot boundary: an empty or duplicate template id rejects the document
/// with a readable error.
#[test]
fn parse_template_snapshot_rejects_empty_or_duplicate_template_ids() {
    let empty_id = r#"[
        { "id": "", "name": "T", "base_url": "https://example.com/v1", "models": [] }
    ]"#;
    let error = parse_template_snapshot(empty_id).expect_err("an empty template id must be rejected");
    assert!(
        !error.trim().is_empty(),
        "rejecting an empty id must report a readable message"
    );

    let duplicate = r#"[
        { "id": "t", "name": "A", "base_url": "https://a.example.com", "models": [] },
        { "id": "t", "name": "B", "base_url": "https://b.example.com", "models": [] }
    ]"#;
    let error =
        parse_template_snapshot(duplicate).expect_err("a duplicate template id must be rejected");
    assert!(
        !error.trim().is_empty(),
        "rejecting a duplicate id must report a readable message"
    );
}

/// Weekday boundary: out-of-range and duplicate days normalize to a sorted,
/// deduplicated set; an all-invalid set collapses to `None` (every day).
#[test]
fn parse_template_snapshot_normalizes_weekdays() {
    let raw = r#"[
        {
            "id": "t",
            "name": "T",
            "base_url": "https://example.com/v1",
            "models": [
                {
                    "upstream_model": "m",
                    "off_peaks": [
                        { "start_time": "00:00", "end_time": "09:00", "days": [5, 1, 1, 9] },
                        { "start_time": "10:00", "end_time": "11:00", "days": [7, 9] }
                    ]
                }
            ]
        }
    ]"#;

    let templates = parse_template_snapshot(raw).expect("weekdays must normalize");
    let off_peaks = &templates[0].models[0].off_peaks;
    assert_eq!(off_peaks.len(), 2);
    assert_eq!(
        off_peaks[0].days,
        Some(vec![1, 5]),
        "[5,1,1,9] must drop 9, deduplicate and sort"
    );
    assert_eq!(off_peaks[1].days, None, "an all-invalid set means every day");
}

/// Snapshot boundary: an unknown template id reports the id in an actionable
/// error while both known ids resolve.
#[test]
fn find_builtin_template_unknown_id_reports_error() {
    let error = find_builtin_template("no-such-template-xyz")
        .expect_err("an unknown template id must be an error");
    assert!(
        error.contains("no-such-template-xyz"),
        "the error must name the requested id: {error}"
    );

    let opencode = find_builtin_template("opencode-zen").expect("opencode-zen must resolve");
    assert_eq!(opencode.id, "opencode-zen");
    let commandcode = find_builtin_template("commandcode").expect("commandcode must resolve");
    assert_eq!(commandcode.id, "commandcode");
}

/// Compatibility: an old `api_gateway.json` without any new field deserializes
/// with serde defaults and survives a serialization round trip unchanged.
#[test]
fn legacy_gateway_config_without_new_fields_deserializes() {
    let legacy = serde_json::json!({
        "enabled": true,
        "port": 17688,
        "providers": [{
            "id": "p1",
            "name": "Provider One",
            "base_url": "https://api.example.com/v1",
            "api_key": "sk-test",
            "default_model": "remote-default",
            "protocol": "chat_completions",
            "mappings": [{
                "local_model": "local-a",
                "upstream_model": "remote-a"
            }]
        }],
        "keys": [{
            "id": "k1",
            "label": "k1",
            "value": "local-key",
            "enabled": true,
            "created_at": 1
        }],
        "default_key_id": "k1",
        "terminal_syncs": [],
        "usage_retention_days": 30,
        "model_prices": [{
            "upstream_model": "remote-a",
            "input": 1.0,
            "cache_read": 2.0,
            "cache_write": 3.0,
            "output": 4.0
        }]
    });

    let config: GatewayConfig =
        serde_json::from_value(legacy).expect("a legacy config must deserialize");
    assert!(
        config.provider_templates.is_empty(),
        "provider_templates must default to empty"
    );
    let provider = &config.providers[0];
    assert_eq!(provider.template_id, None);
    assert!(provider.ignored_models.is_empty());
    assert!(provider.mappings[0].reasoning_efforts.is_empty());

    let encoded = serde_json::to_value(&config).expect("encode config");
    let decoded: GatewayConfig = serde_json::from_value(encoded).expect("round trip config");
    assert!(decoded.enabled);
    assert_eq!(decoded.port, 17688);
    assert_eq!(decoded.providers[0].id, "p1");
    assert_eq!(decoded.providers[0].mappings[0].local_model, "local-a");
    assert_eq!(decoded.providers[0].mappings[0].upstream_model, "remote-a");
    assert_eq!(decoded.default_key_id.as_deref(), Some("k1"));
    assert_eq!(decoded.usage_retention_days, 30);
    assert_eq!(decoded.model_prices.len(), 1);
    assert_eq!(decoded.model_prices[0].input, 1.0);
}

// ---------------------------------------------------------------------------
// Step 3: template query, sync and incremental propagation commands
// ---------------------------------------------------------------------------
//
// These tests pin the Step 3 public boundary (`provider_template_views`,
// `apply_template_sync_with`, `ProviderTemplateView`) and the official-source
// payload shapes. They are expected to fail (unresolved import / missing
// function / missing field) until Step 3 lands.

use crate::api_gateway::templates::{
    apply_template_sync_with, provider_template_views, ProviderTemplateView,
};
use crate::api_gateway::types_config::{
    GatewayUpstreamProvider, ModelMapping, ModelPrice, ProviderTemplate, ProviderTemplateState,
};
use crate::api_gateway::{compute_cost_at_time, UsageTokens};
use serde_json::{json, Value};

/// models.dev payload shape: one provider object with `models` keyed by id.
fn models_dev_body(models: Value) -> String {
    json!({
        "id": "opencode",
        "name": "OpenCode Zen",
        "api": "https://opencode.ai/zen/v1",
        "models": models,
    })
    .to_string()
}

/// CommandCode payload shape: an OpenAI-style list object.
fn commandcode_body(entries: Value) -> String {
    json!({ "object": "list", "data": entries }).to_string()
}

fn bound_provider(id: &str, template_id: &str) -> GatewayUpstreamProvider {
    GatewayUpstreamProvider {
        id: id.to_string(),
        name: format!("Provider {id}"),
        base_url: "https://api.example.com/v1".to_string(),
        api_key: "sk-test".to_string(),
        template_id: Some(template_id.to_string()),
        ..GatewayUpstreamProvider::default()
    }
}

fn opencode_snapshot() -> ProviderTemplate {
    find_builtin_template("opencode-zen").expect("the opencode-zen snapshot must parse")
}

fn snapshot_model<'a>(
    template: &'a ProviderTemplate,
    upstream_model: &str,
) -> &'a ProviderTemplateModel {
    template
        .models
        .iter()
        .find(|model| model.upstream_model == upstream_model)
        .unwrap_or_else(|| panic!("the snapshot must contain the model {upstream_model}"))
}

/// A mapping exactly as template creation writes it, so the sync sees the
/// current template values as untouched.
fn mapping_from_snapshot(
    template: &ProviderTemplate,
    model: &ProviderTemplateModel,
) -> ModelMapping {
    ModelMapping {
        local_model: model.upstream_model.clone(),
        upstream_model: model.upstream_model.clone(),
        enabled: true,
        protocol: Some(model.protocol.unwrap_or(template.protocol)),
        display_name: model.display_name.clone(),
        reasoning_efforts: model.reasoning_efforts.clone(),
    }
}

fn price_from_snapshot(provider_id: &str, model: &ProviderTemplateModel) -> ModelPrice {
    ModelPrice {
        provider_id: Some(provider_id.to_string()),
        upstream_model: model.upstream_model.clone(),
        input: model.input,
        cache_read: model.cache_read,
        cache_write: model.cache_write,
        output: model.output,
        off_peaks: model.off_peaks.clone(),
        off_peak: None,
    }
}

fn find_mapping<'a>(
    provider: &'a GatewayUpstreamProvider,
    upstream_model: &str,
) -> Option<&'a ModelMapping> {
    provider
        .mappings
        .iter()
        .find(|mapping| mapping.upstream_model == upstream_model)
}

fn find_price_row<'a>(
    config: &'a GatewayConfig,
    provider_id: &str,
    upstream_model: &str,
) -> Option<&'a ModelPrice> {
    config.model_prices.iter().find(|row| {
        row.provider_id.as_deref() == Some(provider_id) && row.upstream_model == upstream_model
    })
}

fn provider_json(config: &GatewayConfig, id: &str) -> Value {
    let provider = config
        .providers
        .iter()
        .find(|provider| provider.id == id)
        .unwrap_or_else(|| panic!("provider {id} must exist"));
    serde_json::to_value(provider).expect("serialize provider")
}

/// REQ-002 / AC-003: before any sync every template view falls back to the
/// built-in snapshot in snapshot order, with models and a source but no
/// timestamp.
#[test]
fn provider_template_views_fall_back_to_snapshot_before_any_sync() {
    let config = GatewayConfig::default();
    let views = provider_template_views(&config).expect("built-in templates must resolve");

    assert_eq!(views.len(), 2, "both built-in templates must be returned");
    assert_eq!(views[0].template.id, "opencode-zen");
    assert_eq!(views[1].template.id, "commandcode");

    for view in &views {
        assert!(
            view.from_snapshot,
            "an unsynced template must report the snapshot fallback"
        );
        assert_eq!(view.synced_at, None, "a snapshot view has no sync timestamp");
        assert!(
            !view.source.trim().is_empty(),
            "each view must carry a non-empty source"
        );
        assert!(
            !view.template.models.is_empty(),
            "each template must expose its models"
        );
    }
}

/// REQ-002 / AC-003 / REQ-006: a successful sync replaces the model list and
/// prices with the source values, records `synced_at`/`source`, and the query
/// command then returns the persisted state instead of the snapshot.
#[test]
fn sync_refreshes_template_models_and_prices_from_source() {
    let mut config = GatewayConfig::default();
    let body = models_dev_body(json!({
        "model-a": {
            "name": "Model A",
            "cost": {
                "input": 1.0,
                "cache_read": 0.5,
                "cache_write": 0.25,
                "output": 2.0
            },
            "reasoning_options": [{"type": "effort", "values": ["low", "high"]}]
        },
        "model-n": {"name": "Model N"}
    }));

    let view: ProviderTemplateView = apply_template_sync_with(
        &mut config,
        "opencode-zen",
        |_current| Ok(body.clone()),
        |_next| Ok(()),
    )
    .expect("a well-formed source must refresh the template");

    assert_eq!(view.template.id, "opencode-zen");
    assert!(
        !view.from_snapshot,
        "a synced view must not report the snapshot fallback"
    );
    assert!(view.synced_at.is_some(), "a synced view must carry a timestamp");
    assert!(!view.source.trim().is_empty(), "a synced view must carry the source");

    let mut ids: Vec<&str> = view
        .template
        .models
        .iter()
        .map(|model| model.upstream_model.as_str())
        .collect();
    ids.sort_unstable();
    assert_eq!(ids, vec!["model-a", "model-n"], "only the source models remain");

    let model_a = view
        .template
        .models
        .iter()
        .find(|model| model.upstream_model == "model-a")
        .expect("model-a must be present");
    assert_eq!(model_a.display_name.as_deref(), Some("Model A"));
    assert_eq!(model_a.input, 1.0);
    assert_eq!(model_a.cache_read, 0.5);
    assert_eq!(model_a.cache_write, 0.25);
    assert_eq!(model_a.output, 2.0);
    assert_eq!(model_a.reasoning_efforts, vec!["low", "high"]);

    let state: &ProviderTemplateState = config
        .provider_templates
        .iter()
        .find(|state| state.template_id == "opencode-zen")
        .expect("a successful sync must persist the template state");
    assert!(state.template.is_some());
    assert!(state.synced_at.is_some());

    let views = provider_template_views(&config).expect("persisted views must resolve");
    let persisted = views
        .iter()
        .find(|candidate| candidate.template.id == "opencode-zen")
        .expect("the opencode-zen view must exist");
    assert!(
        !persisted.from_snapshot,
        "a synced template must not fall back to the snapshot"
    );
    assert_eq!(persisted.synced_at, view.synced_at);
    assert_eq!(persisted.template, view.template);

    let commandcode = views
        .iter()
        .find(|candidate| candidate.template.id == "commandcode")
        .expect("the commandcode view must exist");
    assert!(
        commandcode.from_snapshot,
        "an unsynced template still falls back to its snapshot"
    );
}

/// REQ-007 / AC-011: a new official model reaches every provider bound to that
/// template, each with its own provider-scoped price row, while a manual
/// provider and a provider bound to another template stay byte-for-byte
/// unchanged.
#[test]
fn sync_propagates_new_official_model_to_all_derived_providers_only() {
    let snapshot = opencode_snapshot();
    let existing = snapshot_model(&snapshot, "deepseek-v4-flash");

    let mut config = GatewayConfig::default();

    for id in ["derived-a", "derived-b"] {
        let mut provider = bound_provider(id, "opencode-zen");
        provider.mappings = vec![mapping_from_snapshot(&snapshot, existing)];
        config.providers.push(provider);
    }

    let manual = GatewayUpstreamProvider {
        id: "manual".to_string(),
        name: "Manual".to_string(),
        base_url: "https://manual.example.com/v1".to_string(),
        api_key: "sk-manual".to_string(),
        mappings: vec![ModelMapping {
            local_model: "local-manual".to_string(),
            upstream_model: "remote-manual".to_string(),
            enabled: true,
            protocol: Some(UpstreamProtocol::Responses),
            display_name: Some("Manual Model".to_string()),
            reasoning_efforts: vec!["low".to_string()],
        }],
        ..GatewayUpstreamProvider::default()
    };
    config.providers.push(manual);
    config.model_prices.push(ModelPrice {
        provider_id: Some("manual".to_string()),
        upstream_model: "remote-manual".to_string(),
        input: 7.0,
        cache_read: 0.7,
        cache_write: 0.0,
        output: 70.0,
        off_peaks: Vec::new(),
        off_peak: None,
    });

    let mut other_template = bound_provider("cc", "commandcode");
    other_template.mappings = vec![ModelMapping {
        local_model: "ds".to_string(),
        upstream_model: "deepseek/deepseek-v4-pro".to_string(),
        enabled: true,
        protocol: Some(UpstreamProtocol::ChatCompletions),
        display_name: Some("DeepSeek".to_string()),
        reasoning_efforts: vec!["low".to_string()],
    }];
    config.providers.push(other_template);

    let manual_before = provider_json(&config, "manual");
    let other_template_before = provider_json(&config, "cc");

    let body = models_dev_body(json!({
        "deepseek-v4-flash": {
            "name": "DeepSeek V4 Flash",
            "cost": {
                "input": existing.input,
                "cache_read": existing.cache_read,
                "cache_write": existing.cache_write,
                "output": existing.output
            },
            "reasoning_options": [
                {"type": "effort", "values": existing.reasoning_efforts.clone()}
            ]
        },
        "new-model": {
            "name": "New Model",
            "cost": {"input": 3.0, "cache_read": 0.3, "cache_write": 0.0, "output": 15.0},
            "reasoning_options": [{"type": "effort", "values": ["low", "high"]}]
        }
    }));

    apply_template_sync_with(
        &mut config,
        "opencode-zen",
        |_current| Ok(body.clone()),
        |_next| Ok(()),
    )
    .expect("the sync must propagate the new model");

    for provider_id in ["derived-a", "derived-b"] {
        let provider = config
            .providers
            .iter()
            .find(|candidate| candidate.id == provider_id)
            .expect("the derived provider must exist");
        let mapping = find_mapping(provider, "new-model")
            .unwrap_or_else(|| panic!("{provider_id} must receive the new model"));
        assert!(mapping.enabled, "a propagated new model is enabled");
        assert_eq!(mapping.local_model, "new-model");
        assert_eq!(mapping.upstream_model, "new-model");
        assert_eq!(mapping.display_name.as_deref(), Some("New Model"));
        assert_eq!(mapping.protocol, Some(UpstreamProtocol::ChatCompletions));
        assert_eq!(mapping.reasoning_efforts, vec!["low", "high"]);

        let row = find_price_row(&config, provider_id, "new-model")
            .unwrap_or_else(|| panic!("{provider_id} must receive the new model price"));
        assert_eq!(row.input, 3.0);
        assert_eq!(row.cache_read, 0.3);
        assert_eq!(row.cache_write, 0.0);
        assert_eq!(row.output, 15.0);
        assert!(row.off_peaks.is_empty());
    }

    assert_eq!(
        provider_json(&config, "manual"),
        manual_before,
        "a manual provider must not change"
    );
    assert_eq!(
        provider_json(&config, "cc"),
        other_template_before,
        "a provider bound to another template must not change"
    );
}

/// REQ-005 / REQ-006 / AC-008 / AC-009 / AC-010: user intent — disabled,
/// deleted, hand-edited mapping/price and provider fields — survives a sync,
/// while untouched values update; a model the source retired keeps its mapping.
#[test]
fn sync_preserves_user_disable_delete_and_hand_edits() {
    let snapshot = opencode_snapshot();
    let model_a = snapshot_model(&snapshot, "deepseek-v4-flash");
    let model_b = snapshot_model(&snapshot, "deepseek-v4-pro");
    let model_c = snapshot_model(&snapshot, "claude-opus-4-5");
    let model_d = snapshot_model(&snapshot, "claude-sonnet-4-5");
    let model_e = snapshot_model(&snapshot, "gemini-3-pro");

    let mut provider = bound_provider("bound", "opencode-zen");
    provider.name = "My OpenCode".to_string();
    provider.base_url = "https://custom.example.com/v1".to_string();
    provider.protocol = UpstreamProtocol::Responses;

    let mut mapping_a = mapping_from_snapshot(&snapshot, model_a);
    mapping_a.enabled = false;

    let mut mapping_c = mapping_from_snapshot(&snapshot, model_c);
    mapping_c.display_name = Some("My Custom Name".to_string());
    mapping_c.reasoning_efforts = vec!["custom-a".to_string(), "custom-b".to_string()];

    provider.mappings = vec![
        mapping_a,
        mapping_c,
        mapping_from_snapshot(&snapshot, model_d),
        mapping_from_snapshot(&snapshot, model_e),
    ];
    provider.ignored_models = vec![model_b.upstream_model.clone()];

    let mut price_a = price_from_snapshot("bound", model_a);
    price_a.input = 99.0;

    let mut config = GatewayConfig::default();
    config.providers.push(provider);
    config.model_prices.push(price_a);
    config.model_prices.push(price_from_snapshot("bound", model_c));
    config.model_prices.push(price_from_snapshot("bound", model_d));
    config.model_prices.push(price_from_snapshot("bound", model_e));

    let body = models_dev_body(json!({
        "deepseek-v4-flash": {
            "name": "Source Flash",
            "cost": {"input": 2.0, "cache_read": 0.2, "cache_write": 0.0, "output": 4.0},
            "reasoning_options": [{"type": "effort", "values": ["low", "high"]}]
        },
        "deepseek-v4-pro": {
            "name": "Source Pro",
            "cost": {"input": 3.0, "cache_read": 0.3, "cache_write": 0.0, "output": 6.0},
            "reasoning_options": [{"type": "effort", "values": ["low"]}]
        },
        "claude-opus-4-5": {
            "name": "Source C",
            "cost": {"input": 9.0, "cache_read": 0.9, "cache_write": 1.0, "output": 18.0},
            "reasoning_options": [{"type": "effort", "values": ["medium"]}]
        },
        "claude-sonnet-4-5": {
            "name": "Source D",
            "cost": {"input": 4.0, "cache_read": 0.4, "cache_write": 0.5, "output": 20.0},
            "reasoning_options": [{"type": "effort", "values": ["low", "high"]}]
        }
    }));

    let view = apply_template_sync_with(
        &mut config,
        "opencode-zen",
        |_current| Ok(body.clone()),
        |_next| Ok(()),
    )
    .expect("the sync must preserve user intent");

    let provider = config
        .providers
        .iter()
        .find(|candidate| candidate.id == "bound")
        .expect("the bound provider must exist");

    // a: disabled mapping and hand-edited price survive.
    let mapping_a = find_mapping(provider, "deepseek-v4-flash").expect("model-a mapping");
    assert!(!mapping_a.enabled, "a manually disabled mapping must stay disabled");
    let price_a = find_price_row(&config, "bound", "deepseek-v4-flash").expect("model-a price");
    assert_eq!(price_a.input, 99.0, "a hand-edited price must not be overwritten");

    // b: ignored (deleted) must not be resurrected.
    assert!(
        find_mapping(provider, "deepseek-v4-pro").is_none(),
        "an ignored model must not be re-added"
    );

    // c: hand-edited display name survives.
    let mapping_c = find_mapping(provider, "claude-opus-4-5").expect("model-c mapping");
    assert_eq!(
        mapping_c.display_name.as_deref(),
        Some("My Custom Name"),
        "a hand-edited display name must not be overwritten"
    );
    assert_eq!(
        mapping_c.reasoning_efforts,
        vec!["custom-a", "custom-b"],
        "hand-edited reasoning efforts must not be overwritten"
    );

    // d: untouched mapping and price update to the source.
    let mapping_d = find_mapping(provider, "claude-sonnet-4-5").expect("model-d mapping");
    assert_eq!(mapping_d.display_name.as_deref(), Some("Source D"));
    assert_eq!(mapping_d.reasoning_efforts, vec!["low", "high"]);
    let price_d = find_price_row(&config, "bound", "claude-sonnet-4-5").expect("model-d price");
    assert_eq!(price_d.input, 4.0);
    assert_eq!(price_d.cache_read, 0.4);
    assert_eq!(price_d.cache_write, 0.5);
    assert_eq!(price_d.output, 20.0);

    // e: retired model keeps its mapping while the template drops it.
    assert!(
        find_mapping(provider, "gemini-3-pro").is_some(),
        "a retired model's mapping must be kept"
    );
    assert!(
        view.template
            .models
            .iter()
            .all(|model| model.upstream_model != "gemini-3-pro"),
        "the template must drop the model the source no longer offers"
    );

    // Provider-level hand edits survive.
    assert_eq!(provider.name, "My OpenCode");
    assert_eq!(provider.base_url, "https://custom.example.com/v1");
    assert_eq!(provider.protocol, UpstreamProtocol::Responses);
}

fn config_for_failure(template_id: &str) -> GatewayConfig {
    let mut config = GatewayConfig::default();
    let mut provider = bound_provider("p1", template_id);
    provider.mappings = vec![ModelMapping {
        local_model: "local-a".to_string(),
        upstream_model: "remote-a".to_string(),
        enabled: true,
        protocol: Some(UpstreamProtocol::ChatCompletions),
        display_name: Some("Remote A".to_string()),
        reasoning_efforts: vec!["low".to_string()],
    }];
    config.providers.push(provider);
    config.model_prices.push(ModelPrice {
        provider_id: Some("p1".to_string()),
        upstream_model: "remote-a".to_string(),
        input: 1.0,
        cache_read: 0.1,
        cache_write: 0.0,
        output: 2.0,
        off_peaks: Vec::new(),
        off_peak: None,
    });
    config
}

fn assert_sync_fatal_writes_nothing(
    template_id: &str,
    fetch: impl FnOnce(&ProviderTemplate) -> Result<String, String>,
    expected_source: &str,
) {
    let mut config = config_for_failure(template_id);
    let before = serde_json::to_value(&config).expect("encode config before");
    let error = apply_template_sync_with(&mut config, template_id, fetch, |_| Ok(()))
        .expect_err("a fatal source must fail the sync");
    assert!(
        error.contains(expected_source),
        "the error must name the source {expected_source}: {error}"
    );
    let after = serde_json::to_value(&config).expect("encode config after");
    assert_eq!(before, after, "a fatal sync must write nothing");
}

/// REQ-002 / AC-004 + error boundary: each fatal source (fetch error, non-JSON,
/// missing model array, empty model set, missing `data`, entry without an id,
/// only `/messages` entries) fails with a source-naming error and leaves the
/// configuration field-for-field identical.
#[test]
fn sync_failure_writes_nothing() {
    assert_sync_fatal_writes_nothing(
        "opencode-zen",
        |_current| Err("connection refused".to_string()),
        "models.dev",
    );
    assert_sync_fatal_writes_nothing(
        "opencode-zen",
        |_current| Ok("this is not json".to_string()),
        "models.dev",
    );
    assert_sync_fatal_writes_nothing(
        "opencode-zen",
        |_current| {
            Ok(
                json!({"id": "opencode", "name": "OpenCode Zen", "api": "https://opencode.ai/zen/v1"})
                    .to_string(),
            )
        },
        "models.dev",
    );
    assert_sync_fatal_writes_nothing(
        "opencode-zen",
        |_current| Ok(json!({"id": "opencode", "name": "OpenCode Zen", "models": {}}).to_string()),
        "models.dev",
    );
    assert_sync_fatal_writes_nothing(
        "commandcode",
        |_current| Ok(json!({"object": "list"}).to_string()),
        "commandcode",
    );
    assert_sync_fatal_writes_nothing(
        "commandcode",
        |_current| Ok(json!({"object": "list", "data": [{"name": "No Id"}]}).to_string()),
        "commandcode",
    );
    assert_sync_fatal_writes_nothing(
        "commandcode",
        |_current| {
            Ok(json!({
                "object": "list",
                "data": [{
                    "id": "only-messages",
                    "name": "Only Messages",
                    "supported_endpoints": ["/messages"]
                }]
            })
            .to_string())
        },
        "commandcode",
    );
}

/// Error boundary: negative and infinite source prices are non-fatal and are
/// written as the missing value `0.0`.
#[test]
fn sync_treats_invalid_source_numbers_as_missing() {
    let mut config = GatewayConfig::default();
    let body = r#"{
        "id": "opencode",
        "name": "OpenCode Zen",
        "api": "https://opencode.ai/zen/v1",
        "models": {
            "weird-model": {
                "name": "Weird Model",
                "cost": {
                    "input": -3,
                    "cache_read": -1.5,
                    "cache_write": -2,
                    "output": 1e999
                }
            }
        }
    }"#
    .to_string();

    let view = apply_template_sync_with(
        &mut config,
        "opencode-zen",
        |_current| Ok(body.clone()),
        |_next| Ok(()),
    )
    .expect("invalid numbers must be non-fatal");

    let model = view
        .template
        .models
        .iter()
        .find(|model| model.upstream_model == "weird-model")
        .expect("the new model must survive with normalized prices");
    assert_eq!(model.input, 0.0);
    assert_eq!(model.cache_read, 0.0);
    assert_eq!(model.cache_write, 0.0);
    assert_eq!(model.output, 0.0);
}

/// REQ-007 / AC-011: a persistence failure aborts the whole sync, leaving the
/// configuration (including every derived provider) untouched.
#[test]
fn sync_atomicity_persist_failure_leaves_config_unchanged() {
    let snapshot = opencode_snapshot();
    let existing = snapshot_model(&snapshot, "deepseek-v4-flash");
    let mut config = GatewayConfig::default();
    for id in ["derived-a", "derived-b"] {
        let mut provider = bound_provider(id, "opencode-zen");
        provider.mappings = vec![mapping_from_snapshot(&snapshot, existing)];
        config.providers.push(provider);
    }
    let body = models_dev_body(json!({
        "new-model": {
            "name": "New Model",
            "cost": {"input": 1.0, "cache_read": 0.1, "cache_write": 0.0, "output": 2.0}
        }
    }));

    let before = serde_json::to_value(&config).expect("encode config");
    let error = apply_template_sync_with(
        &mut config,
        "opencode-zen",
        |_current| Ok(body.clone()),
        |_next| Err("disk full".to_string()),
    )
    .expect_err("a persistence failure must fail the sync");
    assert!(error.contains("disk full"), "the persistence reason must surface: {error}");

    let after = serde_json::to_value(&config).expect("encode config");
    assert_eq!(before, after, "a failed persistence must leave the config untouched");
    assert!(
        config
            .providers
            .iter()
            .all(|provider| find_mapping(provider, "new-model").is_none()),
        "no derived provider may be partially updated"
    );
    assert!(
        config.provider_templates.is_empty(),
        "template state must not be persisted when the write fails"
    );
}

/// Snapshot boundary: an unknown template id fails with the id in the message
/// and writes nothing.
#[test]
fn sync_unknown_template_id_reports_actionable_error() {
    let mut config = config_for_failure("opencode-zen");
    let before = serde_json::to_value(&config).expect("encode config");
    let error = apply_template_sync_with(
        &mut config,
        "no-such-template-xyz",
        |_current| Ok("{}".to_string()),
        |_next| Ok(()),
    )
    .expect_err("an unknown template id must be an error");
    assert!(
        error.contains("no-such-template-xyz"),
        "the error must name the unknown id: {error}"
    );
    let after = serde_json::to_value(&config).expect("encode config");
    assert_eq!(before, after, "an unknown id must write nothing");
}

/// REQ-002: the CommandCode public source only supplies the model list and
/// protocol, so the curated prices, off-peak windows and reasoning efforts of
/// an existing model survive a sync while a new model lands unpriced.
#[test]
fn sync_commandcode_keeps_curated_fields_for_existing_models() {
    let snapshot =
        find_builtin_template("commandcode").expect("the commandcode snapshot must parse");
    let snapshot_deepseek = snapshot_model(&snapshot, "deepseek/deepseek-v4-pro");

    let body = commandcode_body(json!([
        {
            "id": "deepseek/deepseek-v4-pro",
            "name": "DeepSeek V4 Pro (latest)",
            "supported_endpoints": ["/chat/completions", "/responses"]
        },
        {
            "id": "brand/new-model",
            "name": "Brand New",
            "supported_endpoints": ["/responses"]
        }
    ]));

    let mut config = GatewayConfig::default();
    let view = apply_template_sync_with(
        &mut config,
        "commandcode",
        |_current| Ok(body.clone()),
        |_next| Ok(()),
    )
    .expect("a commandcode sync must succeed");

    let deepseek = view
        .template
        .models
        .iter()
        .find(|model| model.upstream_model == "deepseek/deepseek-v4-pro")
        .expect("the existing deepseek model must remain");
    assert_eq!(deepseek.input, snapshot_deepseek.input);
    assert_eq!(deepseek.cache_read, snapshot_deepseek.cache_read);
    assert_eq!(deepseek.cache_write, snapshot_deepseek.cache_write);
    assert_eq!(deepseek.output, snapshot_deepseek.output);
    assert_eq!(deepseek.off_peaks, snapshot_deepseek.off_peaks);
    assert_eq!(deepseek.reasoning_efforts, snapshot_deepseek.reasoning_efforts);

    let new_model = view
        .template
        .models
        .iter()
        .find(|model| model.upstream_model == "brand/new-model")
        .expect("the new model must be added");
    assert_eq!(new_model.input, 0.0);
    assert_eq!(new_model.cache_read, 0.0);
    assert_eq!(new_model.cache_write, 0.0);
    assert_eq!(new_model.output, 0.0);
    assert!(new_model.off_peaks.is_empty());
    assert!(new_model.reasoning_efforts.is_empty());
    assert_eq!(new_model.protocol, Some(UpstreamProtocol::Responses));
}

// ---------------------------------------------------------------------------
// Step 4: create-from-template and model maintenance commands
// ---------------------------------------------------------------------------
//
// These tests pin the Step 4 public boundary
// (`apply_create_provider_from_template`, `apply_delete_provider_model`,
// `apply_restore_provider_model`) and their persist seams. They are expected to
// fail (unresolved import / missing function) until Step 4 lands.

use crate::api_gateway::templates::{
    apply_create_provider_from_template, apply_delete_provider_model,
    apply_restore_provider_model,
};
use std::cell::{Cell, RefCell};

/// REQ-003 / REQ-004 / AC-005 / AC-007: creating from a template appends one
/// provider carrying every template mapping (local == upstream, official
/// display name, effective protocol, reasoning efforts) plus one
/// provider-scoped price row per model (four tiers and off-peak windows), with
/// the template binding, no default model and the persisted config already
/// containing the new provider.
#[test]
fn create_from_template_carries_mappings_prices_and_reasoning_efforts() {
    let template = find_builtin_template("opencode-zen").expect("the snapshot must parse");
    let mut config = GatewayConfig::default();
    let captured: RefCell<Option<GatewayConfig>> = RefCell::new(None);

    let provider = apply_create_provider_from_template(
        &mut config,
        "opencode-zen",
        "My OpenCode",
        "https://my-zen.example.com/v1",
        UpstreamProtocol::Responses,
        "sk-secret",
        |next: &GatewayConfig| {
            *captured.borrow_mut() = Some(next.clone());
            Ok(())
        },
    )
    .expect("a non-blank API key must create the provider");

    assert_eq!(provider.template_id.as_deref(), Some("opencode-zen"));
    assert_eq!(provider.default_model, None);
    assert!(provider.ignored_models.is_empty());
    assert!(provider.enabled, "a created provider is enabled");
    assert!(
        !provider.auto_disabled,
        "a created provider is not auto-disabled"
    );
    assert_eq!(provider.name, "My OpenCode");
    assert_eq!(provider.base_url, "https://my-zen.example.com/v1");
    assert_eq!(provider.protocol, UpstreamProtocol::Responses);
    assert_eq!(provider.api_key, "sk-secret");
    assert!(!provider.id.trim().is_empty(), "a real provider id is assigned");

    assert_eq!(
        provider.mappings.len(),
        template.models.len(),
        "creation must generate one mapping per template model"
    );
    for model in &template.models {
        let mapping = find_mapping(&provider, &model.upstream_model)
            .unwrap_or_else(|| panic!("the mapping for {} must exist", model.upstream_model));
        assert_eq!(mapping.local_model, model.upstream_model);
        assert_eq!(mapping.upstream_model, model.upstream_model);
        assert!(mapping.enabled, "a created mapping is enabled");
        assert_eq!(mapping.display_name, model.display_name);
        assert_eq!(
            mapping.protocol,
            Some(model.protocol.unwrap_or(template.protocol)),
            "a mapping carries the model's effective protocol"
        );
        assert_eq!(mapping.reasoning_efforts, model.reasoning_efforts);
    }

    let provider_rows = config
        .model_prices
        .iter()
        .filter(|row| row.provider_id.as_deref() == Some(provider.id.as_str()))
        .count();
    assert_eq!(
        provider_rows,
        template.models.len(),
        "creation must generate one provider-scoped price row per model"
    );
    for model in &template.models {
        let row = find_price_row(&config, &provider.id, &model.upstream_model)
            .unwrap_or_else(|| panic!("the price row for {} must exist", model.upstream_model));
        assert_eq!(row.input, model.input);
        assert_eq!(row.cache_read, model.cache_read);
        assert_eq!(row.cache_write, model.cache_write);
        assert_eq!(row.output, model.output);
        assert_eq!(row.off_peaks, model.off_peaks);
        assert_eq!(
            row.off_peak, None,
            "creation writes the weekday-aware list only"
        );
    }

    let persisted = captured
        .into_inner()
        .expect("creation must call persist with the next config");
    assert!(
        persisted
            .providers
            .iter()
            .any(|candidate| candidate.id == provider.id),
        "the persisted config must already contain the new provider"
    );
    assert_eq!(
        persisted
            .model_prices
            .iter()
            .filter(|row| row.provider_id.as_deref() == Some(provider.id.as_str()))
            .count(),
        template.models.len(),
        "the persisted config must already carry every provider-scoped price row"
    );
    assert!(
        config
            .providers
            .iter()
            .any(|candidate| candidate.id == provider.id),
        "a successful creation commits the provider to the config"
    );
}

/// REQ-003 / AC-006: an empty or whitespace-only API key rejects the creation
/// with a readable error, leaves the config field-for-field unchanged and never
/// calls the persistence seam.
#[test]
fn create_from_template_requires_api_key_and_writes_nothing() {
    for api_key in ["", "   "] {
        let mut config = GatewayConfig::default();
        let before = serde_json::to_value(&config).expect("encode config before");
        let calls = Cell::new(0usize);

        let error = apply_create_provider_from_template(
            &mut config,
            "opencode-zen",
            "My OpenCode",
            "https://my-zen.example.com/v1",
            UpstreamProtocol::ChatCompletions,
            api_key,
            |_next| {
                calls.set(calls.get() + 1);
                Ok(())
            },
        )
        .expect_err("a blank API key must be rejected");

        assert!(
            !error.trim().is_empty(),
            "rejecting a blank key must report a readable message"
        );
        assert_eq!(calls.get(), 0, "the persist seam must not run when the key is blank");
        assert_eq!(
            before,
            serde_json::to_value(&config).expect("encode config after"),
            "a rejected creation must write nothing"
        );
    }
}

/// REQ-003 / AC-005: a blank name or base_url falls back to the template value
/// while the protocol argument still wins.
#[test]
fn create_from_template_blank_name_and_base_url_fall_back_to_template() {
    let template = find_builtin_template("opencode-zen").expect("the snapshot must parse");
    let mut config = GatewayConfig::default();

    let provider = apply_create_provider_from_template(
        &mut config,
        "opencode-zen",
        "",
        "   ",
        UpstreamProtocol::ChatCompletions,
        "sk-secret",
        |_next| Ok(()),
    )
    .expect("blank name and base_url must fall back to the template");

    assert_eq!(provider.name, template.name);
    assert_eq!(provider.base_url, template.base_url);
    assert_eq!(provider.protocol, UpstreamProtocol::ChatCompletions);
}

/// REQ-005 / AC-008: deleting a model from a template-bound provider removes its
/// mapping and provider-scoped price row and records it in the ignored set
/// exactly once; an unknown provider is an error that writes nothing.
#[test]
fn delete_model_records_ignored_and_removes_mapping_and_price_row() {
    let snapshot = opencode_snapshot();
    let model = snapshot_model(&snapshot, "deepseek-v4-flash");

    let mut config = GatewayConfig::default();
    let mut provider = bound_provider("bound", "opencode-zen");
    provider.mappings = vec![mapping_from_snapshot(&snapshot, model)];
    config.providers.push(provider);
    config.model_prices.push(price_from_snapshot("bound", model));

    apply_delete_provider_model(&mut config, "bound", &model.upstream_model, |_next| Ok(()))
        .expect("deleting a mapped model must succeed");

    let provider = config
        .providers
        .iter()
        .find(|candidate| candidate.id == "bound")
        .expect("the bound provider must exist");
    assert!(
        find_mapping(provider, &model.upstream_model).is_none(),
        "the deleted mapping must be removed"
    );
    assert_eq!(
        provider
            .ignored_models
            .iter()
            .filter(|id| id.as_str() == model.upstream_model.as_str())
            .count(),
        1,
        "the deleted model must be recorded exactly once in the ignored set"
    );
    assert!(
        find_price_row(&config, "bound", &model.upstream_model).is_none(),
        "the provider-scoped price row must be removed"
    );

    let before = serde_json::to_value(&config).expect("encode config");
    let error = apply_delete_provider_model(
        &mut config,
        "no-such-provider",
        &model.upstream_model,
        |_next| Ok(()),
    )
    .expect_err("an unknown provider must be an error");
    assert!(
        !error.trim().is_empty(),
        "rejecting an unknown provider must report a readable message"
    );
    assert_eq!(
        before,
        serde_json::to_value(&config).expect("encode config"),
        "an unknown provider must write nothing"
    );
}

/// REQ-005 / AC-008 counterexample: after deleting a model, a later template
/// sync whose source still lists that model must not resurrect it, and the sync
/// must actually propagate other models.
#[test]
fn delete_model_survives_later_template_sync() {
    let snapshot = opencode_snapshot();
    let model = snapshot_model(&snapshot, "deepseek-v4-flash");

    let mut config = GatewayConfig::default();
    let mut provider = bound_provider("bound", "opencode-zen");
    provider.mappings = vec![mapping_from_snapshot(&snapshot, model)];
    config.providers.push(provider);
    config.model_prices.push(price_from_snapshot("bound", model));

    apply_delete_provider_model(&mut config, "bound", &model.upstream_model, |_next| Ok(()))
        .expect("deleting a mapped model must succeed");

    let body = models_dev_body(json!({
        "deepseek-v4-flash": {
            "name": "DeepSeek V4 Flash",
            "cost": {"input": 1.0, "cache_read": 0.1, "cache_write": 0.0, "output": 2.0}
        },
        "another-model": {
            "name": "Another Model",
            "cost": {"input": 1.0, "cache_read": 0.1, "cache_write": 0.0, "output": 2.0}
        }
    }));

    apply_template_sync_with(
        &mut config,
        "opencode-zen",
        |_current| Ok(body.clone()),
        |_next| Ok(()),
    )
    .expect("the sync must succeed");

    let provider = config
        .providers
        .iter()
        .find(|candidate| candidate.id == "bound")
        .expect("the bound provider must exist");
    assert!(
        find_mapping(provider, "another-model").is_some(),
        "the sync must actually propagate the source's models"
    );
    assert!(
        find_mapping(provider, &model.upstream_model).is_none(),
        "a template sync must not resurrect a deleted model"
    );
    assert!(
        provider
            .ignored_models
            .iter()
            .any(|id| id == &model.upstream_model),
        "the ignored record must survive the sync"
    );
}

/// B2 counterexample: after deleting a model, its provider-scoped price row was
/// removed; a later template sync that still lists that model must not silently
/// resurrect the row even though the ignored mapping stays gone.
#[test]
fn sync_does_not_resurrect_price_row_for_ignored_model() {
    let snapshot = opencode_snapshot();
    let model = snapshot_model(&snapshot, "deepseek-v4-flash");

    let mut config = GatewayConfig::default();
    let mut provider = bound_provider("bound", "opencode-zen");
    provider.mappings = vec![mapping_from_snapshot(&snapshot, model)];
    config.providers.push(provider);
    config.model_prices.push(price_from_snapshot("bound", model));

    apply_delete_provider_model(&mut config, "bound", &model.upstream_model, |_next| Ok(()))
        .expect("deleting a mapped model must succeed");
    assert!(
        find_price_row(&config, "bound", &model.upstream_model).is_none(),
        "the delete must remove the provider-scoped price row"
    );

    let body = models_dev_body(json!({
        "deepseek-v4-flash": {
            "name": "DeepSeek V4 Flash",
            "cost": {"input": 1.0, "cache_read": 0.1, "cache_write": 0.0, "output": 2.0}
        },
        "another-model": {
            "name": "Another Model",
            "cost": {"input": 1.0, "cache_read": 0.1, "cache_write": 0.0, "output": 2.0}
        }
    }));

    apply_template_sync_with(
        &mut config,
        "opencode-zen",
        |_current| Ok(body.clone()),
        |_next| Ok(()),
    )
    .expect("the sync must succeed");

    let provider = config
        .providers
        .iter()
        .find(|candidate| candidate.id == "bound")
        .expect("the bound provider must exist");
    assert!(
        find_mapping(provider, &model.upstream_model).is_none(),
        "an ignored mapping must not be resurrected"
    );
    assert!(
        provider
            .ignored_models
            .iter()
            .any(|id| id == &model.upstream_model),
        "the ignored record must survive the sync"
    );
    assert!(
        find_price_row(&config, "bound", &model.upstream_model).is_none(),
        "an ignored model's provider-scoped price row must not be resurrected"
    );
    assert!(
        find_price_row(&config, "bound", "another-model").is_some(),
        "the sync must still propagate the source's other models"
    );
}

/// N5 counterexample: saving the price table mirrors the first off-peak window
/// into the legacy singular `off_peak`; that mirror must not stop a later
/// template sync from applying the official price update, and the stale mirror
/// must be cleared so the row reflects the current template data.
#[test]
fn sync_updates_price_after_legacy_off_peak_mirror_save() {
    let snapshot = opencode_snapshot();
    let model = snapshot_model(&snapshot, "deepseek-v4-flash");

    let mut previous_model = model.clone();
    previous_model.off_peaks = vec![OffPeakPrice {
        start_time: "00:00".to_string(),
        end_time: "09:00".to_string(),
        input: 0.5,
        cache_read: 0.05,
        cache_write: 0.0,
        output: 1.0,
        days: Some(vec![0, 6]),
    }];

    // The last synced/persisted template carries the off-peak window; the sync
    // merges new source prices over it.
    let mut previous_template = snapshot.clone();
    previous_template.models = vec![previous_model.clone()];

    let mut config = GatewayConfig::default();
    config.provider_templates.push(ProviderTemplateState {
        template_id: "opencode-zen".to_string(),
        template: Some(previous_template.clone()),
        synced_at: Some(1),
        source: Some("snapshot:models.dev".to_string()),
    });

    let mut provider = bound_provider("bound", "opencode-zen");
    provider.mappings = vec![mapping_from_snapshot(&previous_template, &previous_model)];
    config.providers.push(provider);

    // Exactly what `api_gateway_model_prices_save` persists for a multi-window
    // row: `off_peak` mirrors the first `off_peaks` entry.
    let mut mirrored = price_from_snapshot("bound", &previous_model);
    mirrored.off_peak = mirrored.off_peaks.first().cloned();
    assert!(
        mirrored.off_peak.is_some(),
        "the fixture must model the legacy singular mirror"
    );
    config.model_prices.push(mirrored);

    let body = models_dev_body(json!({
        "deepseek-v4-flash": {
            "name": previous_model.display_name.clone(),
            "cost": {"input": 3.0, "cache_read": 0.3, "cache_write": 0.1, "output": 6.0},
            "reasoning_options": [{"type": "effort", "values": previous_model.reasoning_efforts.clone()}]
        }
    }));

    apply_template_sync_with(
        &mut config,
        "opencode-zen",
        |_current| Ok(body.clone()),
        |_next| Ok(()),
    )
    .expect("the sync must succeed");

    let row = find_price_row(&config, "bound", &model.upstream_model)
        .expect("the provider-scoped price row must still exist");
    assert_eq!(row.input, 3.0, "the official price update must win over the mirror");
    assert_eq!(row.cache_read, 0.3);
    assert_eq!(row.cache_write, 0.1);
    assert_eq!(row.output, 6.0);
    assert_eq!(
        row.off_peak, None,
        "the stale legacy singular mirror must be cleared once the row updates"
    );
}

/// REQ-005: deleting from a manual (unbound) provider only removes the mapping;
/// it must not write an ignored record and must keep the price row.
#[test]
fn delete_model_on_manual_provider_does_not_write_ignored() {
    let snapshot = opencode_snapshot();
    let model = snapshot_model(&snapshot, "deepseek-v4-flash");

    let mut config = GatewayConfig::default();
    let mut provider = GatewayUpstreamProvider {
        id: "manual".to_string(),
        name: "Manual".to_string(),
        base_url: "https://manual.example.com/v1".to_string(),
        api_key: "sk-manual".to_string(),
        mappings: vec![mapping_from_snapshot(&snapshot, model)],
        ..GatewayUpstreamProvider::default()
    };
    provider.template_id = None;
    config.providers.push(provider);
    config.model_prices.push(price_from_snapshot("manual", model));

    apply_delete_provider_model(&mut config, "manual", &model.upstream_model, |_next| Ok(()))
        .expect("deleting from a manual provider must succeed");

    let provider = config
        .providers
        .iter()
        .find(|candidate| candidate.id == "manual")
        .expect("the manual provider must exist");
    assert!(
        find_mapping(provider, &model.upstream_model).is_none(),
        "the manual mapping must be removed"
    );
    assert!(
        provider.ignored_models.is_empty(),
        "a manual provider must not record an ignored model"
    );
    assert!(
        find_price_row(&config, "manual", &model.upstream_model).is_some(),
        "a manual provider keeps its price row; only the mapping is removed"
    );
}

/// REQ-005 / AC-008: restoring an ignored model rebuilds it from the template's
/// current data (the persisted state, not the snapshot) as an enabled mapping
/// with the official fields and a provider-scoped price row, and clears the
/// ignored record.
#[test]
fn restore_model_rebuilds_from_template_current_data() {
    let snapshot = opencode_snapshot();
    let source_model = snapshot_model(&snapshot, "deepseek-v4-flash");

    // Persisted template state wins over the snapshot: the restore must use the
    // template's current data, not the built-in snapshot.
    let mut stored = snapshot.clone();
    let template_protocol = stored.protocol;
    let effective_protocol = {
        let stored_model = stored
            .models
            .iter_mut()
            .find(|candidate| candidate.upstream_model == source_model.upstream_model)
            .expect("the stored template must contain the model");
        stored_model.display_name = Some("Current Official Name".to_string());
        stored_model.input = 4.25;
        stored_model.cache_read = 0.425;
        stored_model.cache_write = 0.5;
        stored_model.output = 8.5;
        stored_model.reasoning_efforts = vec!["low".to_string(), "high".to_string()];
        stored_model.protocol.unwrap_or(template_protocol)
    };

    let mut config = GatewayConfig::default();
    config.provider_templates.push(ProviderTemplateState {
        template_id: "opencode-zen".to_string(),
        template: Some(stored.clone()),
        synced_at: Some(7),
        source: Some("test".to_string()),
    });
    let mut provider = bound_provider("bound", "opencode-zen");
    provider.ignored_models = vec![source_model.upstream_model.clone()];
    config.providers.push(provider);

    apply_restore_provider_model(
        &mut config,
        "bound",
        &source_model.upstream_model,
        |_next| Ok(()),
    )
    .expect("an ignored model must be restorable");

    let provider = config
        .providers
        .iter()
        .find(|candidate| candidate.id == "bound")
        .expect("the bound provider must exist");
    assert!(
        !provider
            .ignored_models
            .iter()
            .any(|id| id == &source_model.upstream_model),
        "the restored model must leave the ignored set"
    );
    let mapping = find_mapping(provider, &source_model.upstream_model)
        .expect("the restored mapping must exist");
    assert!(mapping.enabled, "a restored mapping is enabled");
    assert_eq!(mapping.local_model, source_model.upstream_model);
    assert_eq!(mapping.upstream_model, source_model.upstream_model);
    assert_eq!(
        mapping.display_name.as_deref(),
        Some("Current Official Name")
    );
    assert_eq!(mapping.protocol, Some(effective_protocol));
    assert_eq!(
        mapping.reasoning_efforts,
        vec!["low".to_string(), "high".to_string()]
    );

    let row = find_price_row(&config, "bound", &source_model.upstream_model)
        .expect("the restored provider-scoped price row must exist");
    assert_eq!(row.input, 4.25);
    assert_eq!(row.cache_read, 0.425);
    assert_eq!(row.cache_write, 0.5);
    assert_eq!(row.output, 8.5);
    assert_eq!(row.off_peaks, source_model.off_peaks);
    assert_eq!(row.off_peak, None);
}

/// Restore boundary / REQ-005: an ignored model the template no longer carries
/// reports an actionable error and writes nothing; no empty mapping is created.
#[test]
fn restore_model_missing_from_template_reports_error_and_writes_nothing() {
    let mut config = GatewayConfig::default();
    let mut provider = bound_provider("bound", "opencode-zen");
    provider.ignored_models = vec!["ghost-model".to_string()];
    config.providers.push(provider);

    let before = serde_json::to_value(&config).expect("encode config before");
    let error = apply_restore_provider_model(&mut config, "bound", "ghost-model", |_next| Ok(()))
        .expect_err("an ignored model the template no longer has must fail");
    assert!(
        error.contains("ghost-model"),
        "the error must name the missing model: {error}"
    );

    assert_eq!(
        before,
        serde_json::to_value(&config).expect("encode config after"),
        "a failed restore must write nothing"
    );
    let provider = config
        .providers
        .iter()
        .find(|candidate| candidate.id == "bound")
        .expect("the bound provider must exist");
    assert!(
        provider.mappings.iter().all(|mapping| !mapping.local_model.is_empty()
            && !mapping.upstream_model.is_empty()),
        "a failed restore must not synthesize an empty mapping"
    );
    assert!(
        provider.mappings.is_empty(),
        "no mapping may be created for a model the template does not have"
    );
}

/// REQ-005 / AC-008: only an ignored model may be restored; asking to restore a
/// model that is still present is an error that writes nothing.
#[test]
fn restore_model_not_ignored_reports_error() {
    let snapshot = opencode_snapshot();
    let model = snapshot_model(&snapshot, "deepseek-v4-flash");

    let mut config = GatewayConfig::default();
    let mut provider = bound_provider("bound", "opencode-zen");
    provider.mappings = vec![mapping_from_snapshot(&snapshot, model)];
    config.providers.push(provider);
    config.model_prices.push(price_from_snapshot("bound", model));

    let before = serde_json::to_value(&config).expect("encode config before");
    let error =
        apply_restore_provider_model(&mut config, "bound", &model.upstream_model, |_next| Ok(()))
            .expect_err("a model that was never deleted must not be restorable");
    assert!(
        !error.trim().is_empty(),
        "the rejection must report a readable message"
    );
    assert_eq!(
        before,
        serde_json::to_value(&config).expect("encode config after"),
        "a rejected restore must write nothing"
    );
}

/// REQ-007 / AC-011: a persistence failure in any maintenance operation returns
/// an error and leaves the whole configuration field-for-field identical,
/// including providers, ignored records, price rows and template state.
#[test]
fn maintenance_persist_failure_leaves_config_unchanged() {
    // create
    {
        let mut config = GatewayConfig::default();
        let before = serde_json::to_value(&config).expect("encode config before");
        let error = apply_create_provider_from_template(
            &mut config,
            "opencode-zen",
            "My OpenCode",
            "https://my-zen.example.com/v1",
            UpstreamProtocol::ChatCompletions,
            "sk-secret",
            |_next| Err("disk full".to_string()),
        )
        .expect_err("a failed persist must fail the creation");
        assert!(
            error.contains("disk full"),
            "the persistence reason must surface: {error}"
        );
        assert_eq!(
            before,
            serde_json::to_value(&config).expect("encode config after"),
            "a failed creation must leave the config untouched"
        );
    }

    // delete
    {
        let snapshot = opencode_snapshot();
        let model = snapshot_model(&snapshot, "deepseek-v4-flash");
        let mut config = GatewayConfig::default();
        let mut provider = bound_provider("bound", "opencode-zen");
        provider.mappings = vec![mapping_from_snapshot(&snapshot, model)];
        config.providers.push(provider);
        config.model_prices.push(price_from_snapshot("bound", model));

        let before = serde_json::to_value(&config).expect("encode config before");
        let error = apply_delete_provider_model(
            &mut config,
            "bound",
            &model.upstream_model,
            |_next| Err("disk full".to_string()),
        )
        .expect_err("a failed persist must fail the deletion");
        assert!(
            error.contains("disk full"),
            "the persistence reason must surface: {error}"
        );
        assert_eq!(
            before,
            serde_json::to_value(&config).expect("encode config after"),
            "a failed deletion must leave the config untouched"
        );
    }

    // restore
    {
        let snapshot = opencode_snapshot();
        let model = snapshot_model(&snapshot, "deepseek-v4-flash");
        let mut config = GatewayConfig::default();
        let mut provider = bound_provider("bound", "opencode-zen");
        provider.ignored_models = vec![model.upstream_model.clone()];
        config.providers.push(provider);

        let before = serde_json::to_value(&config).expect("encode config before");
        let error = apply_restore_provider_model(
            &mut config,
            "bound",
            &model.upstream_model,
            |_next| Err("disk full".to_string()),
        )
        .expect_err("a failed persist must fail the restore");
        assert!(
            error.contains("disk full"),
            "the persistence reason must surface: {error}"
        );
        assert_eq!(
            before,
            serde_json::to_value(&config).expect("encode config after"),
            "a failed restore must leave the config untouched"
        );
    }
}

// ---------------------------------------------------------------------------
// Regression: OpenCode Zen syncs from the real models.dev full catalog
// ---------------------------------------------------------------------------
//
// `https://models.dev/api.json` is a mapping of provider id -> provider object,
// not a single provider. These tests pin that real payload shape: the sync must
// select the `opencode` entry from the full catalog. They are expected to be RED
// against the current single-provider parser, which only looks for a top-level
// `models` object and therefore reports "response is missing the model list".

/// Full models.dev catalog body: provider id -> provider object, each carrying
/// its own `models` map. The `other` provider deliberately repeats a model id
/// from the `opencode` entry so a parser that picks the wrong provider cannot
/// pass by accident.
fn models_dev_full_catalog_body() -> String {
    json!({
        "opencode": {
            "id": "opencode",
            "name": "OpenCode Zen",
            "api": "https://opencode.ai/zen/v1",
            "models": {
                "catalog-model-a": {
                    "name": "Catalog Model A",
                    "cost": {
                        "input": 1.25,
                        "cache_read": 0.125,
                        "cache_write": 0.5,
                        "output": 5.0
                    },
                    "reasoning_options": [
                        {"type": "effort", "values": ["low", "high"]}
                    ]
                },
                "catalog-model-b": {
                    "name": "Catalog Model B",
                    "cost": {
                        "input": 2.0,
                        "cache_read": 0.2,
                        "cache_write": 0.0,
                        "output": 8.0
                    },
                    "reasoning_options": [
                        {"type": "effort", "values": ["medium"]}
                    ]
                }
            }
        },
        "other": {
            "id": "other",
            "name": "Other Provider",
            "api": "https://other.example.com/v1",
            "models": {
                "other-model": {
                    "name": "Other Model",
                    "cost": {
                        "input": 99.0,
                        "cache_read": 9.9,
                        "cache_write": 0.0,
                        "output": 999.0
                    }
                },
                "catalog-model-a": {
                    "name": "Shadow Model",
                    "cost": {
                        "input": 42.0,
                        "cache_read": 4.2,
                        "cache_write": 0.0,
                        "output": 420.0
                    }
                }
            }
        }
    })
    .to_string()
}

/// Bug regression: the real models.dev `api.json` is a provider-id-keyed full
/// catalog, so the OpenCode Zen sync must select the `opencode` provider entry
/// and ignore every other provider. A successful sync replaces the template
/// models with exactly the `opencode` entry's models (display names, prices and
/// reasoning efforts included), leaves no `other`-provider model behind, and
/// takes the provider-level name/base_url from the `opencode` entry.
#[test]
fn sync_accepts_models_dev_full_catalog_payload() {
    let mut config = GatewayConfig::default();
    let body = models_dev_full_catalog_body();

    let view = apply_template_sync_with(
        &mut config,
        "opencode-zen",
        |_current| Ok(body.clone()),
        |_next| Ok(()),
    )
    .expect("the real full-catalog models.dev payload must sync OpenCode Zen");

    assert_eq!(view.template.id, "opencode-zen");
    assert_eq!(
        view.template.name, "OpenCode Zen",
        "the provider name must come from the catalog's opencode entry"
    );
    assert_eq!(
        view.template.base_url, "https://opencode.ai/zen/v1",
        "the provider base_url must come from the catalog's opencode entry"
    );

    let mut ids: Vec<&str> = view
        .template
        .models
        .iter()
        .map(|model| model.upstream_model.as_str())
        .collect();
    ids.sort_unstable();
    assert_eq!(
        ids,
        vec!["catalog-model-a", "catalog-model-b"],
        "only the opencode entry's models may remain; other providers must be ignored"
    );
    assert!(
        view.template
            .models
            .iter()
            .all(|model| model.upstream_model != "other-model"),
        "a model from another catalog provider must not enter the template"
    );

    let model_a = view
        .template
        .models
        .iter()
        .find(|model| model.upstream_model == "catalog-model-a")
        .expect("catalog-model-a must be present");
    assert_eq!(model_a.display_name.as_deref(), Some("Catalog Model A"));
    assert_eq!(model_a.input, 1.25);
    assert_eq!(model_a.cache_read, 0.125);
    assert_eq!(model_a.cache_write, 0.5);
    assert_eq!(model_a.output, 5.0);
    assert_eq!(model_a.reasoning_efforts, vec!["low", "high"]);

    let model_b = view
        .template
        .models
        .iter()
        .find(|model| model.upstream_model == "catalog-model-b")
        .expect("catalog-model-b must be present");
    assert_eq!(model_b.display_name.as_deref(), Some("Catalog Model B"));
    assert_eq!(model_b.input, 2.0);
    assert_eq!(model_b.cache_read, 0.2);
    assert_eq!(model_b.cache_write, 0.0);
    assert_eq!(model_b.output, 8.0);
    assert_eq!(model_b.reasoning_efforts, vec!["medium"]);

    let state = config
        .provider_templates
        .iter()
        .find(|state| state.template_id == "opencode-zen")
        .expect("a successful sync must persist the opencode-zen template state");
    let persisted = state
        .template
        .as_ref()
        .expect("the persisted template must be present");
    assert_eq!(
        persisted, &view.template,
        "the persisted template must carry exactly the opencode entry's data"
    );
    assert_eq!(persisted.name, "OpenCode Zen");
    assert_eq!(persisted.base_url, "https://opencode.ai/zen/v1");
}

/// Bug regression: when the models.dev full catalog has no `opencode` provider
/// entry there is nothing to synchronize, so the sync must fail with an error
/// that names the missing entry so the failure is actionable, and it must write
/// nothing.
#[test]
fn sync_reports_missing_opencode_entry_in_models_dev_catalog() {
    let body = json!({
        "other": {
            "id": "other",
            "name": "Other Provider",
            "api": "https://other.example.com/v1",
            "models": {
                "other-model": {
                    "name": "Other Model",
                    "cost": {
                        "input": 1.0,
                        "cache_read": 0.1,
                        "cache_write": 0.0,
                        "output": 2.0
                    }
                }
            }
        }
    })
    .to_string();

    let mut config = GatewayConfig::default();
    let before = serde_json::to_value(&config).expect("encode config before");

    let error = apply_template_sync_with(
        &mut config,
        "opencode-zen",
        |_current| Ok(body.clone()),
        |_next| Ok(()),
    )
    .expect_err("a full catalog without an opencode entry must fail the sync");

    assert!(
        error.contains("opencode"),
        "the error must name the missing opencode entry so it is actionable: {error}"
    );

    assert_eq!(
        before,
        serde_json::to_value(&config).expect("encode config after"),
        "a failed sync must write nothing"
    );
}

/// Real UTC milliseconds for a UTC+8 wall-clock instant.
fn utc8_timestamp_ms(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> i64 {
    use chrono::TimeZone;
    let tz = chrono::FixedOffset::east_opt(8 * 3600).unwrap();
    tz.with_ymd_and_hms(year, month, day, hour, minute, 0)
        .unwrap()
        .timestamp_millis()
}

/// N1 / AC-012 coverage: the CommandCode DeepSeek template's curated off-peak
/// windows must price exactly like the official semantics — UTC+8 workdays
/// 09:00-12:00 and 14:00-18:00 are peak, every other instant is off-peak.
#[test]
fn compute_cost_at_time_commandcode_official_windows_match_ac012() {
    let template = find_builtin_template("commandcode").expect("the commandcode snapshot must parse");
    let model = template
        .models
        .iter()
        .find(|model| model.upstream_model.contains("deepseek") && !model.off_peaks.is_empty())
        .expect("the commandcode template must ship a deepseek model with off-peak windows");

    let price = ModelPrice {
        provider_id: None,
        upstream_model: model.upstream_model.clone(),
        input: model.input,
        cache_read: model.cache_read,
        cache_write: model.cache_write,
        output: model.output,
        off_peaks: model.off_peaks.clone(),
        off_peak: None,
    };
    let tokens = UsageTokens {
        input_tokens: 1_000_000,
        cache_read_tokens: 1_000_000,
        cache_write_tokens: 1_000_000,
        output_tokens: 1_000_000,
    };
    let tier_sum =
        |window: &OffPeakPrice| window.input + window.cache_read + window.cache_write + window.output;
    let standard_sum = price.input + price.cache_read + price.cache_write + price.output;

    // UTC+8 2026-09-19 is a Saturday; 10:00 falls inside the weekend 09:00-18:00
    // off-peak window (official: weekends are off-peak all day).
    let saturday_10 = utc8_timestamp_ms(2026, 9, 19, 10, 0);
    let weekend_window = model
        .off_peaks
        .iter()
        .find(|window| {
            window.start_time == "09:00"
                && window.end_time == "18:00"
                && window.days.as_deref() == Some(&[0, 6][..])
        })
        .expect("the deepseek model must define the weekend 09:00-18:00 window");
    let saturday_cost = compute_cost_at_time(&price, &tokens, saturday_10);
    assert!(
        (saturday_cost - tier_sum(weekend_window)).abs() < 1e-9,
        "Saturday 10:00 must bill the weekend off-peak tier ({}), got {saturday_cost}",
        tier_sum(weekend_window)
    );

    // UTC+8 2026-09-16 is a Wednesday; 10:00 falls inside the peak 09:00-12:00
    // block, so the standard tier must apply.
    let wednesday_10 = utc8_timestamp_ms(2026, 9, 16, 10, 0);
    let wednesday_cost = compute_cost_at_time(&price, &tokens, wednesday_10);
    assert!(
        (wednesday_cost - standard_sum).abs() < 1e-9,
        "Wednesday 10:00 is a workday peak block and must bill the standard tier ({standard_sum}), got {wednesday_cost}"
    );
}

#[test]
fn test_template_upsert_edits_existing_and_creates_new() {
    let mut config = GatewayConfig::default();
    let mut modified_builtin = builtin_templates()
        .expect("built-in templates")
        .into_iter()
        .find(|t| t.id == "opencode-zen")
        .expect("opencode-zen exists");

    modified_builtin.name = "OpenCode Zen Custom".to_string();
    modified_builtin.base_url = "https://custom.zen/v1".to_string();

    let views = apply_upsert_provider_template(&mut config, modified_builtin, |_| Ok(()))
        .expect("upsert should succeed");

    let zen_view = views.iter().find(|v| v.template.id == "opencode-zen").unwrap();
    assert_eq!(zen_view.template.name, "OpenCode Zen Custom");
    assert_eq!(zen_view.template.base_url, "https://custom.zen/v1");
    assert!(!zen_view.from_snapshot);

    // Create a new custom template
    let new_custom = ProviderTemplate {
        id: "my-custom-tpl".to_string(),
        name: "My Custom Template".to_string(),
        description: "A custom test template".to_string(),
        base_url: "https://myapi.com/v1".to_string(),
        protocol: UpstreamProtocol::ChatCompletions,
        source: "".to_string(),
        snapshot_version: "1".to_string(),
        models_url: None,
        models: vec![],
    };

    let views2 = apply_upsert_provider_template(&mut config, new_custom, |_| Ok(()))
        .expect("custom template upsert should succeed");

    let custom_view = views2.iter().find(|v| v.template.id == "my-custom-tpl");
    assert!(custom_view.is_some());
    assert_eq!(custom_view.unwrap().template.name, "My Custom Template");
}

#[test]
fn test_template_delete_fails_when_used_by_provider() {
    let mut config = GatewayConfig::default();
    config.providers.push(GatewayUpstreamProvider {
        id: "p1".to_string(),
        name: "Active Provider".to_string(),
        template_id: Some("opencode-zen".to_string()),
        ..Default::default()
    });

    let err = apply_delete_provider_template(&mut config, "opencode-zen", |_| Ok(()))
        .expect_err("should reject deleting a template that is in use");

    assert!(err.contains("currently used by upstream provider 'Active Provider'"));
}

#[test]
fn test_template_delete_succeeds_when_unused_and_reset_restores() {
    let mut config = GatewayConfig::default();
    // No providers using "commandcode"
    let views = apply_delete_provider_template(&mut config, "commandcode", |_| Ok(()))
        .expect("delete should succeed");

    assert!(views.iter().all(|v| v.template.id != "commandcode"));
    assert!(config.deleted_template_ids.contains(&"commandcode".to_string()));

    // Reset restores builtins
    let restored_views = apply_reset_provider_templates(&mut config, |_| Ok(()))
        .expect("reset should succeed");

    assert!(restored_views.iter().any(|v| v.template.id == "commandcode"));
    assert!(config.deleted_template_ids.is_empty());
}

#[test]
fn test_models_url_is_parsed_and_preserved() {
    let json = r#"[
        {
            "id": "tpl-with-url",
            "name": "Template with models URL",
            "base_url": "https://api.test.com/v1",
            "models_url": "https://api.test.com/v1/models",
            "models": []
        }
    ]"#;
    let templates = parse_template_snapshot(json).expect("should parse");
    assert_eq!(templates.len(), 1);
    assert_eq!(
        templates[0].models_url.as_deref(),
        Some("https://api.test.com/v1/models")
    );
}

#[tokio::test]
async fn test_fetch_models_from_url_rejects_empty() {
    let res = crate::api_gateway::templates::fetch_models_from_url("   ", None).await;
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("models URL cannot be empty"));
}


