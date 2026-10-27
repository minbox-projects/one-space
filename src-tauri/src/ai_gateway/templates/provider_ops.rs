//! Create-from-template and per-model provider maintenance.

use super::super::storage::new_provider_id;
use super::super::types_config::{
    GatewayConfig, GatewayUpstreamProvider, ModelMapping, ModelPrice, ProviderTemplate,
    ProviderTemplateModel, UpstreamKey, UpstreamProtocol,
};
use super::source::complete_model_display_name;
use super::state::effective_template;

/// Build the mapping a template model is created with: `local_model` equals
/// `upstream_model` (or the template's trimmed `local_model` when present),
/// carrying the model's enabled flag, the template's display name verbatim and
/// `None` (follow the provider protocol) when the model's effective protocol
/// equals the provider protocol. A missing or blank template display name
/// falls back to the identifier segment transformed into display words so the
/// new mapping never stays empty.
fn mapping_from_template(
    template: &ProviderTemplate,
    model: &ProviderTemplateModel,
    provider_protocol: UpstreamProtocol,
) -> ModelMapping {
    let effective = model.protocol.unwrap_or(template.protocol);
    ModelMapping {
        local_model: model
            .local_model
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(&model.upstream_model)
            .to_string(),
        upstream_model: model.upstream_model.clone(),
        enabled: model.enabled,
        protocol: if effective == provider_protocol {
            None
        } else {
            Some(effective)
        },
        display_name: model
            .display_name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .or_else(|| complete_model_display_name(None, &model.upstream_model)),
        reasoning_efforts: model.reasoning_efforts.clone(),
        context_window: model.context_window,
        ..Default::default()
    }
}

/// Create an upstream provider from a template with an injectable persistence
/// seam.
///
/// A blank API key is rejected before anything is staged, so the config and the
/// persist seam stay untouched. A blank `name`/`base_url` falls back to the
/// template's value; the protocol argument always wins. The new provider binds
/// the template, leaves `default_model` empty, receives one mapping per enabled
/// template model (`local_model` = `upstream_model`, official display name,
/// `None` i.e. follow-the-provider when the effective protocol equals the
/// provider protocol) and writes prices if defined on the template models.
/// The staged clone is handed to `persist` and only committed to `config` once persistence succeeds.
pub fn apply_create_provider_from_template(
    config: &mut GatewayConfig,
    template_id: &str,
    name: &str,
    base_url: &str,
    protocol: UpstreamProtocol,
    api_key: &str,
    persist: impl FnOnce(&GatewayConfig) -> Result<(), String>,
) -> Result<GatewayUpstreamProvider, String> {
    if api_key.trim().is_empty() {
        return Err("an API key is required to create a provider from a template".to_string());
    }
    let template = effective_template(config, template_id)?;

    let provider_id = new_provider_id();
    let provider = GatewayUpstreamProvider {
        id: provider_id.clone(),
        name: if name.trim().is_empty() {
            template.name.clone()
        } else {
            name.to_string()
        },
        base_url: if base_url.trim().is_empty() {
            template.base_url.clone()
        } else {
            base_url.to_string()
        },
        keys: vec![UpstreamKey {
            id: "default".to_string(),
            name: "Default".to_string(),
            value: api_key.to_string(),
            enabled: true,
            ..UpstreamKey::default()
        }],
        template_id: Some(template_id.to_string()),
        protocol,
        mappings: template
            .models
            .iter()
            .filter(|model| model.enabled)
            .map(|model| mapping_from_template(&template, model, protocol))
            .collect(),
        ignored_models: Vec::new(),
        ..GatewayUpstreamProvider::default()
    };

    let mut next = config.clone();
    next.providers.push(provider.clone());
    for model in &template.models {
        if model.enabled
            && (model.input > 0.0
                || model.output > 0.0
                || model.cache_read > 0.0
                || model.cache_write > 0.0
                || !model.off_peaks.is_empty())
        {
            next.model_prices.push(ModelPrice {
                provider_id: Some(provider_id.clone()),
                upstream_model: model.upstream_model.clone(),
                input: model.input,
                cache_read: model.cache_read,
                cache_write: model.cache_write,
                output: model.output,
                off_peaks: model.off_peaks.clone(),
            });
        }
    }

    persist(&next)?;
    *config = next;
    Ok(provider)
}

