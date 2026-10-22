//! Behavior tests for the AI Gateway's declared total context window.
//!
//! The boundary under test is the whole declared-metadata chain: a remote
//! model list declares a total context window, the declared value survives the
//! production source parser, the template and the bound mapping snapshots keep
//! it, an invalid declaration publishes nothing, and a configured public model
//! never advertises more than its configured sources support.
//!
//! Every assertion in this file is JSON-facing on purpose: the tests read the
//! persisted shape of a template model or a mapping row (`context_window`) and
//! never name a product field that the implementation does not own yet, so a
//! missing capability shows up as a failing behavior assertion instead of a
//! compilation failure. The one exception is the documented resolver contract
//! at the end of this file, which names the new pure module directly.

use crate::ai_gateway::commands::ai_gateway_upsert_provider;
use crate::ai_gateway::storage::{read_config, write_config};
use crate::ai_gateway::templates::{
    apply_create_provider_from_template, apply_restore_provider_model,
    apply_template_sync_with, apply_upsert_provider_template,
};
use crate::ai_gateway::types_config::{
    GatewayConfig, GatewayUpstreamProvider, ModelMapping, ModelPool, ModelPoolCandidate,
    ProviderTemplate, ProviderTemplateModel, ProviderTemplateState, UpstreamProtocol,
};
use serde_json::{json, Value};

const SYNC_URL: &str = "https://tpl.example.com/v1/models";

/// A template with an optional model-list URL.
fn template(
    id: &str,
    models_url: Option<&str>,
    models: Vec<ProviderTemplateModel>,
) -> ProviderTemplate {
    ProviderTemplate {
        id: id.to_string(),
        name: "Test Template".to_string(),
        base_url: "https://tpl.example.com/v1".to_string(),
        protocol: UpstreamProtocol::ChatCompletions,
        source: models_url.unwrap_or_default().to_string(),
        models_url: models_url.map(str::to_string),
        models,
        ..ProviderTemplate::default()
    }
}

/// A configuration carrying one persisted template state.
fn config_with_template(template: ProviderTemplate) -> GatewayConfig {
    let mut config = GatewayConfig::default();
    config.provider_templates.push(ProviderTemplateState {
        template_id: template.id.clone(),
        template: Some(template),
        synced_at: None,
        source: None,
    });
    config
}

/// The declared total context window of a template model, read from its
/// persisted JSON shape. `None` means the model carries no declared value.
fn template_model_context(model: &ProviderTemplateModel) -> Option<u64> {
    serde_json::to_value(model)
        .ok()
        .and_then(|value| value.get("context_window").cloned())
        .and_then(|value| value.as_u64())
}

/// The declared total context window of a mapping row, read from its persisted
/// JSON shape. `None` means the row carries no declared value.
fn mapping_context(mapping: &ModelMapping) -> Option<u64> {
    serde_json::to_value(mapping)
        .ok()
        .and_then(|value| value.get("context_window").cloned())
        .and_then(|value| value.as_u64())
}

/// A template model built from its JSON shape so a test never names a field
/// the product does not own yet.
fn template_model_from_json(value: Value) -> ProviderTemplateModel {
    serde_json::from_value(value).expect("the template model JSON must deserialize")
}

/// A mapping row built from its JSON shape so a test never names a field the
/// product does not own yet.
fn mapping_from_json(value: Value) -> ModelMapping {
    serde_json::from_value(value).expect("the mapping JSON must deserialize")
}

/// A mapping row for one upstream model, optionally declaring a context
/// window, an enabled flag and a runtime health state.
fn mapping(
    local_model: &str,
    upstream_model: &str,
    context_window: Option<u64>,
    enabled: bool,
    auto_disabled: bool,
) -> ModelMapping {
    let mut value = json!({
        "local_model": local_model,
        "upstream_model": upstream_model,
        "enabled": enabled,
    });
    if let Some(context_window) = context_window {
        value["context_window"] = json!(context_window);
    }
    if auto_disabled {
        value["auto_disabled"] = json!(true);
        value["disabled_reason"] = json!("repeated upstream failures");
    }
    mapping_from_json(value)
}

/// A provider bound to one template.
fn bound_provider(id: &str, template_id: &str, mappings: Vec<ModelMapping>) -> GatewayUpstreamProvider {
    GatewayUpstreamProvider {
        id: id.to_string(),
        name: "Bound Provider".to_string(),
        base_url: "https://p.example.com/v1".to_string(),
        template_id: Some(template_id.to_string()),
        mappings,
        ..GatewayUpstreamProvider::default()
    }
}

