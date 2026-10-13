use super::migration::migrate_legacy_files;
use super::model_pools::{
    new_pool_id, pool_status, reserve_alias, reserved_aliases, validate_pool_upsert,
    ModelPoolStatus,
};
use super::runtime_http::{autostart, server_status, start_server, stop_server};
use super::selection::{
    clear_key_runtime_state, clear_mapping_runtime_state, manual_reenable, mapping_matches_key,
    resolve_model_for_protocol, set_user_enabled, ModelResolution,
};
use super::storage::{
    find_provider_mut, local_base_url, local_origin, modify_config, new_key_id, new_key_value,
    new_provider_id, read_config, resolve_default_key_id, touch_key_created_at,
};
use super::templates::{
    apply_create_provider_from_template, apply_delete_provider_model,
    apply_delete_provider_template, apply_restore_provider_model,
    apply_template_sync_from_body, apply_upsert_provider_template, effective_template,
    fetch_template_models, provider_template_views, template_sync_addition_notices,
    ProviderRetirementNotice, ProviderTemplateView, TemplateAdditionNotice,
};
#[cfg(test)]
use super::templates::apply_template_sync_with;
use super::usage_log::{
    normalize_retention_days, now_millis, resolve_range_selector, validate_retention_days,
    LogFilter, UsageLogStore, UsageLogsPage, UsageStats, USAGE_LOG_PAGE_SIZE,
};
use super::{
    normalize_template_auto_refresh_minutes, now_ts, validate_template_auto_refresh_minutes,
    ClaudeModelSelection, GatewayConfig, GatewayKey, GatewayStatus, GatewayUpstreamProvider,
    ModelPool, ModelPrice, ProviderTemplate, TemplateAutoRefreshStatus, TerminalSyncRecord,
    UpstreamProtocol, UsageResult, MAX_PROVIDER_WEIGHT, MIN_PROVIDER_WEIGHT,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

#[cfg(test)]
thread_local! {
    pub(in crate::ai_gateway) static TEMPLATE_MAPPINGS_RETIRED_MESSAGE_INPUTS:
        std::cell::RefCell<Vec<crate::messages::MessageCreateInput>> =
        const { std::cell::RefCell::new(Vec::new()) };
    pub(in crate::ai_gateway) static TEMPLATE_SYNC_MESSAGE_INPUTS:
        std::cell::RefCell<Vec<crate::messages::MessageCreateInput>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Terminal tools that AI Gateway is allowed to write to.
pub(in crate::ai_gateway) const SUPPORTED_TERMINAL_TOOLS: [&str; 3] =
    ["opencode", "codex", "claude"];

/// Stable marker key identifying a provider record written by AI Gateway. The
/// accepted marker value is owned by the [`TerminalSyncProfile`].
const GATEWAY_MARKER_KEY: &str = "ai_gateway_gateway";

/// Identity profile of the AI Gateway terminal-sync records.
///
/// The release profile owns the `gateway` provider key, the `AI Gateway` name
/// and the boolean `true` marker. The dev profile (a debug build outside the
/// test harness) owns the distinct `gateway-dev` key, the `AI Gateway (Dev)`
/// name and the `"dev"` string marker, so a development build never claims,
/// reuses or overwrites the release records and vice versa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ai_gateway) struct TerminalSyncProfile {
    is_dev: bool,
}

impl TerminalSyncProfile {
    // Test-only fixtures for the release/dev identity pair; production code
    // obtains its identity through `current()`.
    #[allow(dead_code)]
    pub(in crate::ai_gateway) const RELEASE: Self = Self { is_dev: false };
    #[allow(dead_code)]
    pub(in crate::ai_gateway) const DEV: Self = Self { is_dev: true };

    /// The profile of the running build: dev only for a debug build outside the
    /// test harness, release everywhere else.
    pub(in crate::ai_gateway) const fn current() -> Self {
        Self {
            is_dev: cfg!(all(debug_assertions, not(test))),
        }
    }

    pub(in crate::ai_gateway) const fn is_dev(self) -> bool {
        self.is_dev
    }

    /// Opencode provider key owned by this profile.
    pub(in crate::ai_gateway) const fn provider_key(self) -> &'static str {
        if self.is_dev() {
            "gateway-dev"
        } else {
            "gateway"
        }
    }

    /// Display name owned by this profile.
    pub(in crate::ai_gateway) const fn provider_name(self) -> &'static str {
        if self.is_dev() {
            "AI Gateway (Dev)"
        } else {
            "AI Gateway"
        }
    }

    /// The marker value this profile writes under `tool_config`.
    fn marker(self) -> Value {
        if self.is_dev() {
            Value::String("dev".to_string())
        } else {
            Value::Bool(true)
        }
    }

    /// Whether `provider` claims this profile's marker, at the top level or
    /// under `tool_config`. Release additionally accepts the legacy rename
    /// marker; dev claims nothing else.
    fn claims_marker(self, provider: &Value) -> bool {
        if self.is_dev() {
            marker_is(provider, |value| value.as_str() == Some("dev"))
        } else {
            marker_is(provider, |value| value.as_bool() == Some(true))
                || super::migration::has_legacy_gateway_marker(provider)
        }
    }
}

/// Whether `provider` carries a marker value accepted by `matches`, either at
/// the top level or under `tool_config`.
fn marker_is(provider: &Value, matches: fn(&Value) -> bool) -> bool {
    provider.get(GATEWAY_MARKER_KEY).map(matches).unwrap_or(false)
        || provider
            .get("tool_config")
            .and_then(|tool_config| tool_config.get(GATEWAY_MARKER_KEY))
            .map(matches)
            .unwrap_or(false)
}
/// npm override for an opencode model the gateway can serve only through the
/// Responses protocol: `@ai-sdk/openai` POSTs `/responses` instead of the
/// `/chat/completions` used by `@ai-sdk/openai-compatible`.
const OPENCODE_RESPONSES_NPM: &str = "@ai-sdk/openai";

/// A terminal service provider record that AI Gateway can configure or sync.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalTarget {
    pub tool: String,
    pub name: String,
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    pub synced: bool,
    pub pending_sync: bool,
    #[serde(default)]
    pub synced_key_id: Option<String>,
    #[serde(default)]
    pub synced_at: Option<u64>,
    /// Model-only Claude selection snapshot (the four local mapping ids) for the
    /// `claude` target; absent for the OpenAI tools. It never carries a
    /// credential value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_models: Option<ClaudeModelSelection>,
}

pub(in crate::ai_gateway) fn is_supported_terminal_tool(tool: &str) -> bool {
    SUPPORTED_TERMINAL_TOOLS
        .iter()
        .any(|supported| supported.eq_ignore_ascii_case(tool.trim()))
}

fn provider_tool(provider: &Value) -> &str {
    provider
        .get("tool")
        .and_then(Value::as_str)
        .unwrap_or("")
}

/// Whether `provider` carries the marker claimed by `profile`, either at the
/// top level or under `tool_config`; both shapes are recognized. Release records
/// written before the ai_gateway rename still carry the legacy marker and stay
/// recognized through [`super::migration::has_legacy_gateway_marker`], so the
/// next sync upgrades them in place. Dev claims only its own `"dev"` string
/// marker and never the release (or legacy) marker.
fn provider_has_gateway_marker(provider: &Value, profile: TerminalSyncProfile) -> bool {
    profile.claims_marker(provider)
}

fn non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Whether any enabled gateway can serve `local_model` under `protocol`,
/// resolved through the same authoritative model resolution the request path
/// uses. Disabled gateways never count.
fn gateway_protocol_servable(
    gateways: &[GatewayUpstreamProvider],
    local_model: &str,
    protocol: UpstreamProtocol,
) -> bool {
    gateways.iter().filter(|gateway| gateway.enabled).any(|gateway| {
        matches!(
            resolve_model_for_protocol(gateway, Some(local_model), protocol),
            ModelResolution::Serve(_)
        )
    })
}

/// Build the terminal provider record written by AI Gateway for one tool.
///
/// The record is always marked with `profile.marker()` under `tool_config`,
/// carries the resolved default local key value as its `api_key` (top-level and,
/// for opencode, `tool_config.options.apiKey`), and never carries an
/// `active`/`is_active` flag. The `provider_key` and display name come from
/// `profile`, so the release and dev profiles register distinct records.
/// Opencode activation plus projection to opencode.json are applied separately
/// via the service-provider active list and projection after the upsert
/// succeeds.
pub(in crate::ai_gateway) fn build_gateway_provider(
    provider_id: &str,
    tool: &str,
    base_url: &str,
    api_key: &str,
    gateways: &[GatewayUpstreamProvider],
    profile: TerminalSyncProfile,
) -> Result<serde_json::Value, String> {
    build_gateway_provider_with_pools(
        provider_id,
        tool,
        base_url,
        api_key,
        gateways,
        &[],
        None,
        profile,
    )
}

