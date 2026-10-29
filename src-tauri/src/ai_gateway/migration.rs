//! Permanent, version-gated migration of on-disk state written by older builds.
//!
//! This module is the ONLY place that may name the legacy files, the legacy
//! gateway marker and the legacy field shapes. Two version gates live here: a
//! configuration older than [`super::GATEWAY_CONFIG_SCHEMA_VERSION`] is rewritten
//! once with its global price rows scoped, its singular `off_peak` window folded
//! into `off_peaks`, its provider-level runtime fields cleared, each pool's
//! legacy single fallback candidate converted into the ordered gateway-model
//! fallback list and its obsolete gateway-local proxy URL removed; a usage
//! database older than [`USAGE_DB_VERSION`] has its legacy `cancelled` rows
//! deleted once (version below 1), then gains the durable provider/day summary
//! tables, the one-shot retained-row backfill and the continuity boundary
//! (version 2). The legacy file renames and the legacy gateway-marker predicate
//! stay as this module's permanent compatibility surface.

use rusqlite::{Connection, TransactionBehavior};
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::path::Path;

/// Legacy encrypted config file name, replaced by `super::CONFIG_FILE`.
pub(in crate::ai_gateway) const LEGACY_CONFIG_FILE_NAME: &str = "api_gateway.json";
/// Legacy usage-log database name, replaced by `super::USAGE_DB_FILE`.
pub(in crate::ai_gateway) const LEGACY_USAGE_DB_FILE_NAME: &str = "api_gateway_usage.db";
/// Legacy gateway marker key, replaced by the marker written by
/// `commands::build_gateway_provider`.
pub(in crate::ai_gateway) const LEGACY_GATEWAY_MARKER_KEY: &str = "api_gateway_gateway";

/// Current usage-database version: the one-time cancelled-row cleanup (version
/// 1) plus the additive provider/day summary tables, one-shot backfill and the
/// continuity boundary (version 2).
pub(in crate::ai_gateway) const USAGE_DB_VERSION: u32 = 2;

/// Provider-level runtime health keys that only mapping rows may carry now.
const LEGACY_PROVIDER_RUNTIME_KEYS: [&str; 5] = [
    "auto_disabled",
    "disabled_reason",
    "disabled_at",
    "consecutive_failures",
    "last_error_at",
];

/// Legacy singular off-peak window key, superseded by the `off_peaks` list.
const LEGACY_SINGULAR_OFF_PEAK_KEY: &str = "off_peak";
const OFF_PEAKS_KEY: &str = "off_peaks";

/// Version-2 OpenCode Zen template identity, superseded in schema 3 by OpenCode
/// Go. This migration module is the only non-test production source allowed to
/// name the legacy identity.
const LEGACY_OPENCODE_ZEN_TEMPLATE_ID: &str = "opencode-zen";
/// Current OpenCode Go template identity.
const OPENCODE_GO_TEMPLATE_ID: &str = "opencode-go";
/// Built-in OpenCode Zen Free template identity introduced with schema 5.
const OPENCODE_ZEN_FREE_TEMPLATE_ID: &str = "opencode-zen-free";
/// Exact provider name the schema-5 binding migration recognizes.
const OPENCODE_ZEN_FREE_PROVIDER_NAME: &str = "OpenCode Zen Free";
/// Normalized OpenCode Zen Free upstream base URL.
const OPENCODE_ZEN_FREE_BASE_URL: &str = "https://opencode.ai/zen/v1";
const NAME_FIELD: &str = "name";
const TEMPLATE_ID_FIELD: &str = "template_id";
const TEMPLATE_FIELD: &str = "template";
const SYNCED_AT_FIELD: &str = "synced_at";
const SOURCE_FIELD: &str = "source";
const BASE_URL_FIELD: &str = "base_url";
const DELETED_TEMPLATE_IDS_FIELD: &str = "deleted_template_ids";
const PROVIDERS_FIELD: &str = "providers";
const PROVIDER_TEMPLATES_FIELD: &str = "provider_templates";
const MAPPINGS_FIELD: &str = "mappings";
const IGNORED_MODELS_FIELD: &str = "ignored_models";
const DEFAULT_MODEL_FIELD: &str = "default_model";
const UPSTREAM_MODEL_FIELD: &str = "upstream_model";
const PROTOCOL_FIELD: &str = "protocol";
const ENABLED_FIELD: &str = "enabled";
const MODEL_POOLS_FIELD: &str = "model_pools";
const LOCAL_MODEL_FIELD: &str = "local_model";
const SETTINGS_FIELD: &str = "settings";
/// Obsolete gateway-local upstream proxy URL, superseded by the application-wide
/// network proxy. This module is the only non-test production source that names
/// the removed field.
const LEGACY_GATEWAY_PROXY_URL_FIELD: &str = "proxy_url";
/// Legacy single provider-specific fallback candidate of one model pool,
/// superseded by the ordered `fallback_models` gateway-model list.
const LEGACY_POOL_FALLBACK_FIELD: &str = "fallback";
/// Current ordered gateway-model fallback list.
const POOL_FALLBACK_MODELS_FIELD: &str = "fallback_models";

