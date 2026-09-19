//! Provider template snapshot loading and validation.
//!
//! The app ships `provider_templates.json` embedded via `include_str!`; this
//! module parses that snapshot (or a later network payload) into the public
//! [`ProviderTemplate`] model, dropping what the gateway cannot serve instead of
//! failing the whole document. Only structural problems (invalid JSON, an empty
//! or duplicate template id) are fatal.

use super::storage::new_provider_id;
use super::types_config::{
    now_ts, GatewayConfig, GatewayUpstreamProvider, ModelMapping, ProviderTemplate,
    ProviderTemplateModel, ProviderTemplateState, UpstreamProtocol,
};
use serde::Deserialize;
use serde_json::Value;
use std::time::Duration;

/// Raw template document shape, kept lenient so unknown protocol strings can be
/// dropped instead of aborting. Removed legacy fields (prices, off-peak windows,
/// reasoning efforts, snapshot version) are ignored as unknown fields.
#[derive(Deserialize)]
struct RawTemplate {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    base_url: String,
    #[serde(default)]
    protocol: Option<String>,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub models_url: Option<String>,
    #[serde(default)]
    pub models: Vec<RawTemplateModel>,
}

#[derive(Deserialize)]
struct RawTemplateModel {
    #[serde(default)]
    upstream_model: String,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    protocol: Option<String>,
    #[serde(default = "super::types_config::default_true")]
    enabled: bool,
}

fn parse_protocol(raw: &str) -> Option<UpstreamProtocol> {
    match raw {
        "chat_completions" => Some(UpstreamProtocol::ChatCompletions),
        "responses" => Some(UpstreamProtocol::Responses),
        _ => None,
    }
}

/// Parse a top-level JSON array of templates into the validated template model.
///
/// Structural failures (invalid JSON, an empty id or a duplicate template id)
/// return a readable error. Model-level problems (an empty identifier, an
/// unknown protocol string, a duplicate upstream name) are non-fatal: the model
/// is dropped, keeping the first occurrence.
pub fn parse_template_snapshot(json: &str) -> Result<Vec<ProviderTemplate>, String> {
    let raw: Vec<RawTemplate> = serde_json::from_str(json).map_err(|error| {
        format!("provider template snapshot is not a valid template array: {error}")
    })?;

    let mut templates: Vec<ProviderTemplate> = Vec::with_capacity(raw.len());
    let mut seen_ids: Vec<String> = Vec::with_capacity(raw.len());
    for item in raw {
        let id = item.id.trim().to_string();
        if id.is_empty() {
            return Err(
                "provider template snapshot contains a template with an empty id".to_string(),
            );
        }
        if seen_ids.iter().any(|existing| existing == &id) {
            return Err(format!(
                "provider template snapshot contains a duplicate template id: {id}"
            ));
        }
        seen_ids.push(id.clone());

        let mut models: Vec<ProviderTemplateModel> = Vec::with_capacity(item.models.len());
        for raw_model in item.models {
            let upstream_model = raw_model.upstream_model;
            if upstream_model.trim().is_empty() {
                continue;
            }
            let protocol = match raw_model.protocol.as_deref() {
                None => None,
                Some(raw) => match parse_protocol(raw) {
                    Some(protocol) => Some(protocol),
                    None => continue,
                },
            };
            if models
                .iter()
                .any(|existing| existing.upstream_model == upstream_model)
            {
                continue;
            }
            models.push(ProviderTemplateModel {
                upstream_model,
                display_name: raw_model.display_name,
                protocol,
                enabled: raw_model.enabled,
            });
        }

        let protocol = match item.protocol.as_deref() {
            Some(raw) => parse_protocol(raw).unwrap_or_default(),
            None => UpstreamProtocol::ChatCompletions,
        };

        let models_url = item
            .models_url
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        templates.push(ProviderTemplate {
            id,
            name: item.name,
            description: item.description,
            base_url: item.base_url,
            protocol,
            source: item.source,
            models_url,
            models,
        });
    }

    Ok(templates)
}

/// Parse the in-app snapshot embedded at compile time.
pub fn builtin_templates() -> Result<Vec<ProviderTemplate>, String> {
    parse_template_snapshot(include_str!("provider_templates.json"))
}