/// A manually maintained provider that is not bound to any template.
fn manual_provider(id: &str, mappings: Vec<ModelMapping>) -> GatewayUpstreamProvider {
    GatewayUpstreamProvider {
        id: id.to_string(),
        name: "Manual Provider".to_string(),
        base_url: "https://p.example.com/v1".to_string(),
        template_id: None,
        mappings,
        ..GatewayUpstreamProvider::default()
    }
}

/// Sync one injected model-list body into `config` and return the resulting
/// template models.
fn sync_models(
    config: &mut GatewayConfig,
    template_id: &str,
    body: Value,
) -> Vec<ProviderTemplateModel> {
    let view = apply_template_sync_with(
        config,
        template_id,
        |_template| Ok(body.to_string()),
        |_next| Ok(()),
    )
    .expect("the injected model list must parse");
    view.template.models
}

/// The context window declared by one synced template model.
fn synced_context(models: &[ProviderTemplateModel], upstream_model: &str) -> Option<u64> {
    let model = models
        .iter()
        .find(|model| model.upstream_model == upstream_model)
        .unwrap_or_else(|| panic!("the synced template must carry {upstream_model}"));
    template_model_context(model)
}

// ---------------------------------------------------------------------------
// Source declaration parsing (REQ-002, AC-003)
// ---------------------------------------------------------------------------

/// REQ-002 / AC-003: a remote model list declares a total context window and
/// the declared value survives the production source parser on the template
/// model.
#[test]
fn source_context_length_is_retained_on_the_synced_template_model() {
    let mut config = config_with_template(template("t", Some(SYNC_URL), vec![]));

    let models = sync_models(
        &mut config,
        "t",
        json!({
            "data": [
                {"id": "model-a", "name": "Model A", "context_length": 256000}
            ]
        }),
    );

    assert_eq!(
        synced_context(&models, "model-a"),
        Some(256000),
        "the declared source context window must survive the sync on the template model"
    );
}

/// REQ-002 / AC-003: the nested `limit.context` declaration is an equally
/// supported source of the total context window.
#[test]
fn nested_limit_context_is_retained_on_the_synced_template_model() {
    let mut config = config_with_template(template("t", Some(SYNC_URL), vec![]));

    let models = sync_models(
        &mut config,
        "t",
        json!({
            "data": [
                {"id": "model-b", "name": "Model B", "limit": {"context": 200000}}
            ]
        }),
    );

    assert_eq!(
        synced_context(&models, "model-b"),
        Some(200000),
        "the nested limit context declaration must survive the sync"
    );
}

/// REQ-002 / AC-003: agreeing dual declarations are accepted and must not
/// fail the sync.
#[test]
fn agreeing_dual_declarations_are_accepted() {
    let mut config = config_with_template(template("t", Some(SYNC_URL), vec![]));

    let models = sync_models(
        &mut config,
        "t",
        json!({
            "data": [
                {"id": "model-a", "context_length": 256000, "limit": {"context": 256000}}
            ]
        }),
    );

    assert_eq!(
        synced_context(&models, "model-a"),
        Some(256000),
        "agreeing dual declarations must publish the agreed value"
    );
}

/// REQ-002 / AC-003: a source that declares neither field preserves the
/// previous value of the same model, and a new model without a declaration
/// stays unspecified.
#[test]
fn an_absent_declaration_preserves_the_previous_value() {
    let previous = template_model_from_json(json!({
        "upstream_model": "model-a",
        "context_window": 128000
    }));
    assert_eq!(
        template_model_context(&previous),
        Some(128000),
        "a persisted template model must keep its declared context window"
    );

    let mut config = config_with_template(template(
        "t",
        Some(SYNC_URL),
        vec![previous.clone()],
    ));

    let models = sync_models(
        &mut config,
        "t",
        json!({
            "data": [
                {"id": "model-a", "name": "Model A"},
                {"id": "model-new", "name": "Model New"}
            ]
        }),
    );

    assert_eq!(
        synced_context(&models, "model-a"),
        Some(128000),
        "an absent declaration must preserve the previous value of the same model"
    );
    assert_eq!(
        synced_context(&models, "model-new"),
        None,
        "a new model without a declaration must stay unspecified"
    );
}