/// Pool-aware variant of [`build_gateway_provider`].
///
/// `pools` contributes the public alias of every enabled configured pool to the
/// generated catalog: an OpenCode Chat-compatible row with no guessed reasoning
/// variants, and a Codex generated model that only fills a gap. `previous_model`
/// is the model already stored on the owned record, used only to retain a valid
/// previously selected enabled pool alias on a Codex resync. The original
/// `build_gateway_provider` keeps its signature and passes no pools.
#[allow(clippy::too_many_arguments)]
fn build_gateway_provider_with_pools(
    provider_id: &str,
    tool: &str,
    base_url: &str,
    api_key: &str,
    gateways: &[GatewayUpstreamProvider],
    pools: &[ModelPool],
    previous_model: Option<&str>,
    profile: TerminalSyncProfile,
) -> Result<serde_json::Value, String> {
    let tool = tool.trim().to_ascii_lowercase();
    if !is_supported_terminal_tool(&tool) {
        return Err(format!(
            "unsupported terminal tool '{tool}': only opencode and codex are supported"
        ));
    }

    let mut tool_config = Map::new();
    tool_config.insert(GATEWAY_MARKER_KEY.to_string(), profile.marker());

    let mut object = Map::new();
    object.insert("id".to_string(), Value::String(provider_id.to_string()));
    object.insert(
        "name".to_string(),
        Value::String(profile.provider_name().to_string()),
    );
    object.insert("tool".to_string(), Value::String(tool.clone()));
    object.insert("base_url".to_string(), Value::String(base_url.to_string()));
    object.insert("api_key".to_string(), Value::String(api_key.to_string()));

    if tool == "opencode" {
        object.insert(
            "provider_key".to_string(),
            Value::String(profile.provider_key().to_string()),
        );
        tool_config.insert(
            "npm".to_string(),
            Value::String("@ai-sdk/openai-compatible".to_string()),
        );
        let mut options = Map::new();
        options.insert("baseURL".to_string(), Value::String(base_url.to_string()));
        options.insert("apiKey".to_string(), Value::String(api_key.to_string()));
        tool_config.insert("options".to_string(), Value::Object(options));

        let mut models = Map::new();
        // A gateway contributes rows only while it is user-enabled, and a row
        // only while it is user-enabled and not auto-disabled: the provider-level
        // legacy `auto_disabled` field never filters.
        for gateway in gateways.iter().filter(|gateway| gateway.enabled) {
            for mapping in gateway
                .mappings
                .iter()
                .filter(|mapping| mapping.enabled && !mapping.auto_disabled)
            {
                let Some(local_model) = non_empty(Some(mapping.local_model.as_str())) else {
                    continue;
                };
                if models.contains_key(&local_model) {
                    continue;
                }
                // Per-local-model servability across every enabled gateway: a
                // model the gateway can serve over Chat or Anthropic Messages
                // keeps opencode's existing Chat adapter (the Chat entry
                // converts for an Anthropic upstream), so only a genuinely
                // Responses-only model gets the `@ai-sdk/openai` npm override.
                let chat_servable = gateway_protocol_servable(
                    gateways,
                    &local_model,
                    UpstreamProtocol::ChatCompletions,
                );
                let responses_servable =
                    gateway_protocol_servable(gateways, &local_model, UpstreamProtocol::Responses);
                let anthropic_servable = gateway_protocol_servable(
                    gateways,
                    &local_model,
                    UpstreamProtocol::AnthropicMessages,
                );
                let name = non_empty(mapping.display_name.as_deref())
                    .or_else(|| non_empty(Some(mapping.upstream_model.as_str())))
                    .unwrap_or_default();
                let mut model = Map::new();
                model.insert("name".to_string(), Value::String(name));

                // OpenCode exposes a reasoning model's selectable strengths as
                // model variants. Carry the mapping's configured levels so the
                // tool offers exactly the gateway's tiers instead of falling
                // back to its own catalog/built-in guess for the model id.
                let mut variants = Map::new();
                for effort in &mapping.reasoning_efforts {
                    let Some(effort) = non_empty(Some(effort.as_str())) else {
                        continue;
                    };
                    if variants.contains_key(&effort) {
                        continue;
                    }
                    let mut variant = Map::new();
                    variant.insert("reasoningEffort".to_string(), Value::String(effort.clone()));
                    variants.insert(effort, Value::Object(variant));
                }
                if !variants.is_empty() {
                    model.insert("reasoning".to_string(), Value::Bool(true));
                    model.insert("variants".to_string(), Value::Object(variants));
                }

                if !chat_servable && !anthropic_servable && responses_servable {
                    let mut provider = Map::new();
                    provider.insert(
                        "npm".to_string(),
                        Value::String(OPENCODE_RESPONSES_NPM.to_string()),
                    );
                    model.insert("provider".to_string(), Value::Object(provider));
                }

                models.insert(local_model, Value::Object(model));
            }
        }
        // Every enabled configured pool contributes its public alias exactly
        // once as a Chat-compatible row. A pool never borrows a source model's
        // reasoning variants and is never given the Responses npm override, so
        // no `variants`/`reasoning`/`provider` is guessed from its candidates
        // (REQ-008/AC-011). A disabled pool is never exposed.
        for pool in pools.iter().filter(|pool| pool.enabled) {
            let Some(local_model) = non_empty(Some(pool.model.as_str())) else {
                continue;
            };
            if models.contains_key(&local_model) {
                continue;
            }
            let name = non_empty(Some(pool.name.as_str())).unwrap_or_else(|| local_model.clone());
            let mut model = Map::new();
            model.insert("name".to_string(), Value::String(name));
            models.insert(local_model, Value::Object(model));
        }
        tool_config.insert("models".to_string(), Value::Object(models));
    } else {
        tool_config.insert("wire_api".to_string(), Value::String("chat".to_string()));
        let ordinary = gateways
            .iter()
            .filter(|gateway| gateway.enabled)
            .find_map(|gateway| {
                gateway
                    .mappings
                    .iter()
                    .filter(|mapping| mapping.enabled && !mapping.auto_disabled)
                    .find_map(|mapping| non_empty(Some(mapping.local_model.as_str())))
            })
            .or_else(|| {
                gateways
                    .iter()
                    .filter(|gateway| gateway.enabled)
                    .find_map(|gateway| non_empty(gateway.default_model.as_deref()))
            });
        // Retain an already selected *enabled* pool alias on resync; a disabled
        // or retired alias is not retainable, so an ordinary default wins.
        let retained_pool = previous_model
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .filter(|value| {
                pools
                    .iter()
                    .any(|pool| pool.enabled && pool.model.trim() == *value)
            })
            .map(str::to_string);
        // A pool-only configuration (no ordinary model) uses the first enabled
        // pool alias as its generated fallback; an ordinary default is never
        // replaced just because a pool exists (REQ-008/AC-011).
        let fallback_pool = pools
            .iter()
            .filter(|pool| pool.enabled)
            .find_map(|pool| non_empty(Some(pool.model.as_str())));
        let model = retained_pool.or(ordinary).or(fallback_pool);
        if let Some(model) = model {
            object.insert("model".to_string(), Value::String(model));
        }
    }

    object.insert("tool_config".to_string(), Value::Object(tool_config));
    Ok(Value::Object(object))
}