/// Resolve one built-in template by its id, naming the id in an actionable
/// error when it is unknown.
pub fn find_builtin_template(template_id: &str) -> Result<ProviderTemplate, String> {
    builtin_templates()?
        .into_iter()
        .find(|template| template.id == template_id)
        .ok_or_else(|| format!("unknown provider template id: {template_id}"))
}

// ---------------------------------------------------------------------------
// Step 3: template query, sync and incremental propagation
// ---------------------------------------------------------------------------

/// A template plus the origin of its current data, returned by the query and
/// sync commands.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ProviderTemplateView {
    pub template: ProviderTemplate,
    /// Last successful sync time, absent while the built-in snapshot is used.
    pub synced_at: Option<u64>,
    /// Human-readable data source (the snapshot's or the last sync's).
    pub source: String,
    /// True while the view falls back to the in-app snapshot.
    pub from_snapshot: bool,
}

/// Resolve the effective template data for `template_id`: the last persisted
/// sync result when present, else the built-in snapshot. An unknown id is an
/// actionable error naming the id.
pub(in crate::api_gateway) fn effective_template(
    config: &GatewayConfig,
    template_id: &str,
) -> Result<ProviderTemplate, String> {
    if config
        .deleted_template_ids
        .iter()
        .any(|id| id == template_id)
    {
        return Err(format!("Template '{template_id}' has been deleted"));
    }
    if let Some(template) = config
        .provider_templates
        .iter()
        .find(|state| state.template_id == template_id)
        .and_then(|state| state.template.clone())
    {
        return Ok(template);
    }
    find_builtin_template(template_id)
}

/// Build the ordered view of provider templates: persisted sync/custom results
/// win, deleted built-in templates are skipped, and user custom templates are appended.
pub fn provider_template_views(
    config: &GatewayConfig,
) -> Result<Vec<ProviderTemplateView>, String> {
    let mut views = Vec::new();
    let builtins = builtin_templates()?;
    for snapshot in &builtins {
        if config
            .deleted_template_ids
            .iter()
            .any(|id| id == &snapshot.id)
        {
            continue;
        }
        match config
            .provider_templates
            .iter()
            .find(|state| state.template_id == snapshot.id)
            .and_then(|state| {
                state
                    .template
                    .clone()
                    .map(|template| (template, state))
            }) {
            Some((template, state)) => views.push(ProviderTemplateView {
                template,
                synced_at: state.synced_at,
                source: state
                    .source
                    .clone()
                    .filter(|source| !source.trim().is_empty())
                    .unwrap_or_else(|| snapshot.source.clone()),
                from_snapshot: false,
            }),
            None => views.push(ProviderTemplateView {
                template: snapshot.clone(),
                synced_at: None,
                source: snapshot.source.clone(),
                from_snapshot: true,
            }),
        }
    }

    for state in &config.provider_templates {
        if builtins.iter().any(|b| b.id == state.template_id) {
            continue;
        }
        if config
            .deleted_template_ids
            .iter()
            .any(|id| id == &state.template_id)
        {
            continue;
        }
        if let Some(template) = &state.template {
            views.push(ProviderTemplateView {
                template: template.clone(),
                synced_at: state.synced_at,
                source: state
                    .source
                    .clone()
                    .filter(|source| !source.trim().is_empty())
                    .unwrap_or_else(|| template.source.clone()),
                from_snapshot: false,
            });
        }
    }

    Ok(views)
}

