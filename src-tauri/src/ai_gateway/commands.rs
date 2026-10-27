mod configuration;
mod key;
mod lifecycle;
mod pool;
mod provider;
mod template;
mod terminal;
mod usage;

pub use configuration::*;
pub use key::*;
pub use lifecycle::*;
pub use pool::*;
pub use provider::*;
pub use template::*;
pub use terminal::*;
pub use usage::*;

#[allow(unused_imports)]
pub(in crate::ai_gateway) use terminal::sync::{
    apply_terminal_sync_with_selection, default_key_for_sync, terminal_sync_pending, UpsertFuture,
};
#[cfg(test)]
pub(in crate::ai_gateway) use terminal::sync::apply_terminal_sync_with;

use super::storage::{modify_config, read_config};
use super::templates::{
    apply_template_sync_from_body, effective_template, fetch_template_models,
    template_sync_addition_notices, ProviderTemplateView,
};
use serde_json::Value;

/// Terminal tools that AI Gateway is allowed to write to.
pub(in crate::ai_gateway) const SUPPORTED_TERMINAL_TOOLS: [&str; 3] =
    ["opencode", "codex", "claude"];

/// Stable marker key identifying a provider record written by AI Gateway. The
/// accepted marker value is owned by the [`TerminalSyncProfile`].
const GATEWAY_MARKER_KEY: &str = "ai_gateway_gateway";

/// Application handle captured at gateway autostart/start so a synchronous save
/// command can launch the same best-effort terminal refresh as an async caller.
static GATEWAY_APP_HANDLE: std::sync::OnceLock<
    std::sync::Mutex<Option<tauri::AppHandle>>,
> = std::sync::OnceLock::new();

fn gateway_app_handle_slot() -> &'static std::sync::Mutex<Option<tauri::AppHandle>> {
    GATEWAY_APP_HANDLE.get_or_init(|| std::sync::Mutex::new(None))
}

fn capture_gateway_app_handle(app: tauri::AppHandle) {
    *gateway_app_handle_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(app);
}

fn captured_gateway_app_handle() -> Option<tauri::AppHandle> {
    gateway_app_handle_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

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
/// Whether `provider` carries the marker claimed by `profile`, either at the
/// top level or under `tool_config`; both shapes are recognized. Release records
/// written before the ai_gateway rename still carry the legacy marker and stay
/// recognized through [`super::migration::has_legacy_gateway_marker`], so the
/// next sync upgrades them in place. Dev claims only its own `"dev"` string
/// marker and never the release (or legacy) marker.
fn provider_has_gateway_marker(provider: &Value, profile: TerminalSyncProfile) -> bool {
    profile.claims_marker(provider)
}
fn api_err_to_string(error: crate::app_store::ApiErr) -> String {
    format!("{}: {}", error.code, error.message)
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
