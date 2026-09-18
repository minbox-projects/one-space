//! Step 2 behavior tests: template data model and the in-app snapshot.
//!
//! These tests pin the public boundary of `api_gateway::templates` and the
//! snapshot JSON shape: `parse_template_snapshot` accepts a top-level JSON
//! array of templates, `builtin_templates` embeds `provider_templates.json`,
//! and `find_builtin_template` resolves an id or reports it in the error.
//! They are expected to fail (unresolved module/types/fields) until Step 2
//! lands.

use crate::api_gateway::templates::{
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