/// Read the four persisted Claude model choices back from a canonical,
/// marker-owned Claude service provider as it appears in the terminal service
/// provider list. A missing mirrored default or any missing/blank family row
/// yields `None`, so an incomplete selection is never resent as a partial one.
fn claude_selection_from_provider(provider: &Value) -> Option<ClaudeModelSelection> {
    let default_model = provider
        .get("claude_default_model")
        .and_then(Value::as_str)
        .or_else(|| provider.get("model").and_then(Value::as_str))?
        .trim();
    if default_model.is_empty() {
        return None;
    }
    let mappings = provider.get("claude_model_mappings")?.as_array()?;
    let family = |name: &str| {
        mappings
            .iter()
            .find(|row| row.get("family").and_then(Value::as_str) == Some(name))
            .and_then(|row| row.get("upstream_model").and_then(Value::as_str))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    Some(ClaudeModelSelection {
        default_model: default_model.to_string(),
        haiku: family("haiku")?,
        sonnet: family("sonnet")?,
        opus: family("opus")?,
    })
}

/// A collision-safe Claude profile code derived from the new provider UUID.
/// The owned canonical record's code is reused verbatim on later syncs; this
/// fallback only runs when no owned record (and therefore no code) exists yet.
fn derived_claude_code(provider_id: &str) -> String {
    let mut suffix: String = provider_id
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .map(|character| character.to_ascii_lowercase())
        .collect();
    if suffix.is_empty() {
        suffix = uuid::Uuid::new_v4().simple().to_string();
    }
    format!("gw-claude-{}", &suffix[..suffix.len().min(24)])
}

/// Build the gateway-owned Claude service-provider payload for one sync.
///
/// It carries the tool-specific origin (never a trailing `/v1`), the resolved
/// default local key, the default and three family local mapping ids, a
/// UUID-derived or reused `code`, and the profile marker under `tool_config`.
/// It never carries an `active`/`is_active` flag; activation and projection are
/// applied separately after the upsert succeeds.
fn build_claude_gateway_provider(
    provider_id: &str,
    origin: &str,
    api_key: &str,
    selection: &ClaudeModelSelection,
    code: &str,
    profile: TerminalSyncProfile,
) -> Value {
    let mapping = |family: &str, display_name: &str, upstream_model: &str| {
        json!({
            "family": family,
            "display_name": display_name,
            "upstream_model": upstream_model,
        })
    };
    let mut tool_config = Map::new();
    tool_config.insert(GATEWAY_MARKER_KEY.to_string(), profile.marker());

    let mut object = Map::new();
    object.insert("id".to_string(), Value::String(provider_id.to_string()));
    object.insert(
        "name".to_string(),
        Value::String(profile.provider_name().to_string()),
    );
    object.insert("tool".to_string(), Value::String("claude".to_string()));
    object.insert("base_url".to_string(), Value::String(origin.to_string()));
    object.insert("api_key".to_string(), Value::String(api_key.to_string()));
    object.insert(
        "model".to_string(),
        Value::String(selection.default_model.clone()),
    );
    object.insert("code".to_string(), Value::String(code.to_string()));
    object.insert(
        "claude_api_format".to_string(),
        Value::String("anthropic_messages".to_string()),
    );
    object.insert(
        "claude_connection_mode".to_string(),
        Value::String("native_anthropic".to_string()),
    );
    object.insert(
        "claude_auth_env_key".to_string(),
        Value::String("ANTHROPIC_API_KEY".to_string()),
    );
    object.insert(
        "claude_model_mappings".to_string(),
        Value::Array(vec![
            mapping("haiku", "Haiku", &selection.haiku),
            mapping("sonnet", "Sonnet", &selection.sonnet),
            mapping("opus", "Opus", &selection.opus),
        ]),
    );
    object.insert("tool_config".to_string(), Value::Object(tool_config));
    Value::Object(object)
}

/// Validate an explicitly submitted Claude selection: every value must be a
/// non-blank, enabled, native Anthropic-Messages-servable local mapping id (an
/// Anthropic mapping row or an Anthropic-protocol provider default). The same
/// id may fill several slots.
fn validate_claude_selection(
    config: &GatewayConfig,
    selection: &ClaudeModelSelection,
) -> Result<(), String> {
    for (label, id) in [
        ("default", &selection.default_model),
        ("haiku", &selection.haiku),
        ("sonnet", &selection.sonnet),
        ("opus", &selection.opus),
    ] {
        let id = id.trim();
        if id.is_empty() {
            return Err(format!("Claude {label} model must not be blank"));
        }
        if !gateway_protocol_servable(&config.providers, id, UpstreamProtocol::AnthropicMessages) {
            return Err(format!(
                "Claude {label} model '{id}' is not an enabled native Anthropic Messages mapping"
            ));
        }
    }
    Ok(())
}

/// Pending-sync is derived from the persisted ledger, never from the redacted api_key.
pub(in crate::ai_gateway) fn terminal_sync_pending(
    record: &TerminalSyncRecord,
    current_key_id: Option<&str>,
    current_base_url: &str,
) -> bool {
    match current_key_id {
        Some(key_id) => {
            record.synced_key_id != key_id || record.synced_base_url != current_base_url
        }
        None => true,
    }
}

/// Whether a fresh terminal sync would write a different model selection than
/// the stored gateway record. Key material is never part of the comparison, so
/// callers may build the fresh payload with a placeholder key.
fn terminal_model_selection_drifted(tool: &str, stored: &Value, generated: &Value) -> bool {
    match tool {
        "opencode" => {
            stored.pointer("/tool_config/models") != generated.pointer("/tool_config/models")
        }
        "codex" => stored.get("model") != generated.get("model"),
        _ => true,
    }
}

pub(in crate::ai_gateway) fn default_key_for_sync(
    config: &GatewayConfig,
) -> Result<(String, String), String> {
    // Mirror the frontend `resolveDefaultKeyId` rule: the stored choice wins
    // while it points at an enabled key, otherwise fall through to the next
    // enabled key in list order (wrapping), so syncing still carries the
    // default API key the UI shows. A `None` result means no enabled key exists.
    let resolved = resolve_default_key_id(&config.keys, config.default_key_id.as_deref());
    match resolved.and_then(|id| {
        config
            .keys
            .iter()
            .find(|key| key.id == id && key.enabled)
    }) {
        Some(key) => Ok((key.id.clone(), key.value.clone())),
        None => Err(
            "no enabled local API key: add and enable a local key before configuring terminals"
                .to_string(),
        ),
    }
}

fn api_err_to_string(error: crate::app_store::ApiErr) -> String {
    format!("{}: {}", error.code, error.message)
}

/// Validate and normalize a brand-new provider's pool: every key gets a stable
/// id when missing, every key must carry a value, and every key starts healthy.
fn prepare_new_provider_keys(provider: &mut GatewayUpstreamProvider) -> Result<(), String> {
    for key in &mut provider.keys {
        if key.id.trim().is_empty() {
            key.id = new_key_id();
        }
        if key.value.trim().is_empty() {
            return Err(format!(
                "a new upstream key '{}' requires a value",
                key.name.trim()
            ));
        }
        clear_key_runtime_state(key);
    }
    Ok(())
}

/// Merge an incoming pool into the stored one by key id. A blank value keeps
/// the stored value and runtime state, an unchanged value preserves the stored
/// runtime state, a changed value clears it (the user revalued the key), and an
/// unknown id starts a healthy new key. Deleting is by omission: a stored id
/// absent from the incoming list is dropped with its runtime state.
fn merge_provider_keys(
    existing: &GatewayUpstreamProvider,
    provider: &mut GatewayUpstreamProvider,
) -> Result<(), String> {
    for key in &mut provider.keys {
        let stored = if key.id.trim().is_empty() {
            None
        } else {
            existing.keys.iter().find(|stored| stored.id == key.id)
        };
        match stored {
            None => {
                if key.id.trim().is_empty() {
                    key.id = new_key_id();
                }
                if key.value.trim().is_empty() {
                    return Err(format!(
                        "a new upstream key '{}' requires a value",
                        key.name.trim()
                    ));
                }
                clear_key_runtime_state(key);
            }
            Some(stored) => {
                if key.value.trim().is_empty() || key.value == stored.value {
                    key.value = stored.value.clone();
                    key.auto_marked = stored.auto_marked;
                    key.failure_kind = stored.failure_kind;
                    key.marked_at = stored.marked_at;
                    key.reason = stored.reason.clone();
                } else {
                    clear_key_runtime_state(key);
                }
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub fn ai_gateway_get_config() -> Result<GatewayConfig, String> {
    read_config()
}

#[tauri::command]
pub fn ai_gateway_upsert_provider(
    mut provider: GatewayUpstreamProvider,
    prices: Option<Vec<ModelPrice>>,
) -> Result<GatewayConfig, String> {
    modify_config(|config| {
        if provider.weight < MIN_PROVIDER_WEIGHT || provider.weight > MAX_PROVIDER_WEIGHT {
            return Err(format!(
                "provider weight must be between {MIN_PROVIDER_WEIGHT} and {MAX_PROVIDER_WEIGHT}"
            ));
        }
        // A submitted template binding must resolve against a known, non-deleted
        // template before anything is staged; `None` (or a missing field)
        // unbinds. An unknown or deleted id is rejected without a write, so a
        // broken binding can never be persisted through the upsert.
        if let Some(template_id) = provider.template_id.as_deref() {
            let template_id = template_id.trim();
            if template_id.is_empty() {
                return Err("provider template id must not be blank".to_string());
            }
            effective_template(config, template_id)?;
        }
        // A key name is required and may repeat; reject before anything is staged.
        for key in &provider.keys {
            if key.name.trim().is_empty() {
                return Err("an upstream key requires a non-blank name".to_string());
            }
        }
        if provider.id.trim().is_empty() {
            provider.id = new_provider_id();
        }
        let provider_id = provider.id.clone();
        // An ordinary provider mapping must not acquire a current or retired
        // model-pool alias (REQ-001/REQ-007). Retaining an already-stored row is
        // allowed so resubmitting an unchanged provider never fails.
        let pool_aliases = reserved_aliases(config);
        if !pool_aliases.is_empty() {
            let stored_mappings: Vec<String> = config
                .providers
                .iter()
                .find(|stored| stored.id == provider_id)
                .map(|stored| {
                    stored
                        .mappings
                        .iter()
                        .map(|mapping| mapping.local_model.trim().to_string())
                        .collect()
                })
                .unwrap_or_default();
            for mapping in &provider.mappings {
                let local = mapping.local_model.trim();
                if local.is_empty() {
                    continue;
                }
                if pool_aliases.contains(local) && !stored_mappings.iter().any(|stored| stored == local)
                {
                    return Err(format!(
                        "mapping local model '{local}' is reserved by a model pool"
                    ));
                }
            }
        }
        if let Some(existing) = find_provider_mut(config, &provider_id) {
            merge_provider_keys(existing, &mut provider)?;
            // Runtime health belongs to a trimmed `(local_model, upstream_model)`
            // key: an unchanged key keeps the stored state, a new or changed key
            // starts healthy, and a deleted row drops its state with the replacement.
            for mapping in &mut provider.mappings {
                match existing.mappings.iter().find(|stored| {
                    mapping_matches_key(stored, &mapping.local_model, &mapping.upstream_model)
                }) {
                    Some(stored) => {
                        mapping.auto_disabled = stored.auto_disabled;
                        mapping.disabled_reason = stored.disabled_reason.clone();
                        mapping.disabled_at = stored.disabled_at;
                        mapping.consecutive_failures = stored.consecutive_failures;
                        mapping.last_error_at = stored.last_error_at;
                    }
                    None => clear_mapping_runtime_state(mapping),
                }
            }
            *existing = provider;
        } else {
            // A brand-new provider has no stored keys or rows, so every key and row
            // starts healthy; a new key still needs a value.
            prepare_new_provider_keys(&mut provider)?;
            for mapping in &mut provider.mappings {
                clear_mapping_runtime_state(mapping);
            }
            config.providers.push(provider);
        }
        if let Some(prices) = prices {
            let mut normalized: Vec<ModelPrice> = Vec::with_capacity(prices.len());
            for mut row in prices {
                let upstream_model = row.upstream_model.trim().to_string();
                if upstream_model.is_empty() {
                    continue;
                }
                if normalized
                    .iter()
                    .any(|existing| existing.upstream_model == upstream_model)
                {
                    continue;
                }
                row.upstream_model = upstream_model;
                row.provider_id = Some(provider_id.clone());
                normalized.push(row);
            }
            config
                .model_prices
                .retain(|row| row.provider_id.as_deref() != Some(provider_id.as_str()));
            config.model_prices.extend(normalized);
        }
        Ok((true, ()))
    })?;
    read_config()
}

#[tauri::command]
pub fn ai_gateway_delete_provider(provider_id: String) -> Result<GatewayConfig, String> {
    modify_config(|config| {
        config.providers.retain(|provider| provider.id != provider_id);
        config
            .model_prices
            .retain(|row| row.provider_id.as_deref() != Some(provider_id.as_str()));
        config
            .terminal_syncs
            .retain(|record| record.provider_id != provider_id);
        Ok((true, ()))
    })?;
    read_config()
}

#[tauri::command]
pub fn ai_gateway_set_provider_enabled(
    provider_id: String,
    enabled: bool,
) -> Result<GatewayConfig, String> {
    modify_config(|config| {
        let provider = find_provider_mut(config, &provider_id)
            .ok_or_else(|| format!("provider not found: {provider_id}"))?;
        set_user_enabled(provider, enabled);
        Ok((true, ()))
    })?;
    read_config()
}

/// Create or edit one model pool (REQ-001).
///
/// The submitted pool is validated against the latest configuration and written
/// through the same serialized atomic primitive as every other gateway write, so
/// a validation or write failure changes nothing. Renaming a pool reserves its
/// previous alias, so neither an ordinary mapping nor a retired route can
/// acquire it; a new pool may reuse a retired alias.
#[tauri::command]
pub fn ai_gateway_upsert_model_pool(mut pool: ModelPool) -> Result<GatewayConfig, String> {
    modify_config(|config| {
        if pool.id.trim().is_empty() {
            pool.id = new_pool_id();
        }
        let normalized = validate_pool_upsert(config, &pool)?;
        if let Some(existing) = config
            .model_pools
            .iter()
            .find(|stored| stored.id == normalized.id)
        {
            let previous = existing.model.trim().to_string();
            if previous != normalized.model {
                reserve_alias(config, &previous);
            }
        }
        if let Some(existing) = config
            .model_pools
            .iter_mut()
            .find(|stored| stored.id == normalized.id)
        {
            *existing = normalized;
        } else {
            config.model_pools.push(normalized);
        }
        Ok((true, ()))
    })?;
    read_config()
}

/// Delete one model pool and reserve its alias so a stale client can never fall
/// through to an unrelated ordinary source. An unknown id is an actionable error
/// that writes nothing.
#[tauri::command]
pub fn ai_gateway_delete_model_pool(pool_id: String) -> Result<GatewayConfig, String> {
    modify_config(|config| {
        let removed: Vec<String> = config
            .model_pools
            .iter()
            .filter(|pool| pool.id == pool_id)
            .map(|pool| pool.model.trim().to_string())
            .collect();
        if removed.is_empty() {
            return Err(format!("model pool not found: {pool_id}"));
        }
        config.model_pools.retain(|pool| pool.id != pool_id);
        for alias in removed {
            reserve_alias(config, &alias);
        }
        Ok((true, ()))
    })?;
    read_config()
}

/// Toggle a pool's `enabled` intent without touching its candidates, so
/// readiness and user intent stay separate (REQ-001/REQ-008). An unknown id is an
/// actionable error that writes nothing.
#[tauri::command]
pub fn ai_gateway_set_model_pool_enabled(
    pool_id: String,
    enabled: bool,
) -> Result<GatewayConfig, String> {
    modify_config(|config| {
        let pool = config
            .model_pools
            .iter_mut()
            .find(|pool| pool.id == pool_id)
            .ok_or_else(|| format!("model pool not found: {pool_id}"))?;
        pool.enabled = enabled;
        Ok((true, ()))
    })?;
    read_config()
}

/// Nonsecret readiness of every stored pool and candidate (REQ-004 of the
/// frozen plan): source identity, availability code and cooldown deadline only.
/// No network/quota probe runs and nothing is persisted.
#[tauri::command]
pub fn ai_gateway_model_pools_status() -> Result<Vec<ModelPoolStatus>, String> {
    let config = read_config()?;
    Ok(pool_status(&config))
}

/// Clear the runtime health state of the mapping row identified by the trimmed
/// `(local_model, upstream_model)` key. Duplicate rows that share the trimmed
/// key are cleared together, and the user's `enabled` intent is never touched.
/// An unknown provider, or a key that matches no row (including a blank local or
/// upstream model), is an actionable error that writes nothing.
#[tauri::command]
pub fn ai_gateway_reenable_provider_model(
    provider_id: String,
    local_model: String,
    upstream_model: String,
) -> Result<GatewayConfig, String> {
    modify_config(|config| {
        let provider = find_provider_mut(config, &provider_id)
            .ok_or_else(|| format!("provider not found: {provider_id}"))?;
        let mut matched = false;
        for mapping in provider.mappings.iter_mut().filter(|mapping| {
            mapping_matches_key(mapping, &local_model, &upstream_model)
        }) {
            clear_mapping_runtime_state(mapping);
            matched = true;
        }
        if !matched {
            return Err(format!(
                "no mapping row matches '{local_model}' -> '{upstream_model}' for provider '{provider_id}'"
            ));
        }
        Ok((true, ()))
    })?;
    read_config()
}

/// Clear the runtime health state of every auto-disabled mapping row of one
/// provider. Each row keeps its `enabled` intent (a user-disabled row stays
/// disabled), rows that are not auto-disabled keep their counter, and other
/// providers are untouched. An unknown provider is an actionable error that
/// writes nothing.
#[tauri::command]
pub fn ai_gateway_reenable_provider_models(provider_id: String) -> Result<GatewayConfig, String> {
    modify_config(|config| {
        let provider = find_provider_mut(config, &provider_id)
            .ok_or_else(|| format!("provider not found: {provider_id}"))?;
        manual_reenable(provider);
        Ok((true, ()))
    })?;
    read_config()
}

/// Clear one upstream key's runtime state without changing its `enabled` flag,
/// so a manual re-enable restores an auth- or quota-marked key. An unknown
/// provider or key id is an actionable error that writes nothing.
#[tauri::command]
pub fn ai_gateway_reenable_provider_key(
    provider_id: String,
    key_id: String,
) -> Result<GatewayConfig, String> {
    modify_config(|config| {
        let provider = find_provider_mut(config, &provider_id)
            .ok_or_else(|| format!("provider not found: {provider_id}"))?;
        let key = provider
            .keys
            .iter_mut()
            .find(|key| key.id == key_id)
            .ok_or_else(|| {
                format!("no upstream key matches '{key_id}' for provider '{provider_id}'")
            })?;
        clear_key_runtime_state(key);
        Ok((true, ()))
    })?;
    read_config()
}

#[tauri::command]
pub fn ai_gateway_upsert_key(mut key: GatewayKey) -> Result<GatewayConfig, String> {
    modify_config(|config| {
        touch_key_created_at(&mut key);
        if key.id.trim().is_empty() {
            key.id = new_key_id();
        }
        if let Some(existing) = config.keys.iter_mut().find(|candidate| candidate.id == key.id) {
            if key.value.trim().is_empty() {
                key.value = existing.value.clone();
            }
            *existing = key;
        } else {
            if key.value.trim().is_empty() {
                key.value = new_key_value();
            }
            config.keys.push(key);
        }
        Ok((true, ()))
    })?;
    read_config()
}

#[tauri::command]
pub fn ai_gateway_delete_key(key_id: String) -> Result<GatewayConfig, String> {
    modify_config(|config| {
        config.keys.retain(|key| key.id != key_id);
        Ok((true, ()))
    })?;
    read_config()
}

#[tauri::command]
pub fn ai_gateway_set_default_key(key_id: String) -> Result<GatewayConfig, String> {
    modify_config(|config| {
        let enabled = config
            .keys
            .iter()
            .any(|key| key.id == key_id && key.enabled);
        if !enabled {
            return Err(format!(
                "cannot switch default key: '{key_id}' is not an enabled local key"
            ));
        }
        config.default_key_id = Some(key_id);
        Ok((true, ()))
    })?;
    read_config()
}

/// Persist the enable intent after a successful start/stop so a later autostart
/// restores the last state. Re-reads the config first so other fields (and any
/// concurrent edits) are preserved; the flag is only written once the listener
/// transition succeeded.
fn persist_enabled(enabled: bool) -> Result<(), String> {
    modify_config(|config| {
        if config.enabled != enabled {
            config.enabled = enabled;
            Ok((true, ()))
        } else {
            Ok((false, ()))
        }
    })
}

#[tauri::command]
pub async fn ai_gateway_start(app: tauri::AppHandle) -> Result<GatewayStatus, String> {
    start_inner(Some(app)).await
}

/// Start the listener and persist the enable intent, with the application
/// handle injected so the server can broadcast runtime-state transitions.
pub(in crate::ai_gateway) async fn start_inner(
    app: Option<tauri::AppHandle>,
) -> Result<GatewayStatus, String> {
    let status = start_server(app).await?;
    persist_enabled(true)?;
    Ok(status)
}

#[tauri::command]
pub async fn ai_gateway_stop() -> Result<GatewayStatus, String> {
    let status = stop_server().await?;
    persist_enabled(false)?;
    Ok(status)
}

#[tauri::command]
pub fn ai_gateway_status() -> Result<GatewayStatus, String> {
    server_status()
}

pub async fn ai_gateway_autostart(app: tauri::AppHandle) -> Result<GatewayStatus, String> {
    migrate_legacy_files();
    autostart(Some(app)).await
}

/// Find the managed gateway provider for a tool: the ledger record's provider id
/// when it still exists for the same tool AND still carries the profile's
/// gateway marker, else the profile-marked provider for the tool. A ledger id
/// that now points at a provider without the profile marker (a user provider or
/// the other profile's record) is stale and must never be claimed.
fn find_managed_gateway_provider<'a>(
    tool: &str,
    providers: &'a [Value],
    ledger: Option<&TerminalSyncRecord>,
    profile: TerminalSyncProfile,
) -> Option<&'a Value> {
    if let Some(record) = ledger {
        if record.tool.eq_ignore_ascii_case(tool) {
            let found = providers.iter().find(|provider| {
                provider.get("id").and_then(Value::as_str) == Some(record.provider_id.as_str())
                    && provider_tool(provider).eq_ignore_ascii_case(tool)
                    && provider_has_gateway_marker(provider, profile)
            });
            if found.is_some() {
                return found;
            }
        }
    }
    providers.iter().find(|provider| {
        provider_tool(provider).eq_ignore_ascii_case(tool)
            && provider_has_gateway_marker(provider, profile)
    })
}

/// Resolve the provider id a sync should write to for one tool: first the ledger
/// record that still matches a profile-marked same-tool provider, then a
/// profile-marked gateway provider, then a freshly generated id. A ledger id
/// pointing at a provider without the profile marker is stale and is skipped, so
/// a foreign-marked record (including the other profile's) is treated as absent.
fn resolve_gateway_provider_id(
    tool: &str,
    providers: &[Value],
    config: &GatewayConfig,
    profile: TerminalSyncProfile,
) -> String {
    for record in &config.terminal_syncs {
        if !record.tool.eq_ignore_ascii_case(tool) {
            continue;
        }
        let existing = providers.iter().find(|provider| {
            provider.get("id").and_then(Value::as_str) == Some(record.provider_id.as_str())
                && provider_tool(provider).eq_ignore_ascii_case(tool)
                && provider_has_gateway_marker(provider, profile)
        });
        if let Some(id) = existing.and_then(|provider| provider.get("id").and_then(Value::as_str)) {
            return id.to_string();
        }
    }
    if let Some(id) = providers
        .iter()
        .find(|provider| {
            provider_tool(provider).eq_ignore_ascii_case(tool)
                && provider_has_gateway_marker(provider, profile)
        })
        .and_then(|provider| provider.get("id").and_then(Value::as_str))
    {
        return id.to_string();
    }
    uuid::Uuid::new_v4().to_string()
}

/// Build the per-tool terminal target projection from the persisted config and
/// the current terminal service provider list. Managed gateways are recognized
/// through the profile's gateway marker only; a stale ledger id that points at a
/// provider without that marker (a user provider or the other profile's record)
/// is never claimed.
pub(in crate::ai_gateway) fn terminal_targets_from(
    config: &GatewayConfig,
    providers_data: &serde_json::Value,
    profile: TerminalSyncProfile,
) -> Vec<TerminalTarget> {
    let providers = providers_data
        .get("providers")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let mut targets = Vec::new();
    for tool in SUPPORTED_TERMINAL_TOOLS {
        let ledger = config
            .terminal_syncs
            .iter()
            .find(|record| record.tool.eq_ignore_ascii_case(tool));
        let managed = find_managed_gateway_provider(tool, providers, ledger, profile);
        let provider_id = managed
            .and_then(|provider| provider.get("id").and_then(Value::as_str))
            .map(str::to_string);
        let provider_base_url = managed
            .and_then(|provider| provider.get("base_url").and_then(Value::as_str))
            .map(str::to_string);
        let synced = provider_id.is_some();
        // Claude's native base URL is the bare origin; the OpenAI tools keep the
        // `/v1` loopback base URL.
        let current_base_url = if tool == "claude" {
            local_origin(config.port)
        } else {
            local_base_url(config.port)
        };
        // The Claude target exposes a model-only snapshot of the persisted
        // choices; an incomplete snapshot counts as drifted (pending) but stays
        // `synced` through the marker, so a manual sync can request replacement.
        let claude_models = if tool == "claude" {
            managed.and_then(claude_selection_from_provider)
        } else {
            None
        };
        let pending_sync = match (synced, ledger) {
            (true, Some(record)) => {
                let ledger_pending = terminal_sync_pending(
                    record,
                    config.default_key_id.as_deref(),
                    &current_base_url,
                );
                let model_drifted = if tool == "claude" {
                    claude_models.is_none()
                } else {
                    match (provider_id.as_deref(), managed) {
                        (Some(id), Some(stored)) => build_gateway_provider_with_pools(
                            id,
                            tool,
                            &current_base_url,
                            "",
                            &config.providers,
                            &config.model_pools,
                            stored.get("model").and_then(Value::as_str),
                            profile,
                        )
                        .map(|generated| terminal_model_selection_drifted(tool, stored, &generated))
                        .unwrap_or(true),
                        _ => true,
                    }
                };
                ledger_pending || model_drifted
            }
            _ => true,
        };
        targets.push(TerminalTarget {
            tool: tool.to_string(),
            name: match tool {
                "opencode" => "OpenCode".to_string(),
                "codex" => "Codex".to_string(),
                _ => "Claude".to_string(),
            },
            provider_id,
            base_url: provider_base_url,
            synced,
            pending_sync,
            synced_key_id: ledger.map(|record| record.synced_key_id.clone()),
            synced_at: ledger.map(|record| record.synced_at),
            claude_models,
        });
    }
    targets
}

/// The supported terminal tools whose managed AI Gateway provider record for
/// `profile` already exists, in `SUPPORTED_TERMINAL_TOOLS` order. Reuses the same
/// `synced` predicate the terminal target list exposes, so a null or unreadable
/// payload yields no tools and the other profile's records are ignored. A marked
/// Claude record participates like the other tools; when its model selection is
/// unavailable the refresh performs no Claude write (the pipeline rejects the
/// missing selection before any write) and the template sync result still
/// stands.
pub(in crate::ai_gateway) fn previously_synced_terminal_tools(
    config: &GatewayConfig,
    providers_data: &serde_json::Value,
    profile: TerminalSyncProfile,
) -> Vec<String> {
    terminal_targets_from(config, providers_data, profile)
        .into_iter()
        .filter(|target| target.synced)
        .map(|target| target.tool)
        .collect()
}

#[tauri::command]
pub fn ai_gateway_terminal_targets() -> Result<Vec<TerminalTarget>, String> {
    let config = read_config()?;
    let payload = crate::app_store::service_providers_list().map_err(api_err_to_string)?;
    Ok(terminal_targets_from(
        &config,
        &payload.data,
        TerminalSyncProfile::current(),
    ))
}

/// Boxed future returned by an injected terminal upsert, kept `Send` so the
/// pipeline can run on the Tauri async runtime.
pub(in crate::ai_gateway) type UpsertFuture =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>>;

/// Terminal sync pipeline with an injectable upsert seam: build one gateway
/// provider per requested tool for `profile`, upsert each one (carrying the
/// resolved default local key value as its `api_key`), then refresh the ledger.
/// Any upsert error aborts before the ledger is written, so the ledger never
/// claims a sync that did not happen. The payload itself never carries an
/// `active`/`is_active` flag; activation plus projection are applied separately
/// in `apply_terminal_sync` after the ledger is persisted.
///
/// This 4-argument entry point reads any Claude selection back from the owned
/// canonical provider; the command layer uses
/// [`apply_terminal_sync_with_selection`] to also accept an explicit operator
/// selection for a first sync.
#[cfg(test)]
pub(in crate::ai_gateway) async fn apply_terminal_sync_with<F>(
    providers_data: &serde_json::Value,
    upsert: F,
    target_tools: Vec<String>,
    profile: TerminalSyncProfile,
) -> Result<Vec<TerminalSyncRecord>, String>
where
    F: FnMut(serde_json::Value) -> UpsertFuture,
{
    apply_terminal_sync_with_selection(providers_data, upsert, target_tools, profile, None).await
}

/// Terminal sync pipeline with an injected upsert and an optional explicit
/// Claude model selection.
///
/// Every payload is built (and every Claude selection validated) before the
/// first upsert, so an invalid or missing Claude selection is rejected before
/// any Claude provider or ledger write. A `claude` target without an explicit
/// selection reads the four saved choices back from the owned canonical Claude
/// provider and resends them whole, so the existing replace-style upsert cannot
/// clear omitted choices.
pub(in crate::ai_gateway) async fn apply_terminal_sync_with_selection<F>(
    providers_data: &serde_json::Value,
    mut upsert: F,
    target_tools: Vec<String>,
    profile: TerminalSyncProfile,
    claude_models: Option<ClaudeModelSelection>,
) -> Result<Vec<TerminalSyncRecord>, String>
where
    F: FnMut(serde_json::Value) -> UpsertFuture,
{
    let config = read_config()?;
    let (key_id, key_value) = default_key_for_sync(&config)?;
    if target_tools.is_empty() {
        return Err("no terminal targets selected".to_string());
    }

    let mut tools: Vec<String> = Vec::new();
    for tool in target_tools {
        let tool = tool.trim().to_string();
        if tools
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(&tool))
        {
            continue;
        }
        tools.push(tool);
    }
    for tool in &tools {
        if !is_supported_terminal_tool(tool) {
            return Err(format!(
                "unsupported terminal tool '{tool}': only opencode, codex and claude are supported"
            ));
        }
    }

    let base_url = local_base_url(config.port);
    let origin = local_origin(config.port);
    let providers: &[Value] = providers_data
        .get("providers")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);

    // Build every payload first so a rejected Claude selection never leaves a
    // partially upserted batch behind.
    struct PlannedTool {
        tool: String,
        provider_id: String,
        base_url: String,
        payload: Value,
    }
    let mut planned: Vec<PlannedTool> = Vec::new();
    for tool in &tools {
        if tool == "claude" {
            let ledger = config
                .terminal_syncs
                .iter()
                .find(|record| record.tool.eq_ignore_ascii_case("claude"));
            let managed = find_managed_gateway_provider("claude", providers, ledger, profile);
            let selection = match claude_models.as_ref() {
                Some(explicit) => {
                    let normalized = explicit.normalized().ok_or_else(|| {
                        "a Claude sync requires all four model choices to be non-blank".to_string()
                    })?;
                    validate_claude_selection(&config, &normalized)?;
                    normalized
                }
                None => {
                    // The read-back path validates exactly like an explicit
                    // submission. A multi-tool best-effort fan-out (the
                    // post-template refresh) requests Claude alongside other
                    // tools: an unavailable or unservable saved selection skips
                    // only Claude so the rest of the batch still syncs, while a
                    // Claude-only sync (manual) surfaces the rejection and
                    // writes nothing.
                    let fan_out = tools.len() > 1;
                    match managed.and_then(claude_selection_from_provider) {
                        Some(saved) => {
                            if let Err(error) = validate_claude_selection(&config, &saved) {
                                if fan_out {
                                    continue;
                                }
                                return Err(error);
                            }
                            saved
                        }
                        None => {
                            if fan_out {
                                continue;
                            }
                            return Err(
                                "a Claude sync requires a saved or submitted model selection; open the Claude model dialog first"
                                    .to_string(),
                            );
                        }
                    }
                }
            };
            let provider_id = managed
                .and_then(|provider| provider.get("id").and_then(Value::as_str))
                .map(str::to_string)
                .unwrap_or_else(|| {
                    resolve_gateway_provider_id("claude", providers, &config, profile)
                });
            let code = managed
                .and_then(|provider| provider.get("code").and_then(Value::as_str))
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| derived_claude_code(&provider_id));
            let payload = build_claude_gateway_provider(
                &provider_id,
                &origin,
                &key_value,
                &selection,
                &code,
                profile,
            );
            planned.push(PlannedTool {
                tool: tool.clone(),
                provider_id,
                base_url: origin.clone(),
                payload,
            });
        } else {
            let provider_id = resolve_gateway_provider_id(tool, providers, &config, profile);
            // The owned record's current model lets a Codex resync retain an
            // already selected enabled pool alias instead of dropping it.
            let managed_ledger = config
                .terminal_syncs
                .iter()
                .find(|record| record.tool.eq_ignore_ascii_case(tool));
            let previous_model =
                find_managed_gateway_provider(tool, providers, managed_ledger, profile)
                    .and_then(|provider| provider.get("model").and_then(Value::as_str));
            let payload = build_gateway_provider_with_pools(
                &provider_id,
                tool,
                &base_url,
                &key_value,
                &config.providers,
                &config.model_pools,
                previous_model,
                profile,
            )?;
            planned.push(PlannedTool {
                tool: tool.clone(),
                provider_id,
                base_url: base_url.clone(),
                payload,
            });
        }
    }

    let mut synced = Vec::new();
    for plan in planned {
        upsert(plan.payload).await?;
        let record = TerminalSyncRecord {
            provider_id: plan.provider_id,
            tool: plan.tool,
            synced_key_id: key_id.clone(),
            synced_base_url: plan.base_url,
            synced_at: now_ts(),
        };
        synced.push(record);
    }
    // The upserts above are external network waits, so the ledger is persisted
    // afterwards as one serialized read-modify-write: the lock is never held
    // across an upsert, and a concurrent configuration write cannot be lost.
    modify_config(|latest| {
        for record in &synced {
            latest
                .terminal_syncs
                .retain(|existing| !existing.tool.eq_ignore_ascii_case(&record.tool));
            latest.terminal_syncs.push(record.clone());
        }
        Ok((true, ()))
    })?;
    Ok(synced)
}