/// Convert every stored pool's legacy single fallback candidate to the ordered
/// gateway-model list and remove the legacy key. Only the trimmed local model id
/// is kept: the legacy candidate's reasoning, weight, provider id, upstream
/// model and protocol have no place in a model-level list. A missing, null or
/// malformed legacy fallback leaves an existing list untouched and adds nothing.
/// Idempotent: a migrated pool has no legacy key to convert.
fn migrate_pool_fallbacks(object: &mut Map<String, Value>) -> bool {
    let Some(pools) = object
        .get_mut(MODEL_POOLS_FIELD)
        .and_then(Value::as_array_mut)
    else {
        return false;
    };
    let mut changed = false;
    for pool in pools.iter_mut() {
        let Some(pool) = pool.as_object_mut() else {
            continue;
        };
        let Some(legacy) = pool.remove(LEGACY_POOL_FALLBACK_FIELD) else {
            continue;
        };
        changed = true;
        let local_model = legacy
            .get(LOCAL_MODEL_FIELD)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|model| !model.is_empty());
        let Some(local_model) = local_model else {
            continue;
        };
        if pool
            .get(POOL_FALLBACK_MODELS_FIELD)
            .and_then(Value::as_array)
            .is_some()
        {
            continue;
        }
        pool.insert(
            POOL_FALLBACK_MODELS_FIELD.to_string(),
            Value::Array(vec![Value::String(local_model.to_string())]),
        );
    }
    changed
}

/// Serialize the shipped OpenCode Go baseline into its persisted JSON shape,
/// plus each declared model id's explicit protocol. A missing baseline yields
/// `None` so the migration no-ops instead of corrupting the value.
struct GoBaseline {
    template: Value,
    base_url: String,
    models_url: String,
    declarations: Vec<(String, String)>,
}

fn opencode_go_baseline() -> Option<GoBaseline> {
    let baseline = super::templates::find_builtin_template(OPENCODE_GO_TEMPLATE_ID).ok()?;
    let declarations = baseline
        .models
        .iter()
        .filter_map(|model| {
            model.protocol.map(|protocol| {
                let protocol = match protocol {
                    super::types_config::UpstreamProtocol::ChatCompletions => "chat_completions",
                    super::types_config::UpstreamProtocol::Responses => "responses",
                    super::types_config::UpstreamProtocol::AnthropicMessages => {
                        "anthropic_messages"
                    }
                };
                (model.upstream_model.clone(), protocol.to_string())
            })
        })
        .collect();
    Some(GoBaseline {
        template: serde_json::to_value(&baseline).ok()?,
        base_url: baseline.base_url.clone(),
        models_url: baseline.models_url.clone().unwrap_or_default(),
        declarations,
    })
}