/// REQ-002 / AC-003: an explicit `null` clears the declared value, while an
/// absent or null `limit` parent and an object without `context` declare
/// nothing at all.
#[test]
fn an_explicit_null_clears_the_previous_value() {
    let previous = template_model_from_json(json!({
        "upstream_model": "model-a",
        "context_window": 128000
    }));
    assert_eq!(
        template_model_context(&previous),
        Some(128000),
        "the seeded template model must keep its declared value before the sync"
    );
    let mut config = config_with_template(template(
        "t",
        Some(SYNC_URL),
        vec![previous.clone()],
    ));

    let models = sync_models(
        &mut config,
        "t",
        json!({
            "data": [
                {"id": "model-a", "context_length": null},
                {"id": "model-b", "limit": null},
                {"id": "model-c", "limit": {}}
            ]
        }),
    );

    assert_eq!(
        synced_context(&models, "model-a"),
        None,
        "an explicit null declaration must clear the value"
    );
    assert_eq!(
        synced_context(&models, "model-b"),
        None,
        "a null limit parent declares nothing and leaves a new model unspecified"
    );
    assert_eq!(
        synced_context(&models, "model-c"),
        None,
        "a limit object without context declares nothing"
    );
}

/// REQ-002 / AC-003: an invalid declared value fails the whole sync and
/// publishes nothing at all.
#[test]
fn invalid_context_declarations_fail_the_whole_sync() {
    let invalid = [
        json!(0),
        json!(-1),
        json!(1.5),
        json!("256000"),
        json!(true),
        json!(4294967296u64),
    ];

    for value in invalid {
        let mut config = config_with_template(template("t", Some(SYNC_URL), vec![]));
        let before = serde_json::to_value(&config).expect("the config must serialize");

        let body = json!({
            "data": [
                {"id": "model-a", "name": "Model A", "context_length": value}
            ]
        });
        let result = apply_template_sync_with(
            &mut config,
            "t",
            |_template| Ok(body.to_string()),
            |_next| Ok(()),
        );

        assert!(
            result.is_err(),
            "the invalid declaration {value} must fail the whole sync"
        );
        assert_eq!(
            serde_json::to_value(&config).expect("the config must serialize"),
            before,
            "a failed sync must publish nothing"
        );
        assert!(
            config.provider_templates[0].synced_at.is_none(),
            "a failed sync must not publish a sync timestamp"
        );
    }
}

/// REQ-002 / AC-003: two disagreeing declarations fail the sync instead of
/// silently choosing one of them.
#[test]
fn conflicting_dual_declarations_fail_the_whole_sync() {
    for body in [
        json!({"data": [{"id": "model-a", "context_length": 256000, "limit": {"context": 128000}}]}),
        json!({"data": [{"id": "model-a", "context_length": null, "limit": {"context": 128000}}]}),
        json!({"data": [{"id": "model-a", "context_length": 256000, "limit": {"context": null}}]}),
    ] {
        let mut config = config_with_template(template("t", Some(SYNC_URL), vec![]));
        let before = serde_json::to_value(&config).expect("the config must serialize");

        let result = apply_template_sync_with(
            &mut config,
            "t",
            |_template| Ok(body.to_string()),
            |_next| Ok(()),
        );

        assert!(
            result.is_err(),
            "the conflicting declarations {body} must fail the whole sync"
        );
        assert_eq!(
            serde_json::to_value(&config).expect("the config must serialize"),
            before,
            "a conflicting declaration must publish nothing"
        );
    }
}

/// REQ-002 / AC-003: context metadata is validated after the existing
/// acceptance filters, so a rejected entry never fails the sync.
#[test]
fn filtered_entries_never_fail_the_sync() {
    let mut config = config_with_template(template("t", Some(SYNC_URL), vec![]));

    let models = sync_models(
        &mut config,
        "t",
        json!({
            "data": [
                {"id": "model-a", "name": "Model A", "context_length": 256000},
                {"id": "model-hidden", "hidden": true, "context_length": 0},
                {"id": "model-unsupported", "supported_endpoints": [], "context_length": 0},
                {"id": "model-a", "context_length": -5}
            ]
        }),
    );

    assert_eq!(
        synced_context(&models, "model-a"),
        Some(256000),
        "the accepted entry must keep its declared value"
    );
    assert!(
        !models.iter().any(|model| model.upstream_model == "model-hidden"),
        "a hidden entry must stay filtered out"
    );
    assert!(
        !models
            .iter()
            .any(|model| model.upstream_model == "model-unsupported"),
        "an unsupported-endpoint entry must stay filtered out"
    );
}

