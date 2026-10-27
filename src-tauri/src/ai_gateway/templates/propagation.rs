//! Template sync orchestration and retirement/addition propagation.

use super::super::types_config::{now_ts, GatewayConfig, ModelMapping, ProviderTemplate};
use super::source::{complete_model_display_name, parse_model_list_source};
use super::state::{effective_template, upsert_template_state, ProviderTemplateView};

/// One provider whose derived mappings a template sync removed: the removed
/// `upstream_model` values in mapping row order without duplicates. Emitted once
/// per bound provider with at least one removal, including a provider whose only
/// removals were already-disabled legacy rows. The provider name is the stored
/// name after propagation; no key value is ever carried.
#[derive(Debug, Clone, PartialEq)]
pub(in crate::ai_gateway) struct ProviderRetirementNotice {
    pub provider_id: String,
    pub provider_name: String,
    pub retired_models: Vec<String>,
}

/// One provider whose template sync added model mappings: the added models in
/// mapping row order without duplicates, named by their trimmed `local_model`
/// when non-empty, otherwise by their `upstream_model`. A provider with no
/// addition yields no notice; no key value is ever carried.
#[derive(Debug, Clone, PartialEq)]
pub(in crate::ai_gateway) struct TemplateAdditionNotice {
    pub provider_name: String,
    pub added_models: Vec<String>,
}

/// Diff the providers bound to `template_id` between the pre-sync `before` and
/// the post-sync `after` configuration, returning one notice per provider that
/// gained at least one mapping. Only a mapping whose `upstream_model` was absent
/// from the same provider before the sync qualifies; a provider not bound to the
/// template is ignored. The provider list and mapping order of `after` define
/// the output order.
pub(in crate::ai_gateway) fn template_sync_addition_notices(
    before: &GatewayConfig,
    after: &GatewayConfig,
    template_id: &str,
) -> Vec<TemplateAdditionNotice> {
    let mut notices = Vec::new();
    for provider in after
        .providers
        .iter()
        .filter(|provider| provider.template_id.as_deref() == Some(template_id))
    {
        let previous_upstream: Vec<&str> = before
            .providers
            .iter()
            .find(|candidate| candidate.id == provider.id)
            .map(|previous| {
                previous
                    .mappings
                    .iter()
                    .map(|mapping| mapping.upstream_model.as_str())
                    .collect()
            })
            .unwrap_or_default();

        let mut added_models: Vec<String> = Vec::new();
        for mapping in &provider.mappings {
            if previous_upstream
                .iter()
                .any(|upstream| *upstream == mapping.upstream_model)
            {
                continue;
            }
            let local_model = mapping.local_model.trim();
            added_models.push(if local_model.is_empty() {
                mapping.upstream_model.clone()
            } else {
                local_model.to_string()
            });
        }

        if !added_models.is_empty() {
            notices.push(TemplateAdditionNotice {
                provider_name: provider.name.clone(),
                added_models,
            });
        }
    }
    notices
}

/// The set form of a declared reasoning-effort list: every token trimmed, blank
/// tokens dropped, duplicates removed and the remainder sorted so two lists
/// compare equal regardless of order, duplication or surrounding whitespace.
fn normalized_effort_set(efforts: &[String]) -> Vec<&str> {
    let mut out: Vec<&str> = Vec::with_capacity(efforts.len());
    for effort in efforts {
        let trimmed = effort.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !out.iter().any(|existing| *existing == trimmed) {
            out.push(trimmed);
        }
    }
    out.sort_unstable();
    out
}

