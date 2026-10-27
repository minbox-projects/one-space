use super::provider::{provider_context_snapshot, refresh_previously_synced_tools_after_save};
#[cfg(test)]
use super::terminal::previously_synced_terminal_tools;
#[cfg(test)]
use super::TerminalSyncProfile;
use crate::ai_gateway::storage::{modify_config, read_config};
#[cfg(test)]
use crate::ai_gateway::TerminalSyncRecord;
use crate::ai_gateway::templates::{
    apply_create_provider_from_template, apply_delete_provider_model,
    apply_delete_provider_template, apply_restore_provider_model,
    apply_upsert_provider_template, provider_template_views, ProviderRetirementNotice,
    ProviderTemplateView, TemplateAdditionNotice,
};
use crate::ai_gateway::{GatewayConfig, ProviderTemplate, UpstreamProtocol};

#[cfg(test)]
thread_local! {
    pub(in crate::ai_gateway) static TEMPLATE_MAPPINGS_RETIRED_MESSAGE_INPUTS:
        std::cell::RefCell<Vec<crate::messages::MessageCreateInput>> =
        const { std::cell::RefCell::new(Vec::new()) };
    pub(in crate::ai_gateway) static TEMPLATE_SYNC_MESSAGE_INPUTS:
        std::cell::RefCell<Vec<crate::messages::MessageCreateInput>> =
        const { std::cell::RefCell::new(Vec::new()) };
}
/// Injectable manual-template-save composition: persist through the production
/// save path, then refresh the previously synchronized tools at most once.
#[cfg(test)]
pub(in crate::ai_gateway) async fn apply_template_save_with_terminal_refresh<F, Fut>(
    template: ProviderTemplate,
    providers_data: &serde_json::Value,
    terminal_sync: F,
) -> Result<Vec<ProviderTemplateView>, String>
where
    F: FnOnce(Vec<String>) -> Fut + Send,
    Fut: std::future::Future<Output = Result<Vec<TerminalSyncRecord>, String>> + Send,
{
    let (views, saved, context_changed) = save_template_persist(template)?;
    if context_changed {
        let tools = previously_synced_terminal_tools(
            &saved,
            providers_data,
            TerminalSyncProfile::current(),
        );
        if !tools.is_empty() {
            let _ = terminal_sync(tools).await;
        }
    }
    Ok(views)
}
// ---------------------------------------------------------------------------
// Provider templates
// ---------------------------------------------------------------------------

/// The two built-in provider templates with their current data source: a
/// persisted sync result when one exists, else the in-app snapshot.
#[tauri::command]
pub fn ai_gateway_provider_templates() -> Result<Vec<ProviderTemplateView>, String> {
    let config = read_config()?;
    provider_template_views(&config)
}

/// Build the message-center input for one provider's template-retirement
/// transition. Only the provider and model names appear; both the summary and
/// the `detail` list the removed models in mapping row order without
/// duplicates, the provider-scoped `dedupe_key` makes repeated transitions
/// inside the store's dedupe window merge into one entry while refreshing the
/// latest removed models, and the targeted AI Gateway tab carries the provider
/// as its entity id. Neither the title, summary, detail, dedupe key, target nor
/// metadata ever carries a key value (REQ-006).
fn template_mappings_retired_message_input(
    notice: &ProviderRetirementNotice,
) -> crate::messages::MessageCreateInput {
    let models = notice.retired_models.join(", ");
    crate::messages::MessageCreateInput {
        source: "ai_gateway".to_string(),
        category: "template_mappings_retired".to_string(),
        severity: "warning".to_string(),
        title: crate::messages::localized(
            "AI 网关模板映射已移除",
            "AI Gateway template mappings retired",
        ),
        summary: Some(crate::messages::localized(
            &format!("服务商 {} 的映射已从模板移除：{}", notice.provider_name, models),
            &format!(
                "Provider {} mappings removed from template: {}",
                notice.provider_name, models
            ),
        )),
        detail: Some(models),
        dedupe_key: Some(format!(
            "ai_gateway_template_mappings_retired:{}",
            notice.provider_id
        )),
        target: Some(crate::messages::MessageTarget {
            tab: "ai-gateway".to_string(),
            section: None,
            entity_id: Some(notice.provider_id.clone()),
        }),
        metadata: None,
    }
}