async fn apply_terminal_sync(
    app: tauri::AppHandle,
    target_tools: Vec<String>,
    claude_models: Option<ClaudeModelSelection>,
) -> Result<Vec<TerminalSyncRecord>, String> {
    let payload = crate::app_store::service_providers_list().map_err(api_err_to_string)?;
    let upsert_app = app.clone();
    let records = apply_terminal_sync_with_selection(
        &payload.data,
        move |value| -> UpsertFuture {
            let app = upsert_app.clone();
            Box::pin(async move {
                crate::app_store::service_providers_upsert(app, value)
                    .await
                    .map(|_| ())
                    .map_err(api_err_to_string)
            })
        },
        target_tools,
        TerminalSyncProfile::current(),
        claude_models,
    )
    .await?;
    // Auto-activate the synced provider so the endpoint takes effect, then
    // project it to disk so the entry (including options.apiKey / the Claude
    // settings) actually lands; codex keeps its manual activation. A stale
    // ledger entry that resolved to an unmarked user record never reaches here
    // because the pipeline writes a fresh gateway record instead. Activation and
    // projection are not one transaction with the upsert, so a projection
    // failure is surfaced rather than reported as a successful application.
    for record in &records {
        if record.tool.eq_ignore_ascii_case("opencode")
            || record.tool.eq_ignore_ascii_case("claude")
        {
            crate::app_store::service_providers_set_active(
                app.clone(),
                record.tool.clone(),
                record.provider_id.clone(),
            )
            .await
            .map_err(api_err_to_string)?;
            crate::app_store::projection_apply(
                app.clone(),
                record.tool.clone(),
                record.provider_id.clone(),
            )
            .await
            .map_err(api_err_to_string)?;
        }
    }
    Ok(records)
}