/// REQ-001 / AC-001: a persisted template model round-trips its declared
/// context window, and an older configuration without the field loads with an
/// unspecified value instead of a guessed one.
#[test]
fn persisted_template_models_round_trip_their_context_value() {
    let declared = template_model_from_json(json!({
        "upstream_model": "model-a",
        "context_window": 4294967295u64
    }));
    assert_eq!(
        template_model_context(&declared),
        Some(4294967295),
        "the largest accepted context window must round-trip"
    );

    let legacy = template_model_from_json(json!({"upstream_model": "model-legacy"}));
    assert_eq!(
        template_model_context(&legacy),
        None,
        "an older configuration without the field must load as unspecified"
    );
}

// ---------------------------------------------------------------------------
// Strict inheritance (REQ-002, AC-002)
// ---------------------------------------------------------------------------

/// REQ-002 / AC-002 / AC-003: after a sync every bound row follows the latest
/// template value, including aliases and user-disabled or automatically
/// disabled rows, while unrelated fields stay untouched.
#[test]
fn bound_rows_follow_the_latest_template_value() {
    let previous = template_model_from_json(json!({
        "upstream_model": "model-a",
        "context_window": 128000
    }));
    let mut config = config_with_template(template(
        "t",
        Some(SYNC_URL),
        vec![previous],
    ));
    let mut provider = bound_provider(
        "p1",
        "t",
        vec![
            mapping("alias-a", "model-a", Some(128000), true, false),
            mapping("model-a", "model-a", Some(128000), false, false),
            mapping("model-c", "model-c", Some(64000), true, true),
        ],
    );
    provider.ignored_models = vec!["model-ignored".to_string()];
    config.providers.push(provider);

    sync_models(
        &mut config,
        "t",
        json!({
            "data": [
                {"id": "model-a", "name": "Model A", "context_length": 256000},
                {"id": "model-c", "name": "Model C", "context_length": 64000}
            ]
        }),
    );

    let provider = &config.providers[0];
    let alias = provider
        .mappings
        .iter()
        .find(|mapping| mapping.local_model == "alias-a")
        .expect("the alias row must stay");
    let disabled = provider
        .mappings
        .iter()
        .find(|mapping| mapping.local_model == "model-a" && !mapping.enabled)
        .expect("the user-disabled row must stay");
    let auto_disabled = provider
        .mappings
        .iter()
        .find(|mapping| mapping.upstream_model == "model-c")
        .expect("the automatically disabled row must stay");

    assert_eq!(
        mapping_context(alias),
        Some(256000),
        "an alias row must follow the latest template value"
    );
    assert_eq!(
        mapping_context(disabled),
        Some(256000),
        "a user-disabled row must follow the latest template value"
    );
    assert_eq!(
        mapping_context(auto_disabled),
        Some(64000),
        "an automatically disabled row must follow the latest template value"
    );
    assert!(
        auto_disabled.auto_disabled,
        "the runtime health of a row must survive the sync"
    );
    assert_eq!(
        auto_disabled.disabled_reason.as_deref(),
        Some("repeated upstream failures"),
        "the disabled reason of a row must survive the sync"
    );
    assert_eq!(
        provider.ignored_models,
        vec!["model-ignored".to_string()],
        "the ignored model set must survive the sync"
    );
}

/// REQ-002 / AC-002: a manual template save updates the template and every
/// bound mapping snapshot at once.
#[test]
fn a_manual_template_save_updates_the_bound_snapshots() {
    let mut config = config_with_template(template("t", None, vec![]));
    config.providers.push(bound_provider(
        "p1",
        "t",
        vec![
            mapping("alias-a", "model-a", Some(128000), true, false),
            mapping("model-a", "model-a", Some(128000), false, false),
        ],
    ));

    let saved = template(
        "t",
        None,
        vec![template_model_from_json(json!({
            "upstream_model": "model-a",
            "context_window": 999999
        }))],
    );
    apply_upsert_provider_template(&mut config, saved, |_next| Ok(()))
        .expect("the manual template save must succeed");

    let provider = &config.providers[0];
    for mapping in provider.mappings.iter() {
        assert_eq!(
            mapping_context(mapping),
            Some(999999),
            "every bound row must follow the manually saved template value"
        );
    }
}