/// Parse a model-list payload and merge it over the template's current models.
///
/// Accepted payload shapes are a `data` array, a `models` array, a root array
/// and string entries. An object entry's identifier is `id` (or `name` for the
/// `models` shape when `id` is absent) and its display name is `name`. When
/// `supported_endpoints` is an array the protocol is `/chat/completions` if
/// declared, else `/responses`; an array declaring neither drops the entry,
/// while a missing or non-array field keeps the entry with no protocol so it
/// inherits the template protocol. Existing models keep their local `enabled`
/// flag and any source-omitted display name/protocol; new models start enabled.
/// Duplicate identifiers keep the first. A missing model array, an entry
/// without an identifier and an empty effective set are fatal.
fn parse_model_list_source(
    raw: &str,
    previous: &ProviderTemplate,
) -> Result<Vec<ProviderTemplateModel>, String> {
    let value: Value = serde_json::from_str(raw)
        .map_err(|error| format!("response is not valid JSON: {error}"))?;

    let (entries, allow_name_identifier) =
        if let Some(data) = value.get("data").and_then(Value::as_array) {
            (data, false)
        } else if let Some(models) = value.get("models").and_then(Value::as_array) {
            (models, true)
        } else if let Some(root) = value.as_array() {
            (root, false)
        } else {
            return Err("response is missing the model list".to_string());
        };

    let mut merged: Vec<ProviderTemplateModel> = Vec::with_capacity(entries.len());
    for entry in entries {
        if let Some(raw_identifier) = entry.as_str() {
            let identifier = raw_identifier.trim();
            if identifier.is_empty() {
                return Err(
                    "response contains a model entry without an identifier".to_string()
                );
            }
            if merged
                .iter()
                .any(|model| model.upstream_model == identifier)
            {
                continue;
            }
            merged.push(ProviderTemplateModel {
                upstream_model: identifier.to_string(),
                display_name: None,
                protocol: None,
                enabled: true,
            });
            continue;
        }

        let object = entry.as_object().ok_or_else(|| {
            "response contains a model entry without an identifier".to_string()
        })?;
        let identifier = object
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .or_else(|| {
                if allow_name_identifier {
                    object
                        .get("name")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                } else {
                    None
                }
            })
            .ok_or_else(|| "response contains a model entry without an identifier".to_string())?
            .to_string();

        let protocol = match object.get("supported_endpoints") {
            Some(Value::Array(endpoints)) => {
                let has = |needle: &str| {
                    endpoints
                        .iter()
                        .any(|endpoint| endpoint.as_str() == Some(needle))
                };
                if has("/chat/completions") {
                    Some(UpstreamProtocol::ChatCompletions)
                } else if has("/responses") {
                    Some(UpstreamProtocol::Responses)
                } else {
                    continue;
                }
            }
            _ => None,
        };

        if merged
            .iter()
            .any(|model| model.upstream_model == identifier)
        {
            continue;
        }
        let previous_model = previous
            .models
            .iter()
            .find(|model| model.upstream_model == identifier);
        merged.push(ProviderTemplateModel {
            upstream_model: identifier,
            display_name: object
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| previous_model.and_then(|model| model.display_name.clone())),
            protocol: protocol.or_else(|| previous_model.and_then(|model| model.protocol)),
            enabled: previous_model.map(|model| model.enabled).unwrap_or(true),
        });
    }

    if merged.is_empty() {
        return Err("response carries an empty model set".to_string());
    }

    Ok(merged)
}

/// Replace (or insert) the persisted state for one template after a sync.
fn upsert_template_state(
    config: &mut GatewayConfig,
    template_id: &str,
    template: ProviderTemplate,
    synced_at: u64,
    source: String,
) {
    let state = ProviderTemplateState {
        template_id: template_id.to_string(),
        template: Some(template),
        synced_at: Some(synced_at),
        source: Some(source),
    };
    if let Some(existing) = config
        .provider_templates
        .iter_mut()
        .find(|existing| existing.template_id == template_id)
    {
        *existing = state;
    } else {
        config.provider_templates.push(state);
    }
}