/// Apply one template sync, then best-effort refresh every previously synced
/// terminal tool through the injected `terminal_sync` port.
///
/// The terminal refresh runs only after the template sync succeeded, only when
/// at least one upstream provider is bound to that template, and only when at
/// least one supported tool already holds a managed gateway record. It is
/// awaited at most once and its error is swallowed, so the template sync result
/// (and its `synced_at` update) always stands.
///
/// This is the injectable composition seam the template/terminal resync tests
/// drive; the production `ai_gateway_sync_provider_template` command splits the
/// same steps around the serialized configuration mutation so the write lock is
/// never held across an upstream wait.
#[cfg(test)]
pub(in crate::ai_gateway) async fn apply_template_sync_with_terminal_refresh<F, Fut>(
    config: &mut GatewayConfig,
    template_id: &str,
    fetch: impl FnOnce(&ProviderTemplate) -> Result<String, String>,
    persist: impl FnOnce(&GatewayConfig) -> Result<(), String>,
    providers_data: &serde_json::Value,
    terminal_sync: F,
) -> Result<ProviderTemplateView, String>
where
    F: FnOnce(Vec<String>) -> Fut + Send,
    Fut: std::future::Future<Output = Result<Vec<TerminalSyncRecord>, String>> + Send,
{
    let view = apply_template_sync_with(config, template_id, fetch, persist)?;
    let bound = config
        .providers
        .iter()
        .any(|provider| provider.template_id.as_deref() == Some(template_id));
    if bound {
        let tools = previously_synced_terminal_tools(config, providers_data, TerminalSyncProfile::current());
        if !tools.is_empty() {
            let _ = terminal_sync(tools).await;
        }
    }
    Ok(view)
}