/// REQ-002 / AC-002: removing a template entry manually clears the orphan
/// inherited value of the matching bound row without retiring the row.
#[test]
fn removing_a_template_entry_clears_the_orphan_inherited_value() {
    let mut config = config_with_template(template("t", None, vec![]));
    config.providers.push(bound_provider(
        "p1",
        "t",
        vec![mapping("model-a", "model-a", Some(128000), true, false)],
    ));
    assert_eq!(
        mapping_context(&config.providers[0].mappings[0]),
        Some(128000),
        "the seeded bound row must keep its inherited value before the manual save"
    );

    let saved = template("t", None, vec![]);
    apply_upsert_provider_template(&mut config, saved, |_next| Ok(()))
        .expect("the manual template save must succeed");

    let provider = &config.providers[0];
    assert_eq!(
        provider.mappings.len(),
        1,
        "removing a template entry must not retire the bound row"
    );
    assert_eq!(
        mapping_context(&provider.mappings[0]),
        None,
        "removing a template entry must clear the orphan inherited value"
    );
}

/// REQ-002 / AC-002: a provider created from a template inherits the current
/// template value.
#[test]
fn a_created_provider_inherits_the_template_value() {
    let mut config = config_with_template(template(
        "t",
        None,
        vec![template_model_from_json(json!({
            "upstream_model": "model-a",
            "context_window": 128000
        }))],
    ));

    let provider = apply_create_provider_from_template(
        &mut config,
        "t",
        "Created",
        "",
        UpstreamProtocol::ChatCompletions,
        "secret-key",
        |_next| Ok(()),
    )
    .expect("the provider creation must succeed");

    let mapping = provider
        .mappings
        .iter()
        .find(|mapping| mapping.upstream_model == "model-a")
        .expect("the created provider must carry the template model");
    assert_eq!(
        mapping_context(mapping),
        Some(128000),
        "a created provider must inherit the current template value"
    );
}

/// REQ-002 / AC-002: a restored model inherits the current template value.
#[test]
fn a_restored_model_inherits_the_template_value() {
    let mut config = config_with_template(template(
        "t",
        None,
        vec![template_model_from_json(json!({
            "upstream_model": "model-a",
            "context_window": 128000
        }))],
    ));
    let mut provider = bound_provider("p1", "t", vec![]);
    provider.ignored_models = vec!["model-a".to_string()];
    config.providers.push(provider);

    apply_restore_provider_model(&mut config, "p1", "model-a", |_next| Ok(()))
        .expect("the restore must succeed");

    let mapping = config.providers[0]
        .mappings
        .iter()
        .find(|mapping| mapping.upstream_model == "model-a")
        .expect("the restored row must exist");
    assert_eq!(
        mapping_context(mapping),
        Some(128000),
        "a restored model must inherit the current template value"
    );
}

/// REQ-002 / AC-002: unbinding a provider retains the last inherited value
/// for later manual maintenance.
#[test]
fn unbinding_a_provider_retains_the_last_inherited_value() {
    let _home = super::isolated_temp_home("context-unbind");

    let mut config = config_with_template(template(
        "t",
        None,
        vec![template_model_from_json(json!({
            "upstream_model": "model-a",
            "context_window": 128000
        }))],
    ));
    config.providers.push(bound_provider(
        "p1",
        "t",
        vec![mapping("model-a", "model-a", Some(128000), true, false)],
    ));
    write_config(&config).expect("seed the bound provider");

    let stored = read_config().expect("read the seeded configuration");
    let mut provider = stored.providers[0].clone();
    provider.template_id = None;
    ai_gateway_upsert_provider(provider, None).expect("the unbinding save must succeed");

    let after = read_config().expect("read the configuration after unbinding");
    let mapping = after.providers[0]
        .mappings
        .iter()
        .find(|mapping| mapping.upstream_model == "model-a")
        .expect("the unbound row must stay");
    assert_eq!(
        mapping_context(mapping),
        Some(128000),
        "unbinding must retain the last inherited value"
    );
}

