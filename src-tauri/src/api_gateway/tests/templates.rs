//! Behavior tests for the provider-template data model, snapshot parsing,
//! manual model-list sync and model maintenance semantics.
//!
//! These tests pin the new public boundary: templates ship no models and no
//! prices, a template declares an optional model-list URL, a sync replaces the
//! template list from that URL with source-wins display name/protocol and a
//! locally owned enabled flag, derived providers gain enabled mappings only and
//! no template code path writes a price row. They are expected to fail
//! (missing `enabled` field, removed price fields, changed behavior) until
//! Step 1 lands.

use crate::api_gateway::templates::{
    apply_create_provider_from_template, apply_delete_provider_model,
    apply_delete_provider_template, apply_reset_provider_templates,
    apply_restore_provider_model, apply_template_sync_with, apply_upsert_provider_template,
    builtin_templates, find_builtin_template, parse_template_snapshot, provider_template_views,
    ProviderTemplateView,
};
use crate::api_gateway::types_config::{
    GatewayConfig, GatewayKey, GatewayUpstreamProvider, ModelMapping, ModelPrice, ProviderTemplate,
    ProviderTemplateModel, ProviderTemplateState, TerminalSyncRecord, UpstreamProtocol,
};
use serde_json::{json, Value};
use std::cell::{Cell, RefCell};

const SYNC_URL: &str = "https://tpl.example.com/v1/models";

fn template_model(
    upstream_model: &str,
    display_name: Option<&str>,
    protocol: Option<UpstreamProtocol>,
    enabled: bool,
) -> ProviderTemplateModel {
    ProviderTemplateModel {
        upstream_model: upstream_model.to_string(),
        display_name: display_name.map(str::to_string),
        protocol,
        enabled,
        ..ProviderTemplateModel::default()
    }
}

fn template_with_models(
    id: &str,
    models_url: Option<&str>,
    protocol: UpstreamProtocol,
    models: Vec<ProviderTemplateModel>,
) -> ProviderTemplate {
    ProviderTemplate {
        id: id.to_string(),
        name: "Test Template".to_string(),
        description: String::new(),
        base_url: "https://tpl.example.com/v1".to_string(),
        protocol,
        source: models_url.map(str::to_string).unwrap_or_default(),
        models_url: models_url.map(str::to_string),
        models,
        icon: None,
    }
}

fn seed_template(config: &mut GatewayConfig, template: ProviderTemplate) {
    config.provider_templates.push(ProviderTemplateState {
        template_id: template.id.clone(),
        template: Some(template),
        synced_at: None,
        source: None,
    });
}

fn seeded_config(
    protocol: UpstreamProtocol,
    models: Vec<ProviderTemplateModel>,
) -> GatewayConfig {
    let mut config = GatewayConfig::default();
    seed_template(
        &mut config,
        template_with_models("t", Some(SYNC_URL), protocol, models),
    );
    config
}

fn sync_models(body: Value) -> Vec<ProviderTemplateModel> {
    let mut config = seeded_config(UpstreamProtocol::ChatCompletions, vec![]);
    let view = apply_template_sync_with(&mut config, "t", |_t| Ok(body.to_string()), |_n| Ok(()))
        .expect("the payload shape must parse");
    view.template.models
}

fn bound_provider(id: &str, template_id: &str) -> GatewayUpstreamProvider {
    GatewayUpstreamProvider {
        id: id.to_string(),
        name: "Test Template".to_string(),
        base_url: "https://tpl.example.com/v1".to_string(),
        api_key: "sk-test".to_string(),
        template_id: Some(template_id.to_string()),
        ..GatewayUpstreamProvider::default()
    }
}

fn mapping_for(model: &ProviderTemplateModel, template: &ProviderTemplate) -> ModelMapping {
    let effective = model.protocol.unwrap_or(template.protocol);
    ModelMapping {
        local_model: model.upstream_model.clone(),
        upstream_model: model.upstream_model.clone(),
        enabled: model.enabled,
        protocol: if effective == UpstreamProtocol::ChatCompletions {
            // Test providers use the default `ChatCompletions` protocol, so an
            // effective `ChatCompletions` is stored as follow-the-provider.
            None
        } else {
            Some(effective)
        },
        display_name: model.display_name.clone(),
        reasoning_efforts: Vec::new(),
        auto_disabled: false,
        disabled_reason: None,
        disabled_at: None,
        consecutive_failures: 0,
        last_error_at: None,
    }
}

fn model_mapping(upstream_model: &str) -> ModelMapping {
    ModelMapping {
        local_model: upstream_model.to_string(),
        upstream_model: upstream_model.to_string(),
        enabled: true,
        protocol: None,
        display_name: None,
        reasoning_efforts: Vec::new(),
        auto_disabled: false,
        disabled_reason: None,
        disabled_at: None,
        consecutive_failures: 0,
        last_error_at: None,
    }
}