#[tauri::command]
pub async fn ai_gateway_configure_terminal(
    app: tauri::AppHandle,
    target_tools: Vec<String>,
    claude_models: Option<ClaudeModelSelection>,
) -> Result<Vec<TerminalSyncRecord>, String> {
    apply_terminal_sync(app, target_tools, claude_models).await
}

#[tauri::command]
pub async fn ai_gateway_sync_terminal(
    app: tauri::AppHandle,
    target_tools: Option<Vec<String>>,
    claude_models: Option<ClaudeModelSelection>,
) -> Result<Vec<TerminalSyncRecord>, String> {
    let config = read_config()?;
    let targets = match target_tools {
        Some(tools) if !tools.is_empty() => tools,
        _ => config
            .terminal_syncs
            .iter()
            .map(|record| record.tool.clone())
            .collect(),
    };
    apply_terminal_sync(app, targets, claude_models).await
}

// ---------------------------------------------------------------------------
// Usage statistics, request logs and retention
// ---------------------------------------------------------------------------

/// Aggregated cards, UTC+8 buckets and per-model/provider detail for `range`.
/// `None`, an empty string and `"all"` mean all time; `"today"` and
/// `"yesterday"` bucket by hour; `"7d"`/`"15d"`/`"30d"` bucket by day. Unknown
/// selectors are rejected with the supported vocabulary.
#[tauri::command]
pub fn ai_gateway_usage_stats(range: Option<String>) -> Result<UsageStats, String> {
    let resolved = resolve_range_selector(range.as_deref(), now_millis())?;
    UsageLogStore::default_store()?.usage_stats(&resolved.range, resolved.hourly)
}