/// Propagate a template update to every derived provider whose `template_id`
/// matches: provider name/base_url/protocol update only while they still equal
/// the previous template values, and only enabled template models add new
/// mappings. A mapping is deleted when its upstream model left the template
/// (whatever its enabled flag) or when it was already disabled and its model the
/// new template does not list; a retired model is appended once to the
/// provider's ignored set so a later sync cannot resurrect it, its
/// provider-scoped price row is deleted, and a `default_model` naming it is
/// cleared. An enabled mapping the sync did not retire and a user-disabled
/// mapping whose model the new template still lists stay untouched. Mapping
/// display name/protocol update only while they equal the previous template
/// values (`local_model` is never touched); a mapping whose effective protocol
/// equals the provider protocol is stored as `None` (follow the provider)
/// instead of an explicit value. Every existing matching mapping whose
/// normalized effort set still equals the previous template model's set — an
/// alias, a user-disabled or auto-disabled row, or a disabled template model —
/// follows the new template capabilities, while an independently customized set
/// is preserved; enablement and health flags are never changed. Ignored models
/// and disabled template models never create a new mapping.
///
/// Returns one [`ProviderRetirementNotice`] for every bound provider with at
/// least one removal in this sync, in provider order. Each notice lists the
/// provider's removed models in mapping row order without duplicates,
/// including a provider whose only removals were already-disabled legacy rows.
/// A provider with no removal yields no notice.
fn propagate_to_derived(
    config: &mut GatewayConfig,
    template_id: &str,
    previous: &ProviderTemplate,
    new_template: &ProviderTemplate,
) -> Vec<ProviderRetirementNotice> {
    // Models the previous template carried that the new template no longer lists
    // are retired regardless of their enabled flag in either template.
    let retired_models: Vec<&str> = previous
        .models
        .iter()
        .filter(|model| {
            !new_template
                .models
                .iter()
                .any(|candidate| candidate.upstream_model == model.upstream_model)
        })
        .map(|model| model.upstream_model.as_str())
        .collect();

    let mut notices: Vec<ProviderRetirementNotice> = Vec::new();
    // Removed models per provider id, in mapping row order without duplicates,
    // collected while the provider loop holds the `config.providers` borrow so
    // the staged price rows can be pruned right after it.
    let mut removed_by_provider: Vec<(String, Vec<String>)> = Vec::new();
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
        let provider_protocol_before = provider.protocol;
        if provider.protocol == previous.protocol {
            provider.protocol = new_template.protocol;
        }
        let provider_protocol = provider.protocol;

        // Delete every mapping this sync retires: one whose upstream model the
        // previous template carried but the new template dropped (whatever its
        // enabled flag), or an already-disabled row whose model the new template
        // does not list. An enabled manual extra the sync never retired stays.
        let mut removed: Vec<String> = Vec::new();
        provider.mappings.retain(|mapping| {
            let retired_by_sync = retired_models
                .iter()
                .any(|retired| *retired == mapping.upstream_model);
            let legacy_retired = !mapping.enabled
                && !new_template
                    .models
                    .iter()
                    .any(|model| model.upstream_model == mapping.upstream_model);
            if retired_by_sync || legacy_retired {
                if !removed
                    .iter()
                    .any(|existing| existing == &mapping.upstream_model)
                {
                    removed.push(mapping.upstream_model.clone());
                }
                false
            } else {
                true
            }
        });

        if !removed.is_empty() {
            // Each removed model is ignored once so a later sync cannot
            // resurrect it, whichever row first carried it.
            for model in &removed {
                if !provider
                    .ignored_models
                    .iter()
                    .any(|ignored| ignored == model)
                {
                    provider.ignored_models.push(model.clone());
                }
            }
            // A `default_model` naming a removed model is cleared; a trimmed
            // comparison keeps whitespace-padded values matching.
            let clears_default_model = provider
                .default_model
                .as_deref()
                .map(str::trim)
                .is_some_and(|default_model| {
                    removed.iter().any(|model| model.as_str() == default_model)
                });
            if clears_default_model {
                provider.default_model = None;
            }
            notices.push(ProviderRetirementNotice {
                provider_id: provider.id.clone(),
                provider_name: provider.name.clone(),
                retired_models: removed.clone(),
            });
            removed_by_provider.push((provider.id.clone(), removed));
        }

        // Declared context is owned by the latest template for every remaining
        // row: aliases, user-disabled rows and auto-disabled rows follow the
        // first template model with the same trimmed upstream id, and a row the
        // template no longer lists becomes unspecified. Mapping health and
        // enablement are never changed here.
        for mapping in &mut provider.mappings {
            mapping.context_window = new_template
                .models
                .iter()
                .find(|model| model.upstream_model.trim() == mapping.upstream_model.trim())
                .and_then(|model| model.context_window);
        }

        // Explicit capability maintenance runs before creation and on existing
        // rows only, so it never creates a mapping and never resurrects an
        // ignored model. A row whose set still equals the previous template
        // model's set is inherited and follows the new set; any other set is an
        // independent override and is preserved.
        for model in &new_template.models {
            if provider
                .ignored_models
                .iter()
                .any(|ignored| ignored == &model.upstream_model)
            {
                continue;
            }
            let Some(previous_model) = previous
                .models
                .iter()
                .find(|candidate| candidate.upstream_model == model.upstream_model)
            else {
                continue;
            };
            let inherited_set = normalized_effort_set(&previous_model.reasoning_efforts);
            for mapping in provider
                .mappings
                .iter_mut()
                .filter(|mapping| mapping.upstream_model == model.upstream_model)
            {
                if normalized_effort_set(&mapping.reasoning_efforts) == inherited_set {
                    mapping.reasoning_efforts = model.reasoning_efforts.clone();
                }
            }
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
            let desired_protocol = if effective_protocol == provider_protocol {
                None
            } else {
                Some(effective_protocol)
            };

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
                let previous_effective =
                    previous_model.protocol.unwrap_or(previous.protocol);
                if mapping.effective_protocol(provider_protocol_before) == previous_effective
                {
                    mapping.protocol = desired_protocol;
                }
            } else {
                provider.mappings.push(ModelMapping {
                    local_model: model
                        .local_model
                        .as_deref()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .unwrap_or(&model.upstream_model)
                        .to_string(),
                    upstream_model: model.upstream_model.clone(),
                    enabled: true,
                    protocol: desired_protocol,
                    display_name: model
                        .display_name
                        .as_deref()
                        .map(str::trim)
                        .filter(|name| !name.is_empty())
                        .map(str::to_string)
                        .or_else(|| {
                            complete_model_display_name(None, &model.upstream_model)
                        }),
                    reasoning_efforts: model.reasoning_efforts.clone(),
                    context_window: model.context_window,
                    ..Default::default()
                });
            }
        }
    }

    // Drop exactly the staged price rows of each affected provider and removed
    // model; rows of other providers or other models stay.
    config.model_prices.retain(|row| {
        !removed_by_provider.iter().any(|(provider_id, removed)| {
            row.provider_id.as_deref() == Some(provider_id.as_str())
                && removed.iter().any(|model| model == &row.upstream_model)
        })
    });

    notices
}