/// Map every persisted `opencode-zen` identity to `opencode-go` and quarantine
/// the provider data the Go baseline cannot serve, in the raw configuration
/// value. Only a `template_id` binding triggers a provider migration, so a
/// provider whose URL merely resembles Zen is left untouched. Idempotent: a
/// second pass finds no Zen identity and changes nothing.
fn migrate_opencode_zen_to_go(object: &mut Map<String, Value>) -> bool {
    let has_zen_deleted = object
        .get(DELETED_TEMPLATE_IDS_FIELD)
        .and_then(Value::as_array)
        .is_some_and(|ids| {
            ids.iter()
                .any(|id| id.as_str() == Some(LEGACY_OPENCODE_ZEN_TEMPLATE_ID))
        });
    let has_zen_state = object
        .get(PROVIDER_TEMPLATES_FIELD)
        .and_then(Value::as_array)
        .is_some_and(|states| {
            states.iter().any(|state| {
                state.get(TEMPLATE_ID_FIELD).and_then(Value::as_str)
                    == Some(LEGACY_OPENCODE_ZEN_TEMPLATE_ID)
            })
        });
    let has_zen_provider = object
        .get(PROVIDERS_FIELD)
        .and_then(Value::as_array)
        .is_some_and(|providers| {
            providers.iter().any(|provider| {
                provider.get(TEMPLATE_ID_FIELD).and_then(Value::as_str)
                    == Some(LEGACY_OPENCODE_ZEN_TEMPLATE_ID)
            })
        });
    if !has_zen_deleted && !has_zen_state && !has_zen_provider {
        return false;
    }

    let Some(baseline) = opencode_go_baseline() else {
        return false;
    };
    let mut changed = false;

    if let Some(deleted) = object
        .get_mut(DELETED_TEMPLATE_IDS_FIELD)
        .and_then(Value::as_array_mut)
    {
        for id in deleted.iter_mut() {
            if id.as_str() == Some(LEGACY_OPENCODE_ZEN_TEMPLATE_ID) {
                *id = Value::String(OPENCODE_GO_TEMPLATE_ID.to_string());
                changed = true;
            }
        }
    }

    if let Some(states) = object
        .get_mut(PROVIDER_TEMPLATES_FIELD)
        .and_then(Value::as_array_mut)
    {
        let go_state_exists = states.iter().any(|state| {
            state.get(TEMPLATE_ID_FIELD).and_then(Value::as_str) == Some(OPENCODE_GO_TEMPLATE_ID)
        });
        if go_state_exists {
            // A Go state already exists: drop the stale Zen duplicate
            // deterministically rather than creating two Go identities.
            let before = states.len();
            states.retain(|state| {
                state.get(TEMPLATE_ID_FIELD).and_then(Value::as_str)
                    != Some(LEGACY_OPENCODE_ZEN_TEMPLATE_ID)
            });
            if states.len() != before {
                changed = true;
            }
        } else {
            for state in states.iter_mut() {
                if state.get(TEMPLATE_ID_FIELD).and_then(Value::as_str)
                    != Some(LEGACY_OPENCODE_ZEN_TEMPLATE_ID)
                {
                    continue;
                }
                let Some(state) = state.as_object_mut() else {
                    continue;
                };
                state.insert(
                    TEMPLATE_ID_FIELD.to_string(),
                    Value::String(OPENCODE_GO_TEMPLATE_ID.to_string()),
                );
                state.insert(TEMPLATE_FIELD.to_string(), baseline.template.clone());
                state.insert(SYNCED_AT_FIELD.to_string(), Value::Null);
                if !baseline.models_url.is_empty() {
                    state.insert(
                        SOURCE_FIELD.to_string(),
                        Value::String(baseline.models_url.clone()),
                    );
                }
                changed = true;
            }
        }
    }

    if let Some(providers) = object
        .get_mut(PROVIDERS_FIELD)
        .and_then(Value::as_array_mut)
    {
        for provider in providers.iter_mut() {
            if provider.get(TEMPLATE_ID_FIELD).and_then(Value::as_str)
                != Some(LEGACY_OPENCODE_ZEN_TEMPLATE_ID)
            {
                continue;
            }
            let Some(provider) = provider.as_object_mut() else {
                continue;
            };
            provider.insert(
                TEMPLATE_ID_FIELD.to_string(),
                Value::String(OPENCODE_GO_TEMPLATE_ID.to_string()),
            );
            provider.insert(
                BASE_URL_FIELD.to_string(),
                Value::String(baseline.base_url.clone()),
            );
            changed = true;

            let mut ignored: Vec<String> = provider
                .get(IGNORED_MODELS_FIELD)
                .and_then(Value::as_array)
                .map(|ids| {
                    ids.iter()
                        .filter_map(|id| id.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();

            if let Some(mappings) = provider
                .get_mut(MAPPINGS_FIELD)
                .and_then(Value::as_array_mut)
            {
                for mapping in mappings.iter_mut() {
                    let Some(mapping) = mapping.as_object_mut() else {
                        continue;
                    };
                    let Some(upstream_model) = mapping
                        .get(UPSTREAM_MODEL_FIELD)
                        .and_then(Value::as_str)
                        .map(str::to_string)
                    else {
                        continue;
                    };
                    let declared_protocol = baseline
                        .declarations
                        .iter()
                        .find(|(identifier, _)| *identifier == upstream_model)
                        .map(|(_, protocol)| protocol.clone());
                    match declared_protocol {
                        Some(protocol) => {
                                // Preserve an explicit protocol; give a supported
                                // non-Chat mapping an explicit protocol when it
                                // previously inherited the old provider default.
                                if mapping
                                    .get(PROTOCOL_FIELD)
                                    .and_then(Value::as_str)
                                    .is_none()
                                    && protocol != "chat_completions"
                                {
                                mapping.insert(PROTOCOL_FIELD.to_string(), Value::String(protocol));
                                changed = true;
                            }
                        }
                        None => {
                            if mapping.get(ENABLED_FIELD).and_then(Value::as_bool) != Some(false) {
                                mapping.insert(ENABLED_FIELD.to_string(), Value::Bool(false));
                                changed = true;
                            }
                            if !ignored.iter().any(|existing| existing == &upstream_model) {
                                ignored.push(upstream_model);
                            }
                        }
                    }
                }
            }

            if !ignored.is_empty() {
                let ignored = Value::Array(ignored.into_iter().map(Value::String).collect());
                if provider.get(IGNORED_MODELS_FIELD) != Some(&ignored) {
                    provider.insert(IGNORED_MODELS_FIELD.to_string(), ignored);
                    changed = true;
                }
            }

            // Clear a default model the Go baseline does not serve.
            if let Some(default_model) = provider
                .get(DEFAULT_MODEL_FIELD)
                .and_then(Value::as_str)
                .map(str::to_string)
            {
                let supported = baseline
                    .declarations
                    .iter()
                    .any(|(identifier, _)| identifier == default_model.trim());
                if !supported {
                    provider.insert(DEFAULT_MODEL_FIELD.to_string(), Value::Null);
                    changed = true;
                }
            }
        }
    }

    changed
}

/// Trim surrounding whitespace and every trailing slash from a stored base URL
/// before comparing it with the canonical OpenCode Zen Free URL.
fn normalize_provider_base_url(value: &str) -> &str {
    value.trim().trim_end_matches('/')
}

/// Whether a raw provider object carries no usable `template_id`: the field is
/// missing, `null`, a blank string, or a non-string shape this migration cannot
/// interpret as an existing binding.
fn provider_has_no_template(provider: &Map<String, Value>) -> bool {
    match provider.get(TEMPLATE_ID_FIELD) {
        None | Some(Value::Null) => true,
        Some(Value::String(id)) => id.trim().is_empty(),
        _ => false,
    }
}

/// Bind the one untemplated provider whose trimmed name is exactly `OpenCode Zen
/// Free` and whose normalized base URL is exactly the canonical Zen URL to the
/// built-in OpenCode Zen Free template. Keys, mappings, prices and every other
/// provider stay untouched. Idempotent: a second pass sees the new binding.
fn migrate_opencode_zen_free_binding(object: &mut Map<String, Value>) -> bool {
    let Some(providers) = object
        .get_mut(PROVIDERS_FIELD)
        .and_then(Value::as_array_mut)
    else {
        return false;
    };
    let mut changed = false;
    for provider in providers.iter_mut() {
        let Some(provider) = provider.as_object_mut() else {
            continue;
        };
        if !provider_has_no_template(provider) {
            continue;
        }
        let name_matches = provider
            .get(NAME_FIELD)
            .and_then(Value::as_str)
            .map(str::trim)
            == Some(OPENCODE_ZEN_FREE_PROVIDER_NAME);
        let url_matches = provider
            .get(BASE_URL_FIELD)
            .and_then(Value::as_str)
            .map(normalize_provider_base_url)
            == Some(OPENCODE_ZEN_FREE_BASE_URL);
        if name_matches && url_matches {
            provider.insert(
                TEMPLATE_ID_FIELD.to_string(),
                Value::String(OPENCODE_ZEN_FREE_TEMPLATE_ID.to_string()),
            );
            changed = true;
        }
    }
    changed
}

/// Legacy provider-level single credential field, superseded by the ordered
/// `keys` pool. This is the only non-test production source allowed to name it.
const LEGACY_SINGLE_KEY_FIELD: &str = "api_key";
/// Current ordered upstream key pool field.
const KEYS_FIELD: &str = "keys";
/// Name of the single entry a migrated legacy credential produces.
const MIGRATED_DEFAULT_KEY_NAME: &str = "Default";
const MIGRATED_DEFAULT_KEY_ID: &str = "default";

/// Convert one raw provider object to the key-pool shape and report whether
/// anything changed.
///
/// Precedence follows the persisted contract: an existing `keys` array is
/// authoritative and only the legacy field is removed; otherwise a non-blank
/// legacy `api_key` becomes exactly one enabled `Default` entry with no runtime
/// state, and a blank or missing legacy value yields an empty pool. Missing
/// fields are tolerated so a legacy shape can never fail the whole parse.
fn convert_provider_keys(provider: &mut Map<String, Value>) -> bool {
    let legacy = provider.remove(LEGACY_SINGLE_KEY_FIELD);
    if provider.get(KEYS_FIELD).and_then(Value::as_array).is_some() {
        return legacy.is_some();
    }
    let legacy_value = legacy
        .as_ref()
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let keys = match legacy_value {
        Some(value) => Value::Array(vec![serde_json::json!({
            "id": MIGRATED_DEFAULT_KEY_ID,
            "name": MIGRATED_DEFAULT_KEY_NAME,
            "value": value,
            "enabled": true,
            "auto_marked": false,
            "failure_kind": null,
            "marked_at": null,
            "reason": null,
        })]),
        None => Value::Array(Vec::new()),
    };
    provider.insert(KEYS_FIELD.to_string(), keys);
    true
}

/// The provider ids together with the upstream models each provider reaches:
/// every non-blank, retained mapping `upstream_model` plus its trimmed default
/// model.
fn reachable_provider_models(config: &Value) -> Vec<(String, HashSet<String>)> {
    config
        .get("providers")
        .and_then(Value::as_array)
        .map(|providers| {
            providers
                .iter()
                .filter_map(|provider| {
                    let provider = provider.as_object()?;
                    let id = provider.get("id").and_then(Value::as_str)?.to_string();
                    let mut models = HashSet::new();
                    if let Some(mappings) = provider.get("mappings").and_then(Value::as_array) {
                        for mapping in mappings {
                            let local_is_set = mapping
                                .get("local_model")
                                .and_then(Value::as_str)
                                .map(|value| !value.trim().is_empty())
                                .unwrap_or(false);
                            let upstream = mapping
                                .get("upstream_model")
                                .and_then(Value::as_str)
                                .filter(|value| !value.trim().is_empty());
                            if local_is_set {
                                if let Some(upstream) = upstream {
                                    models.insert(upstream.to_string());
                                }
                            }
                        }
                    }
                    if let Some(default_model) =
                        provider.get("default_model").and_then(Value::as_str)
                    {
                        let trimmed = default_model.trim();
                        if !trimmed.is_empty() {
                            models.insert(trimmed.to_string());
                        }
                    }
                    Some((id, models))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A price row's provider id: absent, `null` or non-string means a legacy global
/// row.
fn price_row_provider_id(row: &Value) -> Option<String> {
    match row.get("provider_id") {
        Some(Value::String(provider_id)) => Some(provider_id.clone()),
        _ => None,
    }
}

/// Fold a legacy singular `off_peak` window into `off_peaks` when no multi-window
/// list exists yet, preserving every tier and the weekday set. Reports whether
/// the row changed.
fn convert_singular_off_peak(row: &mut Map<String, Value>) -> bool {
    let Some(off_peak) = row.remove(LEGACY_SINGULAR_OFF_PEAK_KEY) else {
        return false;
    };
    if !off_peak.is_null() {
        let has_off_peaks = row
            .get(OFF_PEAKS_KEY)
            .and_then(Value::as_array)
            .map(|entries| !entries.is_empty())
            .unwrap_or(false);
        if !has_off_peaks {
            row.insert(OFF_PEAKS_KEY.to_string(), Value::Array(vec![off_peak]));
        }
    }
    true
}

/// Remove only the obsolete gateway-local upstream proxy URL from the raw
/// settings object. Every other setting, provider, key, pool, price and template
/// value is left untouched, and the obsolete URL is never copied anywhere else.
fn remove_legacy_gateway_proxy_url(object: &mut Map<String, Value>) -> bool {
    let Some(settings) = object.get_mut(SETTINGS_FIELD).and_then(Value::as_object_mut) else {
        return false;
    };
    settings.remove(LEGACY_GATEWAY_PROXY_URL_FIELD).is_some()
}

/// Migrate a raw configuration value in place and report whether anything
/// changed. The transformation reproduces the former per-read normalization:
/// global rows are copied to every provider that reaches their model, scoped
/// duplicates keep the first occurrence and rows whose provider/model is gone
/// are dropped; the singular off-peak window becomes the first `off_peaks`
/// entry; provider-level runtime keys are cleared; the obsolete gateway-local
/// proxy URL is removed. A schema-5 value skips every earlier gate and only
/// drops the obsolete proxy URL. Missing fields are tolerated so a legacy shape
/// can never fail the whole parse.
pub(in crate::ai_gateway) fn migrate_legacy_config(config: &mut Value, stored_version: u32) -> bool {
    if !config.is_object() {
        return false;
    }
    if stored_version >= 5 {
        // A schema-5 file already completed every earlier gate. Advancing it to
        // schema 6 removes only the obsolete gateway-local proxy URL and never
        // reapplies an earlier binding migration to an unbound provider.
        let object = config
            .as_object_mut()
            .expect("config was checked to be an object");
        return remove_legacy_gateway_proxy_url(object);
    }
    // The schema-3 Zen-to-Go identity migration runs first so reachability and
    // price scoping see the migrated provider bindings and mappings.
    let mut changed = {
        let object = config
            .as_object_mut()
            .expect("config was checked to be an object");
        let changed = migrate_opencode_zen_to_go(object);
        let changed = changed | migrate_opencode_zen_free_binding(object);
        let changed = changed | migrate_pool_fallbacks(object);
        changed | remove_legacy_gateway_proxy_url(object)
    };
    let reachable = reachable_provider_models(config);
    let Some(object) = config.as_object_mut() else {
        return changed;
    };

    if let Some(providers) = object.get_mut("providers").and_then(Value::as_array_mut) {
        for provider in providers.iter_mut() {
            let Some(provider) = provider.as_object_mut() else {
                continue;
            };
            for key in LEGACY_PROVIDER_RUNTIME_KEYS {
                if provider.remove(key).is_some() {
                    changed = true;
                }
            }
            if convert_provider_keys(provider) {
                changed = true;
            }
        }
    }

    let Some(rows) = object.get_mut("model_prices").and_then(Value::as_array_mut) else {
        return changed;
    };

    for row in rows.iter_mut() {
        if let Some(row) = row.as_object_mut() {
            if convert_singular_off_peak(row) {
                changed = true;
            }
        }
    }

    let global_rows: Vec<Value> = rows
        .iter()
        .filter(|row| price_row_provider_id(row).is_none())
        .cloned()
        .collect();
    for global in global_rows {
        let Some(upstream_model) = global
            .get("upstream_model")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            continue;
        };
        for (provider_id, models) in &reachable {
            if !models.contains(&upstream_model) {
                continue;
            }
            let already_scoped = rows.iter().any(|existing| {
                price_row_provider_id(existing).as_deref() == Some(provider_id.as_str())
                    && existing.get("upstream_model").and_then(Value::as_str)
                        == Some(upstream_model.as_str())
            });
            if already_scoped {
                continue;
            }
            let mut migrated = global.clone();
            if let Some(object) = migrated.as_object_mut() {
                object.insert(
                    "provider_id".to_string(),
                    Value::String(provider_id.clone()),
                );
            }
            rows.push(migrated);
            changed = true;
        }
    }

    let mut seen: Vec<(String, String)> = Vec::new();
    let mut dropped = false;
    rows.retain(|row| {
        let Some(provider_id) = price_row_provider_id(row) else {
            dropped = true;
            return false;
        };
        let Some(upstream_model) = row.get("upstream_model").and_then(Value::as_str) else {
            dropped = true;
            return false;
        };
        let Some((_, models)) = reachable.iter().find(|(id, _)| id == &provider_id) else {
            dropped = true;
            return false;
        };
        if !models.contains(upstream_model) {
            dropped = true;
            return false;
        }
        let key = (provider_id, upstream_model.to_string());
        if seen.contains(&key) {
            dropped = true;
            return false;
        }
        seen.push(key);
        true
    });
    changed || dropped
}

/// Bring an older usage database to the current version in one serialized write
/// transaction, and recheck the marker after acquiring the write boundary so
/// repeated or concurrent opens never duplicate the work.
///
/// Version below 1 deletes every legacy `result = 'cancelled'` row once; a
/// version-1 database skips that step. Every older database then gains the
/// additive provider/day summary tables, backfills retained terminal
/// valid-result nonblank-provider rows exactly once and stamps the migration
/// UTC+8 day as the continuous-recording boundary. A future version is left
/// untouched and never downgraded. Any failure rolls the whole transaction back,
/// leaving no partial summary table and no advanced marker so a later open can
/// retry.
pub(in crate::ai_gateway) fn migrate_usage_database(
    connection: &mut Connection,
) -> Result<(), String> {
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    if version >= USAGE_DB_VERSION as i64 {
        return Ok(());
    }
    // BEGIN IMMEDIATE acquires the write boundary up front; the recheck below
    // then observes any migration another opener committed while we waited.
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    let current: i64 = transaction
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    if current >= USAGE_DB_VERSION as i64 {
        transaction.rollback().map_err(|error| error.to_string())?;
        return Ok(());
    }
    if current < 1 {
        transaction
            .execute("DELETE FROM usage_logs WHERE result = 'cancelled'", [])
            .map_err(|error| error.to_string())?;
    }
    super::provider_usage::create_summary_tables(&transaction)?;
    super::provider_usage::backfill_provider_summaries(&transaction)?;
    super::provider_usage::stamp_continuity_boundary(&transaction)?;
    transaction
        .pragma_update(None, "user_version", USAGE_DB_VERSION as i64)
        .map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(())
}

/// Rename one legacy file to its new name when the new file is absent and the
/// legacy file exists, reporting whether a rename actually happened. A
/// pre-existing new file always wins (the legacy file is left untouched), every
/// error is ignored, and the operation is idempotent.
fn rename_if_missing(dir: &Path, legacy: &str, current: &str) -> bool {
    let target = dir.join(current);
    if target.exists() {
        return false;
    }
    let source = dir.join(legacy);
    if source.exists() {
        return std::fs::rename(source, target).is_ok();
    }
    false
}

/// Best-effort in-place migration under `get_app_dir()`. The config file is
/// migrated independently; the usage database's `-wal`/`-shm` sidecars belong
/// to the main database file, so they are migrated ONLY when this call renamed
/// the legacy main database itself. When the legacy main database is not
/// renamed (a current database already exists, or there is no legacy database)
/// every legacy sidecar is left untouched, so a stray sidecar can never be
/// attached to an unrelated current database. Each sidecar is still renamed
/// only when its own target is absent. Never overwrites an existing new file,
/// every error is ignored, and repeated calls are safe.
pub(in crate::ai_gateway) fn migrate_legacy_files() {
    let Ok(dir) = crate::config::get_app_dir() else {
        return;
    };
    rename_if_missing(&dir, LEGACY_CONFIG_FILE_NAME, super::CONFIG_FILE);
    let main_db_renamed =
        rename_if_missing(&dir, LEGACY_USAGE_DB_FILE_NAME, super::USAGE_DB_FILE);
    if main_db_renamed {
        for sidecar in ["-wal", "-shm"] {
            rename_if_missing(
                &dir,
                &format!("{LEGACY_USAGE_DB_FILE_NAME}{sidecar}"),
                &format!("{}{sidecar}", super::USAGE_DB_FILE),
            );
        }
    }
}

/// Whether `provider` still carries the legacy gateway marker at the top level
/// or under `tool_config` (both shapes existed before the rename). It is the
/// compatibility half of `commands::provider_has_gateway_marker`; newly written
/// records always use the current marker.
pub(in crate::ai_gateway) fn has_legacy_gateway_marker(provider: &Value) -> bool {
    provider
        .get(LEGACY_GATEWAY_MARKER_KEY)
        .and_then(Value::as_bool)
        == Some(true)
        || provider
            .get("tool_config")
            .and_then(|tool_config| tool_config.get(LEGACY_GATEWAY_MARKER_KEY))
            .and_then(Value::as_bool)
            == Some(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;

    struct IsolatedTempHome {
        path: PathBuf,
        _guard: crate::config::test_home::TestHomeGuard,
    }

    impl Drop for IsolatedTempHome {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn isolated_temp_home(name: &str) -> IsolatedTempHome {
        let path = std::env::temp_dir().join(format!(
            "onespace-ai-gateway-migration-{}-{}",
            name,
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&path).expect("create temp home");
        let guard = crate::config::test_home::TestHomeGuard::set(&path);
        IsolatedTempHome {
            path,
            _guard: guard,
        }
    }

    #[test]
    fn migrate_legacy_files_renames_legacy_files_and_sidecars_in_place() {
        let _home = isolated_temp_home("rename");
        let dir = crate::config::get_app_dir().expect("app dir");
        fs::create_dir_all(&dir).expect("create app dir");

        fs::write(dir.join(LEGACY_CONFIG_FILE_NAME), b"legacy-config").expect("write legacy config");
        fs::write(dir.join(LEGACY_USAGE_DB_FILE_NAME), b"legacy-db").expect("write legacy db");
        fs::write(
            dir.join(format!("{LEGACY_USAGE_DB_FILE_NAME}-wal")),
            b"legacy-wal",
        )
        .expect("write legacy wal");
        fs::write(
            dir.join(format!("{LEGACY_USAGE_DB_FILE_NAME}-shm")),
            b"legacy-shm",
        )
        .expect("write legacy shm");

        migrate_legacy_files();

        assert_eq!(
            fs::read(dir.join(super::super::CONFIG_FILE)).expect("read config"),
            b"legacy-config",
            "legacy config must be renamed to the current config"
        );
        assert_eq!(
            fs::read(dir.join(super::super::USAGE_DB_FILE)).expect("read db"),
            b"legacy-db",
            "legacy db must be renamed to the current db"
        );
        assert_eq!(
            fs::read(dir.join(format!("{}-wal", super::super::USAGE_DB_FILE))).expect("read wal"),
            b"legacy-wal"
        );
        assert_eq!(
            fs::read(dir.join(format!("{}-shm", super::super::USAGE_DB_FILE))).expect("read shm"),
            b"legacy-shm"
        );
        assert!(!dir.join(LEGACY_CONFIG_FILE_NAME).exists());
        assert!(!dir.join(LEGACY_USAGE_DB_FILE_NAME).exists());
        assert!(!dir.join(format!("{LEGACY_USAGE_DB_FILE_NAME}-wal")).exists());
        assert!(!dir.join(format!("{LEGACY_USAGE_DB_FILE_NAME}-shm")).exists());
    }

    #[test]
    fn migrate_legacy_files_never_overwrites_existing_current_files_and_is_idempotent() {
        let _home = isolated_temp_home("no-overwrite");
        let dir = crate::config::get_app_dir().expect("app dir");
        fs::create_dir_all(&dir).expect("create app dir");

        fs::write(dir.join(super::super::CONFIG_FILE), b"current-config")
            .expect("write current config");
        fs::write(dir.join(super::super::USAGE_DB_FILE), b"current-db").expect("write current db");
        fs::write(dir.join(LEGACY_CONFIG_FILE_NAME), b"legacy-config").expect("write legacy config");
        fs::write(dir.join(LEGACY_USAGE_DB_FILE_NAME), b"legacy-db").expect("write legacy db");

        migrate_legacy_files();
        migrate_legacy_files();

        assert_eq!(
            fs::read(dir.join(super::super::CONFIG_FILE)).expect("read config"),
            b"current-config",
            "an existing current config must never be overwritten"
        );
        assert_eq!(
            fs::read(dir.join(super::super::USAGE_DB_FILE)).expect("read db"),
            b"current-db",
            "an existing current db must never be overwritten"
        );
        assert_eq!(
            fs::read(dir.join(LEGACY_CONFIG_FILE_NAME)).expect("read legacy config"),
            b"legacy-config",
            "the legacy config stays untouched while the current file exists"
        );
        assert_eq!(
            fs::read(dir.join(LEGACY_USAGE_DB_FILE_NAME)).expect("read legacy db"),
            b"legacy-db",
            "the legacy db stays untouched while the current file exists"
        );
    }

    /// The usage database and both sidecars migrate together when no current
    /// file exists, so a consistent `db`/`-wal`/`-shm` triple lands in place.
    #[test]
    fn migrate_legacy_files_renames_db_and_sidecars_when_nothing_current_exists() {
        let _home = isolated_temp_home("db-sidecars-rename");
        let dir = crate::config::get_app_dir().expect("app dir");
        fs::create_dir_all(&dir).expect("create app dir");

        fs::write(dir.join(LEGACY_USAGE_DB_FILE_NAME), b"legacy-db").expect("write legacy db");
        fs::write(
            dir.join(format!("{LEGACY_USAGE_DB_FILE_NAME}-wal")),
            b"legacy-wal",
        )
        .expect("write legacy wal");
        fs::write(
            dir.join(format!("{LEGACY_USAGE_DB_FILE_NAME}-shm")),
            b"legacy-shm",
        )
        .expect("write legacy shm");

        migrate_legacy_files();

        assert_eq!(
            fs::read(dir.join(super::super::USAGE_DB_FILE)).expect("read db"),
            b"legacy-db"
        );
        assert_eq!(
            fs::read(dir.join(format!("{}-wal", super::super::USAGE_DB_FILE))).expect("read wal"),
            b"legacy-wal"
        );
        assert_eq!(
            fs::read(dir.join(format!("{}-shm", super::super::USAGE_DB_FILE))).expect("read shm"),
            b"legacy-shm"
        );
        assert!(!dir.join(LEGACY_USAGE_DB_FILE_NAME).exists());
        assert!(!dir.join(format!("{LEGACY_USAGE_DB_FILE_NAME}-wal")).exists());
        assert!(!dir.join(format!("{LEGACY_USAGE_DB_FILE_NAME}-shm")).exists());
    }

    /// A current main database that already exists blocks the whole database
    /// migration: the legacy main database AND its sidecars stay untouched, and
    /// no stray sidecar is attached to the current database.
    #[test]
    fn migrate_legacy_files_leaves_db_and_sidecars_untouched_when_current_db_exists() {
        let _home = isolated_temp_home("current-db-blocks-sidecars");
        let dir = crate::config::get_app_dir().expect("app dir");
        fs::create_dir_all(&dir).expect("create app dir");

        fs::write(dir.join(super::super::USAGE_DB_FILE), b"current-db").expect("write current db");
        fs::write(dir.join(LEGACY_USAGE_DB_FILE_NAME), b"legacy-db").expect("write legacy db");
        fs::write(
            dir.join(format!("{LEGACY_USAGE_DB_FILE_NAME}-wal")),
            b"legacy-wal",
        )
        .expect("write legacy wal");
        fs::write(
            dir.join(format!("{LEGACY_USAGE_DB_FILE_NAME}-shm")),
            b"legacy-shm",
        )
        .expect("write legacy shm");

        migrate_legacy_files();

        assert_eq!(
            fs::read(dir.join(super::super::USAGE_DB_FILE)).expect("read db"),
            b"current-db",
            "an existing current db must never be overwritten"
        );
        assert_eq!(
            fs::read(dir.join(LEGACY_USAGE_DB_FILE_NAME)).expect("read legacy db"),
            b"legacy-db",
            "the legacy db stays untouched while the current db exists"
        );
        assert_eq!(
            fs::read(dir.join(format!("{LEGACY_USAGE_DB_FILE_NAME}-wal"))).expect("read legacy wal"),
            b"legacy-wal",
            "the legacy wal stays untouched while the current db exists"
        );
        assert_eq!(
            fs::read(dir.join(format!("{LEGACY_USAGE_DB_FILE_NAME}-shm"))).expect("read legacy shm"),
            b"legacy-shm",
            "the legacy shm stays untouched while the current db exists"
        );
        assert!(
            !dir.join(format!("{}-wal", super::super::USAGE_DB_FILE)).exists(),
            "a stray legacy wal must never be attached to the current db"
        );
        assert!(
            !dir.join(format!("{}-shm", super::super::USAGE_DB_FILE)).exists(),
            "a stray legacy shm must never be attached to the current db"
        );
    }

    /// Orphan sidecars without their legacy main database are never renamed: a
    /// sidecar belongs to the database file, not the other way around.
    #[test]
    fn migrate_legacy_files_does_not_rename_orphan_sidecars_without_legacy_db() {
        let _home = isolated_temp_home("orphan-sidecars");
        let dir = crate::config::get_app_dir().expect("app dir");
        fs::create_dir_all(&dir).expect("create app dir");

        fs::write(
            dir.join(format!("{LEGACY_USAGE_DB_FILE_NAME}-wal")),
            b"legacy-wal",
        )
        .expect("write legacy wal");
        fs::write(
            dir.join(format!("{LEGACY_USAGE_DB_FILE_NAME}-shm")),
            b"legacy-shm",
        )
        .expect("write legacy shm");

        migrate_legacy_files();

        assert_eq!(
            fs::read(dir.join(format!("{LEGACY_USAGE_DB_FILE_NAME}-wal"))).expect("read legacy wal"),
            b"legacy-wal",
            "an orphan legacy wal must stay untouched"
        );
        assert_eq!(
            fs::read(dir.join(format!("{LEGACY_USAGE_DB_FILE_NAME}-shm"))).expect("read legacy shm"),
            b"legacy-shm",
            "an orphan legacy shm must stay untouched"
        );
        assert!(!dir.join(format!("{}-wal", super::super::USAGE_DB_FILE)).exists());
        assert!(!dir.join(format!("{}-shm", super::super::USAGE_DB_FILE)).exists());
        assert!(!dir.join(super::super::USAGE_DB_FILE).exists());
        assert!(!dir.join(LEGACY_USAGE_DB_FILE_NAME).exists());
    }

    /// When the main database is renamed but one current sidecar already exists,
    /// only the free sidecar moves: the current sidecar keeps its content and
    /// the legacy counterpart is retained rather than overwriting it.
    #[test]
    fn migrate_legacy_files_keeps_current_sidecar_when_both_sidecars_exist() {
        let _home = isolated_temp_home("conflicting-sidecars");
        let dir = crate::config::get_app_dir().expect("app dir");
        fs::create_dir_all(&dir).expect("create app dir");

        fs::write(dir.join(LEGACY_USAGE_DB_FILE_NAME), b"legacy-db").expect("write legacy db");
        fs::write(
            dir.join(format!("{LEGACY_USAGE_DB_FILE_NAME}-wal")),
            b"legacy-wal",
        )
        .expect("write legacy wal");
        fs::write(
            dir.join(format!("{}-wal", super::super::USAGE_DB_FILE)),
            b"current-wal",
        )
        .expect("write current wal");

        migrate_legacy_files();

        assert_eq!(
            fs::read(dir.join(super::super::USAGE_DB_FILE)).expect("read db"),
            b"legacy-db",
            "the main db is renamed because no current db exists"
        );
        assert_eq!(
            fs::read(dir.join(format!("{}-wal", super::super::USAGE_DB_FILE))).expect("read wal"),
            b"current-wal",
            "the existing current wal must never be overwritten"
        );
        assert_eq!(
            fs::read(dir.join(format!("{LEGACY_USAGE_DB_FILE_NAME}-wal"))).expect("read legacy wal"),
            b"legacy-wal",
            "the legacy wal is retained when its current target already exists"
        );
        assert!(
            !dir.join(format!("{LEGACY_USAGE_DB_FILE_NAME}-shm")).exists(),
            "a legacy shm that never existed must not be created"
        );
        assert!(
            !dir.join(format!("{}-shm", super::super::USAGE_DB_FILE)).exists(),
            "a current shm must not be created for a wal-only fixture"
        );
    }

    #[test]
    fn has_legacy_gateway_marker_recognizes_legacy_shapes_only() {
        let legacy_top = json!({ LEGACY_GATEWAY_MARKER_KEY: true });
        assert!(has_legacy_gateway_marker(&legacy_top));
        let legacy_tool_config = json!({ "tool_config": { LEGACY_GATEWAY_MARKER_KEY: true } });
        assert!(has_legacy_gateway_marker(&legacy_tool_config));

        assert!(!has_legacy_gateway_marker(&json!({ "ai_gateway_gateway": true })));
        assert!(!has_legacy_gateway_marker(&json!({ LEGACY_GATEWAY_MARKER_KEY: false })));
        assert!(!has_legacy_gateway_marker(&json!({
            "tool_config": { "npm": "@ai-sdk/openai-compatible" }
        })));
    }
}

#[cfg(test)]
mod proxy_switch_named {
    #[test]
    fn gateway_schema_5_to_6_removes_only_gateway_proxy_url() {
        use serde_json::json;
        use std::fs;
        let dir = std::env::temp_dir().join(format!(
            "onespace-mig5to6-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).unwrap();
        let _guard = crate::config::test_home::TestHomeGuard::set(&dir);
        let cfg = json!({
            "schema_version": 5,
            "enabled": true,
            "settings": {
                "proxy_url": "http://old-proxy.example",
                "bind_address": "127.0.0.1",
                "enable_cors": true
            },
            "providers": [
                {
                    "id": "unbound",
                    "name": "Unbound",
                    "base_url": "https://unbound.example.com/v1",
                    "api_key": "sk-unbound",
                    "protocol": "chat_completions",
                    "mappings": []
                }
            ],
            "keys": [{"id":"k1","label":"K","value":"v","enabled":true,"created_at":1}],
            "default_key_id": "k1",
            "model_pools": [],
            "model_prices": [],
            "templates": []
        });
        let password = crate::crypto::get_or_init_master_password().unwrap();
        let encrypted = crate::crypto::encrypt(&cfg.to_string(), &password).unwrap();
        let path = crate::ai_gateway::storage::config_path().unwrap();
        fs::write(&path, encrypted).unwrap();

        let loaded = crate::ai_gateway::storage::read_config().unwrap();
        assert_eq!(loaded.schema_version, 6);
        assert_eq!(loaded.providers.len(), 1);
        assert_eq!(loaded.providers[0].id, "unbound");
        assert!(loaded.providers[0].template_id.is_none());

        // on-disk proxy_url removed
        let on_disk = {
            let raw = fs::read_to_string(&path).unwrap();
            let dec = crate::crypto::decrypt(raw.trim(), &password).unwrap();
            serde_json::from_str::<serde_json::Value>(&dec).unwrap()
        };
        assert!(on_disk["settings"].get("proxy_url").is_none());

        // repeated read does not rewrite bytes
        let bytes_after_first = fs::read(&path).unwrap();
        let _ = crate::ai_gateway::storage::read_config().unwrap();
        let bytes_after_second = fs::read(&path).unwrap();
        assert_eq!(bytes_after_second, bytes_after_first);

        // future schema number is not downgraded
        let future = json!({"schema_version": 99, "enabled": true, "providers": [], "keys": [], "default_key_id": null, "model_pools": [], "model_prices": [], "templates": []});
        let enc_future = crate::crypto::encrypt(&future.to_string(), &password).unwrap();
        fs::write(&path, enc_future).unwrap();
        let bytes_future_before = fs::read(&path).unwrap();
        let loaded_future = crate::ai_gateway::storage::read_config().unwrap();
        assert_eq!(loaded_future.schema_version, 99);
        let bytes_future_after = fs::read(&path).unwrap();
        assert_eq!(bytes_future_after, bytes_future_before);

        let _ = fs::remove_dir_all(&dir);
    }
}