/// One page (50 rows, newest first) of request logs, or grouped rows when
/// `group_by` is `"model"`, `"provider"` or `"day"`. Range resolution, grouping,
/// filtering and pagination all happen here in the backend.
#[tauri::command]
pub fn ai_gateway_request_logs(
    range: Option<String>,
    group_by: Option<String>,
    status: Option<String>,
    model: Option<String>,
    provider: Option<String>,
    page: Option<u32>,
) -> Result<UsageLogsPage, String> {
    let range = resolve_range_selector(range.as_deref(), now_millis())?.range;
    let filter = LogFilter {
        status: status.as_deref().and_then(UsageResult::parse),
        model: model.filter(|model| !model.trim().is_empty()),
        provider: provider.filter(|value| !value.trim().is_empty()),
    };
    let store = UsageLogStore::default_store()?;
    let group = group_by.as_deref().unwrap_or("none");
    match group {
        "none" | "" => store.query_logs(&range, &filter, page.unwrap_or(1)),
        "model" | "provider" | "day" => {
            let groups = store.group_logs(&range, &filter, group)?;
            Ok(UsageLogsPage {
                page: 1,
                page_size: USAGE_LOG_PAGE_SIZE,
                total: groups.len() as u32,
                total_pages: 1,
                group_by: Some(group.to_string()),
                records: Vec::new(),
                groups,
                models: Vec::new(),
                providers: Vec::new(),
            })
        }
        other => Err(format!(
            "unsupported group_by '{other}': expected 'none', 'model', 'provider' or 'day'"
        )),
    }
}

#[tauri::command]
pub fn ai_gateway_usage_retention_get() -> Result<u32, String> {
    Ok(normalize_retention_days(read_config()?.usage_retention_days))
}

/// Replace only the retention days; invalid values (not 1-365) are rejected
/// with an actionable error and are never persisted.
#[tauri::command]
pub fn ai_gateway_usage_retention_save(days: i64) -> Result<u32, String> {
    let validated = validate_retention_days(days)?;
    modify_config(|config| {
        config.usage_retention_days = validated;
        Ok((true, ()))
    })?;
    Ok(read_config()?.usage_retention_days)
}