/// Propagate a template update to every derived provider whose `template_id`
/// matches: provider name/base_url/protocol update only while they still equal
/// the previous template values, and only enabled template models add or update
/// mappings. Mapping display name/protocol update only while they equal the
/// previous template values (`enabled` and `local_model` are never touched).
/// Ignored models and disabled template models are skipped, retired models keep
/// their mapping, and no price row is ever created or modified.
fn propagate_to_derived(
    config: &mut GatewayConfig,
    template_id: &str,
    previous: &ProviderTemplate,
    new_template: &ProviderTemplate,
) {
    let providers = &mut config.providers;

    for provider in providers
        .iter_mut()
        .filter(|provider| provider.template_id.as_deref() == Some(template_id))
    {
        if provider.name == previous.name {
            provider.name = new_template.name.clone();
        }
        if provider.base_url == previous.base_url {
            provider.base_url = new_template.base_url.clone();
        }
        if provider.protocol == previous.protocol {
            provider.protocol = new_template.protocol;
        }

        for model in new_template.models.iter().filter(|model| model.enabled) {
            if provider
                .ignored_models
                .iter()
                .any(|ignored| ignored == &model.upstream_model)
            {
                continue;
            }
            let previous_model = previous
                .models
                .iter()
                .find(|candidate| candidate.upstream_model == model.upstream_model);
            let effective_protocol = model.protocol.unwrap_or(new_template.protocol);

            if let Some(mapping) = provider
                .mappings
                .iter_mut()
                .find(|mapping| mapping.upstream_model == model.upstream_model)
            {
                let Some(previous_model) = previous_model else {
                    continue;
                };
                if mapping.display_name == previous_model.display_name {
                    mapping.display_name = model.display_name.clone();
                }
                if mapping.protocol
                    == Some(previous_model.protocol.unwrap_or(previous.protocol))
                {
                    mapping.protocol = Some(effective_protocol);
                }
            } else {
                provider.mappings.push(ModelMapping {
                    local_model: model.upstream_model.clone(),
                    upstream_model: model.upstream_model.clone(),
                    enabled: true,
                    protocol: Some(effective_protocol),
                    display_name: model.display_name.clone(),
                    reasoning_efforts: Vec::new(),
                });
            }
        }
    }
}

/// Apply one template sync with injectable fetch and persistence seams.
///
/// The template's model list is replaced wholesale from its `models_url`; a
/// source-provided display name and derived protocol win while a locally owned
/// `enabled` flag and any omitted value are kept. A blank URL and every fatal
/// source problem (fetch error, non-JSON, missing model array, entry without an
/// identifier, empty effective set) write nothing. The new template and every
/// derived provider update are staged on a clone, handed to `persist`, and only
/// committed to `config` once persistence succeeds.
pub fn apply_template_sync_with(
    config: &mut GatewayConfig,
    template_id: &str,
    fetch: impl FnOnce(&ProviderTemplate) -> Result<String, String>,
    persist: impl FnOnce(&GatewayConfig) -> Result<(), String>,
) -> Result<ProviderTemplateView, String> {
    let previous = effective_template(config, template_id)?;
    let url = previous
        .models_url
        .as_deref()
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .ok_or_else(|| format!("template '{template_id}' has no model-list URL configured"))?
        .to_string();

    let raw = fetch(&previous).map_err(|reason| format!("failed to fetch {url}: {reason}"))?;
    let models = parse_model_list_source(&raw, &previous)
        .map_err(|reason| format!("failed to parse {url}: {reason}"))?;

    let mut next_template = previous.clone();
    next_template.models = models;

    let mut next = config.clone();
    propagate_to_derived(&mut next, template_id, &previous, &next_template);
    let synced_at = now_ts();
    let source = next_template.source.clone();
    upsert_template_state(
        &mut next,
        template_id,
        next_template.clone(),
        synced_at,
        source.clone(),
    );

    persist(&next)?;
    *config = next;

    Ok(ProviderTemplateView {
        template: next_template,
        synced_at: Some(synced_at),
        source,
        from_snapshot: false,
    })
}

/// Fetch a template's model list from its configured `models_url` through
/// `reqwest` with a 15-second timeout and no credentials. A blank URL, non-2xx
/// status, timeout and network error return an actionable error naming the URL
/// and the reason.
pub(in crate::api_gateway) async fn fetch_template_models(
    template: &ProviderTemplate,
) -> Result<String, String> {
    let url = template
        .models_url
        .as_deref()
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .ok_or_else(|| "template has no model-list URL configured".to_string())?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|error| format!("failed to prepare a request for {url}: {error}"))?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| format!("failed to fetch {url}: {error}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("failed to fetch {url}: HTTP {status}"));
    }
    response
        .text()
        .await
        .map_err(|error| format!("failed to read {url}: {error}"))
}

// ---------------------------------------------------------------------------
// Step 4: create-from-template and model maintenance
// ---------------------------------------------------------------------------