/// REQ-002 / AC-002: a provider saved from a stale dialog cannot establish an
/// override; the bound value follows the latest template.
#[test]
fn a_stale_provider_save_canonicalizes_against_the_latest_template() {
    let _home = super::isolated_temp_home("context-stale-save");

    let mut config = config_with_template(template(
        "t",
        None,
        vec![template_model_from_json(json!({
            "upstream_model": "model-a",
            "context_window": 256000
        }))],
    ));
    config.providers.push(bound_provider(
        "p1",
        "t",
        vec![mapping("model-a", "model-a", Some(999), true, false)],
    ));
    write_config(&config).expect("seed the bound provider");

    let stored = read_config().expect("read the seeded configuration");
    let stale = stored.providers[0].clone();
    ai_gateway_upsert_provider(stale, None).expect("the stale save must succeed");

    let after = read_config().expect("read the configuration after the stale save");
    let mapping = after.providers[0]
        .mappings
        .iter()
        .find(|mapping| mapping.upstream_model == "model-a")
        .expect("the bound row must stay");
    assert_eq!(
        mapping_context(mapping),
        Some(256000),
        "a stale dialog value must be canonicalized against the latest template"
    );
}

/// REQ-002 / AC-002: a provider rebound to a template follows the current
/// template value.
#[test]
fn a_rebound_provider_follows_the_current_template_value() {
    let _home = super::isolated_temp_home("context-rebind");

    let mut config = config_with_template(template(
        "t",
        None,
        vec![template_model_from_json(json!({
            "upstream_model": "model-a",
            "context_window": 256000
        }))],
    ));
    let mut unbound = bound_provider(
        "p1",
        "t",
        vec![mapping("model-a", "model-a", None, true, false)],
    );
    unbound.template_id = None;
    config.providers.push(unbound);
    write_config(&config).expect("seed the unbound provider");

    let stored = read_config().expect("read the seeded configuration");
    let mut rebound = stored.providers[0].clone();
    rebound.template_id = Some("t".to_string());
    ai_gateway_upsert_provider(rebound, None).expect("the rebinding save must succeed");

    let after = read_config().expect("read the configuration after rebinding");
    let mapping = after.providers[0]
        .mappings
        .iter()
        .find(|mapping| mapping.upstream_model == "model-a")
        .expect("the rebound row must stay");
    assert_eq!(
        mapping_context(mapping),
        Some(256000),
        "a rebound provider must follow the current template value"
    );
}

// ---------------------------------------------------------------------------
// Persistence boundaries (REQ-001, AC-001)
// ---------------------------------------------------------------------------

/// REQ-001 / AC-001: a valid declared value round-trips through the real
/// encrypted configuration, and the stored bytes stay encrypted.
#[test]
fn a_valid_context_value_round_trips_through_the_encrypted_configuration() {
    let _home = super::isolated_temp_home("context-round-trip");

    let mut provider = GatewayUpstreamProvider {
        id: "p1".to_string(),
        name: "P".to_string(),
        base_url: "https://p.example.com/v1".to_string(),
        ..GatewayUpstreamProvider::default()
    };
    provider.mappings = vec![mapping("model-a", "model-a", Some(131072), true, false)];
    ai_gateway_upsert_provider(provider, None).expect("the provider save must succeed");

    let stored = read_config().expect("read the stored configuration");
    let mapping = stored.providers[0]
        .mappings
        .iter()
        .find(|mapping| mapping.upstream_model == "model-a")
        .expect("the stored row must exist");
    assert_eq!(
        mapping_context(mapping),
        Some(131072),
        "a valid declared value must round-trip through the encrypted configuration"
    );

    let path = crate::ai_gateway::storage::config_path().expect("resolve the configuration path");
    let raw = std::fs::read_to_string(&path).expect("read the encrypted configuration file");
    assert!(
        !raw.contains("context_window"),
        "the configuration file must stay encrypted"
    );
}