#[tauri::command]
pub fn ai_gateway_template_auto_refresh_get() -> Result<u32, String> {
    Ok(normalize_template_auto_refresh_minutes(
        read_config()?.template_auto_refresh_minutes,
    ))
}

/// Replace only the automatic provider-template refresh interval; values other
/// than `0` (disabled) or 10-1440 minutes are rejected with an actionable error
/// and are never persisted. A successful save re-arms the process scheduler
/// without starting an immediate batch.
#[tauri::command]
pub fn ai_gateway_template_auto_refresh_save(minutes: i64) -> Result<u32, String> {
    let validated = validate_template_auto_refresh_minutes(minutes)?;
    modify_config(|config| {
        config.template_auto_refresh_minutes = validated;
        Ok((true, ()))
    })?;
    super::auto_refresh::request_rearm();
    Ok(read_config()?.template_auto_refresh_minutes)
}

/// The process-memory automatic-refresh failure snapshot, so opening a page
/// receives existing inline errors without starting another schedule.
#[tauri::command]
pub fn ai_gateway_template_auto_refresh_status() -> Result<TemplateAutoRefreshStatus, String> {
    Ok(super::auto_refresh::failure_snapshot())
}

/// Start the process-owned template scheduler exactly once. Called from the app
/// setup after gateway autostart; a second call is a no-op.
pub fn start_template_auto_refresh_scheduler(app: tauri::AppHandle) {
    super::auto_refresh::start_scheduler(app);
}

/// Stop the process-owned template scheduler. Idempotent and safe when no
/// scheduler runs.
pub fn stop_template_auto_refresh_scheduler() {
    super::auto_refresh::stop_scheduler();
}

/// Stop the gateway listener from a synchronous shutdown path: signal the
/// running listener and clear the slot. Idempotent and safe when nothing runs.
pub fn stop_gateway_for_shutdown() {
    super::runtime_http::stop_server_now();
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

/// Perform one provider-template sync with all of its production side effects:
/// fetch the model list outside the configuration write lock, merge it under the
/// "source fields win, untouched template fields kept" rule, record the
/// additions-only and retirement messages after the write stands, then run the
/// best-effort terminal refresh. Any fatal source problem (network, non-JSON,
/// invalid structure, empty model set) leaves the configuration unchanged.
pub(in crate::ai_gateway) async fn execute_template_sync(
    app: Option<tauri::AppHandle>,
    template_id: &str,
) -> Result<ProviderTemplateView, String> {
    // Resolve the template and fetch its model list outside the configuration
    // write lock: the lock is never held across an upstream network wait.
    let initial = read_config()?;
    let template = effective_template(&initial, template_id)?;
    let raw = fetch_template_models(&template).await?;
    // The service-provider payload only decides which tools were previously
    // synced: a read failure degrades to an empty payload (refresh skipped)
    // instead of failing the template sync.
    let providers_data = crate::app_store::service_providers_list()
        .map(|payload| payload.data)
        .unwrap_or(Value::Null);
    // The read, staging and persistence all happen inside the serialized
    // mutation, so a concurrent configuration write cannot be lost.
    let (view, tools, notices, additions) = modify_config(|config| {
        let before = config.clone();
        let (view, notices) = apply_template_sync_from_body(config, template_id, &raw)?;
        let additions = template_sync_addition_notices(&before, config, template_id);
        let bound = config
            .providers
            .iter()
            .any(|provider| provider.template_id.as_deref() == Some(template_id));
        let tools = if bound {
            previously_synced_terminal_tools(config, &providers_data, TerminalSyncProfile::current())
        } else {
            Vec::new()
        };
        Ok((true, (view, tools, notices, additions)))
    })?;
    // The additions and retirement messages are recorded only after the
    // configuration write stood; a failed write returned before this point, so
    // no message escapes. Terminal resync stays after the template sync as today.
    record_template_sync_addition_message(app.as_ref(), &view, &additions);
    record_template_mappings_retired_messages(app.as_ref(), &notices);
    // The terminal refresh performs its own network upserts and serialized
    // ledger write after the template sync stands, so a failure is swallowed.
    if !tools.is_empty() {
        if let Some(handle) = app {
            let _ = apply_terminal_sync(handle, tools, None).await;
        }
    }
    Ok(view)
}

/// Refresh one template from its public source through the shared same-template
/// operation guard. An in-flight manual or automatic refresh of the same
/// template is reused instead of fetched again; the owner performs the sync.
#[tauri::command]
pub async fn ai_gateway_sync_provider_template(
    app: tauri::AppHandle,
    template_id: String,
) -> Result<ProviderTemplateView, String> {
    super::auto_refresh::sync_template_manual(Some(app), &template_id).await
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
    modify_config(|config| {
        let views = apply_upsert_provider_template(config, template, |_| Ok(()))?;
        Ok((true, views))
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Map, Value};

    fn bool_field(key: &str, value: bool) -> Value {
        let mut map = Map::new();
        map.insert(key.to_string(), Value::Bool(value));
        Value::Object(map)
    }

    fn string_field(key: &str, value: &str) -> Value {
        let mut map = Map::new();
        map.insert(key.to_string(), Value::String(value.to_string()));
        Value::Object(map)
    }

    fn with_tool_config(inner: Value) -> Value {
        let mut map = Map::new();
        map.insert("tool_config".to_string(), inner);
        Value::Object(map)
    }

    /// Each profile claims only its own marker: release claims the boolean
    /// `true` (top level or under `tool_config`) plus both legacy rename shapes;
    /// dev claims only the string `"dev"` (top level or under `tool_config`).
    /// Neither profile claims the other's marker, a `false` value, unrelated
    /// keys or `Null`.
    #[test]
    fn provider_marker_claims_only_the_current_profile() {
        let release = TerminalSyncProfile::RELEASE;
        let dev = TerminalSyncProfile::DEV;
        let legacy = super::super::migration::LEGACY_GATEWAY_MARKER_KEY;

        // Release claims the boolean marker at the top level and under tool_config.
        assert!(provider_has_gateway_marker(
            &bool_field(GATEWAY_MARKER_KEY, true),
            release
        ));
        assert!(provider_has_gateway_marker(
            &with_tool_config(bool_field(GATEWAY_MARKER_KEY, true)),
            release
        ));

        // Release also claims both legacy rename shapes.
        assert!(provider_has_gateway_marker(&bool_field(legacy, true), release));
        assert!(provider_has_gateway_marker(
            &with_tool_config(bool_field(legacy, true)),
            release
        ));

        // Release never claims the dev string marker.
        assert!(!provider_has_gateway_marker(
            &string_field(GATEWAY_MARKER_KEY, "dev"),
            release
        ));
        assert!(!provider_has_gateway_marker(
            &with_tool_config(string_field(GATEWAY_MARKER_KEY, "dev")),
            release
        ));

        // Dev claims only the string "dev", at the top level and under tool_config.
        assert!(provider_has_gateway_marker(
            &string_field(GATEWAY_MARKER_KEY, "dev"),
            dev
        ));
        assert!(provider_has_gateway_marker(
            &with_tool_config(string_field(GATEWAY_MARKER_KEY, "dev")),
            dev
        ));

        // Dev rejects the release boolean and both legacy shapes.
        assert!(!provider_has_gateway_marker(
            &bool_field(GATEWAY_MARKER_KEY, true),
            dev
        ));
        assert!(!provider_has_gateway_marker(
            &with_tool_config(bool_field(GATEWAY_MARKER_KEY, true)),
            dev
        ));
        assert!(!provider_has_gateway_marker(&bool_field(legacy, true), dev));
        assert!(!provider_has_gateway_marker(
            &with_tool_config(bool_field(legacy, true)),
            dev
        ));

        // Each profile rejects `false`, unrelated keys and `Null`.
        for profile in [release, dev] {
            assert!(!provider_has_gateway_marker(
                &bool_field(GATEWAY_MARKER_KEY, false),
                profile
            ));
            assert!(!provider_has_gateway_marker(
                &with_tool_config(bool_field("npm", true)),
                profile
            ));
            assert!(!provider_has_gateway_marker(
                &string_field("npm", "dev"),
                profile
            ));
            assert!(!provider_has_gateway_marker(&Value::Null, profile));
        }
    }
}