/// Build the mapping a template model is created with: `local_model` equals
/// `upstream_model`, carrying the model's enabled flag, the official display
/// name and the model's effective protocol.
fn mapping_from_template(
    template: &ProviderTemplate,
    model: &ProviderTemplateModel,
) -> ModelMapping {
    ModelMapping {
        local_model: model.upstream_model.clone(),
        upstream_model: model.upstream_model.clone(),
        enabled: model.enabled,
        protocol: Some(model.protocol.unwrap_or(template.protocol)),
        display_name: model.display_name.clone(),
        reasoning_efforts: Vec::new(),
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
/// effective protocol) and writes no price row. The staged clone is handed to
/// `persist` and only committed to `config` once persistence succeeds.
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

    let provider = GatewayUpstreamProvider {
        id: new_provider_id(),
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
        api_key: api_key.to_string(),
        template_id: Some(template_id.to_string()),
        protocol,
        mappings: template
            .models
            .iter()
            .filter(|model| model.enabled)
            .map(|model| mapping_from_template(&template, model))
            .collect(),
        ignored_models: Vec::new(),
        ..GatewayUpstreamProvider::default()
    };

    let mut next = config.clone();
    next.providers.push(provider.clone());

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
            .push(mapping_from_template(&template, &model));
    }

    persist(&next)?;
    *config = next;
    Ok(())
}

/// Save an updated or new provider template into persisted state.
pub fn apply_upsert_provider_template<W>(
    config: &mut GatewayConfig,
    mut template: ProviderTemplate,
    write: W,
) -> Result<Vec<ProviderTemplateView>, String>
where
    W: FnOnce(&GatewayConfig) -> Result<(), String>,
{
    template.id = template.id.trim().to_string();
    if template.id.is_empty() {
        template.id = format!("tpl-{}", new_provider_id());
    }
    template.name = template.name.trim().to_string();
    if template.name.is_empty() {
        return Err("Template name is required".to_string());
    }
    template.base_url = template.base_url.trim().to_string();
    if template.base_url.is_empty() {
        return Err("Template base URL is required".to_string());
    }

    config.deleted_template_ids.retain(|id| id != &template.id);

    let template_id = template.id.clone();
    let source = if template.source.trim().is_empty() {
        None
    } else {
        Some(template.source.clone())
    };

    if let Some(state) = config
        .provider_templates
        .iter_mut()
        .find(|s| s.template_id == template_id)
    {
        state.template = Some(template);
        if source.is_some() {
            state.source = source;
        }
    } else {
        config.provider_templates.push(ProviderTemplateState {
            template_id,
            template: Some(template),
            synced_at: None,
            source,
        });
    }

    write(config)?;
    provider_template_views(config)
}

/// Delete a provider template. Fails if the template is currently in use by any upstream provider.
pub fn apply_delete_provider_template<W>(
    config: &mut GatewayConfig,
    template_id: &str,
    write: W,
) -> Result<Vec<ProviderTemplateView>, String>
where
    W: FnOnce(&GatewayConfig) -> Result<(), String>,
{
    let tid = template_id.trim();
    if tid.is_empty() {
        return Err("Template ID is required".to_string());
    }

    let used_by = config
        .providers
        .iter()
        .find(|p| p.template_id.as_deref() == Some(tid));
    if let Some(provider) = used_by {
        return Err(format!(
            "Cannot delete template '{tid}': it is currently used by upstream provider '{}'",
            provider.name
        ));
    }

    config.provider_templates.retain(|s| s.template_id != tid);
    if !config.deleted_template_ids.iter().any(|id| id == tid) {
        config.deleted_template_ids.push(tid.to_string());
    }

    write(config)?;
    provider_template_views(config)
}

/// Reset built-in provider templates back to snapshot defaults.
pub fn apply_reset_provider_templates<W>(
    config: &mut GatewayConfig,
    write: W,
) -> Result<Vec<ProviderTemplateView>, String>
where
    W: FnOnce(&GatewayConfig) -> Result<(), String>,
{
    config.deleted_template_ids.clear();
    let builtins = builtin_templates()?;
    for state in &mut config.provider_templates {
        if builtins.iter().any(|b| b.id == state.template_id) {
            state.template = None;
        }
    }
    write(config)?;
    provider_template_views(config)
}