/// Delete one upstream model from a provider with an injectable persistence
/// seam.
///
/// An unknown provider is an actionable error that writes nothing. The mapping
/// and the provider-scoped price row for that model are removed on both a
/// template-bound and a manual provider. A template-bound provider additionally
/// records the model in its ignored set exactly once so a later sync cannot
/// resurrect it; a manual provider writes no ignored record.
pub fn apply_delete_provider_model(
    config: &mut GatewayConfig,
    provider_id: &str,
    upstream_model: &str,
    persist: impl FnOnce(&GatewayConfig) -> Result<(), String>,
) -> Result<(), String> {
    if !config.providers.iter().any(|provider| provider.id == provider_id) {
        return Err(format!("provider not found: {provider_id}"));
    }

    let mut next = config.clone();
    {
        let provider = next
            .providers
            .iter_mut()
            .find(|provider| provider.id == provider_id)
            .expect("the provider was checked above");
        provider
            .mappings
            .retain(|mapping| mapping.upstream_model != upstream_model);
        if provider.template_id.is_some()
            && !provider
                .ignored_models
                .iter()
                .any(|ignored| ignored == upstream_model)
        {
            provider.ignored_models.push(upstream_model.to_string());
        }
    }
    next.model_prices.retain(|row| {
        !(row.provider_id.as_deref() == Some(provider_id)
            && row.upstream_model == upstream_model)
    });

    persist(&next)?;
    *config = next;
    Ok(())
}

/// Restore one previously ignored model from the template's current data with
/// an injectable persistence seam.
///
/// Only a model present in the provider's `ignored_models` can be restored;
/// anything else is an actionable error that writes nothing. The mapping is
/// rebuilt from the persisted template state when one exists, else from the
/// built-in snapshot, and the model leaves the ignored set; no price row is
/// written or modified. A model the template no longer carries reports an error
/// naming it and creates no mapping.
pub fn apply_restore_provider_model(
    config: &mut GatewayConfig,
    provider_id: &str,
    upstream_model: &str,
    persist: impl FnOnce(&GatewayConfig) -> Result<(), String>,
) -> Result<(), String> {
    let provider = config
        .providers
        .iter()
        .find(|provider| provider.id == provider_id)
        .ok_or_else(|| format!("provider not found: {provider_id}"))?;
    if !provider
        .ignored_models
        .iter()
        .any(|ignored| ignored == upstream_model)
    {
        return Err(format!(
            "model '{upstream_model}' is not ignored and cannot be restored"
        ));
    }
    let template_id = provider.template_id.clone().ok_or_else(|| {
        format!("provider {provider_id} is not bound to a provider template")
    })?;

    let template = effective_template(config, &template_id)?;
    let model = template
        .models
        .iter()
        .find(|model| model.upstream_model == upstream_model)
        .cloned()
        .ok_or_else(|| {
            format!("model '{upstream_model}' is not present in template '{template_id}'")
        })?;
    let provider_protocol = provider.protocol;

    let mut next = config.clone();
    {
        let provider = next
            .providers
            .iter_mut()
            .find(|provider| provider.id == provider_id)
            .expect("the provider was checked above");
        provider
            .ignored_models
            .retain(|ignored| ignored != upstream_model);
        provider
            .mappings
            .retain(|mapping| mapping.upstream_model != upstream_model);
        provider
            .mappings
            .push(mapping_from_template(&template, &model, provider_protocol));
    }

    persist(&next)?;
    *config = next;
    Ok(())
}