fn price_row(provider_id: &str, upstream_model: &str) -> ModelPrice {
    ModelPrice {
        provider_id: Some(provider_id.to_string()),
        upstream_model: upstream_model.to_string(),
        input: 1.0,
        ..ModelPrice::default()
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

// ---------------------------------------------------------------------------
// Built-in snapshot and template model shape (REQ-001, REQ-002)
// ---------------------------------------------------------------------------

/// AC-001 / REQ-001 / REQ-002: the built-in snapshot ships both templates with
/// an empty model list, each declaring its official model-list URL as both
/// `models_url` and `source`.
#[test]
fn builtin_templates_are_model_free_and_declare_their_model_list_urls() {
    let templates = builtin_templates().expect("built-in templates must parse");
    assert_eq!(templates.len(), 2, "the snapshot must ship exactly two templates");

    let opencode = templates
        .iter()
        .find(|template| template.id == "opencode-zen")
        .expect("the opencode-zen template must exist");
    let commandcode = templates
        .iter()
        .find(|template| template.id == "commandcode")
        .expect("the commandcode template must exist");

    assert!(
        opencode.models.is_empty(),
        "opencode-zen must ship no models"
    );
    assert!(
        commandcode.models.is_empty(),
        "commandcode must ship no models"
    );

    assert_eq!(
        opencode.models_url.as_deref(),
        Some("https://opencode.ai/zen/v1/models")
    );
    assert_eq!(
        commandcode.models_url.as_deref(),
        Some("https://api.commandcode.ai/provider/v1/models")
    );
    assert_eq!(
        opencode.source, "https://opencode.ai/zen/v1/models",
        "the source must equal the model-list URL"
    );
    assert_eq!(
        commandcode.source, "https://api.commandcode.ai/provider/v1/models",
        "the source must equal the model-list URL"
    );
    assert_eq!(opencode.icon.as_deref(), Some("opencode"));
    assert_eq!(commandcode.icon.as_deref(), Some("commandcode"));

    assert_eq!(
        find_builtin_template("opencode-zen")
            .expect("opencode-zen must resolve")
            .id,
        "opencode-zen"
    );
    assert_eq!(
        find_builtin_template("commandcode")
            .expect("commandcode must resolve")
            .id,
        "commandcode"
    );
}

/// AC-001 / REQ-002: before any sync both built-in views fall back to the
/// snapshot, expose no models, and carry a model-list URL equal to the source.
#[test]
fn provider_template_views_expose_the_empty_builtin_snapshot() {
    let config = GatewayConfig::default();
    let views = provider_template_views(&config).expect("built-in templates must resolve");
    assert_eq!(views.len(), 2, "both built-in templates must be returned");
    assert_eq!(views[0].template.id, "opencode-zen");
    assert_eq!(views[1].template.id, "commandcode");

    for view in &views {
        assert!(view.from_snapshot, "an unsynced template reports the snapshot");
        assert_eq!(view.synced_at, None, "a snapshot view has no sync timestamp");
        assert!(view.template.models.is_empty(), "the snapshot ships no models");
        assert_eq!(
            view.template.models_url.as_deref(),
            Some(view.template.source.as_str()),
            "the snapshot source must equal its model-list URL"
        );
    }
}

/// AC-002 / REQ-001: a template model serializes only its upstream name, an
/// optional display name, an optional protocol and the always-present enabled
/// flag; a model persisted without the flag reads as enabled and no price
/// field ever appears.
#[test]
fn template_model_serializes_the_reduced_shape_and_defaults_enabled() {
    let model = ProviderTemplateModel {
        upstream_model: "m".to_string(),
        display_name: None,
        protocol: None,
        enabled: false,
        ..ProviderTemplateModel::default()
    };
    let value = serde_json::to_value(&model).expect("serialize model");
    let object = value.as_object().expect("a model serializes as an object");
    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec!["enabled", "upstream_model"],
        "the model shape must carry only the upstream name and the enabled flag"
    );
    assert_eq!(object["enabled"], json!(false), "enabled is always serialized");

    for removed in [
        "input",
        "cache_read",
        "cache_write",
        "output",
        "off_peaks",
        "reasoning_efforts",
    ] {
        assert!(
            !object.contains_key(removed),
            "the model shape must not carry the removed field {removed}"
        );
    }

    let defaulted: ProviderTemplateModel =
        serde_json::from_value(json!({"upstream_model": "m"})).expect("a model without enabled loads");
    assert!(defaulted.enabled, "a model without the flag reads as enabled");

    let explicit: ProviderTemplateModel = serde_json::from_value(json!({
        "upstream_model": "m",
        "display_name": "M",
        "protocol": "responses",
        "enabled": false
    }))
    .expect("an explicit model must deserialize");
    assert_eq!(explicit.display_name.as_deref(), Some("M"));
    assert_eq!(explicit.protocol, Some(UpstreamProtocol::Responses));
    assert!(!explicit.enabled);
}

/// REQ-001: a template no longer serializes `snapshot_version`, while older
/// persisted documents that still carry it keep parsing.
#[test]
fn provider_template_omits_snapshot_version_and_ignores_it_on_read() {
    let template = template_with_models(
        "t",
        Some("https://example.com/v1/models"),
        UpstreamProtocol::ChatCompletions,
        vec![],
    );
    let value = serde_json::to_value(&template).expect("serialize template");
    assert!(
        value.get("snapshot_version").is_none(),
        "snapshot_version must be removed from the template shape"
    );

    let legacy: ProviderTemplate = serde_json::from_value(json!({
        "id": "t",
        "name": "T",
        "base_url": "https://example.com/v1",
        "snapshot_version": "v1",
        "models": []
    }))
    .expect("a document carrying the removed snapshot_version must still parse");
    assert_eq!(legacy.id, "t");
    assert!(legacy.models.is_empty());
}

// ---------------------------------------------------------------------------
// Snapshot parsing (REQ-001)
// ---------------------------------------------------------------------------

/// REQ-001 / AC-006: the snapshot parser keeps the reduced model shape, drops
/// empty identifiers and unknown protocols, keeps the first duplicate and reads
/// an absent enabled flag as enabled.
#[test]
fn parse_template_snapshot_drops_invalid_models_and_keeps_first_duplicate() {
    let raw = json!([
        {
            "id": "t",
            "name": "T",
            "base_url": "https://example.com/v1",
            "models": [
                {"upstream_model": "", "protocol": "chat_completions"},
                {"upstream_model": "m-unknown", "protocol": "messages"},
                {"upstream_model": "m", "display_name": "First", "enabled": false},
                {"upstream_model": "m", "display_name": "Second"},
                {"upstream_model": "m-ok", "protocol": "responses"}
            ]
        }
    ])
    .to_string();

    let templates = parse_template_snapshot(&raw).expect("invalid models must be non-fatal");
    assert_eq!(templates.len(), 1);
    let ids: Vec<&str> = templates[0]
        .models
        .iter()
        .map(|model| model.upstream_model.as_str())
        .collect();
    assert_eq!(ids, vec!["m", "m-ok"], "only valid, deduplicated models remain");

    let first = &templates[0].models[0];
    assert_eq!(first.display_name.as_deref(), Some("First"), "the first duplicate wins");
    assert!(!first.enabled, "an explicit enabled flag must be parsed");
    assert_eq!(
        templates[0].models[1].protocol,
        Some(UpstreamProtocol::Responses)
    );
    assert!(
        templates[0].models[1].enabled,
        "an absent enabled flag defaults to true"
    );
}

/// Compatibility / REQ-001: a persisted snapshot carrying the removed price,
/// off-peak, reasoning-effort and snapshot-version fields parses with those
/// fields ignored and the reduced model shape intact.
#[test]
fn parse_template_snapshot_ignores_removed_legacy_fields() {
    let raw = json!([
        {
            "id": "t",
            "name": "T",
            "base_url": "https://example.com/v1",
            "snapshot_version": "v9",
            "models": [
                {
                    "upstream_model": "m",
                    "display_name": "M",
                    "protocol": "chat_completions",
                    "enabled": false,
                    "input": 1.0,
                    "cache_read": 0.5,
                    "cache_write": 0.25,
                    "output": 2.0,
                    "off_peaks": [{"start_time": "00:00", "end_time": "09:00"}],
                    "reasoning_efforts": ["low", "high"]
                }
            ]
        }
    ])
    .to_string();

    let templates = parse_template_snapshot(&raw).expect("legacy fields must be ignored");
    let model = &templates[0].models[0];
    assert_eq!(model.upstream_model, "m");
    assert_eq!(model.display_name.as_deref(), Some("M"));
    assert_eq!(model.protocol, Some(UpstreamProtocol::ChatCompletions));
    assert!(!model.enabled);
}

/// Snapshot boundary: an empty or duplicate template id rejects the document
/// with a readable error.
#[test]
fn parse_template_snapshot_rejects_empty_or_duplicate_template_ids() {
    let empty_id = json!([
        { "id": "", "name": "T", "base_url": "https://example.com/v1", "models": [] }
    ])
    .to_string();
    let error =
        parse_template_snapshot(&empty_id).expect_err("an empty template id must be rejected");
    assert!(
        !error.trim().is_empty(),
        "rejecting an empty id must report a readable message"
    );

    let duplicate = json!([
        { "id": "t", "name": "A", "base_url": "https://a.example.com", "models": [] },
        { "id": "t", "name": "B", "base_url": "https://b.example.com", "models": [] }
    ])
    .to_string();
    let error =
        parse_template_snapshot(&duplicate).expect_err("a duplicate template id must be rejected");
    assert!(
        !error.trim().is_empty(),
        "rejecting a duplicate id must report a readable message"
    );
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
    let legacy = json!({
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

/// Compatibility / REQ-001: an older persisted template state whose models
/// still carry prices, off-peak windows, reasoning efforts and a snapshot
/// version loads with those fields ignored while the identifiers, display
/// names, protocols and enabled flags keep working.
#[test]
fn legacy_persisted_template_with_priced_models_still_loads() {
    let legacy = json!({
        "enabled": false,
        "port": 17688,
        "providers": [],
        "keys": [],
        "provider_templates": [{
            "template_id": "opencode-zen",
            "synced_at": 42,
            "source": "https://models.dev/api.json",
            "template": {
                "id": "opencode-zen",
                "name": "OpenCode Zen",
                "base_url": "https://opencode.ai/zen/v1",
                "protocol": "chat_completions",
                "snapshot_version": "v3",
                "models": [{
                    "upstream_model": "legacy-model",
                    "display_name": "Legacy Model",
                    "protocol": "responses",
                    "input": 1.0,
                    "cache_read": 0.1,
                    "cache_write": 0.0,
                    "output": 2.0,
                    "off_peaks": [],
                    "reasoning_efforts": ["low"]
                }]
            }
        }]
    });

    let config: GatewayConfig =
        serde_json::from_value(legacy).expect("a legacy template state must load");
    let state = config
        .provider_templates
        .iter()
        .find(|state| state.template_id == "opencode-zen")
        .expect("the legacy template state must be present");
    let template = state.template.as_ref().expect("the legacy template must be present");
    assert_eq!(template.models.len(), 1);
    let model = &template.models[0];
    assert_eq!(model.upstream_model, "legacy-model");
    assert_eq!(model.display_name.as_deref(), Some("Legacy Model"));
    assert_eq!(model.protocol, Some(UpstreamProtocol::Responses));
    assert!(model.enabled, "a legacy model without the flag reads as enabled");

    let encoded = serde_json::to_value(&config).expect("encode config");
    assert!(
        encoded["provider_templates"][0]["template"]
            .get("snapshot_version")
            .is_none(),
        "the removed snapshot_version must not be re-serialized"
    );
}

/// Snapshot boundary: `models_url` is parsed and preserved.
#[test]
fn test_models_url_is_parsed_and_preserved() {
    let raw = json!([
        {
            "id": "tpl-with-url",
            "name": "Template with models URL",
            "base_url": "https://api.test.com/v1",
            "models_url": "https://api.test.com/v1/models",
            "models": []
        }
    ])
    .to_string();
    let templates = parse_template_snapshot(&raw).expect("should parse");
    assert_eq!(templates.len(), 1);
    assert_eq!(
        templates[0].models_url.as_deref(),
        Some("https://api.test.com/v1/models")
    );
}

// ---------------------------------------------------------------------------
// Manual model-list sync (REQ-004, REQ-005, AC-004, AC-005, AC-006, AC-013)
// ---------------------------------------------------------------------------

/// REQ-004 / AC-004: the sync fetches the template's own `models_url`, replaces
/// the template list wholesale, keeps an unlabeled entry's protocol at `None`
/// so it inherits the template protocol, and records the sync time while the
/// persisted state is replaced.
#[test]
fn sync_fetches_the_configured_models_url_and_replaces_the_template_list() {
    let mut config = seeded_config(
        UpstreamProtocol::ChatCompletions,
        vec![template_model("retired", Some("Retired"), None, true)],
    );
    let seen_url: RefCell<Option<String>> = RefCell::new(None);
    let body = json!({"data": [{"id": "model-a", "name": "Model A"}]}).to_string();

    let view: ProviderTemplateView = apply_template_sync_with(
        &mut config,
        "t",
        |template| {
            *seen_url.borrow_mut() = template.models_url.clone();
            Ok(body.clone())
        },
        |_next| Ok(()),
    )
    .expect("a well-formed source must sync");

    assert_eq!(
        seen_url.into_inner().as_deref(),
        Some(SYNC_URL),
        "the fetch seam must receive the configured models_url"
    );
    assert_eq!(view.template.models.len(), 1, "the list is replaced wholesale");
    let model = &view.template.models[0];
    assert_eq!(model.upstream_model, "model-a");
    assert_eq!(model.display_name.as_deref(), Some("Model A"));
    assert_eq!(model.protocol, None, "an unlabeled entry inherits the template protocol");
    assert!(model.enabled);
    assert!(view.synced_at.is_some(), "a synced view records the sync time");
    assert!(!view.from_snapshot, "a synced view is not a snapshot fallback");
    assert!(
        view.template
            .models
            .iter()
            .all(|model| model.upstream_model != "retired"),
        "a model the endpoint no longer returns is dropped"
    );

    let state = config
        .provider_templates
        .iter()
        .find(|state| state.template_id == "t")
        .expect("a successful sync must persist the template state");
    assert_eq!(
        state.template.as_ref().expect("the persisted template"),
        &view.template
    );
    assert_eq!(state.synced_at, view.synced_at);
}

/// REQ-004: the sync accepts a `data` array, a `models` array (an object
/// without `id` may use `name` as its identifier), a root array and string
/// entries.
#[test]
fn sync_accepts_data_models_root_and_string_shapes() {
    let data_shape = sync_models(json!({"data": [{"id": "a", "name": "A"}]}));
    assert_eq!(data_shape.len(), 1);
    assert_eq!(data_shape[0].upstream_model, "a");
    assert_eq!(data_shape[0].display_name.as_deref(), Some("A"));

    let models_shape = sync_models(json!({"models": [{"id": "b", "name": "B"}, {"name": "c"}]}));
    let ids: Vec<&str> = models_shape
        .iter()
        .map(|model| model.upstream_model.as_str())
        .collect();
    assert_eq!(
        ids,
        vec!["b", "c"],
        "a models entry without id uses name as its identifier"
    );
    assert_eq!(models_shape[0].display_name.as_deref(), Some("B"));
    assert_eq!(models_shape[1].display_name.as_deref(), Some("c"));

    let root_shape = sync_models(json!([{"id": "d"}, "e"]));
    let ids: Vec<&str> = root_shape
        .iter()
        .map(|model| model.upstream_model.as_str())
        .collect();
    assert_eq!(ids, vec!["d", "e"]);

    let string_shape = sync_models(json!({"data": ["f", "g"]}));
    let ids: Vec<&str> = string_shape
        .iter()
        .map(|model| model.upstream_model.as_str())
        .collect();
    assert_eq!(ids, vec!["f", "g"]);
}

/// AC-006 / REQ-004: `supported_endpoints` decides the protocol; an entry
/// declaring neither supported endpoint is dropped while an entry without
/// endpoint information is kept with no protocol (inheriting the template
/// protocol).
#[test]
fn sync_derives_protocol_from_supported_endpoints_and_drops_unservable_entries() {
    let mut config = seeded_config(UpstreamProtocol::ChatCompletions, vec![]);
    let body = json!({
        "data": [
            {"id": "chat", "supported_endpoints": ["/messages", "/chat/completions"]},
            {"id": "resp", "supported_endpoints": ["/responses"]},
            {"id": "unlabeled"},
            {"id": "dropped", "supported_endpoints": ["/messages"]},
            {"id": "non-array", "supported_endpoints": "/chat/completions"}
        ]
    })
    .to_string();

    let view = apply_template_sync_with(&mut config, "t", |_t| Ok(body.clone()), |_n| Ok(()))
        .expect("the payload must parse");
    let find = |id: &str| {
        view.template
            .models
            .iter()
            .find(|model| model.upstream_model == id)
    };

    assert_eq!(
        find("chat").expect("chat").protocol,
        Some(UpstreamProtocol::ChatCompletions)
    );
    assert_eq!(
        find("resp").expect("resp").protocol,
        Some(UpstreamProtocol::Responses)
    );
    assert_eq!(find("unlabeled").expect("unlabeled").protocol, None);
    assert_eq!(
        find("non-array").expect("non-array").protocol,
        None,
        "a non-array endpoints field counts as no endpoint information"
    );
    assert!(
        find("dropped").is_none(),
        "an entry declaring only unsupported endpoints is dropped"
    );
}

/// AC-004 / AC-013 / REQ-004: an identically named model takes the source's
/// display name and derived protocol, keeps its local enabled flag and its
/// omitted local values, and duplicate source entries keep the first.
#[test]
fn sync_merges_identically_named_models_with_source_wins_and_local_enabled() {
    let mut config = seeded_config(
        UpstreamProtocol::ChatCompletions,
        vec![
            template_model("a", Some("Local A"), Some(UpstreamProtocol::Responses), false),
            template_model("c", Some("Local C"), Some(UpstreamProtocol::Responses), true),
            template_model("retired", Some("Retired"), None, true),
        ],
    );
    let body = json!({
        "data": [
            {"id": "a", "name": "Source A", "supported_endpoints": ["/chat/completions"]},
            {"id": "a", "name": "Duplicate A"},
            {"id": "c"},
            {"id": "b", "name": "Source B"}
        ]
    })
    .to_string();

    let view = apply_template_sync_with(&mut config, "t", |_t| Ok(body.clone()), |_n| Ok(()))
        .expect("the sync must merge the source");
    let find = |id: &str| {
        view.template
            .models
            .iter()
            .find(|model| model.upstream_model == id)
            .unwrap_or_else(|| panic!("the model {id} must be present"))
    };

    let a = find("a");
    assert_eq!(a.display_name.as_deref(), Some("Source A"), "the source display name wins");
    assert_eq!(
        a.protocol,
        Some(UpstreamProtocol::ChatCompletions),
        "the derived protocol wins"
    );
    assert!(!a.enabled, "the local enabled flag always survives");
    assert_eq!(
        view.template
            .models
            .iter()
            .filter(|model| model.upstream_model == "a")
            .count(),
        1,
        "duplicate source entries keep the first"
    );

    let c = find("c");
    assert_eq!(
        c.display_name.as_deref(),
        Some("Local C"),
        "an omitted source name keeps the local value"
    );
    assert_eq!(
        c.protocol,
        Some(UpstreamProtocol::Responses),
        "omitted endpoint information keeps the local protocol"
    );

    let b = find("b");
    assert!(b.enabled, "a new model starts enabled");
    assert_eq!(b.protocol, None);

    assert!(
        view.template
            .models
            .iter()
            .all(|model| model.upstream_model != "retired"),
        "a model the source no longer returns is dropped"
    );
}

/// AC-005 boundary / REQ-004: a template without a usable model-list URL fails
/// with an actionable error naming the URL context, never calls persist and
/// leaves the configuration unchanged.
#[test]
fn sync_without_models_url_reports_error_and_writes_nothing() {
    for models_url in [None, Some("   ")] {
        let mut config = GatewayConfig::default();
        seed_template(
            &mut config,
            template_with_models(
                "tpl-no-url",
                models_url,
                UpstreamProtocol::ChatCompletions,
                vec![],
            ),
        );
        let before = serde_json::to_value(&config).expect("encode before");
        let calls = Cell::new(0usize);

        let error = apply_template_sync_with(
            &mut config,
            "tpl-no-url",
            |_template| Ok("{}".to_string()),
            |_next| {
                calls.set(calls.get() + 1);
                Ok(())
            },
        )
        .expect_err("a missing model-list URL must be an error");

        assert!(
            error.to_lowercase().contains("url"),
            "the error must name the URL context: {error}"
        );
        assert_eq!(calls.get(), 0, "the persist seam must not run");
        assert_eq!(
            before,
            serde_json::to_value(&config).expect("encode after"),
            "a missing URL must write nothing"
        );
    }
}

fn assert_sync_fatal_keeps_config(body: Result<&str, &str>, url: &str, reason_fragment: &str) {
    let mut config = seeded_config(
        UpstreamProtocol::ChatCompletions,
        vec![template_model("existing", Some("Existing"), None, true)],
    );
    let before = serde_json::to_value(&config).expect("encode before");
    let calls = Cell::new(0usize);

    let error = apply_template_sync_with(
        &mut config,
        "t",
        |_template| match body {
            Ok(text) => Ok(text.to_string()),
            Err(reason) => Err(reason.to_string()),
        },
        |_next| {
            calls.set(calls.get() + 1);
            Ok(())
        },
    )
    .expect_err("a fatal source must fail the sync");

    assert!(
        error.contains(url),
        "the error must name the URL {url}: {error}"
    );
    assert!(
        error.to_lowercase().contains(reason_fragment),
        "the error must name the reason ({reason_fragment}): {error}"
    );
    assert_eq!(calls.get(), 0, "a fatal sync must not persist");
    assert_eq!(
        before,
        serde_json::to_value(&config).expect("encode after"),
        "a fatal sync must write nothing"
    );
}

/// AC-005 / REQ-004: each fatal source — a fetch error, non-JSON, a missing
/// model array, an entry without an identifier, an empty array and a set whose
/// entries are all filtered out — fails with an error naming the URL and the
/// reason, never persists and leaves the configuration untouched.
#[test]
fn sync_fatal_failures_name_the_url_and_write_nothing() {
    assert_sync_fatal_keeps_config(Err("connection refused"), SYNC_URL, "connection refused");
    assert_sync_fatal_keeps_config(Ok("this is not json"), SYNC_URL, "json");
    assert_sync_fatal_keeps_config(Ok(r#"{"meta": true}"#), SYNC_URL, "model");
    assert_sync_fatal_keeps_config(
        Ok(r#"{"data": [{"name": "no id"}]}"#),
        SYNC_URL,
        "identif",
    );
    // A blank string entry carries no identifier, so a mixed list must be fatal
    // rather than silently dropping it.
    assert_sync_fatal_keeps_config(
        Ok(r#"{"data": ["", {"id": "valid-model"}]}"#),
        SYNC_URL,
        "identif",
    );
    assert_sync_fatal_keeps_config(
        Ok(r#"{"data": ["   ", {"id": "valid-model"}]}"#),
        SYNC_URL,
        "identif",
    );
    assert_sync_fatal_keeps_config(Ok(r#"{"data": []}"#), SYNC_URL, "empty");
    assert_sync_fatal_keeps_config(
        Ok(r#"{"data": [{"id": "only-messages", "supported_endpoints": ["/messages"]}]}"#),
        SYNC_URL,
        "empty",
    );
}

/// REQ-005 / AC-005: a persistence failure aborts the entire sync, leaving the
/// template state, every derived provider and every price row untouched.
#[test]
fn sync_persist_failure_leaves_the_config_unchanged() {
    let mut config = seeded_config(UpstreamProtocol::ChatCompletions, vec![]);
    let mut provider = bound_provider("p", "t");
    provider.mappings = vec![model_mapping("existing")];
    config.providers.push(provider);
    let before = serde_json::to_value(&config).expect("encode before");

    let error = apply_template_sync_with(
        &mut config,
        "t",
        |_template| Ok(json!({"data": [{"id": "new-model"}]}).to_string()),
        |_next| Err("disk full".to_string()),
    )
    .expect_err("a failed persist must fail the sync");

    assert!(error.contains("disk full"), "the reason must surface: {error}");
    assert_eq!(
        before,
        serde_json::to_value(&config).expect("encode after"),
        "a failed persistence must leave the config untouched"
    );
    assert!(
        config
            .providers
            .iter()
            .all(|provider| find_mapping(provider, "new-model").is_none()),
        "no derived provider may be partially updated"
    );
}

/// Snapshot boundary: an unknown template id fails with the id in the message
/// and writes nothing.
#[test]
fn sync_unknown_template_id_reports_actionable_error() {
    let mut config = GatewayConfig::default();
    let before = serde_json::to_value(&config).expect("encode before");
    let error = apply_template_sync_with(
        &mut config,
        "no-such-template-xyz",
        |_template| Ok("{}".to_string()),
        |_next| Ok(()),
    )
    .expect_err("an unknown template id must be an error");
    assert!(
        error.contains("no-such-template-xyz"),
        "the error must name the unknown id: {error}"
    );
    assert_eq!(
        before,
        serde_json::to_value(&config).expect("encode after"),
        "an unknown id must write nothing"
    );
}

/// REQ-005 / AC-004 / AC-005: a successful sync adds enabled mappings for newly
/// available enabled models only, skips ignored and disabled models, updates an
/// untouched display name, keeps retired mappings while disabling them, preserves
/// a locally disabled mapping and a locally rewritten protocol, leaves an
/// unrelated manual provider alone and never touches price rows.
#[test]
fn sync_propagates_enabled_models_and_never_writes_prices() {
    let previous = template_with_models(
        "t",
        Some(SYNC_URL),
        UpstreamProtocol::ChatCompletions,
        vec![
            template_model("a", Some("Local A"), Some(UpstreamProtocol::Responses), true),
            template_model("c", Some("Local C"), None, true),
            template_model("d", Some("Disabled D"), None, false),
            template_model("e", Some("Local E"), None, true),
            template_model("retired", Some("Retired"), None, true),
        ],
    );
    let mut config = GatewayConfig::default();
    config.provider_templates.push(ProviderTemplateState {
        template_id: "t".to_string(),
        template: Some(previous.clone()),
        synced_at: Some(1),
        source: Some(SYNC_URL.to_string()),
    });

    let mut provider = bound_provider("p", "t");
    let mapping_a = mapping_for(
        previous
            .models
            .iter()
            .find(|model| model.upstream_model == "a")
            .expect("model a"),
        &previous,
    );
    // A locally rewritten protocol must survive even when it differs from the
    // previous template-derived protocol.
    let mut mapping_c = mapping_for(
        previous
            .models
            .iter()
            .find(|model| model.upstream_model == "c")
            .expect("model c"),
        &previous,
    );
    mapping_c.protocol = Some(UpstreamProtocol::Responses);
    // A locally disabled mapping must stay disabled across a sync.
    let mut mapping_e = mapping_for(
        previous
            .models
            .iter()
            .find(|model| model.upstream_model == "e")
            .expect("model e"),
        &previous,
    );
    mapping_e.enabled = false;
    let retired_mapping = model_mapping("retired");
    provider.mappings = vec![
        mapping_a,
        mapping_c,
        mapping_e,
        retired_mapping.clone(),
    ];
    provider.ignored_models = vec!["ignored".to_string()];
    config.providers.push(provider);

    let mut manual = GatewayUpstreamProvider::default();
    manual.id = "manual".to_string();
    manual.name = "Manual".to_string();
    manual.base_url = "https://manual.example.com/v1".to_string();
    manual.mappings = vec![model_mapping("m")];
    config.providers.push(manual);
    let manual_before = serde_json::to_value(&config.providers[1]).expect("encode manual");

    // A provider bound to another template, with hand-edited metadata, must not change.
    let mut other = bound_provider("p2", "other-template");
    other.name = "Edited P2".to_string();
    other.base_url = "https://edited.example.com/v1".to_string();
    other.protocol = UpstreamProtocol::Responses;
    other.mappings = vec![model_mapping("manual-model")];
    config.providers.push(other);
    let other_before = serde_json::to_value(&config.providers[2]).expect("encode other");

    config.model_prices = vec![
        price_row("p", "a"),
        price_row("p", "retired"),
    ];
    let prices_before = config.model_prices.clone();

    let body = json!({
        "data": [
            {"id": "a", "name": "Source A", "supported_endpoints": ["/chat/completions"]},
            {"id": "c", "name": "Source C", "supported_endpoints": ["/chat/completions"]},
            {"id": "d", "name": "Disabled D"},
            {"id": "e", "name": "Source E", "supported_endpoints": ["/chat/completions"]},
            {"id": "b", "name": "Source B", "supported_endpoints": ["/chat/completions"]},
            {"id": "u", "name": "Unlabeled"},
            {"id": "ignored", "name": "Ignored"}
        ]
    })
    .to_string();

    apply_template_sync_with(&mut config, "t", |_t| Ok(body.clone()), |_n| Ok(()))
        .expect("the sync must propagate");

    let provider = config
        .providers
        .iter()
        .find(|provider| provider.id == "p")
        .expect("the bound provider must exist");

    let b = find_mapping(provider, "b").expect("a new enabled model must be added");
    assert!(b.enabled);
    assert_eq!(b.local_model, "b");
    assert_eq!(b.upstream_model, "b");
    assert_eq!(b.display_name.as_deref(), Some("Source B"));
    assert_eq!(
        b.protocol, None,
        "a new mapping whose protocol matches the provider follows the provider"
    );

    let u = find_mapping(provider, "u").expect("an unlabeled new model must be added");
    assert_eq!(
        u.protocol, None,
        "an unlabeled model inherits the template protocol and follows the provider"
    );

    assert!(
        find_mapping(provider, "ignored").is_none(),
        "an ignored model must not be re-added"
    );
    assert!(
        find_mapping(provider, "d").is_none(),
        "a disabled template model must not be added"
    );
    let retired = find_mapping(provider, "retired")
        .expect("a retired model's existing mapping must be kept");
    assert!(
        !retired.enabled,
        "a mapping for a model the sync removed must be disabled"
    );
    assert_eq!(
        retired.local_model, retired_mapping.local_model,
        "retirement must only flip the enabled flag"
    );
    assert_eq!(
        retired.display_name, retired_mapping.display_name,
        "retirement must not rewrite the display name"
    );
    assert_eq!(
        retired.protocol, retired_mapping.protocol,
        "retirement must not rewrite the protocol"
    );

    let a = find_mapping(provider, "a").expect("the existing mapping must remain");
    assert_eq!(
        a.display_name.as_deref(),
        Some("Source A"),
        "an untouched display name follows the source"
    );
    assert_eq!(
        a.protocol, None,
        "an untouched protocol matching the provider stays as follow-the-provider"
    );
    assert_eq!(a.local_model, "a", "the local model name is never touched");
    assert!(a.enabled, "a mapping's enabled flag is never touched");

    let c = find_mapping(provider, "c").expect("the existing mapping must remain");
    assert_eq!(
        c.protocol,
        Some(UpstreamProtocol::Responses),
        "a locally rewritten protocol must never be overwritten by the sync"
    );

    let e = find_mapping(provider, "e").expect("the existing mapping must remain");
    assert!(
        !e.enabled,
        "a locally disabled mapping must never be re-enabled by the sync"
    );
    assert_eq!(
        e.display_name.as_deref(),
        Some("Source E"),
        "the sync still refreshes the display name of a disabled mapping"
    );
    assert_eq!(
        e.protocol, None,
        "the sync still refreshes the protocol of a disabled mapping, following the provider when equal"
    );

    assert_eq!(
        serde_json::to_value(&config.providers[1]).expect("encode manual after"),
        manual_before,
        "a manual provider must not change"
    );
    assert_eq!(
        serde_json::to_value(&config.providers[2]).expect("encode other after"),
        other_before,
        "a provider bound to another template must not change"
    );
    assert_eq!(
        config.model_prices, prices_before,
        "a sync must never create or modify a price row"
    );
}

/// REQ-001 / AC-001: a sync that drops a model disables the matching mapping on
/// every provider bound to the template, keeping the row and every other field
/// (local model, display name, protocol, provider metadata, price row) intact.
/// A model the previous template had already disabled is retired exactly like an
/// enabled one when the sync removes it.
#[test]
fn sync_disables_mappings_for_models_removed_by_the_sync() {
    let previous = template_with_models(
        "t",
        Some(SYNC_URL),
        UpstreamProtocol::ChatCompletions,
        vec![
            template_model("A", Some("Local A"), None, true),
            template_model("M", Some("Local M"), Some(UpstreamProtocol::Responses), true),
            // The previous template had already disabled this model while the
            // provider mapping kept serving it; the removal still retires it.
            template_model(
                "disabled-gone",
                Some("Local Disabled Gone"),
                Some(UpstreamProtocol::Responses),
                false,
            ),
        ],
    );
    let mut config = GatewayConfig::default();
    seed_template(&mut config, previous.clone());

    let mapping_a_before = mapping_for(&previous.models[0], &previous);
    let mapping_m_before = mapping_for(&previous.models[1], &previous);
    // The template entry is disabled, but the derived mapping is still enabled.
    let mut mapping_disabled_gone_before = mapping_for(&previous.models[2], &previous);
    mapping_disabled_gone_before.enabled = true;
    let provider_before = bound_provider("p", "t");
    let mut provider = provider_before.clone();
    provider.mappings = vec![
        mapping_a_before.clone(),
        mapping_m_before.clone(),
        mapping_disabled_gone_before.clone(),
    ];
    config.providers.push(provider);
    config.model_prices = vec![price_row("p", "A"), price_row("p", "M")];
    let prices_before = config.model_prices.clone();

    let body = json!({"data": [{"id": "A"}]}).to_string();
    let view = apply_template_sync_with(&mut config, "t", |_t| Ok(body.clone()), |_n| Ok(()))
        .expect("the sync must succeed");

    let ids: Vec<&str> = view
        .template
        .models
        .iter()
        .map(|model| model.upstream_model.as_str())
        .collect();
    assert_eq!(
        ids,
        vec!["A"],
        "the removed model must leave the template list"
    );

    let provider = config
        .providers
        .iter()
        .find(|provider| provider.id == "p")
        .expect("the bound provider must exist");

    let a = find_mapping(provider, "A").expect("the surviving mapping must stay");
    assert_eq!(a, &mapping_a_before, "a surviving mapping must not change");

    let m = find_mapping(provider, "M").expect("the retired mapping must be kept");
    assert!(
        !m.enabled,
        "a mapping for a model the sync removed must be disabled"
    );
    let mut mapping_m_expected = mapping_m_before.clone();
    mapping_m_expected.enabled = false;
    assert_eq!(
        m, &mapping_m_expected,
        "retirement must only flip the enabled flag"
    );

    let disabled_gone = find_mapping(provider, "disabled-gone")
        .expect("a retired mapping for a template-disabled model must be kept");
    assert!(
        !disabled_gone.enabled,
        "a template-disabled model the sync removed must still disable its mapping"
    );
    let mut disabled_gone_expected = mapping_disabled_gone_before.clone();
    disabled_gone_expected.enabled = false;
    assert_eq!(
        disabled_gone, &disabled_gone_expected,
        "retirement must only flip the enabled flag for a formerly template-disabled model"
    );
    assert_eq!(
        disabled_gone.local_model, mapping_disabled_gone_before.local_model,
        "retirement must not rewrite the local model of a template-disabled model"
    );
    assert_eq!(
        disabled_gone.display_name, mapping_disabled_gone_before.display_name,
        "retirement must not rewrite the display name of a template-disabled model"
    );
    assert_eq!(
        disabled_gone.protocol, mapping_disabled_gone_before.protocol,
        "retirement must not rewrite the protocol of a template-disabled model"
    );

    let mut provider_after_without_mappings = provider.clone();
    provider_after_without_mappings.mappings = Vec::new();
    let mut provider_before_without_mappings = provider_before.clone();
    provider_before_without_mappings.mappings = Vec::new();
    assert_eq!(
        serde_json::to_value(&provider_after_without_mappings).expect("encode provider after"),
        serde_json::to_value(&provider_before_without_mappings).expect("encode provider before"),
        "retirement must not change any provider field other than the mapping flag"
    );

    assert_eq!(
        config.model_prices, prices_before,
        "retirement must not create, remove or modify a price row"
    );
    assert!(
        find_price_row(&config, "p", "M").is_some(),
        "the retired model's price row must be kept"
    );
}

/// REQ-002 / AC-002 / AC-003: a sync disables the removed model's mapping on
/// every provider bound to the template, while a manual mapping, an
/// already-disabled mapping, an ignored model the previous template carried
/// without a mapping, a manual provider, a provider bound to another template,
/// local keys and terminal-sync records stay byte-for-byte unchanged and no
/// ignored record is written.
#[test]
fn sync_retirement_scope_excludes_manual_disabled_and_ignored_mappings() {
    let previous = template_with_models(
        "t",
        Some(SYNC_URL),
        UpstreamProtocol::ChatCompletions,
        vec![
            template_model("A", Some("Local A"), None, true),
            template_model("M", Some("Local M"), None, true),
            // The previous template carried a model the provider ignores and has
            // no mapping for; the sync removes it without adding a record.
            template_model("ignored-only", Some("Ignored Only"), None, true),
        ],
    );
    let mut config = GatewayConfig::default();
    seed_template(&mut config, previous.clone());

    // An already-disabled mapping whose model still appears in the sync result.
    let mut mapping_a = mapping_for(&previous.models[0], &previous);
    mapping_a.enabled = false;
    // The mapping the sync must retire.
    let mapping_m = mapping_for(&previous.models[1], &previous);
    // A manual mapping whose model was never in the previous template.
    let mapping_manual = model_mapping("manual-only");
    // The ignored set as it stands before the sync; the sync removes the model
    // from the template but must neither add nor drop a record here.
    let ignored_before = vec!["ignored-only".to_string()];

    for id in ["p1", "p2"] {
        let mut provider = bound_provider(id, "t");
        provider.mappings = vec![
            mapping_a.clone(),
            mapping_m.clone(),
            mapping_manual.clone(),
        ];
        provider.ignored_models = ignored_before.clone();
        config.providers.push(provider);
    }

    // A manual provider (no `template_id`) that even holds the retired model
    // must never be touched.
    let mut manual = GatewayUpstreamProvider::default();
    manual.id = "pm".to_string();
    manual.name = "Manual Provider".to_string();
    manual.base_url = "https://manual.example.com/v1".to_string();
    manual.mappings = vec![model_mapping("M")];
    config.providers.push(manual);

    // A provider bound to another template must never be touched.
    let mut other = bound_provider("po", "other-template");
    other.mappings = vec![model_mapping("M")];
    config.providers.push(other);

    config.keys.push(GatewayKey {
        id: "k1".to_string(),
        label: "Key 1".to_string(),
        value: "local-key".to_string(),
        enabled: true,
        created_at: 1,
    });
    config.terminal_syncs.push(TerminalSyncRecord {
        provider_id: "p1".to_string(),
        tool: "opencode".to_string(),
        synced_key_id: "k1".to_string(),
        synced_base_url: "http://127.0.0.1:17688/v1".to_string(),
        synced_at: 1,
    });

    let pm_index = config
        .providers
        .iter()
        .position(|provider| provider.id == "pm")
        .expect("the manual provider must exist");
    let po_index = config
        .providers
        .iter()
        .position(|provider| provider.id == "po")
        .expect("the other-template provider must exist");
    let pm_before = serde_json::to_value(&config.providers[pm_index]).expect("encode pm before");
    let po_before = serde_json::to_value(&config.providers[po_index]).expect("encode po before");
    let keys_before = serde_json::to_value(&config.keys).expect("encode keys before");
    let terminal_before =
        serde_json::to_value(&config.terminal_syncs).expect("encode terminal syncs before");

    let body = json!({"data": [{"id": "A"}]}).to_string();
    apply_template_sync_with(&mut config, "t", |_t| Ok(body.clone()), |_n| Ok(()))
        .expect("the sync must succeed");

    let mut expected_m = mapping_m.clone();
    expected_m.enabled = false;
    for id in ["p1", "p2"] {
        let provider = config
            .providers
            .iter()
            .find(|provider| provider.id == id)
            .unwrap_or_else(|| panic!("provider {id} must exist"));

        let m = find_mapping(provider, "M")
            .unwrap_or_else(|| panic!("provider {id} must keep the retired mapping"));
        assert!(
            !m.enabled,
            "provider {id}: a mapping for a model the sync removed must be disabled"
        );
        assert_eq!(
            m, &expected_m,
            "provider {id}: retirement must only flip the enabled flag"
        );

        assert_eq!(
            find_mapping(provider, "A"),
            Some(&mapping_a),
            "provider {id}: an already-disabled mapping must stay unchanged"
        );
        assert_eq!(
            find_mapping(provider, "manual-only"),
            Some(&mapping_manual),
            "provider {id}: a manual mapping must stay unchanged"
        );
        assert!(
            find_mapping(provider, "ignored-only").is_none(),
            "provider {id}: an ignored model the sync removed must stay mapping-free"
        );
        assert_eq!(
            provider.ignored_models, ignored_before,
            "provider {id}: the ignored set must stay exactly as it was"
        );
    }

    assert_eq!(
        serde_json::to_value(&config.providers[pm_index]).expect("encode pm after"),
        pm_before,
        "a manual provider must be byte-for-byte unchanged"
    );
    assert_eq!(
        serde_json::to_value(&config.providers[po_index]).expect("encode po after"),
        po_before,
        "a provider bound to another template must be byte-for-byte unchanged"
    );
    assert_eq!(
        serde_json::to_value(&config.keys).expect("encode keys after"),
        keys_before,
        "local keys must be byte-for-byte unchanged"
    );
    assert_eq!(
        serde_json::to_value(&config.terminal_syncs).expect("encode terminal syncs after"),
        terminal_before,
        "terminal-sync records must be byte-for-byte unchanged"
    );
}

/// REQ-003 / AC-004: retirement is one-way. A model a sync removed stays
/// disabled when a later sync returns it, and a mapping the operator explicitly
/// re-enables survives later syncs that still list its model.
#[test]
fn retired_disabled_mappings_never_re_enable_on_return_or_later_syncs() {
    let previous = template_with_models(
        "t",
        Some(SYNC_URL),
        UpstreamProtocol::ChatCompletions,
        vec![
            template_model("A", Some("Local A"), None, true),
            template_model("M", Some("Local M"), None, true),
        ],
    );
    let mut config = GatewayConfig::default();
    seed_template(&mut config, previous.clone());
    let mut provider = bound_provider("p", "t");
    provider.mappings = vec![
        mapping_for(&previous.models[0], &previous),
        mapping_for(&previous.models[1], &previous),
    ];
    config.providers.push(provider);

    let sync = |config: &mut GatewayConfig, ids: &[&str]| {
        let data: Vec<Value> = ids.iter().map(|id| json!({"id": id})).collect();
        let body = json!({ "data": data }).to_string();
        apply_template_sync_with(config, "t", |_t| Ok(body.clone()), |_n| Ok(()))
            .expect("the sync must succeed");
    };

    // 1. The first sync removes M and disables its mapping.
    sync(&mut config, &["A"]);
    {
        let provider = config
            .providers
            .iter()
            .find(|provider| provider.id == "p")
            .expect("the bound provider must exist");
        let m = find_mapping(provider, "M").expect("the retired mapping must be kept");
        assert!(
            !m.enabled,
            "the removed model's mapping must be disabled by the sync"
        );
    }

    // 2. A later sync returning M must not re-enable it.
    sync(&mut config, &["A", "M"]);
    {
        let provider = config
            .providers
            .iter()
            .find(|provider| provider.id == "p")
            .expect("the bound provider must exist");
        let m = find_mapping(provider, "M").expect("the retired mapping must be kept");
        assert!(
            !m.enabled,
            "a sync must never re-enable a mapping it retired"
        );
    }

    // 3. The operator explicitly re-enables M through the provider dialog.
    {
        let provider = config
            .providers
            .iter_mut()
            .find(|provider| provider.id == "p")
            .expect("the bound provider must exist");
        let m = provider
            .mappings
            .iter_mut()
            .find(|mapping| mapping.upstream_model == "M")
            .expect("the retired mapping must exist");
        m.enabled = true;
    }

    // 4. A later sync still listing M must leave the user's enable in place.
    sync(&mut config, &["A", "M"]);
    let provider = config
        .providers
        .iter()
        .find(|provider| provider.id == "p")
        .expect("the bound provider must exist");
    let m = find_mapping(provider, "M").expect("the explicitly enabled mapping must be kept");
    assert!(
        m.enabled,
        "a sync must not disable a mapping whose model is still in the template"
    );
}

/// AC-013: a source-provided display name replaces the template value, but a
/// mapping the operator renamed locally keeps its name.
#[test]
fn sync_source_names_replace_template_values_while_local_mapping_renames_survive() {
    let previous = template_with_models(
        "t",
        Some(SYNC_URL),
        UpstreamProtocol::ChatCompletions,
        vec![
            template_model("a", Some("Tpl A"), None, true),
            template_model("b", Some("Tpl B"), None, true),
        ],
    );
    let mut config = GatewayConfig::default();
    config.provider_templates.push(ProviderTemplateState {
        template_id: "t".to_string(),
        template: Some(previous.clone()),
        synced_at: Some(1),
        source: Some(SYNC_URL.to_string()),
    });

    let mut provider = bound_provider("p", "t");
    let mapping_a = mapping_for(&previous.models[0], &previous);
    let mut mapping_b = mapping_for(&previous.models[1], &previous);
    mapping_b.display_name = Some("My B".to_string());
    provider.mappings = vec![mapping_a, mapping_b];
    config.providers.push(provider);

    let body = json!({"data": [{"id": "a", "name": "Source A"}, {"id": "b", "name": "Source B"}]})
        .to_string();
    let view = apply_template_sync_with(&mut config, "t", |_t| Ok(body.clone()), |_n| Ok(()))
        .expect("the sync must succeed");

    let template_a = view
        .template
        .models
        .iter()
        .find(|model| model.upstream_model == "a")
        .expect("template a");
    let template_b = view
        .template
        .models
        .iter()
        .find(|model| model.upstream_model == "b")
        .expect("template b");
    assert_eq!(template_a.display_name.as_deref(), Some("Source A"));
    assert_eq!(template_b.display_name.as_deref(), Some("Source B"));

    let provider = config
        .providers
        .iter()
        .find(|provider| provider.id == "p")
        .expect("the bound provider must exist");
    assert_eq!(
        find_mapping(provider, "a").expect("mapping a").display_name.as_deref(),
        Some("Source A"),
        "a mapping that still equals the template value follows the source"
    );
    assert_eq!(
        find_mapping(provider, "b").expect("mapping b").display_name.as_deref(),
        Some("My B"),
        "a locally renamed mapping keeps its name"
    );
}

// ---------------------------------------------------------------------------
// Create-from-template (REQ-006, AC-007)
// ---------------------------------------------------------------------------

/// REQ-006 / AC-007: creating a provider from a template copies only its
/// enabled models, derives each mapping's effective protocol, leaves the
/// default model empty and writes no price rows.
#[test]
fn create_from_template_copies_enabled_models_only_and_writes_no_prices() {
    let mut config = GatewayConfig::default();
    seed_template(
        &mut config,
        template_with_models(
            "t",
            None,
            UpstreamProtocol::ChatCompletions,
            vec![
                template_model(
                    "enabled-responses",
                    Some("Enabled Responses"),
                    Some(UpstreamProtocol::Responses),
                    true,
                ),
                template_model("enabled-inherit", Some("Enabled Inherit"), None, true),
                template_model(
                    "disabled-model",
                    Some("Disabled"),
                    Some(UpstreamProtocol::Responses),
                    false,
                ),
            ],
        ),
    );
    let captured: RefCell<Option<GatewayConfig>> = RefCell::new(None);

    let provider = apply_create_provider_from_template(
        &mut config,
        "t",
        "My Provider",
        "https://my.example.com/v1",
        UpstreamProtocol::ChatCompletions,
        "sk-secret",
        |next| {
            *captured.borrow_mut() = Some(next.clone());
            Ok(())
        },
    )
    .expect("a non-blank API key must create the provider");

    assert_eq!(provider.template_id.as_deref(), Some("t"));
    assert_eq!(provider.default_model, None, "the default model stays empty");
    assert!(provider.ignored_models.is_empty());
    assert_eq!(
        provider.mappings.len(),
        2,
        "only the enabled models produce mappings"
    );
    let responses = find_mapping(&provider, "enabled-responses").expect("enabled responses mapping");
    assert_eq!(responses.local_model, "enabled-responses");
    assert_eq!(responses.upstream_model, "enabled-responses");
    assert!(responses.enabled);
    assert_eq!(responses.display_name.as_deref(), Some("Enabled Responses"));
    assert_eq!(responses.protocol, Some(UpstreamProtocol::Responses));
    let inherit = find_mapping(&provider, "enabled-inherit").expect("enabled inherit mapping");
    assert_eq!(
        inherit.protocol, None,
        "a model matching the provider protocol stays as follow-the-provider"
    );
    assert!(
        find_mapping(&provider, "disabled-model").is_none(),
        "a disabled model produces no mapping"
    );
    assert!(
        config.model_prices.is_empty(),
        "creation must not write price rows"
    );

    let persisted = captured.into_inner().expect("creation must persist");
    assert!(
        persisted
            .providers
            .iter()
            .any(|candidate| candidate.id == provider.id),
        "the persisted config must contain the new provider"
    );
    assert!(
        persisted.model_prices.is_empty(),
        "the persisted config must carry no price rows"
    );
}

/// Creating a provider from a template must carry the gateway display name:
/// a missing template display name is completed from the upstream identifier
/// so the new mapping never stays empty.
#[test]
fn create_from_template_completes_missing_display_name() {
    let mut config = GatewayConfig::default();
    seed_template(
        &mut config,
        template_with_models(
            "t",
            None,
            UpstreamProtocol::ChatCompletions,
            vec![
                template_model("my-model-x", None, None, true),
                template_model("  ", None, None, true),
            ]
            .into_iter()
            .filter(|model| !model.upstream_model.trim().is_empty())
            .collect(),
        ),
    );

    let provider = apply_create_provider_from_template(
        &mut config,
        "t",
        "My Provider",
        "https://my.example.com/v1",
        UpstreamProtocol::ChatCompletions,
        "sk-secret",
        |_next| Ok(()),
    )
    .expect("creation must succeed");

    let mapping = provider
        .mappings
        .iter()
        .find(|mapping| mapping.upstream_model == "my-model-x")
        .expect("the enabled model must produce a mapping");
    assert_eq!(
        mapping.display_name.as_deref(),
        Some("My Model X"),
        "a missing template display name must be completed from the identifier"
    );
}

/// REQ-006 / AC-007: an empty or whitespace-only API key rejects the creation
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

/// REQ-006: a blank name or base URL falls back to the template value while
/// the protocol argument still wins.
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

// ---------------------------------------------------------------------------
// Delete and restore (REQ-008, AC-009)
// ---------------------------------------------------------------------------

/// REQ-008 / AC-009: deleting a model removes its mapping and its
/// provider-scoped price row for both bound and manual providers; a bound
/// provider records the model in the ignored set exactly once, and an unknown
/// provider is an error that writes nothing.
#[test]
fn delete_model_removes_mapping_and_price_row_for_bound_and_manual_providers() {
    // A bound provider that already ignores the model keeps exactly one record.
    let mut config = GatewayConfig::default();
    let mut bound = bound_provider("bound", "t");
    bound.mappings = vec![model_mapping("model-m")];
    bound.ignored_models = vec!["model-m".to_string()];
    config.providers.push(bound);
    config.model_prices.push(price_row("bound", "model-m"));

    apply_delete_provider_model(&mut config, "bound", "model-m", |_next| Ok(()))
        .expect("deleting a bound mapping must succeed");
    let bound = config
        .providers
        .iter()
        .find(|provider| provider.id == "bound")
        .expect("the bound provider must exist");
    assert!(
        find_mapping(bound, "model-m").is_none(),
        "the deleted mapping must be removed"
    );
    assert_eq!(
        bound
            .ignored_models
            .iter()
            .filter(|id| id.as_str() == "model-m")
            .count(),
        1,
        "the ignored record must be written exactly once"
    );
    assert!(
        find_price_row(&config, "bound", "model-m").is_none(),
        "the bound provider's price row must be removed"
    );

    // A manual provider removes the mapping and its row but writes no ignored record.
    let mut config = GatewayConfig::default();
    let mut manual = bound_provider("manual", "t");
    manual.template_id = None;
    manual.mappings = vec![model_mapping("model-m")];
    config.providers.push(manual);
    config.model_prices.push(price_row("manual", "model-m"));

    apply_delete_provider_model(&mut config, "manual", "model-m", |_next| Ok(()))
        .expect("deleting a manual mapping must succeed");
    let manual = config
        .providers
        .iter()
        .find(|provider| provider.id == "manual")
        .expect("the manual provider must exist");
    assert!(find_mapping(manual, "model-m").is_none());
    assert!(
        manual.ignored_models.is_empty(),
        "a manual provider must not record an ignored model"
    );
    assert!(
        find_price_row(&config, "manual", "model-m").is_none(),
        "a manual provider's price row is removed with the mapping"
    );

    // An unknown provider is an error that writes nothing.
    let before = serde_json::to_value(&config).expect("encode config");
    let error = apply_delete_provider_model(&mut config, "no-such-provider", "model-m", |_next| {
        Ok(())
    })
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

/// REQ-008 / AC-009 counterexample: after deleting a model, a later sync whose
/// source still lists it must not resurrect the mapping or a price row, while
/// other source models still propagate.
#[test]
fn delete_model_survives_later_template_sync() {
    let previous = template_with_models(
        "t",
        Some(SYNC_URL),
        UpstreamProtocol::ChatCompletions,
        vec![template_model("model-m", Some("Model M"), None, true)],
    );
    let mut config = GatewayConfig::default();
    config.provider_templates.push(ProviderTemplateState {
        template_id: "t".to_string(),
        template: Some(previous.clone()),
        synced_at: Some(1),
        source: Some(SYNC_URL.to_string()),
    });
    let mut provider = bound_provider("p", "t");
    provider.mappings = vec![mapping_for(&previous.models[0], &previous)];
    config.providers.push(provider);
    config.model_prices.push(price_row("p", "model-m"));

    apply_delete_provider_model(&mut config, "p", "model-m", |_next| Ok(()))
        .expect("deleting a mapped model must succeed");

    let body = json!({
        "data": [
            {"id": "model-m", "name": "Model M"},
            {"id": "other-model", "name": "Other"}
        ]
    })
    .to_string();
    apply_template_sync_with(&mut config, "t", |_t| Ok(body.clone()), |_n| Ok(()))
        .expect("the sync must succeed");

    let provider = config
        .providers
        .iter()
        .find(|provider| provider.id == "p")
        .expect("the bound provider must exist");
    assert!(
        find_mapping(provider, "model-m").is_none(),
        "a template sync must not resurrect a deleted model"
    );
    assert!(
        provider
            .ignored_models
            .iter()
            .any(|id| id == "model-m"),
        "the ignored record must survive the sync"
    );
    assert!(
        find_mapping(provider, "other-model").is_some(),
        "the sync must still propagate the source's other models"
    );
    assert!(
        find_price_row(&config, "p", "model-m").is_none(),
        "an ignored model's price row must not be resurrected"
    );
    assert!(
        find_price_row(&config, "p", "other-model").is_none(),
        "a sync must not create a price row for a propagated model"
    );
}

/// REQ-008 / AC-009: restoring an ignored model rebuilds its mapping only — it
/// leaves the ignored set and never writes or modifies a price row.
#[test]
fn restore_model_rebuilds_the_mapping_without_touching_prices() {
    let mut config = GatewayConfig::default();
    seed_template(
        &mut config,
        template_with_models(
            "t",
            Some(SYNC_URL),
            UpstreamProtocol::ChatCompletions,
            vec![template_model(
                "model-m",
                Some("Current Official"),
                Some(UpstreamProtocol::Responses),
                true,
            )],
        ),
    );
    let mut provider = bound_provider("p", "t");
    provider.ignored_models = vec!["model-m".to_string()];
    config.providers.push(provider);
    let prices_before = config.model_prices.clone();

    apply_restore_provider_model(&mut config, "p", "model-m", |_next| Ok(()))
        .expect("an ignored model must be restorable");

    let provider = config
        .providers
        .iter()
        .find(|provider| provider.id == "p")
        .expect("the bound provider must exist");
    assert!(
        !provider.ignored_models.iter().any(|id| id == "model-m"),
        "the restored model must leave the ignored set"
    );
    let mapping = find_mapping(provider, "model-m").expect("the restored mapping must exist");
    assert!(mapping.enabled, "a restored mapping is enabled");
    assert_eq!(mapping.local_model, "model-m");
    assert_eq!(mapping.upstream_model, "model-m");
    assert_eq!(mapping.display_name.as_deref(), Some("Current Official"));
    assert_eq!(mapping.protocol, Some(UpstreamProtocol::Responses));
    assert_eq!(
        config.model_prices, prices_before,
        "restore must not write or modify a price row"
    );
}

/// Restore boundary / REQ-008: an ignored model the template no longer carries
/// reports an actionable error and writes nothing.
#[test]
fn restore_model_missing_from_template_reports_error_and_writes_nothing() {
    let mut config = GatewayConfig::default();
    seed_template(
        &mut config,
        template_with_models(
            "t",
            Some(SYNC_URL),
            UpstreamProtocol::ChatCompletions,
            vec![],
        ),
    );
    let mut provider = bound_provider("p", "t");
    provider.ignored_models = vec!["ghost-model".to_string()];
    config.providers.push(provider);

    let before = serde_json::to_value(&config).expect("encode config before");
    let error = apply_restore_provider_model(&mut config, "p", "ghost-model", |_next| Ok(()))
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
        .find(|provider| provider.id == "p")
        .expect("the bound provider must exist");
    assert!(
        provider.mappings.is_empty(),
        "no mapping may be created for a model the template does not have"
    );
}

/// Restore boundary / REQ-008: only an ignored model may be restored; asking to
/// restore a model that is still present is an error that writes nothing.
#[test]
fn restore_model_not_ignored_reports_error() {
    let mut config = GatewayConfig::default();
    seed_template(
        &mut config,
        template_with_models(
            "t",
            Some(SYNC_URL),
            UpstreamProtocol::ChatCompletions,
            vec![template_model("model-m", Some("Model M"), None, true)],
        ),
    );
    let mut provider = bound_provider("p", "t");
    provider.mappings = vec![model_mapping("model-m")];
    config.providers.push(provider);
    config.model_prices.push(price_row("p", "model-m"));

    let before = serde_json::to_value(&config).expect("encode config before");
    let error = apply_restore_provider_model(&mut config, "p", "model-m", |_next| Ok(()))
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

/// REQ-008 / AC-009: a persistence failure in any maintenance operation returns
/// an error and leaves the whole configuration field-for-field identical.
#[test]
fn maintenance_persist_failure_leaves_config_unchanged() {
    // create
    {
        let mut config = GatewayConfig::default();
        seed_template(
            &mut config,
            template_with_models(
                "t",
                None,
                UpstreamProtocol::ChatCompletions,
                vec![template_model("model-m", Some("Model M"), None, true)],
            ),
        );
        let before = serde_json::to_value(&config).expect("encode config before");
        let error = apply_create_provider_from_template(
            &mut config,
            "t",
            "My Provider",
            "https://my.example.com/v1",
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
        let mut config = GatewayConfig::default();
        seed_template(
            &mut config,
            template_with_models("t", Some(SYNC_URL), UpstreamProtocol::ChatCompletions, vec![]),
        );
        let mut provider = bound_provider("p", "t");
        provider.mappings = vec![model_mapping("model-m")];
        config.providers.push(provider);
        config.model_prices.push(price_row("p", "model-m"));

        let before = serde_json::to_value(&config).expect("encode config before");
        let error = apply_delete_provider_model(&mut config, "p", "model-m", |_next| {
            Err("disk full".to_string())
        })
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
        let mut config = GatewayConfig::default();
        seed_template(
            &mut config,
            template_with_models(
                "t",
                Some(SYNC_URL),
                UpstreamProtocol::ChatCompletions,
                vec![template_model("model-m", Some("Model M"), None, true)],
            ),
        );
        let mut provider = bound_provider("p", "t");
        provider.ignored_models = vec!["model-m".to_string()];
        config.providers.push(provider);

        let before = serde_json::to_value(&config).expect("encode config before");
        let error = apply_restore_provider_model(&mut config, "p", "model-m", |_next| {
            Err("disk full".to_string())
        })
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
// Template maintenance commands (REQ-007 backing store)
// ---------------------------------------------------------------------------

/// REQ-007: upserting edits a built-in and adds a custom template without any
/// snapshot version or price field.
#[test]
fn test_template_upsert_edits_existing_and_creates_new() {
    let mut config = GatewayConfig::default();
    let mut modified_builtin = builtin_templates()
        .expect("built-in templates")
        .into_iter()
        .find(|template| template.id == "opencode-zen")
        .expect("opencode-zen exists");

    modified_builtin.name = "OpenCode Zen Custom".to_string();
    modified_builtin.base_url = "https://custom.zen/v1".to_string();

    let views = apply_upsert_provider_template(&mut config, modified_builtin, |_| Ok(()))
        .expect("upsert should succeed");

    let zen_view = views
        .iter()
        .find(|view| view.template.id == "opencode-zen")
        .expect("the opencode-zen view must exist");
    assert_eq!(zen_view.template.name, "OpenCode Zen Custom");
    assert_eq!(zen_view.template.base_url, "https://custom.zen/v1");
    assert!(!zen_view.from_snapshot);

    let new_custom = ProviderTemplate {
        id: "my-custom-tpl".to_string(),
        name: "My Custom Template".to_string(),
        description: "A custom test template".to_string(),
        base_url: "https://myapi.com/v1".to_string(),
        protocol: UpstreamProtocol::ChatCompletions,
        source: String::new(),
        models_url: None,
        models: vec![],
        icon: Some("openai".to_string()),
    };

    let views2 = apply_upsert_provider_template(&mut config, new_custom, |_| Ok(()))
        .expect("custom template upsert should succeed");

    let custom_view = views2
        .iter()
        .find(|view| view.template.id == "my-custom-tpl")
        .expect("the custom template must be present");
    assert_eq!(custom_view.template.name, "My Custom Template");
    assert_eq!(custom_view.template.icon.as_deref(), Some("openai"));
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
    let views = apply_delete_provider_template(&mut config, "commandcode", |_| Ok(()))
        .expect("delete should succeed");

    assert!(views.iter().all(|view| view.template.id != "commandcode"));
    assert!(config.deleted_template_ids.contains(&"commandcode".to_string()));

    let restored_views = apply_reset_provider_templates(&mut config, |_| Ok(()))
        .expect("reset should succeed");

    assert!(restored_views
        .iter()
        .any(|view| view.template.id == "commandcode"));
    assert!(config.deleted_template_ids.is_empty());
}

// ---------------------------------------------------------------------------
// Synced model-name completion (REQ-001, REQ-002, REQ-003, AC-001..AC-006, AC-008)
// ---------------------------------------------------------------------------

/// AC-001 / REQ-001: a source name that stops short of the upstream identifier
/// is completed with the identifier segment's uncovered remainder, while the
/// stored upstream identifier stays untouched.
#[test]
fn sync_completes_a_truncated_source_name_with_the_identifier_remainder() {
    let models = sync_models(
        json!({"data": [{"id": "poolside/laguna-s-2.1-free", "name": "Laguna S 2.1"}]}),
    );
    assert_eq!(models.len(), 1);
    let model = &models[0];
    assert_eq!(model.upstream_model, "poolside/laguna-s-2.1-free");
    assert_eq!(
        model.display_name.as_deref(),
        Some("Laguna S 2.1 Free"),
        "the display name must cover the identifier's trailing `free` token"
    );
}

/// AC-002 / REQ-002: a source name that already covers its identifier segment
/// is stored byte-identical (trimmed only): no vendor prefix is added and no
/// word is duplicated, even when the name carries extra content.
#[test]
fn sync_keeps_already_complete_source_names_byte_identical() {
    let models = sync_models(json!({"data": [
        {"id": "gpt-5.6-sol", "name": "GPT-5.6 Sol"},
        {"id": "Qwen/Qwen3.8-Max-0902", "name": "Qwen 3.8 Max 0902"},
        {"id": "zai-org/GLM-5.3", "name": "GLM-5.3"},
        {"id": "deepseek/deepseek-v4-pro", "name": "DeepSeek V4 Pro (latest)"},
        {"id": "xiaomi/mimo-v2.5-pro", "name": "MiMo V2.5 Pro"}
    ]}));
    let expected = [
        ("gpt-5.6-sol", "GPT-5.6 Sol"),
        ("Qwen/Qwen3.8-Max-0902", "Qwen 3.8 Max 0902"),
        ("zai-org/GLM-5.3", "GLM-5.3"),
        ("deepseek/deepseek-v4-pro", "DeepSeek V4 Pro (latest)"),
        ("xiaomi/mimo-v2.5-pro", "MiMo V2.5 Pro"),
    ];
    assert_eq!(models.len(), expected.len());
    for (model, (identifier, name)) in models.iter().zip(expected.iter()) {
        assert_eq!(model.upstream_model, *identifier);
        assert_eq!(
            model.display_name.as_deref(),
            Some(*name),
            "the complete source name for {identifier} must be preserved verbatim"
        );
    }
}

/// AC-003 / REQ-003: a missing or blank source name completes the previously
/// stored display name for the same identifier, and an identifier with no
/// stored name at all is transformed from its last path segment.
#[test]
fn sync_completes_a_stored_name_and_transforms_bare_identifiers_for_missing_names() {
    // An id-only entry completes the previously stored display name.
    let mut config = seeded_config(
        UpstreamProtocol::ChatCompletions,
        vec![template_model(
            "tencent/hy3-paid",
            Some("Tencent Hy3"),
            None,
            true,
        )],
    );
    let body = json!({"data": [{"id": "tencent/hy3-paid"}]}).to_string();
    let view = apply_template_sync_with(&mut config, "t", |_t| Ok(body.clone()), |_n| Ok(()))
        .expect("the id-only payload must parse");
    let stored = view
        .template
        .models
        .iter()
        .find(|model| model.upstream_model == "tencent/hy3-paid")
        .expect("the synced model must be present");
    assert_eq!(
        stored.display_name.as_deref(),
        Some("Tencent Hy3 Paid"),
        "the stored base name must gain the identifier's uncovered `paid` token"
    );

    // A bare identifier with no stored name is transformed on its own.
    let bare = sync_models(json!({"data": [{"id": "hy3-paid"}]}));
    assert_eq!(bare.len(), 1);
    assert_eq!(
        bare[0].display_name.as_deref(),
        Some("Hy3 Paid"),
        "a name-less entry without a stored name stores the transformed segment"
    );

    // A string entry with no stored name behaves the same way.
    let string_entry = sync_models(json!({"data": ["hy3-paid"]}));
    assert_eq!(string_entry.len(), 1);
    assert_eq!(
        string_entry[0].display_name.as_deref(),
        Some("Hy3 Paid"),
        "a string entry without a stored name stores the transformed segment"
    );

    // A whitespace-only source name counts as missing.
    let blank = sync_models(json!({"data": [{"id": "hy3-paid", "name": "   "}]}));
    assert_eq!(blank.len(), 1);
    assert_eq!(
        blank[0].display_name.as_deref(),
        Some("Hy3 Paid"),
        "a blank source name must fall back to the transformed segment"
    );
}

/// AC-004 / REQ-004: a sync that changes only the display name leaves the
/// local model id, the upstream model, the protocol, the enabled flag, the
/// price rows and the ignored set byte-identical.
#[test]
fn sync_changing_only_the_display_name_leaves_every_other_field_byte_identical() {
    let previous = ProviderTemplateModel {
        upstream_model: "poolside/laguna-s-2.1-free".to_string(),
        local_model: Some("laguna-local".to_string()),
        display_name: Some("Laguna S 2.1".to_string()),
        protocol: Some(UpstreamProtocol::Responses),
        enabled: false,
        ..ProviderTemplateModel::default()
    };
    let mut config = seeded_config(UpstreamProtocol::ChatCompletions, vec![previous.clone()]);
    let mut provider = bound_provider("p", "t");
    provider.mappings = vec![model_mapping("other-model")];
    provider.ignored_models = vec!["other-model".to_string()];
    config.providers.push(provider);
    config.model_prices = vec![
        price_row("p", "poolside/laguna-s-2.1-free"),
        price_row("p", "other-model"),
    ];
    let prices_before = config.model_prices.clone();
    let ignored_before = config.providers[0].ignored_models.clone();

    let body = json!({"data": [{"id": "poolside/laguna-s-2.1-free", "name": "Laguna S 2.1"}]})
        .to_string();
    let view = apply_template_sync_with(&mut config, "t", |_t| Ok(body.clone()), |_n| Ok(()))
        .expect("the sync must succeed");

    assert_eq!(view.template.models.len(), 1);
    let model = &view.template.models[0];
    assert_eq!(
        model.display_name.as_deref(),
        Some("Laguna S 2.1 Free"),
        "only the display name may change"
    );
    assert_eq!(
        model.local_model, previous.local_model,
        "the local model id is never touched"
    );
    assert_eq!(
        model.upstream_model, previous.upstream_model,
        "the upstream model is never touched"
    );
    assert_eq!(
        model.protocol, previous.protocol,
        "an unlabeled entry keeps the previous protocol"
    );
    assert_eq!(
        model.enabled, previous.enabled,
        "the local enabled flag always survives"
    );
    assert_eq!(
        config.model_prices, prices_before,
        "a sync must never create or modify a price row"
    );
    assert_eq!(
        config.providers[0].ignored_models, ignored_before,
        "a sync must never touch the ignored set"
    );
}

/// AC-005 / REQ-005: the completed template name reaches a derived mapping
/// that still equals the previous template value, while a locally renamed
/// mapping keeps the operator's name.
#[test]
fn sync_completed_names_reach_only_untouched_derived_mappings() {
    let previous = template_with_models(
        "t",
        Some(SYNC_URL),
        UpstreamProtocol::ChatCompletions,
        vec![
            template_model(
                "poolside/laguna-s-2.1-free",
                Some("Laguna S 2.1"),
                None,
                true,
            ),
            template_model("tencent/hy3-paid", Some("Tencent Hy3"), None, true),
        ],
    );
    let mut config = GatewayConfig::default();
    config.provider_templates.push(ProviderTemplateState {
        template_id: "t".to_string(),
        template: Some(previous.clone()),
        synced_at: Some(1),
        source: Some(SYNC_URL.to_string()),
    });

    let mut provider = bound_provider("p", "t");
    let mapping_a = mapping_for(&previous.models[0], &previous);
    let mut mapping_b = mapping_for(&previous.models[1], &previous);
    mapping_b.display_name = Some("My Hy3".to_string());
    provider.mappings = vec![mapping_a, mapping_b];
    config.providers.push(provider);

    let body = json!({"data": [
        {"id": "poolside/laguna-s-2.1-free", "name": "Laguna S 2.1"},
        {"id": "tencent/hy3-paid", "name": "Tencent Hy3"}
    ]})
    .to_string();
    let view = apply_template_sync_with(&mut config, "t", |_t| Ok(body.clone()), |_n| Ok(()))
        .expect("the sync must succeed");

    let template_name = |id: &str| {
        view.template
            .models
            .iter()
            .find(|model| model.upstream_model == id)
            .unwrap_or_else(|| panic!("the template model {id} must be present"))
            .display_name
            .clone()
    };
    assert_eq!(
        template_name("poolside/laguna-s-2.1-free").as_deref(),
        Some("Laguna S 2.1 Free")
    );
    assert_eq!(
        template_name("tencent/hy3-paid").as_deref(),
        Some("Tencent Hy3 Paid")
    );

    let provider = config
        .providers
        .iter()
        .find(|provider| provider.id == "p")
        .expect("the bound provider must exist");
    assert_eq!(
        find_mapping(provider, "poolside/laguna-s-2.1-free")
            .expect("mapping A")
            .display_name
            .as_deref(),
        Some("Laguna S 2.1 Free"),
        "a mapping that still equals the previous template value takes the completed name"
    );
    assert_eq!(
        find_mapping(provider, "tencent/hy3-paid")
            .expect("mapping B")
            .display_name
            .as_deref(),
        Some("My Hy3"),
        "a locally renamed mapping keeps the operator's name"
    );
}

/// AC-006 / REQ-006: loading a configuration written before this change
/// rewrites no persisted name, and parsing the same model-list response twice
/// produces byte-identical completed names (never a doubled suffix).
#[test]
fn persisted_names_survive_load_and_repeat_syncs_keep_completed_names_byte_identical() {
    let persisted = json!({
        "provider_templates": [{
            "template_id": "t",
            "synced_at": 1,
            "source": SYNC_URL,
            "template": {
                "id": "t",
                "name": "T",
                "base_url": "https://tpl.example.com/v1",
                "models": [{
                    "upstream_model": "poolside/laguna-s-2.1-free",
                    "display_name": "Laguna S 2.1"
                }]
            }
        }]
    });
    let loaded: GatewayConfig =
        serde_json::from_value(persisted).expect("an older config must load");
    let round_tripped: GatewayConfig = serde_json::from_value(
        serde_json::to_value(&loaded).expect("encode the loaded config"),
    )
    .expect("the loaded config must round-trip");
    let stored = round_tripped
        .provider_templates
        .iter()
        .find(|state| state.template_id == "t")
        .expect("the template state must be present")
        .template
        .as_ref()
        .expect("the template must be present")
        .models
        .iter()
        .find(|model| model.upstream_model == "poolside/laguna-s-2.1-free")
        .expect("the stored model must be present");
    assert_eq!(
        stored.display_name.as_deref(),
        Some("Laguna S 2.1"),
        "loading without a sync must not rewrite a persisted name"
    );

    let mut config = seeded_config(
        UpstreamProtocol::ChatCompletions,
        vec![template_model(
            "poolside/laguna-s-2.1-free",
            Some("Laguna S 2.1"),
            None,
            true,
        )],
    );
    let body = json!({"data": [{"id": "poolside/laguna-s-2.1-free", "name": "Laguna S 2.1"}]})
        .to_string();
    let sync_once = |config: &mut GatewayConfig| {
        apply_template_sync_with(config, "t", |_t| Ok(body.clone()), |_n| Ok(()))
            .expect("the sync must succeed")
            .template
            .models
            .iter()
            .find(|model| model.upstream_model == "poolside/laguna-s-2.1-free")
            .expect("the synced model must be present")
            .display_name
            .clone()
    };
    let first = sync_once(&mut config);
    let second = sync_once(&mut config);
    assert_eq!(
        first.as_deref(),
        Some("Laguna S 2.1 Free"),
        "the first sync completes the name"
    );
    assert_eq!(
        second, first,
        "parsing the same response twice must produce byte-identical names"
    );
    assert!(
        second
            .as_deref()
            .is_none_or(|name| !name.contains("Free Free")),
        "a repeat sync must never append the remainder twice"
    );
}

/// AC-008 / REQ-008: the verbatim live catalog captured from
/// `https://api.commandcode.ai/provider/v1/models` on 2026-09-20 (71 entries).
/// Every synced display name must cover its identifier segment's
/// alphanumerics in order, and the already-complete AC-002 names stay
/// byte-identical.
const LIVE_MODEL_CATALOG_2026_09_20: &str = r#"{"object":"list","data":[{"id":"claude-sonnet-5","object":"model","created":1789872913,"owned_by":"command-code","name":"Claude Sonnet 5","context_length":1000000,"supported_endpoints":["/messages"]},{"id":"claude-sonnet-4-6","object":"model","created":1789872913,"owned_by":"command-code","name":"Claude Sonnet 4.6","context_length":1000000,"supported_endpoints":["/messages"]},{"id":"claude-fable-5-1","object":"model","created":1789872913,"owned_by":"command-code","name":"Claude Fable 5.1","context_length":1000000,"supported_endpoints":["/messages"]},{"id":"claude-fable-5","object":"model","created":1789872913,"owned_by":"command-code","name":"Claude Fable 5","context_length":1000000,"supported_endpoints":["/messages"]},{"id":"claude-opus-5","object":"model","created":1789872913,"owned_by":"command-code","name":"Claude Opus 5","context_length":1000000,"supported_endpoints":["/messages"]},{"id":"claude-opus-4-8","object":"model","created":1789872913,"owned_by":"command-code","name":"Claude Opus 4.8","context_length":1000000,"supported_endpoints":["/messages"]},{"id":"claude-opus-4-7","object":"model","created":1789872913,"owned_by":"command-code","name":"Claude Opus 4.7","context_length":1000000,"supported_endpoints":["/messages"]},{"id":"claude-haiku-4-5-20251001","object":"model","created":1789872913,"owned_by":"command-code","name":"Claude Haiku 4.5","context_length":200000,"supported_endpoints":["/messages"]},{"id":"gpt-5.6-sol","object":"model","created":1789872913,"owned_by":"command-code","name":"GPT-5.6 Sol","context_length":1050000,"supported_endpoints":["/chat/completions","/responses"]},{"id":"gpt-5.6-terra","object":"model","created":1789872913,"owned_by":"command-code","name":"GPT-5.6 Terra","context_length":1050000,"supported_endpoints":["/chat/completions","/responses"]},{"id":"gpt-5.6-luna","object":"model","created":1789872913,"owned_by":"command-code","name":"GPT-5.6 Luna","context_length":1050000,"supported_endpoints":["/chat/completions","/responses"]},{"id":"gpt-5.5","object":"model","created":1789872913,"owned_by":"command-code","name":"GPT-5.5","context_length":400000,"supported_endpoints":["/chat/completions","/responses"]},{"id":"gpt-5.4","object":"model","created":1789872913,"owned_by":"command-code","name":"GPT-5.4","context_length":400000,"supported_endpoints":["/chat/completions","/responses"]},{"id":"gpt-5.3-codex","object":"model","created":1789872913,"owned_by":"command-code","name":"GPT-5.3 Codex","context_length":400000,"supported_endpoints":["/chat/completions","/responses"]},{"id":"gpt-5.4-mini","object":"model","created":1789872913,"owned_by":"command-code","name":"GPT-5.4 Mini","context_length":400000,"supported_endpoints":["/chat/completions","/responses"]},{"id":"deepseek/deepseek-v4-pro","object":"model","created":1789872913,"owned_by":"command-code","name":"DeepSeek V4 Pro (latest)","context_length":1000000,"supported_endpoints":["/chat/completions","/responses"]},{"id":"deepseek/deepseek-v4-flash","object":"model","created":1789872913,"owned_by":"command-code","name":"DeepSeek V4 Flash (latest)","context_length":1000000,"supported_endpoints":["/chat/completions","/responses"]},{"id":"deepseek/deepseek-v4-flash-vision-exp","object":"model","created":1789872913,"owned_by":"command-code","name":"DeepSeek V4 Flash Vision (exp)","context_length":1000000,"supported_endpoints":["/chat/completions","/responses"]},{"id":"deepseek/deepseek-v4-flash-fast","object":"model","created":1789872913,"owned_by":"command-code","name":"DeepSeek V4 Flash Fast","context_length":1000000,"supported_endpoints":["/chat/completions"]},{"id":"deepseek/deepseek-v4.1-flash","object":"model","created":1789872913,"owned_by":"command-code","name":"DeepSeek V4.1 Flash","context_length":1000000,"supported_endpoints":["/chat/completions","/responses"]},{"id":"moonshotai/Kimi-K3","object":"model","created":1789872913,"owned_by":"command-code","name":"Kimi K3","context_length":1000000,"supported_endpoints":["/chat/completions","/responses"]},{"id":"moonshotai/Kimi-K2.7-Code","object":"model","created":1789872913,"owned_by":"command-code","name":"Kimi K2.7 Code","context_length":256000,"supported_endpoints":["/chat/completions","/responses"]},{"id":"moonshotai/Kimi-K2.7-Code-Highspeed","object":"model","created":1789872913,"owned_by":"command-code","name":"Kimi K2.7 Code HighSpeed","context_length":262000,"supported_endpoints":["/chat/completions","/responses"]},{"id":"moonshotai/Kimi-K2.6","object":"model","created":1789872913,"owned_by":"command-code","name":"Kimi K2.6","context_length":256000,"supported_endpoints":["/chat/completions","/responses"]},{"id":"moonshotai/Kimi-K2.5","object":"model","created":1789872913,"owned_by":"command-code","name":"Kimi K2.5","context_length":256000,"supported_endpoints":["/chat/completions","/responses"]},{"id":"z-ai/glm-5.3-flash","object":"model","created":1789872913,"owned_by":"command-code","name":"GLM-5.3 Flash","context_length":1048576,"supported_endpoints":["/chat/completions","/responses"]},{"id":"z-ai/glm-5.3-flashx","object":"model","created":1789872913,"owned_by":"command-code","name":"GLM-5.3 FlashX","context_length":1000000,"supported_endpoints":["/chat/completions","/responses"]},{"id":"zai-org/GLM-5.3","object":"model","created":1789872913,"owned_by":"command-code","name":"GLM-5.3","context_length":1000000,"supported_endpoints":["/chat/completions","/responses"]},{"id":"zai-org/GLM-5.2","object":"model","created":1789872913,"owned_by":"command-code","name":"GLM-5.2","context_length":1000000,"supported_endpoints":["/chat/completions","/responses"]},{"id":"zai-org/GLM-5.2-Fast","object":"model","created":1789872913,"owned_by":"command-code","name":"GLM-5.2 Fast","context_length":1000000,"supported_endpoints":["/chat/completions","/responses"]},{"id":"zai-org/GLM-5.1","object":"model","created":1789872913,"owned_by":"command-code","name":"GLM-5.1","context_length":200000,"supported_endpoints":["/chat/completions","/responses"]},{"id":"zai-org/GLM-5","object":"model","created":1789872913,"owned_by":"command-code","name":"GLM-5","context_length":200000,"supported_endpoints":["/chat/completions","/responses"]},{"id":"MiniMaxAI/MiniMax-M3","object":"model","created":1789872913,"owned_by":"command-code","name":"MiniMax M3","context_length":1000000,"supported_endpoints":["/chat/completions","/responses"]},{"id":"MiniMaxAI/MiniMax-M2.7","object":"model","created":1789872913,"owned_by":"command-code","name":"MiniMax M2.7","context_length":200000,"supported_endpoints":["/chat/completions","/responses"]},{"id":"MiniMaxAI/MiniMax-M2.5","object":"model","created":1789872913,"owned_by":"command-code","name":"MiniMax M2.5","context_length":200000,"supported_endpoints":["/chat/completions","/responses"]},{"id":"xiaomi/mimo-v2.5-pro","object":"model","created":1789872913,"owned_by":"command-code","name":"MiMo V2.5 Pro","context_length":1000000,"supported_endpoints":["/chat/completions","/responses"]},{"id":"xiaomi/mimo-v2.5","object":"model","created":1789872913,"owned_by":"command-code","name":"MiMo V2.5","context_length":1000000,"supported_endpoints":["/chat/completions","/responses"]},{"id":"Qwen/Qwen3.8-Omni-Flash","object":"model","created":1789872913,"owned_by":"command-code","name":"Qwen 3.8 Omni Flash","context_length":1000000,"supported_endpoints":["/chat/completions","/responses"]},{"id":"Qwen/Qwen3.8-Max-0902","object":"model","created":1789872913,"owned_by":"command-code","name":"Qwen 3.8 Max 0902","context_length":1000000,"supported_endpoints":["/chat/completions"]},{"id":"Qwen/Qwen3.8-Max","object":"model","created":1789872913,"owned_by":"command-code","name":"Qwen 3.8 Max","context_length":1000000,"supported_endpoints":["/chat/completions","/responses"]},{"id":"Qwen/Qwen3.8-27B","object":"model","created":1789872913,"owned_by":"command-code","name":"Qwen 3.8 27B","context_length":262144,"supported_endpoints":["/chat/completions","/responses"]},{"id":"Qwen/Qwen3.8-Flash","object":"model","created":1789872913,"owned_by":"command-code","name":"Qwen 3.8 Flash","context_length":1000000,"supported_endpoints":["/chat/completions"]},{"id":"Qwen/Qwen3.7-Max","object":"model","created":1789872913,"owned_by":"command-code","name":"Qwen 3.7 Max","context_length":1000000,"supported_endpoints":["/chat/completions","/responses"]},{"id":"Qwen/Qwen3.7-Plus","object":"model","created":1789872913,"owned_by":"command-code","name":"Qwen 3.7 Plus","context_length":1000000,"supported_endpoints":["/chat/completions","/responses"]},{"id":"Qwen/Qwen3.7-Flash","object":"model","created":1789872913,"owned_by":"command-code","name":"Qwen 3.7 Flash","context_length":1000000,"supported_endpoints":["/chat/completions","/responses"]},{"id":"Qwen/Qwen3.6-Max-Preview","object":"model","created":1789872913,"owned_by":"command-code","name":"Qwen 3.6 Max Preview","context_length":200000,"supported_endpoints":["/chat/completions","/responses"]},{"id":"Qwen/Qwen3.6-Plus","object":"model","created":1789872913,"owned_by":"command-code","name":"Qwen 3.6 Plus","context_length":200000,"supported_endpoints":["/chat/completions","/responses"]},{"id":"meituan/LongCat-2.0","object":"model","created":1789872913,"owned_by":"command-code","name":"LongCat 2.0","context_length":1048576,"supported_endpoints":["/chat/completions"]},{"id":"stepfun/Step-3.7-Flash","object":"model","created":1789872913,"owned_by":"command-code","name":"Step 3.7 Flash","context_length":256000,"supported_endpoints":["/chat/completions","/responses"]},{"id":"stepfun/Step-3.5-Flash","object":"model","created":1789872913,"owned_by":"command-code","name":"Step 3.5 Flash","context_length":1000000,"supported_endpoints":["/chat/completions"]},{"id":"tencent/hy3-paid","object":"model","created":1789872913,"owned_by":"command-code","name":"Tencent Hy3","context_length":262144,"supported_endpoints":["/chat/completions","/responses"]},{"id":"tencent/hy4-preview","object":"model","created":1789872913,"owned_by":"command-code","name":"Tencent Hy4 Preview","context_length":1048576,"supported_endpoints":["/chat/completions"]},{"id":"google/gemini-3.8-flash","object":"model","created":1789872913,"owned_by":"command-code","name":"Gemini 3.8 Flash","context_length":1000000,"supported_endpoints":["/chat/completions","/responses"]},{"id":"google/gemini-3.7-flash","object":"model","created":1789872913,"owned_by":"command-code","name":"Gemini 3.7 Flash","context_length":1048576,"supported_endpoints":["/chat/completions"]},{"id":"google/gemini-3.6-flash","object":"model","created":1789872913,"owned_by":"command-code","name":"Gemini 3.6 Flash","context_length":1000000,"supported_endpoints":["/chat/completions","/responses"]},{"id":"google/gemini-3.5-flash","object":"model","created":1789872913,"owned_by":"command-code","name":"Gemini 3.5 Flash","context_length":1000000,"supported_endpoints":["/chat/completions","/responses"]},{"id":"google/gemini-3.5-flash-lite","object":"model","created":1789872913,"owned_by":"command-code","name":"Gemini 3.5 Flash Lite","context_length":1000000,"supported_endpoints":["/chat/completions","/responses"]},{"id":"google/gemini-3.1-flash-lite","object":"model","created":1789872913,"owned_by":"command-code","name":"Gemini 3.1 Flash Lite","context_length":1000000,"supported_endpoints":["/chat/completions","/responses"]},{"id":"sakana/fugu-ultra","object":"model","created":1789872913,"owned_by":"command-code","name":"Fugu Ultra","context_length":1000000,"supported_endpoints":["/chat/completions","/responses"]},{"id":"nvidia/nemotron-3-ultra-550b-a55b","object":"model","created":1789872913,"owned_by":"command-code","name":"Nemotron 3 Ultra","context_length":1000000,"supported_endpoints":["/chat/completions","/responses"]},{"id":"thinkingmachines/inkling","object":"model","created":1789872913,"owned_by":"command-code","name":"Inkling","context_length":256000,"supported_endpoints":["/chat/completions","/responses"]},{"id":"thinkingmachines/inkling-small","object":"model","created":1789872913,"owned_by":"command-code","name":"Inkling Small","context_length":1000000,"supported_endpoints":["/chat/completions","/responses"]},{"id":"poolside/laguna-s-2.1-free","object":"model","created":1789872913,"owned_by":"command-code","name":"Laguna S 2.1","context_length":256000,"supported_endpoints":["/chat/completions","/responses"]},{"id":"inclusionai/ling-3.0-flash-sante:free","object":"model","created":1789872913,"owned_by":"command-code","name":"Ling 3.0 Flash Sante","context_length":262144,"supported_endpoints":["/chat/completions"]},{"id":"meta/muse-spark-1.1","object":"model","created":1789872913,"owned_by":"command-code","name":"Muse Spark 1.1","context_length":1048576,"supported_endpoints":["/chat/completions","/responses"]},{"id":"meta/muse-spark-1.2","object":"model","created":1789872913,"owned_by":"command-code","name":"Muse Spark 1.2","context_length":1048576,"supported_endpoints":["/chat/completions","/responses"]},{"id":"meta/muse-spark-1.2-contributor","object":"model","created":1789872913,"owned_by":"command-code","name":"Muse Spark 1.2 Contributor","context_length":1048576,"supported_endpoints":["/chat/completions","/responses"]},{"id":"meta/muse-spark-1.3","object":"model","created":1789872913,"owned_by":"command-code","name":"Muse Spark 1.3","context_length":1048576,"supported_endpoints":["/chat/completions","/responses"]},{"id":"meta/muse-spark-1.3-contributor","object":"model","created":1789872913,"owned_by":"command-code","name":"Muse Spark 1.3 Contributor","context_length":1048576,"supported_endpoints":["/chat/completions","/responses"]},{"id":"xai/grok-4.5","object":"model","created":1789872913,"owned_by":"command-code","name":"Grok 4.5","context_length":500000,"supported_endpoints":["/chat/completions","/responses"]},{"id":"xai/grok-4.6","object":"model","created":1789872913,"owned_by":"command-code","name":"Grok 4.6","context_length":500000,"supported_endpoints":["/chat/completions","/responses"]}]}"#;

/// The coverage rule from REQ-001 restated for the test: the identifier's last
/// `/`-separated segment (or the whole identifier when that segment is empty)
/// is covered when its alphanumeric characters appear in the display name in
/// order, compared case-insensitively.
fn identifier_segment_is_covered(identifier: &str, display_name: &str) -> bool {
    let segment = identifier.rsplit('/').next().unwrap_or(identifier);
    let segment = if segment.is_empty() {
        identifier
    } else {
        segment
    };
    let wanted: Vec<char> = segment
        .chars()
        .filter(|candidate| candidate.is_ascii_alphanumeric())
        .map(|candidate| candidate.to_ascii_lowercase())
        .collect();
    let mut available = display_name
        .chars()
        .filter(|candidate| candidate.is_ascii_alphanumeric())
        .map(|candidate| candidate.to_ascii_lowercase());
    wanted
        .iter()
        .all(|wanted| available.any(|candidate| candidate == *wanted))
}

#[test]
fn live_catalog_fixture_names_cover_their_identifier_segments() {
    let payload: Value =
        serde_json::from_str(LIVE_MODEL_CATALOG_2026_09_20).expect("the fixture must parse");
    let entries = payload
        .get("data")
        .and_then(Value::as_array)
        .expect("the fixture must carry a data array");
    assert_eq!(
        entries.len(),
        71,
        "the fixture must hold the 71-entry live payload verbatim"
    );

    let models = sync_models(serde_json::from_str(LIVE_MODEL_CATALOG_2026_09_20).expect(
        "the fixture must parse as a sync payload",
    ));
    // Eight fixture entries declare only `/messages` and are dropped by the
    // existing endpoint filter, so 63 models remain.
    assert_eq!(
        models.len(),
        63,
        "the sync keeps every fixture entry the endpoint filter allows"
    );

    for model in &models {
        let display = model.display_name.as_deref().unwrap_or_else(|| {
            panic!(
                "the synced model {} must carry a display name",
                model.upstream_model
            )
        });
        assert!(
            identifier_segment_is_covered(&model.upstream_model, display),
            "the display name {display:?} must cover the identifier segment of {}",
            model.upstream_model
        );
    }

    for (identifier, name) in [
        ("gpt-5.6-sol", "GPT-5.6 Sol"),
        ("Qwen/Qwen3.8-Max-0902", "Qwen 3.8 Max 0902"),
        ("zai-org/GLM-5.3", "GLM-5.3"),
        ("deepseek/deepseek-v4-pro", "DeepSeek V4 Pro (latest)"),
        ("xiaomi/mimo-v2.5-pro", "MiMo V2.5 Pro"),
    ] {
        let model = models
            .iter()
            .find(|model| model.upstream_model == identifier)
            .unwrap_or_else(|| panic!("the fixture model {identifier} must sync"));
        assert_eq!(
            model.display_name.as_deref(),
            Some(name),
            "the complete fixture name for {identifier} must stay byte-identical"
        );
    }

    let laguna = models
        .iter()
        .find(|model| model.upstream_model == "poolside/laguna-s-2.1-free")
        .expect("the laguna fixture model must sync");
    assert_eq!(
        laguna.display_name.as_deref(),
        Some("Laguna S 2.1 Free"),
        "the fixture's truncated laguna name must be completed"
    );
}