/// Apply one template sync with injectable fetch and persistence seams, returning
/// the retirement notices alongside the view.
///
/// The template's model list is replaced wholesale from its `models_url`; a
/// source-provided display name completed with the identifier's uncovered
/// remainder (and the derived protocol) wins while a locally owned
/// `enabled` flag and any omitted value are kept for models the new list still
/// carries. A sync deletes the derived mapping of every model the previous
/// template carried but the new list removed (whatever the mapping's enabled
/// flag) and of every already-disabled mapping whose model the new list no
/// longer has; each removed model is recorded once in the provider's ignored
/// set, its provider-scoped price row is deleted, and a `default_model` naming
/// it is cleared. A blank URL and every fatal source problem (fetch error,
/// non-JSON, missing model array, entry without an identifier, empty effective
/// set) write nothing. The new template and every derived provider update are
/// staged on a clone, handed to `persist`, and only committed to `config` once
/// persistence succeeds. The notices are computed from the staged clone and
/// returned only after `persist` succeeds and `config` is updated; any error
/// returns no notices.
pub(in crate::ai_gateway) fn apply_template_sync_with_notices(
    config: &mut GatewayConfig,
    template_id: &str,
    fetch: impl FnOnce(&ProviderTemplate) -> Result<String, String>,
    persist: impl FnOnce(&GatewayConfig) -> Result<(), String>,
) -> Result<(ProviderTemplateView, Vec<ProviderRetirementNotice>), String> {
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
    let notices = propagate_to_derived(&mut next, template_id, &previous, &next_template);
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

    Ok((
        ProviderTemplateView {
            template: next_template,
            synced_at: Some(synced_at),
            source,
            from_snapshot: false,
        },
        notices,
    ))
}

/// Apply one template sync with injectable fetch and persistence seams, dropping
/// the retirement notices.
///
/// Test-only wrapper; production syncs go through
/// [`apply_template_sync_with_notices`], which carries the same behavior plus
/// the notices.
#[cfg(test)]
pub fn apply_template_sync_with(
    config: &mut GatewayConfig,
    template_id: &str,
    fetch: impl FnOnce(&ProviderTemplate) -> Result<String, String>,
    persist: impl FnOnce(&GatewayConfig) -> Result<(), String>,
) -> Result<ProviderTemplateView, String> {
    apply_template_sync_with_notices(config, template_id, fetch, persist)
        .map(|(view, _notices)| view)
}

/// Apply one template sync from an already-fetched model-list body, with no
/// network call and no nested persistence.
///
/// The caller performs the HTTP fetch outside the configuration write lock and
/// then runs this inside the serialized mutation: the body is parsed against
/// the latest persisted template exactly like [`apply_template_sync_with`], and
/// every template/derived-provider change is committed to `config`. A parse
/// failure writes nothing. The retirement notices computed by the sync are
/// returned so the caller can record them once the write stands.
pub(in crate::ai_gateway) fn apply_template_sync_from_body(
    config: &mut GatewayConfig,
    template_id: &str,
    raw: &str,
) -> Result<(ProviderTemplateView, Vec<ProviderRetirementNotice>), String> {
    apply_template_sync_with_notices(config, template_id, |_| Ok(raw.to_string()), |_| Ok(()))
}
