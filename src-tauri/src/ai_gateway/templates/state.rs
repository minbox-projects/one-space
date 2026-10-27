//! Effective template resolution, ordered template views and persisted template state.

use super::super::storage::new_provider_id;
use super::super::types_config::{
    GatewayConfig, GatewayUpstreamProvider, ProviderTemplate, ProviderTemplateState, QuotaStrategy,
};
use super::snapshot::{builtin_templates, find_builtin_template};

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
pub(in crate::ai_gateway) fn effective_template(
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

/// Resolve the effective template for a provider binding, or `None` when the
/// provider is unbound or its stored `template_id` no longer resolves (unknown
/// or deleted). A missing/broken binding yields the generic policy at runtime
/// through the callers' defaults; no URL, name or icon is ever consulted.
pub(in crate::ai_gateway) fn effective_provider_template(
    config: &GatewayConfig,
    provider: &GatewayUpstreamProvider,
) -> Option<ProviderTemplate> {
    provider
        .template_id
        .as_deref()
        .and_then(|template_id| effective_template(config, template_id).ok())
}

/// The effective quota strategy for a provider: its bound template's declared
/// strategy, else `none` for an unbound, unknown or deleted binding.
pub(in crate::ai_gateway) fn effective_quota_strategy(
    config: &GatewayConfig,
    provider: &GatewayUpstreamProvider,
) -> QuotaStrategy {
    effective_provider_template(config, provider)
        .map(|template| template.quota_strategy)
        .unwrap_or_default()
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

/// Replace (or insert) the persisted state for one template after a sync.
pub(super) fn upsert_template_state(
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
    template.icon = template
        .icon
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    for model in &template.models {
        if model.context_window == Some(0) {
            return Err(format!(
                "model '{}' declares an invalid context window: expected a positive integer",
                model.upstream_model
            ));
        }
    }

    let saved_models = template.models.clone();
    let mut next = config.clone();
    next.deleted_template_ids.retain(|id| id != &template.id);

    let template_id = template.id.clone();
    let source = if template.source.trim().is_empty() {
        None
    } else {
        Some(template.source.clone())
    };

    if let Some(state) = next
        .provider_templates
        .iter_mut()
        .find(|s| s.template_id == template_id)
    {
        state.template = Some(template);
        if source.is_some() {
            state.source = source;
        }
    } else {
        next.provider_templates.push(ProviderTemplateState {
            template_id: template_id.clone(),
            template: Some(template),
            synced_at: None,
            source,
        });
    }

    // A manual template save owns the inherited context of every bound mapping,
    // including aliases and disabled rows. A mapping whose trimmed upstream id
    // the template no longer lists becomes unspecified without retiring the row.
    for provider in next
        .providers
        .iter_mut()
        .filter(|provider| provider.template_id.as_deref() == Some(template_id.as_str()))
    {
        for mapping in &mut provider.mappings {
            mapping.context_window = saved_models
                .iter()
                .find(|model| model.upstream_model.trim() == mapping.upstream_model.trim())
                .and_then(|model| model.context_window);
        }
    }

    write(&next)?;
    *config = next;
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
