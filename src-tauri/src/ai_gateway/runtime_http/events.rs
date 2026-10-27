use super::*;

/// Application handle captured when the gateway server starts, so a settlement
/// can broadcast a runtime-state transition without any frontend action. `None`
/// means no handle was available and emission is skipped.
static APP_HANDLE: OnceLock<std::sync::Mutex<Option<tauri::AppHandle>>> = OnceLock::new();

fn app_handle_slot() -> &'static std::sync::Mutex<Option<tauri::AppHandle>> {
    APP_HANDLE.get_or_init(|| std::sync::Mutex::new(None))
}

/// Replace the process-wide captured handle; `None` clears it. Called before
/// the early-return path of [`start_server`] so a restart refreshes the slot.
pub(super) fn capture_app_handle(app: Option<tauri::AppHandle>) {
    let mut guard = app_handle_slot()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    *guard = app;
}

pub(super) fn captured_app_handle() -> Option<tauri::AppHandle> {
    app_handle_slot()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone()
}

#[cfg(test)]
thread_local! {
    pub(in crate::ai_gateway) static CONFIG_UPDATE_EVENTS: std::cell::RefCell<Vec<String>> =
        const { std::cell::RefCell::new(Vec::new()) };
    pub(in crate::ai_gateway) static KEY_AUTH_FAILED_EVENTS: std::cell::RefCell<Vec<String>> =
        const { std::cell::RefCell::new(Vec::new()) };
    pub(in crate::ai_gateway) static KEY_AUTH_FAILED_PAYLOADS:
        std::cell::RefCell<Vec<super::GatewayKeyAuthFailedPayload>> =
        const { std::cell::RefCell::new(Vec::new()) };
    pub(in crate::ai_gateway) static KEY_AUTH_FAILED_MESSAGE_INPUTS:
        std::cell::RefCell<Vec<crate::messages::MessageCreateInput>> =
        const { std::cell::RefCell::new(Vec::new()) };
    pub(in crate::ai_gateway) static MAPPING_AUTO_DISABLED_MESSAGE_INPUTS:
        std::cell::RefCell<Vec<crate::messages::MessageCreateInput>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Emit one `ai-gateway-config-update` event when a handle was captured; a
/// missing handle is a no-op that never affects settlement.
pub(in crate::ai_gateway) fn emit_config_updated() {
    #[cfg(test)]
    CONFIG_UPDATE_EVENTS.with(|events| {
        events
            .borrow_mut()
            .push(super::AI_GATEWAY_CONFIG_UPDATED_EVENT.to_string())
    });
    if let Some(handle) = captured_app_handle() {
        let _ = handle.emit(super::AI_GATEWAY_CONFIG_UPDATED_EVENT, ());
    }
}

/// Build the message-center input for one authentication-failed transition. The
/// provider-scoped `dedupe_key` makes repeated transitions inside the store's
/// dedupe window merge into one entry with an incremented occurrence count, and
/// the targeted AI Gateway tab carries the provider as its entity id. Neither
/// the title, summary, dedupe key, target nor metadata ever carries a key value.
fn key_auth_failed_message_input(
    payload: &super::GatewayKeyAuthFailedPayload,
) -> crate::messages::MessageCreateInput {
    crate::messages::MessageCreateInput {
        source: "ai_gateway".to_string(),
        category: "key_auth_failed".to_string(),
        severity: "warning".to_string(),
        title: crate::messages::localized(
            "AI 网关密钥鉴权失败",
            "AI Gateway key authentication failed",
        ),
        summary: Some(crate::messages::localized(
            &format!(
                "服务商 {} 的密钥 {} 鉴权失败：{}",
                payload.provider_name, payload.key_name, payload.reason
            ),
            &format!(
                "Provider {} key {} failed authentication: {}",
                payload.provider_name, payload.key_name, payload.reason
            ),
        )),
        detail: None,
        dedupe_key: Some(format!(
            "ai_gateway_key_auth_failed:{}",
            payload.provider_id
        )),
        target: Some(crate::messages::MessageTarget {
            tab: "ai-gateway".to_string(),
            section: None,
            entity_id: Some(payload.provider_id.clone()),
        }),
        metadata: None,
    }
}

/// Broadcast one authentication-failed transition: record the literal and
/// payload on the test seams, emit the notification event and persist exactly
/// one message input through the captured handle. A missing handle skips both
/// production emissions but still records the seam values so the transition is
/// observable in tests.
pub(in crate::ai_gateway) fn emit_key_auth_failed(transition: &KeyAuthFailedTransition) {
    let payload = super::GatewayKeyAuthFailedPayload {
        provider_id: transition.provider_id.clone(),
        provider_name: transition.provider_name.clone(),
        key_id: transition.key_id.clone(),
        key_name: transition.key_name.clone(),
        reason: transition.reason.clone(),
        marked_at: transition.marked_at,
    };
    #[cfg(test)]
    KEY_AUTH_FAILED_EVENTS.with(|events| {
        events
            .borrow_mut()
            .push(super::AI_GATEWAY_KEY_AUTH_FAILED_EVENT.to_string())
    });
    #[cfg(test)]
    KEY_AUTH_FAILED_PAYLOADS.with(|payloads| payloads.borrow_mut().push(payload.clone()));
    let input = key_auth_failed_message_input(&payload);
    #[cfg(test)]
    KEY_AUTH_FAILED_MESSAGE_INPUTS.with(|inputs| inputs.borrow_mut().push(input.clone()));
    if let Some(handle) = captured_app_handle() {
        let _ = handle.emit(super::AI_GATEWAY_KEY_AUTH_FAILED_EVENT, &payload);
        crate::messages::record_message_silent(&handle, input);
    }
}

/// Build the message-center input for one provider's auto-disable transition.
/// The provider-scoped `dedupe_key` makes repeated transitions inside the
/// store's dedupe window merge into one entry with an incremented occurrence
/// count while refreshing the full set; the targeted AI Gateway tab carries the
/// provider as its entity id. Only provider and model names appear.
fn mapping_auto_disabled_message_input(
    transition: &MappingAutoDisabledTransition,
) -> crate::messages::MessageCreateInput {
    crate::messages::MessageCreateInput {
        source: "ai_gateway".to_string(),
        category: "mapping_auto_disabled".to_string(),
        severity: "warning".to_string(),
        title: crate::messages::localized(
            "AI 网关映射自动禁用",
            "AI Gateway mappings auto-disabled",
        ),
        summary: Some(crate::messages::localized(
            &format!(
                "服务商 {} 的映射已自动禁用：{}",
                transition.provider_name, transition.models
            ),
            &format!(
                "Provider {} mappings auto-disabled: {}",
                transition.provider_name, transition.models
            ),
        )),
        detail: Some(transition.models.clone()),
        dedupe_key: Some(format!(
            "ai_gateway_mapping_auto_disabled:{}",
            transition.provider_id
        )),
        target: Some(crate::messages::MessageTarget {
            tab: "ai-gateway".to_string(),
            section: None,
            entity_id: Some(transition.provider_id.clone()),
        }),
        metadata: None,
    }
}

/// Record one auto-disable transition: push the literal input onto the test
/// seam and persist exactly one message input through the captured handle. A
/// missing handle skips the production write but still records the seam value so
/// the transition is observable in tests.
pub(in crate::ai_gateway) fn emit_mapping_auto_disabled(
    transition: &MappingAutoDisabledTransition,
) {
    let input = mapping_auto_disabled_message_input(transition);
    #[cfg(test)]
    MAPPING_AUTO_DISABLED_MESSAGE_INPUTS.with(|inputs| inputs.borrow_mut().push(input.clone()));
    if let Some(handle) = captured_app_handle() {
        crate::messages::record_message_silent(&handle, input);
    }
}