/// REQ-001 / AC-001: an invalid declared value is rejected before any write,
/// so the prior encrypted bytes stay intact.
#[test]
fn an_invalid_context_value_never_reaches_the_encrypted_configuration() {
    let _home = super::isolated_temp_home("context-invalid-write");

    let mut provider = GatewayUpstreamProvider {
        id: "p1".to_string(),
        name: "P".to_string(),
        base_url: "https://p.example.com/v1".to_string(),
        ..GatewayUpstreamProvider::default()
    };
    provider.mappings = vec![mapping("model-a", "model-a", Some(131072), true, false)];
    ai_gateway_upsert_provider(provider, None).expect("the provider save must succeed");

    let path = crate::ai_gateway::storage::config_path().expect("resolve the configuration path");
    let before = std::fs::read(&path).expect("read the encrypted configuration file");

    let mut invalid = GatewayUpstreamProvider {
        id: "p1".to_string(),
        name: "P".to_string(),
        base_url: "https://p.example.com/v1".to_string(),
        ..GatewayUpstreamProvider::default()
    };
    invalid.mappings = vec![mapping("model-a", "model-a", Some(0), true, false)];
    let result = ai_gateway_upsert_provider(invalid, None);

    assert!(
        result.is_err(),
        "an invalid declared value must be rejected before the write"
    );
    let after = std::fs::read(&path).expect("read the encrypted configuration file");
    assert_eq!(
        before, after,
        "a rejected save must leave the prior encrypted bytes intact"
    );
}

// ---------------------------------------------------------------------------
// Pool fixtures for the resolver contract (REQ-003, AC-004)
// ---------------------------------------------------------------------------

/// A pool candidate for one provider mapping row.
fn pool_candidate(provider_id: &str, local_model: &str, upstream_model: &str) -> ModelPoolCandidate {
    ModelPoolCandidate {
        provider_id: provider_id.to_string(),
        local_model: local_model.to_string(),
        upstream_model: upstream_model.to_string(),
        protocol: UpstreamProtocol::ChatCompletions,
        ..ModelPoolCandidate::default()
    }
}

/// A model pool exposing one public alias.
fn model_pool(id: &str, model: &str, candidates: Vec<ModelPoolCandidate>) -> ModelPool {
    ModelPool {
        id: id.to_string(),
        name: id.to_string(),
        model: model.to_string(),
        candidates,
        ..ModelPool::default()
    }
}

// ---------------------------------------------------------------------------
// Conservative public-model resolution (REQ-003, AC-004)
// ---------------------------------------------------------------------------

// The intended contract of the new pure module `crate::ai_gateway::context_limits`
// (created by the implementation step) is:
//
//     pub(in crate::ai_gateway) fn resolve_public_context_window(
//         config: &GatewayConfig,
//         public_model: &str,
//         protocol: UpstreamProtocol,
//     ) -> Option<u32>
//
// It returns the conservative advertised total context window of a public
// model: the minimum of every distinct configured contributing source's known
// value, or `None` when any contributing source is unknown, when a referenced
// source cannot be resolved, or when the model is not available under the
// given ingress protocol. A configured but disabled source still contributes
// conservatively, and runtime health (an auto-disabled row, a marked key, a
// cooldown) never raises the advertised bound. The function is pure: it never
// touches request routing, the filesystem or the network.

use crate::ai_gateway::context_limits::resolve_public_context_window;

/// REQ-003 / AC-004: the advertised window of an ordinary public model is the
/// minimum of its configured sources.
#[test]
fn the_public_context_window_is_the_minimum_of_its_configured_sources() {
    let mut config = GatewayConfig::default();
    config.providers.push(manual_provider(
        "p1",
        vec![mapping("shared", "model-a", Some(128000), true, false)],
    ));
    config.providers.push(manual_provider(
        "p2",
        vec![mapping("shared", "model-a", Some(256000), true, false)],
    ));

    assert_eq!(
        resolve_public_context_window(&config, "shared", UpstreamProtocol::ChatCompletions),
        Some(128000),
        "the advertised window must be the minimum of the configured sources"
    );
}

/// REQ-003 / AC-004: one unknown contributing source makes the whole aggregate
/// unknown instead of borrowing another source's larger value.
#[test]
fn an_unknown_contributing_source_makes_the_aggregate_unknown() {
    let mut config = GatewayConfig::default();
    config.providers.push(manual_provider(
        "p1",
        vec![mapping("shared", "model-a", Some(128000), true, false)],
    ));
    config.providers.push(manual_provider(
        "p2",
        vec![mapping("shared", "model-a", None, true, false)],
    ));

    assert_eq!(
        resolve_public_context_window(&config, "shared", UpstreamProtocol::ChatCompletions),
        None,
        "an unknown contributing source must make the aggregate unknown"
    );
}