/// Record one template-retirement transition per notice, in order: push the
/// literal input onto the test seam and persist exactly one message input through
/// the supplied handle. A missing handle skips the production write but still
/// records the seam value so the transition is observable in tests (REQ-006).
pub(in crate::ai_gateway) fn record_template_mappings_retired_messages(
    app: Option<&tauri::AppHandle>,
    notices: &[ProviderRetirementNotice],
) {
    for notice in notices {
        let input = template_mappings_retired_message_input(notice);
        #[cfg(test)]
        TEMPLATE_MAPPINGS_RETIRED_MESSAGE_INPUTS.with(|inputs| inputs.borrow_mut().push(input.clone()));
        if let Some(handle) = app {
            crate::messages::record_message_silent(handle, input);
        }
    }
}
/// Build the message-center input for one template sync that actually added
/// model mappings. The title names the template, the summary carries the
/// affected-provider and added-model counts, and the detail lists each
/// provider's added models. Only provider and model names appear; no key value
/// is ever carried (REQ-002/AC-002).
fn template_sync_message_input(
    view: &ProviderTemplateView,
    additions: &[TemplateAdditionNotice],
) -> crate::messages::MessageCreateInput {
    let provider_count = additions.len();
    let added_count: usize = additions
        .iter()
        .map(|notice| notice.added_models.len())
        .sum();
    let detail = additions
        .iter()
        .map(|notice| {
            let models = notice.added_models.join(", ");
            crate::messages::localized(
                &format!("{}：{}", notice.provider_name, models),
                &format!("{}: {}", notice.provider_name, models),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let summary = format!(
        "{}; {}",
        crate::messages::localized(
            &format!("{provider_count} 个服务商受影响"),
            &format!("{provider_count} provider(s) affected"),
        ),
        crate::messages::localized(
            &format!("新增 {added_count}"),
            &format!("{added_count} added"),
        ),
    );
    crate::messages::MessageCreateInput {
        source: "ai_gateway".to_string(),
        category: "template_sync".to_string(),
        severity: "info".to_string(),
        title: crate::messages::localized(
            &format!("服务商模板 {} 模型已变更", view.template.name),
            &format!("Provider template {} models changed", view.template.name),
        ),
        summary: Some(summary),
        detail: Some(detail),
        dedupe_key: None,
        target: Some(crate::messages::MessageTarget {
            tab: "ai-gateway".to_string(),
            section: None,
            entity_id: None,
        }),
        metadata: None,
    }
}

/// Record one additions-only `template_sync` info message when the sync added at
/// least one model mapping. A sync without additions records nothing. The write
/// is best-effort and never fails the sync: the literal input is pushed onto the
/// test seam and persisted through the supplied handle if one is present
/// (REQ-002/AC-002).
pub(in crate::ai_gateway) fn record_template_sync_addition_message(
    app: Option<&tauri::AppHandle>,
    view: &ProviderTemplateView,
    additions: &[TemplateAdditionNotice],
) {
    if additions.is_empty() {
        return;
    }
    let input = template_sync_message_input(view, additions);
    #[cfg(test)]
    TEMPLATE_SYNC_MESSAGE_INPUTS.with(|inputs| inputs.borrow_mut().push(input.clone()));
    if let Some(handle) = app {
        crate::messages::record_message_silent(handle, input);
    }
}
/// Create an upstream provider from a template, carrying one enabled mapping per
/// enabled template model and writing no price row. A blank API key is rejected
/// and writes nothing.
#[tauri::command]
pub fn ai_gateway_create_provider_from_template(
    template_id: String,
    name: String,
    base_url: String,
    protocol: UpstreamProtocol,
    api_key: String,
) -> Result<GatewayConfig, String> {
    modify_config(|config| {
        apply_create_provider_from_template(
            config,
            &template_id,
            &name,
            &base_url,
            protocol,
            &api_key,
            |_| Ok(()),
        )?;
        Ok((true, ()))
    })?;
    read_config()
}

/// Delete one model from a provider: a template-bound provider records it in the
/// ignored set exactly once and both bound and manual providers drop that model's
/// provider-scoped price row.
#[tauri::command]
pub fn ai_gateway_delete_provider_model(
    provider_id: String,
    upstream_model: String,
) -> Result<GatewayConfig, String> {
    modify_config(|config| {
        apply_delete_provider_model(config, &provider_id, &upstream_model, |_| Ok(()))?;
        Ok((true, ()))
    })?;
    read_config()
}

/// Restore one ignored model from the template's current data and return the
/// refreshed configuration.
#[tauri::command]
pub fn ai_gateway_restore_provider_model(
    provider_id: String,
    upstream_model: String,
) -> Result<GatewayConfig, String> {
    modify_config(|config| {
        apply_restore_provider_model(config, &provider_id, &upstream_model, |_| Ok(()))?;
        Ok((true, ()))
    })?;
    read_config()
}

/// Upsert a provider template: update an existing template or add a new custom template.
#[tauri::command]
pub fn ai_gateway_upsert_provider_template(
    template: ProviderTemplate,
) -> Result<Vec<ProviderTemplateView>, String> {
    let (views, saved, context_changed) = save_template_persist(template)?;
    if context_changed {
        refresh_previously_synced_tools_after_save(&saved);
    }
    Ok(views)
}

/// Persist one manual template save, reporting whether any provider's inherited
/// context changed. Bound mappings are updated by the existing save path; the
/// comparison only decides whether a terminal refresh is warranted.
fn save_template_persist(
    template: ProviderTemplate,
) -> Result<(Vec<ProviderTemplateView>, GatewayConfig, bool), String> {
    modify_config(|config| {
        let before = provider_context_snapshot(config);
        let views = apply_upsert_provider_template(config, template, |_| Ok(()))?;
        let after = provider_context_snapshot(config);
        let context_changed = before != after;
        Ok((true, (views, config.clone(), context_changed)))
    })
}

/// Delete a provider template. Fails if the template is currently in use by any upstream provider.
#[tauri::command]
pub fn ai_gateway_delete_provider_template(
    template_id: String,
) -> Result<Vec<ProviderTemplateView>, String> {
    modify_config(|config| {
        let views = apply_delete_provider_template(config, &template_id, |_| Ok(()))?;
        Ok((true, views))
    })
}