/// REQ-003 / AC-004: every pool candidate and every expanded ordered fallback
/// source contributes to the pool's advertised window, and a duplicated source
/// identity is counted once.
#[test]
fn pool_candidates_and_expanded_fallback_sources_contribute() {
    let mut config = GatewayConfig::default();
    config.providers.push(manual_provider(
        "p1",
        vec![mapping("model-a", "model-a", Some(128000), true, false)],
    ));
    config.providers.push(manual_provider(
        "p2",
        vec![mapping("model-b", "model-b", Some(256000), true, false)],
    ));
    config.providers.push(manual_provider(
        "p3",
        vec![mapping("fallback-model", "model-c", Some(200000), true, false)],
    ));
    config.model_pools.push(model_pool(
        "pool-1",
        "pool-alias",
        vec![
            pool_candidate("p1", "model-a", "model-a"),
            pool_candidate("p1", "model-a", "model-a"),
            pool_candidate("p2", "model-b", "model-b"),
        ],
    ));
    config.model_pools[0].fallback_models = vec!["fallback-model".to_string()];

    assert_eq!(
        resolve_public_context_window(&config, "pool-alias", UpstreamProtocol::ChatCompletions),
        Some(128000),
        "every candidate and expanded fallback source must contribute to the aggregate"
    );
}

/// REQ-003 / AC-004: a pool reference that cannot be resolved makes the
/// aggregate unknown.
#[test]
fn an_unresolved_pool_reference_makes_the_aggregate_unknown() {
    let mut config = GatewayConfig::default();
    config.providers.push(manual_provider(
        "p1",
        vec![mapping("model-a", "model-a", Some(128000), true, false)],
    ));
    config.model_pools.push(model_pool(
        "pool-1",
        "pool-alias",
        vec![
            pool_candidate("p1", "model-a", "model-a"),
            pool_candidate("p-missing", "model-x", "model-x"),
        ],
    ));

    assert_eq!(
        resolve_public_context_window(&config, "pool-alias", UpstreamProtocol::ChatCompletions),
        None,
        "an unresolved pool reference must make the aggregate unknown"
    );
}

/// REQ-003 / AC-004: an ordinary default-model fallback route resolves its
/// metadata only from the same provider's own mapping declaration.
#[test]
fn a_default_model_route_uses_the_same_provider_declaration() {
    let mut config = GatewayConfig::default();
    let mut provider = manual_provider(
        "p1",
        vec![mapping("other", "model-a", Some(128000), true, false)],
    );
    provider.default_model = Some("model-a".to_string());
    config.providers.push(provider);

    let mut unknown = manual_provider("p2", vec![]);
    unknown.default_model = Some("model-z".to_string());
    config.providers.push(unknown);

    assert_eq!(
        resolve_public_context_window(&config, "unmapped", UpstreamProtocol::ChatCompletions),
        None,
        "an undeclared default-model metadata must stay unknown"
    );
    assert_eq!(
        resolve_public_context_window(&config, "other", UpstreamProtocol::ChatCompletions),
        Some(128000),
        "a declared mapping must advertise its own declared value"
    );
}

/// REQ-003 / AC-004: runtime health never raises the advertised bound.
#[test]
fn runtime_health_never_raises_the_advertised_bound() {
    let mut config = GatewayConfig::default();
    config.providers.push(manual_provider(
        "p1",
        vec![mapping("shared", "model-a", Some(128000), true, true)],
    ));
    config.providers.push(manual_provider(
        "p2",
        vec![mapping("shared", "model-a", Some(256000), true, false)],
    ));

    assert_eq!(
        resolve_public_context_window(&config, "shared", UpstreamProtocol::ChatCompletions),
        Some(128000),
        "an automatically disabled source must still contribute conservatively"
    );
}

/// REQ-003 / AC-004: a model that is not available under the given ingress
/// protocol advertises no window at all.
#[test]
fn a_pool_alias_is_unavailable_under_the_anthropic_ingress_protocol() {
    let mut config = GatewayConfig::default();
    config.providers.push(manual_provider(
        "p1",
        vec![mapping("model-a", "model-a", Some(128000), true, false)],
    ));
    config.model_pools.push(model_pool(
        "pool-1",
        "pool-alias",
        vec![pool_candidate("p1", "model-a", "model-a")],
    ));

    assert_eq!(
        resolve_public_context_window(&config, "pool-alias", UpstreamProtocol::ChatCompletions),
        Some(128000),
        "the pool alias must advertise its conservative window"
    );
    assert_eq!(
        resolve_public_context_window(&config, "pool-alias", UpstreamProtocol::AnthropicMessages),
        None,
        "a model fenced off by the ingress protocol must advertise no window"
    );
}
