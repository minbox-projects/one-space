//! Behavior tests for pool-owned reasoning policy persistence (Step 1 of
//! `20261015-pool-only-reasoning`: REQ-001 / REQ-002, the config half of
//! AC-001..AC-004, and AC-012).
//!
//! Boundary asserted here: the existing public pool upsert
//! (`commands::ai_gateway_upsert_model_pool`) followed by the encrypted reload
//! (`read_raw_gateway_config`). Only already-compiled entry points and JSON
//! fixtures are used, so a RED is an observable behavior failure (a dropped
//! policy field, an accepted invalid save, or an unrepaired concrete default)
//! rather than a missing Rust symbol.
//!
//! Policy placement is pool-only and additive under schema 3:
//!
//! - pool `default_reasoning_effort`: one of `low | medium | high | xhigh |
//!   none | auto`; a missing persisted value reads as `auto` and is never sent
//!   as an effort string.
//! - candidate `reasoning`: optional object with `profile` (one of the bounded
//!   shapes), an optional declared `values` enum set, an optional
//!   `budget { min, max }` range, an optional `supports_disable` flag and a
//!   `mappings` array of `{ effort, action }` where `action` is
//!   `{ kind: "enum", value }`, `{ kind: "budget", budget_tokens }` or
//!   `{ kind: "disable" }`.
//!
//! `GatewayUpstreamProvider` and `ModelMapping` gain no persistent field: every
//! policy value lives under `model_pools[*]`.

use super::{
    call_gateway, default_usage_store, free_port, pool_key, pool_provider, read_raw_gateway_config,
    spawn_mock_upstream, temp_home, wait_for_usage_logs, write_raw_gateway_config, CapturedLog,
    MockReply,
};
use crate::ai_gateway::commands::{ai_gateway_model_pools_status, ai_gateway_upsert_model_pool};
use crate::ai_gateway::runtime_http::{start_server, stop_server};
use crate::ai_gateway::storage::{config_path, modify_config};
use crate::ai_gateway::{
    GatewayConfig, ModelPool, UsageLogRecord, UsageResult, GATEWAY_CONFIG_SCHEMA_VERSION,
};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};

// ---------------------------------------------------------------------------
// Fixture builders (the stable serialized policy shape Backend must implement)
// ---------------------------------------------------------------------------

/// One raw provider mapping row carrying its trimmed pair and pinned protocol.
fn mapping(local: &str, upstream: &str, protocol: &str) -> Value {
    json!({
        "local_model": local,
        "upstream_model": upstream,
        "protocol": protocol,
        "enabled": true,
    })
}

/// One raw provider mapping row that also carries source `reasoning_efforts`,
/// used to prove a source label never becomes a pool-owned mapping.
fn mapping_with_efforts(local: &str, upstream: &str, protocol: &str, efforts: Vec<&str>) -> Value {
    let mut value = mapping(local, upstream, protocol);
    value["reasoning_efforts"] = json!(efforts);
    value
}

/// A schema-3 raw configuration with one local relay key plus the given
/// providers, prices and pools. Written raw so the (still-unknown) policy fields
/// survive the fixture round trip at baseline.
fn config_with(providers: Vec<Value>, prices: Vec<Value>, pools: Vec<Value>) -> Value {
    json!({
        "schema_version": GATEWAY_CONFIG_SCHEMA_VERSION,
        "port": 0,
        "keys": [{
            "id": "k-local",
            "label": "k-local",
            "value": "local-key",
            "enabled": true,
            "created_at": 1,
        }],
        "providers": providers,
        "model_prices": prices,
        "model_pools": pools,
        "model_pool_reserved_models": [],
    })
}

/// The shared provider every pool references. Its mappings span Chat Completions,
/// Responses and Anthropic Messages so every bounded profile has a supported
/// target.
fn shared_provider() -> Value {
    pool_provider(
        "p-shared",
        "Shared",
        "https://shared.example.invalid/v1",
        vec![pool_key("key-shared", "Shared", "sk-shared", true)],
        vec![
            mapping("local-chat", "remote-chat", "chat_completions"),
            mapping("local-switch", "remote-switch", "chat_completions"),
            mapping("local-resp", "remote-resp", "responses"),
            mapping("local-anth", "remote-anth", "anthropic_messages"),
            mapping("local-anthb", "remote-anthb", "anthropic_messages"),
        ],
    )
}

fn scoped_price(upstream_model: &str) -> Value {
    json!({
        "provider_id": "p-shared",
        "upstream_model": upstream_model,
        "input": 1.0,
        "cache_read": 0.0,
        "cache_write": 0.0,
        "output": 2.0,
    })
}

/// One raw pool candidate; `reasoning` is omitted to express unknown
/// capabilities.
fn candidate(
    provider_id: &str,
    local: &str,
    upstream: &str,
    protocol: &str,
    reasoning: Option<Value>,
) -> Value {
    let mut value = json!({
        "provider_id": provider_id,
        "local_model": local,
        "upstream_model": upstream,
        "protocol": protocol,
        "weight": 1,
    });
    if let Some(reasoning) = reasoning {
        value["reasoning"] = reasoning;
    }
    value
}

/// One candidate-local declared profile plus its public-to-native mappings.
fn reasoning(
    profile: &str,
    values: Option<Vec<&str>>,
    budget: Option<(u32, u32)>,
    supports_disable: bool,
    mappings: Vec<Value>,
) -> Value {
    let mut value = json!({ "profile": profile, "mappings": mappings });
    if let Some(values) = values {
        value["values"] = json!(values);
    }
    if let Some((min, max)) = budget {
        value["budget"] = json!({ "min": min, "max": max });
    }
    if supports_disable {
        value["supports_disable"] = json!(true);
    }
    value
}

fn enum_mapping(effort: &str, native_value: &str) -> Value {
    json!({ "effort": effort, "action": { "kind": "enum", "value": native_value } })
}

fn budget_mapping(effort: &str, budget_tokens: u32) -> Value {
    json!({ "effort": effort, "action": { "kind": "budget", "budget_tokens": budget_tokens } })
}

fn disable_mapping(effort: &str) -> Value {
    json!({ "effort": effort, "action": { "kind": "disable" } })
}

/// One raw pool; `default_effort` is omitted to express a legacy pool whose
/// persisted default must read as `auto`.
fn pool(
    id: &str,
    name: &str,
    model: &str,
    default_effort: Option<&str>,
    candidates: Vec<Value>,
) -> Value {
    let mut value = json!({
        "id": id,
        "name": name,
        "model": model,
        "enabled": true,
        "candidates": candidates,
    });
    if let Some(effort) = default_effort {
        value["default_reasoning_effort"] = json!(effort);
    }
    value
}

fn valid_chat_reasoning() -> Value {
    reasoning(
        "openai_effort",
        Some(vec!["low", "medium", "high"]),
        None,
        true,
        vec![
            enum_mapping("low", "low"),
            enum_mapping("medium", "medium"),
            enum_mapping("high", "high"),
            disable_mapping("none"),
        ],
    )
}

/// Drive the public pool upsert through the typed boundary.
fn upsert(value: Value) -> Result<GatewayConfig, String> {
    let pool: ModelPool = serde_json::from_value(value).expect("pool fixture must deserialize");
    ai_gateway_upsert_model_pool(pool)
}

/// The encrypted on-disk configuration bytes, so `preserve the encrypted bytes`
/// is asserted on the real ciphertext rather than a re-parsed value.
fn encrypted_config_bytes() -> String {
    std::fs::read_to_string(config_path().expect("config path")).expect("read encrypted config")
}

/// Force the fixture through the typed write path once so provider/price
/// snapshots compare against the canonical serialization.
fn canonicalize() {
    modify_config::<()>(|config| {
        config.keys.push(super::key_named("k-warmup", "warmup"));
        Ok((true, ()))
    })
    .expect("canonicalize the fixture through the typed write path");
}

fn pool_of<'a>(raw: &'a Value, id: &str) -> &'a Value {
    raw["model_pools"]
        .as_array()
        .expect("pool array")
        .iter()
        .find(|pool| pool["id"] == id)
        .unwrap_or_else(|| panic!("missing pool '{id}': {raw}"))
}

fn mappings_of(candidate: &Value) -> Vec<Value> {
    candidate["reasoning"]["mappings"]
        .as_array()
        .expect("reasoning mappings")
        .clone()
}

fn action_of(candidate: &Value, effort: &str) -> Value {
    mappings_of(candidate)
        .into_iter()
        .find(|mapping| mapping["effort"] == effort)
        .map(|mapping| mapping["action"].clone())
        .unwrap_or_else(|| panic!("missing mapping for '{effort}': {candidate}"))
}

// ---------------------------------------------------------------------------
// AC-003 / AC-012: two-pool independent round trip, provider/price unchanged
// ---------------------------------------------------------------------------

/// AC-003 / AC-012: two pools referencing the same provider round-trip their own
/// enum / adaptive / manual-budget / switch profiles and many-to-one positive
/// mappings independently, while the referenced provider and price table stay
/// byte-for-byte unchanged by pool CRUD.
#[test]
fn pool_reasoning_policy_round_trips_independently_for_two_pools() {
    super::with_temp_home("pool-reasoning-roundtrip", |_home| {
        write_raw_gateway_config(&config_with(
            vec![shared_provider()],
            vec![scoped_price("remote-chat")],
            vec![],
        ));
        canonicalize();
        let baseline = read_raw_gateway_config();
        let providers_baseline = baseline["providers"].clone();
        let prices_baseline = baseline["model_prices"].clone();

        let pool_a = pool(
            "pool-a",
            "A",
            "free-a",
            Some("high"),
            vec![
                // OpenAI effort on Chat: enum values plus an explicit disable.
                candidate(
                    "p-shared",
                    "local-chat",
                    "remote-chat",
                    "chat_completions",
                    Some(reasoning(
                        "openai_effort",
                        Some(vec!["low", "medium", "high"]),
                        None,
                        true,
                        vec![
                            enum_mapping("low", "low"),
                            enum_mapping("medium", "medium"),
                            enum_mapping("high", "high"),
                            disable_mapping("none"),
                        ],
                    )),
                ),
                // Chat thinking switch: intensity-less, many-to-one positives.
                candidate(
                    "p-shared",
                    "local-switch",
                    "remote-switch",
                    "chat_completions",
                    Some(reasoning(
                        "chat_thinking_switch",
                        Some(vec!["enabled"]),
                        None,
                        true,
                        vec![
                            enum_mapping("low", "enabled"),
                            enum_mapping("high", "enabled"),
                            disable_mapping("none"),
                        ],
                    )),
                ),
            ],
        );
        let pool_b = pool(
            "pool-b",
            "B",
            "free-b",
            Some("medium"),
            vec![
                // OpenAI effort on Responses.
                candidate(
                    "p-shared",
                    "local-resp",
                    "remote-resp",
                    "responses",
                    Some(reasoning(
                        "openai_effort",
                        Some(vec!["minimal", "medium", "high"]),
                        None,
                        false,
                        vec![
                            enum_mapping("medium", "medium"),
                            enum_mapping("high", "high"),
                        ],
                    )),
                ),
                // Anthropic adaptive thinking.
                candidate(
                    "p-shared",
                    "local-anth",
                    "remote-anth",
                    "anthropic_messages",
                    Some(reasoning(
                        "anthropic_effort_adaptive",
                        Some(vec!["low", "high"]),
                        None,
                        true,
                        vec![enum_mapping("high", "high"), disable_mapping("none")],
                    )),
                ),
                // Anthropic manual budget.
                candidate(
                    "p-shared",
                    "local-anthb",
                    "remote-anthb",
                    "anthropic_messages",
                    Some(reasoning(
                        "anthropic_manual_budget",
                        None,
                        Some((1024, 8192)),
                        true,
                        vec![
                            budget_mapping("medium", 2048),
                            budget_mapping("high", 8192),
                            disable_mapping("none"),
                        ],
                    )),
                ),
            ],
        );

        upsert(pool_a).expect("pool A must save");
        upsert(pool_b).expect("pool B must save");

        let raw = read_raw_gateway_config();
        assert_eq!(
            raw["model_pools"].as_array().expect("pool array").len(),
            2,
            "exactly the two pools must persist: {raw}"
        );

        let a = pool_of(&raw, "pool-a");
        assert_eq!(
            a["default_reasoning_effort"], "high",
            "pool A's concrete default must round-trip: {a}"
        );
        assert_eq!(
            a["candidates"][0]["reasoning"]["profile"], "openai_effort",
            "pool A's enum profile must round-trip: {a}"
        );
        assert_eq!(
            a["candidates"][0]["reasoning"]["values"]
                .as_array()
                .expect("declared values")
                .len(),
            3,
            "pool A's declared native value set must round-trip: {a}"
        );
        assert_eq!(
            action_of(&a["candidates"][0], "high")["value"], "high",
            "pool A's positive mapping must round-trip: {a}"
        );
        assert_eq!(
            action_of(&a["candidates"][0], "none")["kind"], "disable",
            "pool A's explicit disable mapping must round-trip: {a}"
        );
        assert_eq!(
            a["candidates"][1]["reasoning"]["profile"], "chat_thinking_switch",
            "pool A's switch profile must round-trip: {a}"
        );
        assert_eq!(
            action_of(&a["candidates"][1], "low")["value"],
            action_of(&a["candidates"][1], "high")["value"],
            "two positive preferences must share one positive native action: {a}"
        );

        let b = pool_of(&raw, "pool-b");
        assert_eq!(
            b["default_reasoning_effort"], "medium",
            "pool B's default must stay independent from pool A: {b}"
        );
        assert_eq!(
            b["candidates"][0]["reasoning"]["values"][0], "minimal",
            "pool B's own value set must not be rewritten by pool A: {b}"
        );
        assert_eq!(
            b["candidates"][1]["reasoning"]["profile"], "anthropic_effort_adaptive",
            "pool B's adaptive profile must round-trip: {b}"
        );
        assert_eq!(
            b["candidates"][2]["reasoning"]["profile"], "anthropic_manual_budget",
            "pool B's manual-budget profile must round-trip: {b}"
        );
        assert_eq!(
            b["candidates"][2]["reasoning"]["budget"]["min"], 1024,
            "pool B's budget range must round-trip: {b}"
        );
        assert_eq!(
            action_of(&b["candidates"][2], "high")["budget_tokens"], 8192,
            "pool B's budget mapping must round-trip: {b}"
        );

        assert_eq!(
            raw["providers"], providers_baseline,
            "pool CRUD must not change any provider/source value: {raw}"
        );
        assert_eq!(
            raw["model_prices"], prices_baseline,
            "pool CRUD must not change the price table: {raw}"
        );
    });
}

// ---------------------------------------------------------------------------
// AC-001 config half / AC-004: invalid policy is rejected atomically
// ---------------------------------------------------------------------------

/// AC-001 (default set) / AC-004 / AC-002: every invalid default, profile,
/// native value, disable action or budget is rejected by the public upsert, and
/// a rejected save preserves the encrypted bytes so the sibling pool and the
/// source configuration are untouched.
#[test]
fn pool_reasoning_rejects_invalid_policy_without_changing_encrypted_bytes() {
    super::with_temp_home("pool-reasoning-atomic-reject", |_home| {
        // A resolvable Anthropic mapping so the budget-profile references pass
        // the baseline reference check; the reasoning policy is what must fail.
        let provider = pool_provider(
            "p-shared",
            "Shared",
            "https://shared.example.invalid/v1",
            vec![pool_key("key-shared", "Shared", "sk-shared", true)],
            vec![
                mapping_with_efforts(
                    "local-chat",
                    "remote-chat",
                    "chat_completions",
                    vec!["low", "high"],
                ),
                mapping("local-anthb", "remote-anthb", "anthropic_messages"),
            ],
        );
        let seeded = pool(
            "pool-seed",
            "Seeded",
            "free-seed",
            Some("medium"),
            vec![candidate(
                "p-shared",
                "local-chat",
                "remote-chat",
                "chat_completions",
                Some(valid_chat_reasoning()),
            )],
        );
        let other = pool(
            "pool-other",
            "Other",
            "free-other",
            Some("high"),
            vec![candidate(
                "p-shared",
                "local-chat",
                "remote-chat",
                "chat_completions",
                Some(valid_chat_reasoning()),
            )],
        );
        write_raw_gateway_config(&config_with(vec![provider], vec![], vec![seeded, other]));
        let baseline_bytes = encrypted_config_bytes();

        let assert_rejected = |value: Value, label: &str| {
            let result = upsert(value);
            assert!(
                result.is_err(),
                "{label}: an invalid pool reasoning policy must be rejected"
            );
            assert_eq!(
                encrypted_config_bytes(),
                baseline_bytes,
                "{label}: a rejected save must preserve the encrypted bytes"
            );
        };

        let chat = |reasoning: Value| {
            candidate(
                "p-shared",
                "local-chat",
                "remote-chat",
                "chat_completions",
                Some(reasoning),
            )
        };
        let anth = |reasoning: Value| {
            candidate(
                "p-shared",
                "local-anthb",
                "remote-anthb",
                "anthropic_messages",
                Some(reasoning),
            )
        };
        let seed = |default_effort: &str, candidates: Vec<Value>| {
            pool("pool-seed", "Seeded", "free-seed", Some(default_effort), candidates)
        };

        // An out-of-set default is not a canonical preference.
        assert_rejected(
            seed("max", vec![chat(valid_chat_reasoning())]),
            "unknown default",
        );
        // A changed concrete default with no valid mapping.
        assert_rejected(
            seed(
                "medium",
                vec![chat(reasoning(
                    "openai_effort",
                    Some(vec!["low", "medium", "high"]),
                    None,
                    true,
                    vec![enum_mapping("high", "high"), disable_mapping("none")],
                ))],
            ),
            "default without a mapping",
        );
        // An unknown profile is never inferred.
        assert_rejected(
            seed(
                "medium",
                vec![chat(reasoning(
                    "gemini_effort",
                    Some(vec!["low"]),
                    None,
                    false,
                    vec![enum_mapping("medium", "low")],
                ))],
            ),
            "unknown profile",
        );
        // A native enum target outside the declared value set.
        assert_rejected(
            seed(
                "medium",
                vec![chat(reasoning(
                    "openai_effort",
                    Some(vec!["low", "medium", "high"]),
                    None,
                    false,
                    vec![
                        enum_mapping("medium", "medium"),
                        enum_mapping("high", "ultra"),
                    ],
                ))],
            ),
            "enum target outside declared set",
        );
        // A positive preference must never map to disabled reasoning.
        assert_rejected(
            seed(
                "medium",
                vec![chat(reasoning(
                    "openai_effort",
                    Some(vec!["low", "medium", "high"]),
                    None,
                    true,
                    vec![
                        disable_mapping("medium"),
                        enum_mapping("high", "high"),
                        disable_mapping("none"),
                    ],
                ))],
            ),
            "positive mapped to disable",
        );
        // `none` may only map to an explicit disable action.
        assert_rejected(
            seed(
                "medium",
                vec![chat(reasoning(
                    "openai_effort",
                    Some(vec!["low", "medium", "high"]),
                    None,
                    true,
                    vec![
                        enum_mapping("medium", "medium"),
                        enum_mapping("high", "high"),
                        enum_mapping("none", "low"),
                    ],
                ))],
            ),
            "none mapped to a positive value",
        );
        // A disable mapping needs declared disable support.
        assert_rejected(
            seed(
                "medium",
                vec![chat(reasoning(
                    "openai_effort",
                    Some(vec!["low", "medium", "high"]),
                    None,
                    false,
                    vec![
                        enum_mapping("medium", "medium"),
                        enum_mapping("high", "high"),
                        disable_mapping("none"),
                    ],
                ))],
            ),
            "disable mapping without declared support",
        );
        // An internally inconsistent budget range.
        assert_rejected(
            seed(
                "medium",
                vec![anth(reasoning(
                    "anthropic_manual_budget",
                    None,
                    Some((2048, 1024)),
                    true,
                    vec![
                        budget_mapping("medium", 2048),
                        disable_mapping("none"),
                    ],
                ))],
            ),
            "inverted budget range",
        );
        // A budget below the adapter minimum.
        assert_rejected(
            seed(
                "medium",
                vec![anth(reasoning(
                    "anthropic_manual_budget",
                    None,
                    Some((512, 4096)),
                    true,
                    vec![
                        budget_mapping("medium", 2048),
                        disable_mapping("none"),
                    ],
                ))],
            ),
            "budget below the adapter minimum",
        );
        // A budget mapping outside the declared range.
        assert_rejected(
            seed(
                "medium",
                vec![anth(reasoning(
                    "anthropic_manual_budget",
                    None,
                    Some((1024, 4096)),
                    true,
                    vec![
                        budget_mapping("medium", 8192),
                        disable_mapping("none"),
                    ],
                ))],
            ),
            "budget token outside declared range",
        );
        // A source `reasoning_efforts` label never becomes a pool mapping.
        assert_rejected(
            seed(
                "high",
                vec![candidate(
                    "p-shared",
                    "local-chat",
                    "remote-chat",
                    "chat_completions",
                    None,
                )],
            ),
            "unknown capabilities are not inferred from source effort labels",
        );

        // The sibling pool and the seeded pool are still present and untouched.
        let raw = read_raw_gateway_config();
        assert!(
            raw["model_pools"]
                .as_array()
                .expect("pool array")
                .iter()
                .any(|pool| pool["id"] == "pool-other"),
            "the sibling pool must be untouched by rejected saves: {raw}"
        );
        assert!(
            raw["model_pools"]
                .as_array()
                .expect("pool array")
                .iter()
                .any(|pool| pool["id"] == "pool-seed"),
            "the seeded pool must survive rejected saves: {raw}"
        );
    });
}

// ---------------------------------------------------------------------------
// AC-002: last concrete default mapping needs a same-save repair
// ---------------------------------------------------------------------------

/// AC-002: deleting the retained concrete default's last mapping is rejected
/// unless the same save selects `auto` or another default with a valid mapping.
#[test]
fn pool_reasoning_last_concrete_default_mapping_requires_same_save_repair() {
    super::with_temp_home("pool-reasoning-last-mapping", |_home| {
        let provider = pool_provider(
            "p-shared",
            "Shared",
            "https://shared.example.invalid/v1",
            vec![pool_key("key-shared", "Shared", "sk-shared", true)],
            vec![mapping("local-chat", "remote-chat", "chat_completions")],
        );
        let seeded = pool(
            "pool-seed",
            "Seeded",
            "free-seed",
            Some("medium"),
            vec![candidate(
                "p-shared",
                "local-chat",
                "remote-chat",
                "chat_completions",
                Some(reasoning(
                    "openai_effort",
                    Some(vec!["low", "medium", "high"]),
                    None,
                    true,
                    vec![
                        enum_mapping("medium", "medium"),
                        enum_mapping("high", "high"),
                        disable_mapping("none"),
                    ],
                )),
            )],
        );
        write_raw_gateway_config(&config_with(vec![provider], vec![], vec![seeded]));
        let baseline_bytes = encrypted_config_bytes();

        // `medium` removed while the retained default stays `medium`: reject.
        let removed = vec![
            enum_mapping("high", "high"),
            disable_mapping("none"),
        ];
        let rejected = upsert(pool(
            "pool-seed",
            "Seeded",
            "free-seed",
            Some("medium"),
            vec![candidate(
                "p-shared",
                "local-chat",
                "remote-chat",
                "chat_completions",
                Some(reasoning(
                    "openai_effort",
                    Some(vec!["low", "medium", "high"]),
                    None,
                    true,
                    removed.clone(),
                )),
            )],
        ));
        assert!(
            rejected.is_err(),
            "removing the last mapping for the retained concrete default must be rejected"
        );
        assert_eq!(
            encrypted_config_bytes(),
            baseline_bytes,
            "a rejected last-mapping save must preserve the encrypted bytes"
        );

        // The same save selecting `auto` is allowed.
        upsert(pool(
            "pool-seed",
            "Seeded",
            "free-seed",
            Some("auto"),
            vec![candidate(
                "p-shared",
                "local-chat",
                "remote-chat",
                "chat_completions",
                Some(reasoning(
                    "openai_effort",
                    Some(vec!["low", "medium", "high"]),
                    None,
                    true,
                    removed.clone(),
                )),
            )],
        ))
        .expect("selecting auto in the same save must allow the deletion");
        let raw = read_raw_gateway_config();
        assert_eq!(
            pool_of(&raw, "pool-seed")["default_reasoning_effort"], "auto",
            "the same-save auto repair must persist: {raw}"
        );

        // The same save selecting another default with a valid mapping is allowed.
        upsert(pool(
            "pool-seed",
            "Seeded",
            "free-seed",
            Some("high"),
            vec![candidate(
                "p-shared",
                "local-chat",
                "remote-chat",
                "chat_completions",
                Some(reasoning(
                    "openai_effort",
                    Some(vec!["low", "medium", "high"]),
                    None,
                    true,
                    removed,
                )),
            )],
        ))
        .expect("selecting another default with a valid mapping must allow the deletion");
        let raw = read_raw_gateway_config();
        assert_eq!(
            pool_of(&raw, "pool-seed")["default_reasoning_effort"], "high",
            "the same-save default repair must persist: {raw}"
        );
    });
}

// ---------------------------------------------------------------------------
// AC-002 / AC-004: retained unavailable declarations and the legacy default
// ---------------------------------------------------------------------------

/// AC-002 / AC-004: a retained declaration whose source later becomes
/// unavailable survives an unrelated edit (the pool is never guessed or
/// rewritten), and a legacy pool without the default field reads as `auto`
/// without inferring any candidate profile.
#[test]
fn pool_reasoning_retains_unavailable_declaration_and_legacy_default() {
    super::with_temp_home("pool-reasoning-retained", |_home| {
        let provider = pool_provider(
            "p-shared",
            "Shared",
            "https://shared.example.invalid/v1",
            vec![pool_key("key-shared", "Shared", "sk-shared", true)],
            vec![mapping("local-a", "remote-a", "chat_completions")],
        );
        let retained = pool(
            "pool-ret",
            "Retained",
            "free-ret",
            Some("high"),
            vec![candidate(
                "p-shared",
                "local-a",
                "remote-a",
                "chat_completions",
                Some(valid_chat_reasoning()),
            )],
        );
        let legacy = pool(
            "pool-legacy",
            "Legacy",
            "free-legacy",
            None,
            vec![candidate(
                "p-shared",
                "local-a",
                "remote-a",
                "chat_completions",
                None,
            )],
        );
        write_raw_gateway_config(&config_with(
            vec![provider],
            vec![],
            vec![retained.clone(), legacy.clone()],
        ));

        // The referenced mapping later disappears, so the stored declaration is
        // unavailable but must be retained, not guessed or rewritten.
        let provider_without_mapping = pool_provider(
            "p-shared",
            "Shared",
            "https://shared.example.invalid/v1",
            vec![pool_key("key-shared", "Shared", "sk-shared", true)],
            vec![],
        );
        write_raw_gateway_config(&config_with(
            vec![provider_without_mapping],
            vec![],
            vec![retained.clone(), legacy.clone()],
        ));

        // An unrelated edit (a rename) must preserve the retained declaration.
        let mut renamed = retained.clone();
        renamed["name"] = json!("Retained v2");
        upsert(renamed).expect("an unrelated edit of a retained declaration must save");

        let raw = read_raw_gateway_config();
        let pool = pool_of(&raw, "pool-ret");
        assert_eq!(
            pool["name"], "Retained v2",
            "the unrelated edit must persist: {raw}"
        );
        assert_eq!(
            pool["default_reasoning_effort"], "high",
            "the retained concrete default must survive an unrelated edit: {raw}"
        );
        assert_eq!(
            pool["candidates"][0]["reasoning"]["profile"], "openai_effort",
            "the retained profile must survive an unrelated edit: {raw}"
        );
        assert_eq!(
            mappings_of(&pool["candidates"][0]).len(),
            4,
            "the retained mappings must survive an unrelated edit: {raw}"
        );
        assert_eq!(
            pool["candidates"][0]["local_model"], "local-a",
            "the retained source reference must never be guessed away: {raw}"
        );

        // A legacy pool without the default field reads as `auto` and infers no
        // candidate profile from source metadata.
        upsert(legacy).expect("a legacy pool must save unchanged");
        let raw = read_raw_gateway_config();
        let legacy_pool = pool_of(&raw, "pool-legacy");
        assert_eq!(
            legacy_pool["default_reasoning_effort"], "auto",
            "a missing persisted default must read and persist as auto: {raw}"
        );
        assert!(
            legacy_pool["candidates"][0].get("reasoning").is_none(),
            "an undeclared candidate must not gain an inferred profile: {raw}"
        );
    });
}

// ---------------------------------------------------------------------------
// Acceptance-gap repair: changed-default validity, positive omission targets,
// switch value and adaptive/manual budget exclusivity
// ---------------------------------------------------------------------------

/// REQ-001 / AC-002: a *new or changed* concrete default requires at least one
/// currently valid mapping. A retained declaration whose source later disappears
/// or changes protocol keeps its own stored default, but the same retained
/// declaration may not back a different concrete default introduced by this save.
#[test]
fn pool_reasoning_rejects_changed_default_without_a_currently_valid_mapping() {
    super::with_temp_home("pool-reasoning-changed-default", |_home| {
        // The stored pool keeps `auto`; its `high` declaration is retained even
        // though the referenced source is about to become unavailable.
        let retained_reasoning = reasoning(
            "openai_effort",
            Some(vec!["low", "medium", "high"]),
            None,
            true,
            vec![
                enum_mapping("low", "low"),
                enum_mapping("medium", "medium"),
                enum_mapping("high", "high"),
                disable_mapping("none"),
            ],
        );
        let stored = pool(
            "pool-ret",
            "Retained",
            "free-ret",
            Some("auto"),
            vec![candidate(
                "p-shared",
                "local-a",
                "remote-a",
                "chat_completions",
                Some(retained_reasoning.clone()),
            )],
        );
        // The same save changes the pool default from `auto` to a concrete `high`
        // while reusing the retained (unavailable) declaration.
        let changed_default = pool(
            "pool-ret",
            "Retained",
            "free-ret",
            Some("high"),
            vec![candidate(
                "p-shared",
                "local-a",
                "remote-a",
                "chat_completions",
                Some(retained_reasoning.clone()),
            )],
        );

        let mut failures: Vec<String> = Vec::new();
        let check_rejected =
            |value: Value, label: &str, baseline: &str, failures: &mut Vec<String>| {
                match upsert(value) {
                    Ok(_) => failures.push(format!("{label}: invalid policy was accepted")),
                    Err(_) => {
                        if encrypted_config_bytes() != baseline {
                            failures.push(format!(
                                "{label}: a rejected save changed the encrypted bytes"
                            ));
                        }
                    }
                }
            };

        // The referenced mapping disappeared entirely, so no currently valid
        // mapping can back the newly introduced concrete default.
        let missing = pool_provider(
            "p-shared",
            "Shared",
            "https://shared.example.invalid/v1",
            vec![pool_key("key-shared", "Shared", "sk-shared", true)],
            vec![],
        );
        write_raw_gateway_config(&config_with(vec![missing], vec![], vec![stored.clone()]));
        let baseline_bytes = encrypted_config_bytes();
        check_rejected(
            changed_default.clone(),
            "changed default backed by a missing retained source",
            &baseline_bytes,
            &mut failures,
        );

        // The source row changed protocol, so the stored reference no longer
        // resolves; a changed concrete default still cannot rely on it.
        let reprotocolled = pool_provider(
            "p-shared",
            "Shared",
            "https://shared.example.invalid/v1",
            vec![pool_key("key-shared", "Shared", "sk-shared", true)],
            vec![mapping("local-a", "remote-a", "responses")],
        );
        write_raw_gateway_config(&config_with(
            vec![reprotocolled],
            vec![],
            vec![stored.clone()],
        ));
        let baseline_bytes = encrypted_config_bytes();
        check_rejected(
            changed_default,
            "changed default backed by a protocol-changed retained source",
            &baseline_bytes,
            &mut failures,
        );

        assert!(
            failures.is_empty(),
            "a new or changed concrete default requires a currently valid mapping: {failures:?}"
        );
    });
}

/// REQ-002 / AC-004: a positive preference must never target a native value that
/// disables or omits reasoning even when it is self-declared, a switch profile's
/// only positive native value is `enabled`, and an adaptive thinking profile may
/// not also declare the manual-budget range that makes it a manual combination.
#[test]
fn pool_reasoning_rejects_positive_omission_switch_and_adaptive_budget() {
    super::with_temp_home("pool-reasoning-policy-boundaries", |_home| {
        let provider = pool_provider(
            "p-shared",
            "Shared",
            "https://shared.example.invalid/v1",
            vec![pool_key("key-shared", "Shared", "sk-shared", true)],
            vec![
                mapping("local-chat", "remote-chat", "chat_completions"),
                mapping("local-switch", "remote-switch", "chat_completions"),
                mapping("local-anth", "remote-anth", "anthropic_messages"),
            ],
        );
        let seeded = pool(
            "pool-seed",
            "Seeded",
            "free-seed",
            Some("medium"),
            vec![candidate(
                "p-shared",
                "local-chat",
                "remote-chat",
                "chat_completions",
                Some(valid_chat_reasoning()),
            )],
        );
        write_raw_gateway_config(&config_with(vec![provider], vec![], vec![seeded]));
        let baseline_bytes = encrypted_config_bytes();

        let mut failures: Vec<String> = Vec::new();
        let assert_rejected =
            |value: Value, label: &str, failures: &mut Vec<String>| match upsert(value) {
                Ok(_) => failures.push(format!("{label}: invalid policy was accepted")),
                Err(_) => {
                    if encrypted_config_bytes() != baseline_bytes {
                        failures.push(format!(
                            "{label}: a rejected save changed the encrypted bytes"
                        ));
                    }
                }
            };
        let chat = |local: &str, upstream: &str, reasoning: Value| {
            candidate(
                "p-shared",
                local,
                upstream,
                "chat_completions",
                Some(reasoning),
            )
        };
        let seed = |default_effort: &str, candidates: Vec<Value>| {
            pool(
                "pool-seed",
                "Seeded",
                "free-seed",
                Some(default_effort),
                candidates,
            )
        };

        // A positive preference targeting the native `none` value disables
        // reasoning even though `none` is self-declared.
        assert_rejected(
            seed(
                "medium",
                vec![chat(
                    "local-chat",
                    "remote-chat",
                    reasoning(
                        "openai_effort",
                        Some(vec!["none", "low", "medium", "high"]),
                        None,
                        true,
                        vec![
                            enum_mapping("medium", "none"),
                            enum_mapping("high", "high"),
                            disable_mapping("none"),
                        ],
                    ),
                )],
            ),
            "positive preference mapped to the native 'none' value",
            &mut failures,
        );
        // A positive preference targeting a blank native value omits reasoning.
        assert_rejected(
            seed(
                "medium",
                vec![chat(
                    "local-chat",
                    "remote-chat",
                    reasoning(
                        "openai_effort",
                        Some(vec!["", "low", "medium", "high"]),
                        None,
                        true,
                        vec![
                            enum_mapping("medium", ""),
                            enum_mapping("high", "high"),
                            disable_mapping("none"),
                        ],
                    ),
                )],
            ),
            "positive preference mapped to a blank native value",
            &mut failures,
        );
        // A switch-only profile's positive value is exactly `enabled`; another
        // self-declared identifier is not a valid intensity-less positive control.
        assert_rejected(
            seed(
                "low",
                vec![chat(
                    "local-switch",
                    "remote-switch",
                    reasoning(
                        "chat_thinking_switch",
                        Some(vec!["enabled", "turbo"]),
                        None,
                        true,
                        vec![enum_mapping("low", "turbo"), disable_mapping("none")],
                    ),
                )],
            ),
            "switch positive value other than 'enabled'",
            &mut failures,
        );
        // An adaptive thinking profile may not also declare a manual budget range.
        assert_rejected(
            seed(
                "high",
                vec![candidate(
                    "p-shared",
                    "local-anth",
                    "remote-anth",
                    "anthropic_messages",
                    Some(reasoning(
                        "anthropic_effort_adaptive",
                        Some(vec!["low", "high"]),
                        Some((1024, 8192)),
                        true,
                        vec![enum_mapping("high", "high"), disable_mapping("none")],
                    )),
                )],
            ),
            "adaptive thinking profile declaring a manual budget range",
            &mut failures,
        );

        assert!(
            failures.is_empty(),
            "invalid pool reasoning boundaries were accepted: {failures:?}"
        );
    });
}

/// Guard for the repair above: a provider-specific native effort identifier that
/// is neither a canonical public tier nor an omission value (`max` here) must
/// still save as a positive enum target. Prevents blanket-banning declared
/// provider-specific identifiers.
#[test]
fn pool_reasoning_accepts_provider_specific_positive_enum_identifier() {
    super::with_temp_home("pool-reasoning-positive-guard", |_home| {
        write_raw_gateway_config(&config_with(vec![shared_provider()], vec![], vec![]));
        upsert(pool(
            "pool-guard",
            "Guard",
            "free-guard",
            Some("high"),
            vec![candidate(
                "p-shared",
                "local-chat",
                "remote-chat",
                "chat_completions",
                Some(reasoning(
                    "openai_effort",
                    Some(vec!["low", "max"]),
                    None,
                    true,
                    vec![
                        enum_mapping("low", "low"),
                        enum_mapping("high", "max"),
                        disable_mapping("none"),
                    ],
                )),
            )],
        ))
        .expect("a provider-specific positive enum identifier must remain acceptable");
        let raw = read_raw_gateway_config();
        assert_eq!(
            action_of(&pool_of(&raw, "pool-guard")["candidates"][0], "high")["value"], "max",
            "the provider-specific positive mapping must round-trip: {raw}"
        );
    });
}

// ===========================================================================
// Step 2 (20261015-pool-only-reasoning): request-time parsing, classification,
// per-attempt native controls and durable attempt evidence.
//
// These tests drive only the already-compiled public boundary: the encrypted
// configuration writer, the real loopback listener, in-process mock upstreams
// and the durable usage-log query. New policy values enter as raw JSON, so a RED
// is a compiled, observable behavior failure — a blanket rejection of a
// canonical control, a missing native field, a wrong error code or absent log
// evidence — rather than a missing Rust symbol.
//
// Proposed record contract asserted below (report to Backend):
//   * `reasoning_effort`: the raw client value, unchanged; null when unspecified.
//   * `resolved_reasoning_effort`: `high`/`medium`/`auto` (pool rows only); null
//     on ordinary, historical and synthetic no-attempt rows.
//   * `native_reasoning`: `{ protocol, mode, parameters }` where `mode` is one of
//     `enum` | `budget` | `disable` | `provider_default`, `protocol` is the
//     attempt's actual upstream protocol and `parameters` carries only the wire
//     controls actually sent (an attempted `auto` is `provider_default` with an
//     empty `parameters`; a synthetic no-attempt row has a null snapshot).
// ===========================================================================

const CHAT: &str = "/v1/chat/completions";
const RESP: &str = "/v1/responses";

/// A schema-3 raw configuration bound to the real listener `port`.
fn http_config(port: u16, providers: Vec<Value>, pools: Vec<Value>) -> Value {
    let mut config = config_with(providers, vec![], pools);
    config["port"] = json!(port);
    config
}

/// One JSON request to the loopback gateway carrying the local relay key.
async fn post_json(port: u16, path: &str, body: Value) -> (u16, String) {
    let (status, _content_type, text) = call_gateway(
        port,
        "POST",
        path,
        &[
            ("authorization", "Bearer local-key"),
            ("x-opencode-session", "sess-reasoning"),
        ],
        Some(body),
    )
    .await;
    (status, text)
}

fn error_code(text: &str) -> String {
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|value| {
            value
                .pointer("/error/code")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default()
}

fn error_text(text: &str) -> String {
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|value| {
            value
                .pointer("/error/message")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default()
}

/// The parsed body of every captured upstream attempt, in arrival order.
fn captured_bodies(log: &CapturedLog) -> Vec<Value> {
    log.lock()
        .expect("mock log")
        .iter()
        .map(|entry| serde_json::from_slice::<Value>(&entry.body).unwrap_or(Value::Null))
        .collect()
}

fn captured_len(log: &CapturedLog) -> usize {
    log.lock().expect("mock log").len()
}

fn record_json(record: &UsageLogRecord) -> Value {
    serde_json::to_value(record).expect("a usage record serializes to JSON")
}

/// One provider whose mappings span all three target protocols, so one mock
/// upstream can serve Chat, Responses and native Messages captures.
fn reasoning_provider(base_url: &str) -> Value {
    pool_provider(
        "p-shared",
        "Shared",
        base_url,
        vec![pool_key("key-shared", "Shared", "sk-shared", true)],
        vec![
            mapping("local-chat", "remote-chat", "chat_completions"),
            mapping("local-resp", "remote-resp", "responses"),
            mapping("local-anth", "remote-anth", "anthropic_messages"),
        ],
    )
}

/// An upstream that answers each protocol with a minimal success body.
async fn spawn_reasoning_upstream() -> (String, CapturedLog) {
    spawn_mock_upstream(|captured| {
        if captured.path.ends_with("/messages") {
            MockReply::Json(
                200,
                json!({
                    "id": "msg_1", "type": "message", "role": "assistant", "model": "remote",
                    "content": [{"type": "text", "text": "ok"}], "stop_reason": "end_turn",
                    "usage": {"input_tokens": 1, "output_tokens": 1},
                }),
            )
        } else if captured.path.ends_with("/responses") {
            MockReply::Json(
                200,
                json!({
                    "id": "resp_1", "object": "response", "model": "remote", "status": "completed",
                    "output": [{"id": "m1", "type": "message", "role": "assistant",
                        "status": "completed",
                        "content": [{"type": "output_text", "text": "ok", "annotations": []}]}],
                    "usage": {"input_tokens": 1, "output_tokens": 1, "total_tokens": 2},
                }),
            )
        } else {
            MockReply::Json(
                200,
                json!({
                    "id": "chatcmpl-1", "object": "chat.completion", "model": "remote",
                    "choices": [{"index": 0, "message": {"role": "assistant", "content": "ok"},
                        "finish_reason": "stop"}],
                    "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2},
                }),
            )
        }
    })
    .await
}

/// A pool with exactly one candidate, used to pin one profile/ingress pair.
fn single_candidate_pool(
    id: &str,
    alias: &str,
    local: &str,
    upstream: &str,
    protocol: &str,
    reasoning: Value,
) -> Value {
    pool(
        id,
        "Reasoning",
        alias,
        None,
        vec![candidate(
            "p-shared",
            local,
            upstream,
            protocol,
            Some(reasoning),
        )],
    )
}

/// One attempt: post, then return only the upstream body this request added.
async fn attempt(
    port: u16,
    log: &CapturedLog,
    path: &str,
    body: Value,
) -> (u16, String, Option<Value>) {
    let before = captured_len(log);
    let (status, text) = post_json(port, path, body).await;
    let bodies = captured_bodies(log);
    let captured = if bodies.len() > before {
        bodies.get(before).cloned()
    } else {
        None
    };
    (status, text, captured)
}

fn chat_body(alias: &str, extra: Value) -> Value {
    let mut body = json!({"model": alias, "messages": [{"role": "user", "content": "hi"}]});
    if let Value::Object(map) = extra {
        for (key, value) in map {
            body[key.as_str()] = value;
        }
    }
    body
}

fn responses_body(alias: &str, extra: Value) -> Value {
    let mut body = json!({
        "model": alias,
        "input": [{"role": "user", "content": [{"type": "input_text", "text": "hi"}]}],
    });
    if let Value::Object(map) = extra {
        for (key, value) in map {
            body[key.as_str()] = value;
        }
    }
    body
}

fn new_tag() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

// ---------------------------------------------------------------------------
// AC-001: both ingress effort spellings parse before any attempt
// ---------------------------------------------------------------------------

struct ParseRow {
    label: &'static str,
    path: &'static str,
    extra: Value,
    rejected: Option<&'static str>,
}

/// AC-001: canonical trimmed values are accepted on either ingress, equal Chat
/// spellings agree, and missing/null/blank mean unspecified; conflicting
/// spellings, non-strings, unknown identifiers, case variants, literal `auto`
/// and extra/opaque reasoning state are rejected with a field-named `400` before
/// any upstream attempt, scheduler, key or health change.
#[tokio::test]
async fn pool_reasoning_parses_both_ingress_effort_controls() {
    let _home = temp_home("pool-reasoning-parse");
    let port = free_port().await;
    let tag = new_tag();
    let alias = format!("free-parse-{tag}");
    let (url, log) = spawn_reasoning_upstream().await;
    let pool = single_candidate_pool(
        &format!("pool-parse-{tag}"),
        &alias,
        "local-chat",
        "remote-chat",
        "chat_completions",
        valid_chat_reasoning(),
    );
    write_raw_gateway_config(&http_config(port, vec![reasoning_provider(&url)], vec![pool]));
    start_server(None).await.expect("start the listener");

    let rows = vec![
        ParseRow { label: "chat canonical", path: CHAT, extra: json!({"reasoning_effort": "high"}), rejected: None },
        ParseRow { label: "chat padded lowercase", path: CHAT, extra: json!({"reasoning_effort": "  high  "}), rejected: None },
        ParseRow { label: "chat compat spelling", path: CHAT, extra: json!({"reasoningEffort": "high"}), rejected: None },
        ParseRow { label: "chat equal spellings", path: CHAT, extra: json!({"reasoning_effort": "high", "reasoningEffort": "high"}), rejected: None },
        ParseRow { label: "chat missing", path: CHAT, extra: json!({}), rejected: None },
        ParseRow { label: "chat null", path: CHAT, extra: json!({"reasoning_effort": null}), rejected: None },
        ParseRow { label: "chat blank", path: CHAT, extra: json!({"reasoning_effort": "   "}), rejected: None },
        ParseRow { label: "chat conflicting spellings", path: CHAT, extra: json!({"reasoning_effort": "high", "reasoningEffort": "low"}), rejected: Some("reasoning") },
        ParseRow { label: "chat non-string", path: CHAT, extra: json!({"reasoning_effort": 42}), rejected: Some("reasoning_effort") },
        ParseRow { label: "chat unknown identifier", path: CHAT, extra: json!({"reasoning_effort": "turbo"}), rejected: Some("reasoning_effort") },
        ParseRow { label: "chat case variant", path: CHAT, extra: json!({"reasoning_effort": "High"}), rejected: Some("reasoning_effort") },
        ParseRow { label: "chat literal auto", path: CHAT, extra: json!({"reasoning_effort": "auto"}), rejected: Some("reasoning_effort") },
        ParseRow { label: "chat array value", path: CHAT, extra: json!({"reasoning_effort": ["high"]}), rejected: Some("reasoning_effort") },
        ParseRow { label: "chat extra reasoning object", path: CHAT, extra: json!({"reasoning": {"effort": "high"}}), rejected: Some("reasoning") },
        ParseRow { label: "responses canonical", path: RESP, extra: json!({"reasoning": {"effort": "high"}}), rejected: None },
        ParseRow { label: "responses padded lowercase", path: RESP, extra: json!({"reasoning": {"effort": " high "}}), rejected: None },
        ParseRow { label: "responses null", path: RESP, extra: json!({"reasoning": null}), rejected: None },
        ParseRow { label: "responses case variant", path: RESP, extra: json!({"reasoning": {"effort": "High"}}), rejected: Some("reasoning") },
        ParseRow { label: "responses literal auto", path: RESP, extra: json!({"reasoning": {"effort": "auto"}}), rejected: Some("reasoning") },
        ParseRow { label: "responses non-string", path: RESP, extra: json!({"reasoning": {"effort": 42}}), rejected: Some("reasoning") },
        ParseRow { label: "responses extra member", path: RESP, extra: json!({"reasoning": {"effort": "high", "summary": "auto"}}), rejected: Some("reasoning") },
        ParseRow { label: "responses opaque state", path: RESP, extra: json!({"reasoning": "opaque"}), rejected: Some("reasoning") },
    ];

    let mut failures: Vec<String> = Vec::new();
    for row in rows {
        let before = captured_len(&log);
        let body = if row.path == CHAT {
            chat_body(&alias, row.extra.clone())
        } else {
            responses_body(&alias, row.extra.clone())
        };
        let (status, text) = post_json(port, row.path, body).await;
        let after = captured_len(&log);
        match row.rejected {
            None => {
                if status != 200 {
                    failures.push(format!("{}: expected 200, got {status}: {text}", row.label));
                } else if after != before + 1 {
                    failures.push(format!("{}: expected exactly one upstream attempt", row.label));
                }
            }
            Some(token) => {
                if status != 400 {
                    failures.push(format!("{}: expected 400, got {status}: {text}", row.label));
                } else if !error_text(&text)
                    .to_ascii_lowercase()
                    .contains(&token.to_ascii_lowercase())
                {
                    failures.push(format!(
                        "{}: the 400 must name '{token}': {}",
                        row.label,
                        error_text(&text)
                    ));
                }
                if after != before {
                    failures.push(format!("{}: a rejected control must not attempt upstream", row.label));
                }
            }
        }
    }

    let raw = read_raw_gateway_config();
    if let Some(keys) = raw["providers"][0]["keys"].as_array() {
        for key in keys {
            if key["auto_marked"] != json!(false) || key["failure_kind"] != Value::Null {
                failures.push(format!("a request mutated source key health: {key}"));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "pool reasoning ingress parsing mismatches: {failures:#?}"
    );
    stop_server().await.expect("stop the listener");
}

// ---------------------------------------------------------------------------
// AC-002: explicit effort overrides the stored default; auto omits controls
// ---------------------------------------------------------------------------

/// AC-002: an explicit preference overrides the pool default, an omitted effort
/// resolves the concrete default's native control, and an `auto` (or legacy
/// missing) default emits none of the feature's controls on either ingress.
#[tokio::test]
async fn pool_reasoning_default_override_and_auto_omission() {
    let _home = temp_home("pool-reasoning-default");
    let port = free_port().await;
    let tag = new_tag();
    let medium_alias = format!("free-medium-{tag}");
    let auto_alias = format!("free-auto-{tag}");
    let (url, log) = spawn_reasoning_upstream().await;

    let profile = || {
        reasoning(
            "openai_effort",
            Some(vec!["low", "medium", "high"]),
            None,
            true,
            vec![
                enum_mapping("low", "low"),
                enum_mapping("medium", "med-native"),
                enum_mapping("high", "high-native"),
                disable_mapping("none"),
            ],
        )
    };
    let medium_pool = pool(
        &format!("pool-medium-{tag}"),
        "Medium",
        &medium_alias,
        Some("medium"),
        vec![candidate(
            "p-shared",
            "local-chat",
            "remote-chat",
            "chat_completions",
            Some(profile()),
        )],
    );
    let auto_pool = pool(
        &format!("pool-auto-{tag}"),
        "Auto",
        &auto_alias,
        None,
        vec![candidate(
            "p-shared",
            "local-chat",
            "remote-chat",
            "chat_completions",
            Some(profile()),
        )],
    );
    write_raw_gateway_config(&http_config(
        port,
        vec![reasoning_provider(&url)],
        vec![medium_pool, auto_pool],
    ));
    start_server(None).await.expect("start the listener");

    let mut failures: Vec<String> = Vec::new();
    let (status, text, body) = attempt(
        port,
        &log,
        CHAT,
        chat_body(&medium_alias, json!({})),
    )
    .await;
    if status != 200 {
        failures.push(format!("medium omission: expected 200, got {status}: {text}"));
    } else if body
        .as_ref()
        .and_then(|body| body.get("reasoning_effort"))
        .and_then(Value::as_str)
        != Some("med-native")
    {
        failures.push(format!("medium omission must send the default native control: {body:?}"));
    }

    let (status, text, body) = attempt(
        port,
        &log,
        CHAT,
        chat_body(&medium_alias, json!({"reasoning_effort": "high"})),
    )
    .await;
    if status != 200 {
        failures.push(format!("medium explicit high: expected 200, got {status}: {text}"));
    } else if body
        .as_ref()
        .and_then(|body| body.get("reasoning_effort"))
        .and_then(Value::as_str)
        != Some("high-native")
    {
        failures.push(format!("explicit high must override the stored default: {body:?}"));
    }

    let (status, text, body) = attempt(port, &log, CHAT, chat_body(&auto_alias, json!({}))).await;
    if status != 200 {
        failures.push(format!("auto omission: expected 200, got {status}: {text}"));
    } else if let Some(body) = body.as_ref() {
        if body.get("reasoning_effort").is_some()
            || body.get("reasoningEffort").is_some()
            || body.get("reasoning").is_some()
            || body.get("thinking").is_some()
            || body.get("output_config").is_some()
        {
            failures.push(format!("auto must omit every reasoning control: {body}"));
        }
    }

    let (status, text, body) = attempt(
        port,
        &log,
        CHAT,
        chat_body(&auto_alias, json!({"reasoning_effort": "high"})),
    )
    .await;
    if status != 200 {
        failures.push(format!("auto explicit high: expected 200, got {status}: {text}"));
    } else if body
        .as_ref()
        .and_then(|body| body.get("reasoning_effort"))
        .and_then(Value::as_str)
        != Some("high-native")
    {
        failures.push(format!("an explicit high on an auto pool must still inject: {body:?}"));
    }

    assert!(
        failures.is_empty(),
        "pool reasoning default/auto mismatches: {failures:#?}"
    );
    stop_server().await.expect("stop the listener");
}

// ---------------------------------------------------------------------------
// AC-004 / AC-005: ordered runtime classification
// ---------------------------------------------------------------------------

/// AC-004 / AC-005: the request-time classification is, in order, a `400
/// unsupported_reasoning_effort` when no candidate declares the preference, a
/// `502 all_providers_unavailable` when every declaration is invalid against the
/// latest source identity, a `400 reasoning_output_limit_incompatible` naming
/// the output-limit field and budget when every valid mapping is cap
/// incompatible, and a `502` when no compatible candidate has a usable key. Each
/// outcome happens before any upstream attempt.
#[tokio::test]
async fn pool_reasoning_classifies_missing_declared_budget_and_unavailable() {
    let _home = temp_home("pool-reasoning-classify");
    let port = free_port().await;
    let tag = new_tag();
    let (url, log) = spawn_reasoning_upstream().await;
    let no_decl_alias = format!("free-nodecl-{tag}");
    let invalid_alias = format!("free-invalid-{tag}");
    let budget_alias = format!("free-budget-{tag}");
    let nokey_alias = format!("free-nokey-{tag}");

    let provider = reasoning_provider(&url);
    let mut nokey_provider = pool_provider(
        "p-nokey",
        "NoKey",
        &url,
        vec![pool_key("key-disabled", "Disabled", "sk-disabled", false)],
        vec![mapping("local-nk", "remote-nk", "chat_completions")],
    );
    nokey_provider["enabled"] = json!(true);

    let pools = vec![
        // No declaration at all.
        single_candidate_pool(
            &format!("pool-nodecl-{tag}"),
            &no_decl_alias,
            "local-chat",
            "remote-chat",
            "chat_completions",
            Value::Null,
        ),
        // A declaration whose source reference no longer resolves.
        single_candidate_pool(
            &format!("pool-invalid-{tag}"),
            &invalid_alias,
            "local-gone",
            "remote-gone",
            "chat_completions",
            reasoning(
                "openai_effort",
                Some(vec!["high"]),
                None,
                false,
                vec![enum_mapping("high", "high")],
            ),
        ),
        // A valid manual-budget mapping that cannot fit the request's output cap.
        single_candidate_pool(
            &format!("pool-budget-{tag}"),
            &budget_alias,
            "local-anth",
            "remote-anth",
            "anthropic_messages",
            reasoning(
                "anthropic_manual_budget",
                None,
                Some((1024, 8192)),
                false,
                vec![budget_mapping("high", 8192)],
            ),
        ),
        // A valid mapping whose provider has no usable key.
        single_candidate_pool(
            &format!("pool-nokey-{tag}"),
            &nokey_alias,
            "local-nk",
            "remote-nk",
            "chat_completions",
            reasoning(
                "openai_effort",
                Some(vec!["high"]),
                None,
                false,
                vec![enum_mapping("high", "high")],
            ),
        ),
    ];
    // The no-declaration pool candidate must not acquire a declaration.
    let mut pools = pools;
    pools[0]["candidates"][0].as_object_mut().unwrap().remove("reasoning");

    write_raw_gateway_config(&http_config(port, vec![provider, nokey_provider], pools));
    start_server(None).await.expect("start the listener");

    let mut failures: Vec<String> = Vec::new();
    let cases = [
        (no_decl_alias.as_str(), "unsupported_reasoning_effort", true),
        (invalid_alias.as_str(), "all_providers_unavailable", false),
        (budget_alias.as_str(), "reasoning_output_limit_incompatible", false),
        (nokey_alias.as_str(), "all_providers_unavailable", false),
    ];
    for (alias, code, is_cap) in cases {
        let mut extra = json!({"reasoning_effort": "high"});
        if is_cap {
            extra["max_tokens"] = json!(1024);
        }
        let (status, text, captured) =
            attempt(port, &log, CHAT, chat_body(alias, extra)).await;
        if captured.is_some() {
            failures.push(format!("{alias}: classification must precede any attempt"));
        }
        if error_code(&text) != code {
            failures.push(format!(
                "{alias}: expected code '{code}', got '{}' ({status}): {text}",
                error_code(&text)
            ));
        }
        if code == "all_providers_unavailable" && error_text(&text).trim().is_empty() {
            failures.push(format!("{alias}: an unavailable result needs a bounded reason: {text}"));
        }
        if code == "reasoning_output_limit_incompatible" {
            let message = error_text(&text);
            let names_field = message.contains("max_tokens") || message.contains("max_completion_tokens");
            if !message.contains("8192") || !names_field {
                failures.push(format!(
                    "{alias}: the cap error must name the output-limit field and budget: {message}"
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "pool reasoning classification mismatches: {failures:#?}"
    );
    stop_server().await.expect("stop the listener");
}

/// AC-005: a valid declared mapping with no usable key does not mutate the
/// disabled key or attempt an upstream, and returns the existing 502 envelope.
#[tokio::test]
async fn pool_reasoning_unavailable_source_does_not_mutate_health() {
    let _home = temp_home("pool-reasoning-unavailable-health");
    let port = free_port().await;
    let tag = new_tag();
    let alias = format!("free-health-{tag}");
    let (url, log) = spawn_reasoning_upstream().await;
    let provider = pool_provider(
        "p-nokey",
        "NoKey",
        &url,
        vec![pool_key("key-disabled", "Disabled", "sk-disabled", false)],
        vec![mapping("local-nk", "remote-nk", "chat_completions")],
    );
    let pool = single_candidate_pool(
        &format!("pool-health-{tag}"),
        &alias,
        "local-nk",
        "remote-nk",
        "chat_completions",
        reasoning(
            "openai_effort",
            Some(vec!["high"]),
            None,
            false,
            vec![enum_mapping("high", "high")],
        ),
    );
    write_raw_gateway_config(&http_config(port, vec![provider], vec![pool]));
    start_server(None).await.expect("start the listener");

    let (status, text) = post_json(port, CHAT, chat_body(&alias, json!({"reasoning_effort": "high"}))).await;
    assert_eq!(status, 502, "a declared-but-unusable candidate must fail as unavailable: {text}");
    assert_eq!(error_code(&text), "all_providers_unavailable", "{text}");
    assert!(captured_len(&log) == 0, "no attempt may settle a disabled key");

    let raw = read_raw_gateway_config();
    let key = &raw["providers"][0]["keys"][0];
    assert_eq!(key["enabled"], json!(false), "the disabled key must stay disabled: {key}");
    assert_eq!(key["auto_marked"], json!(false), "no health mark may be settled: {key}");

    stop_server().await.expect("stop the listener");
}

// ---------------------------------------------------------------------------
// AC-006: candidate-specific native wire controls on every target protocol
// ---------------------------------------------------------------------------

/// AC-006: both OpenAI ingresses inject exactly the selected candidate's native
/// controls for enum, adaptive-thinking, inherit-thinking, manual-budget and
/// chat-switch profiles, and an explicit `none` maps to the declared disable
/// control, with the recognized entry spelling removed.
#[tokio::test]
async fn pool_reasoning_injects_candidate_native_controls() {
    let _home = temp_home("pool-reasoning-native-controls");
    let port = free_port().await;
    let tag = new_tag();
    let (url, log) = spawn_reasoning_upstream().await;
    let enum_chat = format!("free-enum-chat-{tag}");
    let enum_resp = format!("free-enum-resp-{tag}");
    let switch = format!("free-switch-{tag}");
    let inherit = format!("free-inherit-{tag}");
    let adaptive = format!("free-adaptive-{tag}");
    let budget = format!("free-budget-{tag}");
    let disable_chat = format!("free-disable-chat-{tag}");
    let disable_anth = format!("free-disable-anth-{tag}");

    let pools = vec![
        single_candidate_pool(
            &format!("pool-enum-chat-{tag}"),
            &enum_chat,
            "local-chat",
            "remote-chat",
            "chat_completions",
            reasoning(
                "openai_effort",
                Some(vec!["low", "high"]),
                None,
                false,
                vec![enum_mapping("low", "low"), enum_mapping("high", "max")],
            ),
        ),
        single_candidate_pool(
            &format!("pool-enum-resp-{tag}"),
            &enum_resp,
            "local-resp",
            "remote-resp",
            "responses",
            reasoning(
                "openai_effort",
                Some(vec!["low", "high"]),
                None,
                false,
                vec![enum_mapping("low", "low"), enum_mapping("high", "max")],
            ),
        ),
        single_candidate_pool(
            &format!("pool-switch-{tag}"),
            &switch,
            "local-chat",
            "remote-chat",
            "chat_completions",
            reasoning(
                "chat_thinking_switch",
                Some(vec!["enabled"]),
                None,
                false,
                vec![enum_mapping("high", "enabled")],
            ),
        ),
        single_candidate_pool(
            &format!("pool-inherit-{tag}"),
            &inherit,
            "local-anth",
            "remote-anth",
            "anthropic_messages",
            reasoning(
                "anthropic_effort_inherit",
                Some(vec!["high"]),
                None,
                false,
                vec![enum_mapping("high", "high")],
            ),
        ),
        single_candidate_pool(
            &format!("pool-adaptive-{tag}"),
            &adaptive,
            "local-anth",
            "remote-anth",
            "anthropic_messages",
            reasoning(
                "anthropic_effort_adaptive",
                Some(vec!["high"]),
                None,
                false,
                vec![enum_mapping("high", "high")],
            ),
        ),
        single_candidate_pool(
            &format!("pool-budget-{tag}"),
            &budget,
            "local-anth",
            "remote-anth",
            "anthropic_messages",
            reasoning(
                "anthropic_manual_budget",
                None,
                Some((1024, 8192)),
                false,
                vec![budget_mapping("high", 2048)],
            ),
        ),
        single_candidate_pool(
            &format!("pool-disable-chat-{tag}"),
            &disable_chat,
            "local-chat",
            "remote-chat",
            "chat_completions",
            reasoning(
                "openai_effort",
                Some(vec!["low"]),
                None,
                true,
                vec![enum_mapping("low", "low"), disable_mapping("none")],
            ),
        ),
        single_candidate_pool(
            &format!("pool-disable-anth-{tag}"),
            &disable_anth,
            "local-anth",
            "remote-anth",
            "anthropic_messages",
            reasoning(
                "anthropic_effort_adaptive",
                Some(vec!["high"]),
                None,
                true,
                vec![enum_mapping("high", "high"), disable_mapping("none")],
            ),
        ),
    ];
    write_raw_gateway_config(&http_config(port, vec![reasoning_provider(&url)], pools));
    start_server(None).await.expect("start the listener");

    let mut failures: Vec<String> = Vec::new();
    let mut check = |label: &str, status: u16, text: &str, body: Option<Value>, expected: &[(&str, Value)]| {
        if status != 200 {
            failures.push(format!("{label}: expected 200, got {status}: {text}"));
            return;
        }
        let Some(body) = body else {
            failures.push(format!("{label}: no upstream attempt was captured"));
            return;
        };
        for (pointer, value) in expected {
            if body.pointer(pointer) != Some(value) {
                failures.push(format!(
                    "{label}: expected {pointer}={value}, got {:?} in {body}",
                    body.pointer(pointer)
                ));
            }
        }
    };

    let (status, text, body) = attempt(
        port,
        &log,
        CHAT,
        chat_body(&enum_chat, json!({"reasoning_effort": "high", "reasoningEffort": "high"})),
    )
    .await;
    check("chat enum", status, &text, body, &[("/reasoning_effort", json!("max"))]);

    let (status, text, body) = attempt(
        port,
        &log,
        RESP,
        responses_body(&enum_resp, json!({"reasoning": {"effort": "high"}})),
    )
    .await;
    check("responses enum", status, &text, body, &[("/reasoning/effort", json!("max"))]);

    let (status, text, body) = attempt(
        port,
        &log,
        CHAT,
        chat_body(&switch, json!({"reasoning_effort": "high"})),
    )
    .await;
    check("chat switch", status, &text, body, &[("/thinking/type", json!("enabled"))]);

    let (status, text, body) = attempt(
        port,
        &log,
        CHAT,
        chat_body(&inherit, json!({"reasoning_effort": "high"})),
    )
    .await;
    check("anthropic inherit", status, &text, body, &[("/output_config/effort", json!("high"))]);

    let (status, text, body) = attempt(
        port,
        &log,
        CHAT,
        chat_body(&adaptive, json!({"reasoning_effort": "high"})),
    )
    .await;
    check(
        "anthropic adaptive",
        status,
        &text,
        body,
        &[("/output_config/effort", json!("high")), ("/thinking/type", json!("adaptive"))],
    );

    let (status, text, body) = attempt(
        port,
        &log,
        CHAT,
        chat_body(&budget, json!({"reasoning_effort": "high", "max_tokens": 8192})),
    )
    .await;
    check(
        "anthropic manual budget",
        status,
        &text,
        body,
        &[
            ("/thinking/type", json!("enabled")),
            ("/thinking/budget_tokens", json!(2048)),
        ],
    );

    let (status, text, body) = attempt(
        port,
        &log,
        CHAT,
        chat_body(&disable_chat, json!({"reasoning_effort": "none"})),
    )
    .await;
    check("chat disable", status, &text, body, &[("/reasoning_effort", json!("none"))]);

    let (status, text, body) = attempt(
        port,
        &log,
        CHAT,
        chat_body(&disable_anth, json!({"reasoning_effort": "none"})),
    )
    .await;
    check("anthropic disable", status, &text, body, &[("/thinking/type", json!("disabled"))]);

    // The recognized entry spelling must be removed on a same-protocol Chat
    // attempt whose native field is the canonical `reasoning_effort`.
    if let Some(body) = captured_bodies(&log).first() {
        if body.get("reasoningEffort").is_some() {
            failures.push(format!("the compat entry spelling must be stripped: {body}"));
        }
    }

    assert!(
        failures.is_empty(),
        "pool reasoning native control mismatches: {failures:#?}"
    );
    stop_server().await.expect("stop the listener");
}

/// AC-006: an Anthropic manual budget must be strictly below the effective
/// output cap; Chat `max_completion_tokens` retains precedence over `max_tokens`,
/// Responses uses `max_output_tokens`, and an omitted Anthropic cap keeps 4096.
/// A budget at or above the cap is excluded before any attempt.
#[tokio::test]
async fn pool_reasoning_budget_cap_precedence_and_default_cap() {
    let _home = temp_home("pool-reasoning-budget-cap");
    let port = free_port().await;
    let tag = new_tag();
    let (url, log) = spawn_reasoning_upstream().await;
    let chat_cap = format!("free-cap-chat-{tag}");
    let resp_cap = format!("free-cap-resp-{tag}");
    let anth_cap = format!("free-cap-anth-{tag}");
    let anth_default = format!("free-cap-anth-default-{tag}");

    let budget_pool = |id: &str, alias: &str, protocol: &str, local: &str, upstream: &str, budget: u32, range: (u32, u32)| {
        single_candidate_pool(
            id,
            alias,
            local,
            upstream,
            protocol,
            reasoning(
                "anthropic_manual_budget",
                None,
                Some(range),
                false,
                vec![budget_mapping("high", budget)],
            ),
        )
    };
    let pools = vec![
        budget_pool(&format!("pool-cap-chat-{tag}"), &chat_cap, "anthropic_messages", "local-anth", "remote-anth", 2048, (1024, 4096)),
        budget_pool(&format!("pool-cap-resp-{tag}"), &resp_cap, "anthropic_messages", "local-anth", "remote-anth", 2048, (1024, 4096)),
        budget_pool(&format!("pool-cap-anth-{tag}"), &anth_cap, "anthropic_messages", "local-anth", "remote-anth", 4096, (1024, 8192)),
        budget_pool(&format!("pool-cap-anth-default-{tag}"), &anth_default, "anthropic_messages", "local-anth", "remote-anth", 2048, (1024, 8192)),
    ];
    write_raw_gateway_config(&http_config(port, vec![reasoning_provider(&url)], pools));
    start_server(None).await.expect("start the listener");

    let mut failures: Vec<String> = Vec::new();

    // Chat precedence: max_completion_tokens=1024 wins over max_tokens=8192.
    let (status, text) = post_json(
        port,
        CHAT,
        chat_body(
            &chat_cap,
            json!({"reasoning_effort": "high", "max_completion_tokens": 1024, "max_tokens": 8192}),
        ),
    )
    .await;
    if status != 400 || error_code(&text) != "reasoning_output_limit_incompatible" {
        failures.push(format!("chat cap precedence: expected 400 incompatible, got {status}: {text}"));
    } else if !error_text(&text).contains("max_completion_tokens") {
        failures.push(format!("chat cap precedence must name max_completion_tokens: {text}"));
    }

    // Chat with a large cap sends the configured budget, never enlarging the cap.
    let (status, text, body) = attempt(
        port,
        &log,
        CHAT,
        chat_body(&chat_cap, json!({"reasoning_effort": "high", "max_completion_tokens": 4096})),
    )
    .await;
    if status != 200 {
        failures.push(format!("chat large cap: expected 200, got {status}: {text}"));
    } else if let Some(body) = body {
        if body.pointer("/thinking/budget_tokens") != Some(&json!(2048)) {
            failures.push(format!("chat large cap must send budget 2048: {body}"));
        }
        if body.pointer("/max_tokens") != Some(&json!(4096)) {
            failures.push(format!("the gateway must not enlarge the client cap: {body}"));
        }
    }

    // Responses uses max_output_tokens; equality is incompatible.
    let (status, text) = post_json(
        port,
        RESP,
        responses_body(&resp_cap, json!({"reasoning": {"effort": "high"}, "max_output_tokens": 2048})),
    )
    .await;
    if status != 400 || error_code(&text) != "reasoning_output_limit_incompatible" {
        failures.push(format!("responses equal cap: expected 400 incompatible, got {status}: {text}"));
    } else if !error_text(&text).contains("max_output_tokens") {
        failures.push(format!("responses cap error must name max_output_tokens: {text}"));
    }

    // Responses with a larger cap is allowed.
    let (status, text, _body) = attempt(
        port,
        &log,
        RESP,
        responses_body(&resp_cap, json!({"reasoning": {"effort": "high"}, "max_output_tokens": 4096})),
    )
    .await;
    if status != 200 {
        failures.push(format!("responses large cap: expected 200, got {status}: {text}"));
    }

    // An omitted Anthropic cap keeps the existing 4096 default, so an equal
    // budget is incompatible while a smaller budget serves.
    let (status, text) = post_json(port, CHAT, chat_body(&anth_cap, json!({"reasoning_effort": "high"}))).await;
    if status != 400 || error_code(&text) != "reasoning_output_limit_incompatible" {
        failures.push(format!("omitted cap 4096 equality: expected 400 incompatible, got {status}: {text}"));
    }
    let (status, text, body) = attempt(
        port,
        &log,
        CHAT,
        chat_body(&anth_default, json!({"reasoning_effort": "high"})),
    )
    .await;
    if status != 200 {
        failures.push(format!("omitted cap smaller budget: expected 200, got {status}: {text}"));
    } else if let Some(body) = body {
        if body.pointer("/thinking/budget_tokens") != Some(&json!(2048)) {
            failures.push(format!("the omitted-cap default must keep budget 2048: {body}"));
        }
    }

    assert!(
        failures.is_empty(),
        "pool reasoning budget/cap mismatches: {failures:#?}"
    );
    stop_server().await.expect("stop the listener");
}

// ---------------------------------------------------------------------------
// AC-006 / AC-010 / AC-011: fallback-native control and durable evidence
// ---------------------------------------------------------------------------

/// AC-006 / AC-010: an A-to-B fallback independently applies each candidate's
/// mapping, and the durable rows carry the raw requested value, the resolved
/// preference and a native snapshot derived from the same constructed value as
/// the captured body, with exactly one terminal row.
#[tokio::test]
async fn pool_reasoning_fallback_records_candidate_specific_evidence() {
    let _home = temp_home("pool-reasoning-fallback");
    let port = free_port().await;
    let tag = new_tag();
    let alias = format!("free-fallback-{tag}");
    let (url, log) = spawn_mock_upstream(|captured| {
        let model = serde_json::from_slice::<Value>(&captured.body)
            .ok()
            .and_then(|value| value.get("model").and_then(Value::as_str).map(str::to_string))
            .unwrap_or_default();
        if model == "remote-a" {
            MockReply::Json(
                429,
                json!({"error": {"type": "FreeUsageLimitError", "message": "free usage limit reached"}}),
            )
        } else {
            MockReply::Json(
                200,
                json!({
                    "id": "chatcmpl-1", "object": "chat.completion", "model": "remote-b",
                    "choices": [{"index": 0, "message": {"role": "assistant", "content": "ok"},
                        "finish_reason": "stop"}],
                    "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2},
                }),
            )
        }
    })
    .await;

    let provider = pool_provider(
        "p-shared",
        "Shared",
        &url,
        vec![pool_key("key-shared", "Shared", "sk-shared", true)],
        vec![
            mapping("local-a", "remote-a", "chat_completions"),
            mapping("local-b", "remote-b", "chat_completions"),
        ],
    );
    let mut candidate_a = candidate(
        "p-shared",
        "local-a",
        "remote-a",
        "chat_completions",
        Some(reasoning(
            "openai_effort",
            Some(vec!["high"]),
            None,
            false,
            vec![enum_mapping("high", "first")],
        )),
    );
    candidate_a["weight"] = json!(100);
    let candidate_b = candidate(
        "p-shared",
        "local-b",
        "remote-b",
        "chat_completions",
        Some(reasoning(
            "openai_effort",
            Some(vec!["high"]),
            None,
            false,
            vec![enum_mapping("high", "second")],
        )),
    );
    let pool = pool(
        &format!("pool-fallback-{tag}"),
        "Fallback",
        &alias,
        None,
        vec![candidate_a, candidate_b],
    );
    write_raw_gateway_config(&http_config(port, vec![provider], vec![pool]));
    start_server(None).await.expect("start the listener");

    let (status, text) = post_json(port, CHAT, chat_body(&alias, json!({"reasoning_effort": "high"}))).await;
    assert_eq!(status, 200, "the fallback must serve the mapped request: {text}");

    let bodies = captured_bodies(&log);
    assert_eq!(bodies.len(), 2, "the fallback must attempt A then B: {bodies:?}");
    assert_eq!(bodies[0].get("reasoning_effort").and_then(Value::as_str), Some("first"));
    assert_eq!(bodies[1].get("reasoning_effort").and_then(Value::as_str), Some("second"));

    let records = wait_for_usage_logs(2).await;
    let non_terminal = records
        .iter()
        .find(|record| !record.terminal)
        .expect("the A attempt must be a non-terminal row");
    let terminal = records
        .iter()
        .find(|record| record.terminal)
        .expect("the B attempt must be the terminal row");
    assert_eq!(
        records.iter().filter(|record| record.terminal).count(),
        1,
        "a request keeps exactly one terminal row: {records:?}"
    );

    let mut failures: Vec<String> = Vec::new();
    for (label, record) in [("A", non_terminal), ("B", terminal)] {
        let json = record_json(record);
        if json.get("reasoning_effort").and_then(Value::as_str) != Some("high") {
            failures.push(format!("{label}: requested reasoning_effort must stay raw 'high': {json}"));
        }
        if json.get("resolved_reasoning_effort").and_then(Value::as_str) != Some("high") {
            failures.push(format!("{label}: resolved effort must be 'high': {json}"));
        }
        if json.pointer("/native_reasoning/protocol").and_then(Value::as_str) != Some("chat_completions") {
            failures.push(format!("{label}: native protocol must be chat_completions: {json}"));
        }
        if json.pointer("/native_reasoning/mode").and_then(Value::as_str) != Some("enum") {
            failures.push(format!("{label}: native mode must be enum: {json}"));
        }
    }
    if record_json(non_terminal)
        .pointer("/native_reasoning/parameters/reasoning_effort")
        .and_then(Value::as_str)
        != Some("first")
    {
        failures.push("A: native parameters must match its own capture 'first'".to_string());
    }
    if record_json(terminal)
        .pointer("/native_reasoning/parameters/reasoning_effort")
        .and_then(Value::as_str)
        != Some("second")
    {
        failures.push("B: native parameters must match its own capture 'second'".to_string());
    }

    assert!(
        failures.is_empty(),
        "pool reasoning attempt evidence mismatches: {failures:#?}"
    );
    stop_server().await.expect("stop the listener");
}

/// AC-010 / AC-011: an omitted effort resolves the stored default (`medium`) or
/// `auto`, an attempted `auto` records `provider_default` with empty parameters,
/// and a synthetic no-attempt row carries no native snapshot.
#[tokio::test]
async fn pool_reasoning_records_resolved_default_auto_and_synthetic_absence() {
    let _home = temp_home("pool-reasoning-resolved-records");
    let port = free_port().await;
    let tag = new_tag();
    let (url, _log) = spawn_reasoning_upstream().await;
    let medium_alias = format!("free-rec-medium-{tag}");
    let auto_alias = format!("free-rec-auto-{tag}");
    let disabled_alias = format!("free-rec-disabled-{tag}");

    let medium_pool = pool(
        &format!("pool-rec-medium-{tag}"),
        "Medium",
        &medium_alias,
        Some("medium"),
        vec![candidate(
            "p-shared",
            "local-chat",
            "remote-chat",
            "chat_completions",
            Some(reasoning(
                "openai_effort",
                Some(vec!["medium"]),
                None,
                false,
                vec![enum_mapping("medium", "med-native")],
            )),
        )],
    );
    let auto_pool = pool(
        &format!("pool-rec-auto-{tag}"),
        "Auto",
        &auto_alias,
        None,
        vec![candidate(
            "p-shared",
            "local-chat",
            "remote-chat",
            "chat_completions",
            Some(reasoning(
                "openai_effort",
                Some(vec!["low"]),
                None,
                false,
                vec![enum_mapping("low", "low")],
            )),
        )],
    );
    let disabled_pool = json!({
        "id": format!("pool-rec-disabled-{tag}"),
        "name": "Disabled",
        "model": disabled_alias,
        "enabled": false,
        "candidates": [],
    });
    write_raw_gateway_config(&http_config(
        port,
        vec![reasoning_provider(&url)],
        vec![medium_pool, auto_pool, disabled_pool],
    ));
    start_server(None).await.expect("start the listener");

    let (medium_status, _) = post_json(port, CHAT, chat_body(&medium_alias, json!({}))).await;
    assert_eq!(medium_status, 200);
    let (auto_status, _) = post_json(port, CHAT, chat_body(&auto_alias, json!({}))).await;
    assert_eq!(auto_status, 200);
    let (disabled_status, disabled_text) =
        post_json(port, CHAT, chat_body(&disabled_alias, json!({}))).await;
    assert_eq!(disabled_status, 502, "a disabled pool must fail closed: {disabled_text}");

    let records = wait_for_usage_logs(3).await;
    let find = |model: &str| {
        records
            .iter()
            .find(|record| record.local_model == model)
            .unwrap_or_else(|| panic!("missing row for '{model}': {records:?}"))
    };
    let mut failures: Vec<String> = Vec::new();

    let medium = record_json(find(&medium_alias));
    if medium.get("reasoning_effort") != Some(&Value::Null) {
        failures.push(format!("medium omission must keep a null requested value: {medium}"));
    }
    if medium.get("resolved_reasoning_effort").and_then(Value::as_str) != Some("medium") {
        failures.push(format!("medium omission must resolve 'medium': {medium}"));
    }

    let auto = record_json(find(&auto_alias));
    if auto.get("resolved_reasoning_effort").and_then(Value::as_str) != Some("auto") {
        failures.push(format!("auto omission must resolve 'auto': {auto}"));
    }
    if auto.pointer("/native_reasoning/mode").and_then(Value::as_str) != Some("provider_default") {
        failures.push(format!("an attempted auto must record provider_default: {auto}"));
    }
    if auto.pointer("/native_reasoning/parameters") != Some(&json!({})) {
        failures.push(format!("provider_default parameters must be empty: {auto}"));
    }

    let synthetic = record_json(find(&disabled_alias));
    if synthetic.get("native_reasoning") != Some(&Value::Null) {
        failures.push(format!("a synthetic no-attempt row must carry a null snapshot: {synthetic}"));
    }
    // REQ-006 only requires the native snapshot to be absent for a no-attempt
    // row; it does not require `resolved_reasoning_effort` to be absent, so the
    // synthetic row's resolved value is deliberately left unconstrained here.

    assert!(
        failures.is_empty(),
        "pool reasoning resolved-record mismatches: {failures:#?}"
    );
    stop_server().await.expect("stop the listener");
}

// ---------------------------------------------------------------------------
// AC-007: mapped retry, streaming and downstream-cancellation boundaries
// ---------------------------------------------------------------------------

/// The `model` value the relay forwarded to the mock upstream for one capture.
fn forwarded_model(captured: &super::Captured) -> String {
    serde_json::from_str::<Value>(&String::from_utf8_lossy(&captured.body))
        .ok()
        .and_then(|value| {
            value
                .get("model")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default()
}

/// Poll the request's durable rows for up to ~2 seconds without panicking, so a
/// missing row is reported by the caller's own failure list after the listener
/// has stopped instead of leaking the process-wide server through a panic here.
async fn poll_usage_logs(expected: usize) -> Vec<UsageLogRecord> {
    let store = default_usage_store();
    for _ in 0..200 {
        let records = store.all_records().unwrap_or_default();
        if records.len() >= expected {
            return records;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    default_usage_store().all_records().unwrap_or_default()
}

/// A minimal Chat Completions success body for one mock upstream attempt.
fn chat_success(model: &str) -> Value {
    json!({
        "id": "chatcmpl-1", "object": "chat.completion", "model": model,
        "choices": [{"index": 0, "message": {"role": "assistant", "content": "ok"},
            "finish_reason": "stop"}],
        "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2},
    })
}

/// Assert one completed pool attempt row carries the fixed evidence contract:
/// the raw requested value stays `high`, the resolved preference is `high`, the
/// native snapshot names the candidate's own protocol and the allowlisted
/// native control that its capture carried.
fn check_pool_native_row(
    label: &str,
    record: &UsageLogRecord,
    expected_native: &str,
    failures: &mut Vec<String>,
) {
    let json = record_json(record);
    if json.get("resolved_reasoning_effort").and_then(Value::as_str) != Some("high") {
        failures.push(format!("{label}: resolved effort must stay 'high': {json}"));
    }
    if json.pointer("/native_reasoning/protocol").and_then(Value::as_str) != Some("chat_completions")
    {
        failures.push(format!("{label}: native protocol must be chat_completions: {json}"));
    }
    if json.pointer("/native_reasoning/mode").and_then(Value::as_str) != Some("enum") {
        failures.push(format!("{label}: native mode must be enum: {json}"));
    }
    if json
        .pointer("/native_reasoning/parameters/reasoning_effort")
        .and_then(Value::as_str)
        != Some(expected_native)
    {
        failures.push(format!(
            "{label}: native parameters must match the capture '{expected_native}': {json}"
        ));
    }
}

/// AC-007: an explicit mapped pool request whose first attempt fails with a
/// retryable status and recovers on the bounded retry forwards the same
/// candidate-native control on both attempts, and each completed attempt's
/// durable row carries that candidate's resolved preference and native snapshot.
#[tokio::test]
async fn pool_reasoning_mapped_retry_retains_native_mapping() {
    let _home = temp_home("pool-reasoning-retry");
    let port = free_port().await;
    let tag = new_tag();
    let alias = format!("free-retry-{tag}");
    let calls = Arc::new(AtomicUsize::new(0));
    let calls_for_mock = calls.clone();
    let (url, log) = spawn_mock_upstream(move |_| {
        if calls_for_mock.fetch_add(1, Ordering::SeqCst) == 0 {
            MockReply::Json(500, json!({"error": {"message": "temporarily unavailable"}}))
        } else {
            MockReply::Json(200, chat_success("remote-chat"))
        }
    })
    .await;

    let pool = single_candidate_pool(
        &format!("pool-retry-{tag}"),
        &alias,
        "local-chat",
        "remote-chat",
        "chat_completions",
        reasoning(
            "openai_effort",
            Some(vec!["high"]),
            None,
            false,
            vec![enum_mapping("high", "deep")],
        ),
    );
    write_raw_gateway_config(&http_config(port, vec![reasoning_provider(&url)], vec![pool]));
    start_server(None).await.expect("start the listener");

    let (status, text) = post_json(
        port,
        CHAT,
        chat_body(&alias, json!({"reasoning_effort": "high"})),
    )
    .await;
    let bodies = captured_bodies(&log);
    let recorded = poll_usage_logs(2).await;
    stop_server().await.expect("stop the listener");

    let mut failures: Vec<String> = Vec::new();
    if status != 200 {
        failures.push(format!("the mapped retry must recover: {status}: {text}"));
    }
    if bodies.len() != 2 {
        failures.push(format!(
            "one initial attempt plus one bounded retry must both be captured: {bodies:?}"
        ));
    }
    for (index, body) in bodies.iter().enumerate() {
        if body.get("reasoning_effort").and_then(Value::as_str) != Some("deep") {
            failures.push(format!(
                "retry attempt {index} must retain the candidate-native control: {body}"
            ));
        }
    }
    if recorded.len() != 2 {
        failures.push(format!("exactly two completed attempt rows are expected: {recorded:?}"));
    }
    for record in &recorded {
        check_pool_native_row("retry", record, "deep", &mut failures);
    }
    assert!(
        failures.is_empty(),
        "mapped retry evidence mismatches: {failures:#?}"
    );
}

/// AC-007: a streaming request whose first candidate fails before the first
/// client byte falls back to the next candidate, and every attempt carries its
/// own candidate-native control rather than reusing the failed body.
#[tokio::test]
async fn pool_reasoning_streaming_pre_byte_fallback_remaps_candidates() {
    let _home = temp_home("pool-reasoning-stream-switch");
    let port = free_port().await;
    let tag = new_tag();
    let alias = format!("free-stream-{tag}");
    let sse =
        "data: {\"choices\":[{\"delta\":{\"content\":\"from-b\"}}]}\n\ndata: [DONE]\n\n".to_string();
    let (url, log) = spawn_mock_upstream(move |captured| {
        if forwarded_model(captured) == "remote-a" {
            MockReply::Json(500, json!({"error": {"message": "temporarily unavailable"}}))
        } else {
            MockReply::Stream(sse.clone())
        }
    })
    .await;

    let mut candidate_a = candidate(
        "p-shared",
        "local-a",
        "remote-a",
        "chat_completions",
        Some(reasoning(
            "openai_effort",
            Some(vec!["high"]),
            None,
            false,
            vec![enum_mapping("high", "deep-a")],
        )),
    );
    candidate_a["weight"] = json!(100);
    let candidate_b = candidate(
        "p-shared",
        "local-b",
        "remote-b",
        "chat_completions",
        Some(reasoning(
            "openai_effort",
            Some(vec!["high"]),
            None,
            false,
            vec![enum_mapping("high", "deep-b")],
        )),
    );
    let pool = pool(
        &format!("pool-stream-{tag}"),
        "Stream",
        &alias,
        None,
        vec![candidate_a, candidate_b],
    );
    let provider = pool_provider(
        "p-shared",
        "Shared",
        &url,
        vec![pool_key("key-shared", "Shared", "sk-shared", true)],
        vec![
            mapping("local-a", "remote-a", "chat_completions"),
            mapping("local-b", "remote-b", "chat_completions"),
        ],
    );
    write_raw_gateway_config(&http_config(port, vec![provider], vec![pool]));
    start_server(None).await.expect("start the listener");

    let (status, text) = post_json(
        port,
        CHAT,
        chat_body(&alias, json!({"reasoning_effort": "high", "stream": true})),
    )
    .await;
    let bodies = captured_bodies(&log);
    let recorded = poll_usage_logs(2).await;
    stop_server().await.expect("stop the listener");

    let mut failures: Vec<String> = Vec::new();
    if status != 200 {
        failures.push(format!("the pre-byte streaming fallback must serve B: {status}: {text}"));
    }
    if !text.contains("from-b") {
        failures.push(format!("the fallback candidate's stream must reach the client: {text}"));
    }
    if bodies.len() != 2 {
        failures.push(format!("A fails before the first byte and B serves: {bodies:?}"));
    } else {
        if bodies[0].get("reasoning_effort").and_then(Value::as_str) != Some("deep-a") {
            failures.push(format!(
                "the first candidate's capture must carry its own native control: {:?}",
                bodies[0]
            ));
        }
        if bodies[1].get("reasoning_effort").and_then(Value::as_str) != Some("deep-b") {
            failures.push(format!(
                "the fallback candidate's capture must be remapped to its own native control: {:?}",
                bodies[1]
            ));
        }
    }
    match recorded.iter().find(|record| record.upstream_model == "remote-a") {
        Some(record) => check_pool_native_row("stream A", record, "deep-a", &mut failures),
        None => failures.push(format!("missing the failing candidate's attempt row: {recorded:?}")),
    }
    match recorded.iter().find(|record| record.upstream_model == "remote-b") {
        Some(record) => check_pool_native_row("stream B", record, "deep-b", &mut failures),
        None => failures.push(format!("missing the fallback candidate's attempt row: {recorded:?}")),
    }
    if recorded.iter().filter(|record| record.terminal).count() != 1 {
        failures.push(format!("a request keeps exactly one terminal row: {recorded:?}"));
    }
    assert!(
        failures.is_empty(),
        "streaming pre-byte remap evidence mismatches: {failures:#?}"
    );
}

/// AC-007: after the first client byte a mid-stream failure never switches or
/// replays across candidates and never emits a success marker, while the failed
/// attempt still records its own candidate-native control.
#[tokio::test]
async fn pool_reasoning_streaming_post_byte_failure_never_switches_or_replays() {
    let _home = temp_home("pool-reasoning-stream-terminate");
    let port = free_port().await;
    let tag = new_tag();
    let alias = format!("free-terminate-{tag}");
    let partial = "data: {\"choices\":[{\"delta\":{\"content\":\"partial-a\"}}]}\n\n".to_string();
    let declared = partial.len() + 500;
    let (url, log) = spawn_mock_upstream(move |captured| {
        if forwarded_model(captured) == "remote-a" {
            MockReply::PartialStream(partial.clone(), declared)
        } else {
            MockReply::Stream(
                "data: {\"choices\":[{\"delta\":{\"content\":\"from-b\"}}]}\n\ndata: [DONE]\n\n"
                    .to_string(),
            )
        }
    })
    .await;

    let mut candidate_a = candidate(
        "p-shared",
        "local-a",
        "remote-a",
        "chat_completions",
        Some(reasoning(
            "openai_effort",
            Some(vec!["high"]),
            None,
            false,
            vec![enum_mapping("high", "deep-a")],
        )),
    );
    candidate_a["weight"] = json!(100);
    let candidate_b = candidate(
        "p-shared",
        "local-b",
        "remote-b",
        "chat_completions",
        Some(reasoning(
            "openai_effort",
            Some(vec!["high"]),
            None,
            false,
            vec![enum_mapping("high", "deep-b")],
        )),
    );
    let pool = pool(
        &format!("pool-terminate-{tag}"),
        "Terminate",
        &alias,
        None,
        vec![candidate_a, candidate_b],
    );
    let provider = pool_provider(
        "p-shared",
        "Shared",
        &url,
        vec![pool_key("key-shared", "Shared", "sk-shared", true)],
        vec![
            mapping("local-a", "remote-a", "chat_completions"),
            mapping("local-b", "remote-b", "chat_completions"),
        ],
    );
    write_raw_gateway_config(&http_config(port, vec![provider], vec![pool]));
    start_server(None).await.expect("start the listener");

    let (status, text) = post_json(
        port,
        CHAT,
        chat_body(&alias, json!({"reasoning_effort": "high", "stream": true})),
    )
    .await;
    let captured = log.lock().expect("mock log").clone();
    let recorded = poll_usage_logs(1).await;
    stop_server().await.expect("stop the listener");

    let mut failures: Vec<String> = Vec::new();
    if status != 200 {
        failures.push(format!("the SSE status is committed before the mid-stream failure: {status}: {text}"));
    }
    if !text.contains("partial-a") {
        failures.push(format!("the partial bytes must reach the caller: {text}"));
    }
    if text.matches("partial-a").count() != 1 {
        failures.push(format!("the failed stream must not be replayed: {text}"));
    }
    if text.contains("from-b") {
        failures.push(format!("no sibling may serve after the first byte: {text}"));
    }
    if text.contains("[DONE]") {
        failures.push(format!("an abnormal stream must not emit a success marker: {text}"));
    }
    let models: Vec<String> = captured.iter().map(forwarded_model).collect();
    if models != vec!["remote-a".to_string()] {
        failures.push(format!(
            "only the first candidate may be contacted after a mid-stream failure: {models:?}"
        ));
    } else {
        let body: Value = serde_json::from_slice(&captured[0].body).unwrap_or(Value::Null);
        if body.get("reasoning_effort").and_then(Value::as_str) != Some("deep-a") {
            failures.push(format!(
                "the failed attempt must carry its candidate-native control: {body}"
            ));
        }
    }
    if recorded.len() != 1 {
        failures.push(format!("the mid-stream failure is the request's only row: {recorded:?}"));
    } else {
        check_pool_native_row("post-byte", &recorded[0], "deep-a", &mut failures);
        if recorded[0].result != UsageResult::Failure {
            failures.push(format!("the mid-stream attempt must record a failure: {:?}", recorded[0]));
        }
    }
    assert!(
        failures.is_empty(),
        "streaming post-byte boundary evidence mismatches: {failures:#?}"
    );
}

/// Open one loopback request against the gateway's real connection handler for
/// the pool alias `alias`, carrying the mapped `high` preference. The caller
/// closes the returned client to cancel the downstream request, so disconnect
/// detection runs at the same boundary as the running server.
async fn spawn_pool_handle_connection(
    alias: &str,
) -> (TcpStream, tokio::task::JoinHandle<Result<(), String>>) {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind relay loopback");
    let addr = listener.local_addr().expect("relay loopback address");
    let accept = tokio::spawn(async move {
        listener
            .accept()
            .await
            .expect("accept relay loopback")
            .0
    });
    let mut client = TcpStream::connect(addr).await.expect("connect relay loopback");
    let server = accept.await.expect("relay accept task");
    let handler = tokio::spawn(crate::ai_gateway::runtime_http::handle_connection(server));
    tokio::task::yield_now().await;
    let body = serde_json::to_vec(&json!({
        "model": alias,
        "messages": [{"role": "user", "content": "hi"}],
        "reasoning_effort": "high",
        "stream": false,
    }))
    .expect("encode relay request");
    let request = format!(
        "POST /v1/chat/completions HTTP/1.1\r\nhost: 127.0.0.1\r\nauthorization: Bearer local-key\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        body.len()
    );
    let mut frame = request.into_bytes();
    frame.extend_from_slice(&body);
    client
        .write_all(&frame)
        .await
        .expect("write relay request");
    client.flush().await.expect("flush relay request");
    (client, handler)
}

/// AC-007: a mapped pool request cancelled downstream while its upstream attempt
/// is still in flight persists zero log rows and leaves the candidate and key
/// health untouched, while the attempt that already started still forwarded the
/// candidate-native control.
#[tokio::test]
async fn pool_reasoning_cancelled_mapped_request_writes_no_rows_and_no_health() {
    let _home = temp_home("pool-reasoning-cancel");
    let port = free_port().await;
    let tag = new_tag();
    let alias = format!("free-cancel-{tag}");
    let arrived = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let (url, log) = {
        let arrived = arrived.clone();
        let release = release.clone();
        spawn_mock_upstream(move |_| {
            MockReply::Withhold(
                arrived.clone(),
                release.clone(),
                200,
                chat_success("remote-chat"),
            )
        })
        .await
    };

    let pool = single_candidate_pool(
        &format!("pool-cancel-{tag}"),
        &alias,
        "local-chat",
        "remote-chat",
        "chat_completions",
        reasoning(
            "openai_effort",
            Some(vec!["high"]),
            None,
            false,
            vec![enum_mapping("high", "deep")],
        ),
    );
    write_raw_gateway_config(&http_config(port, vec![reasoning_provider(&url)], vec![pool]));

    let (client, mut handler) = spawn_pool_handle_connection(&alias).await;
    tokio::time::timeout(Duration::from_secs(5), async {
        tokio::select! {
            _ = arrived.notified() => {}
            result = &mut handler => panic!("handler exited before the mapped attempt: {result:?}"),
        }
    })
    .await
    .expect("the mapped upstream attempt must begin");

    let mut failures: Vec<String> = Vec::new();
    let forwarded = captured_bodies(&log).into_iter().next().unwrap_or(Value::Null);
    if forwarded.get("reasoning_effort").and_then(Value::as_str) != Some("deep") {
        failures.push(format!(
            "the cancelled request's started attempt must carry the candidate-native control: {forwarded}"
        ));
    }

    drop(client);
    // The relay aborts the in-flight mapped attempt when the downstream client
    // disconnects. Await that single completion under a bounded timeout (as the
    // ordinary disconnect boundary does), instead of polling a completed
    // `JoinHandle` a second time; abort the handle only if it did not exit.
    let exited = tokio::time::timeout(Duration::from_secs(2), &mut handler).await;
    release.notify_one();
    if exited.is_err() {
        handler.abort();
        let _ = handler.await;
    }

    let records = default_usage_store().all_records().unwrap_or_default();
    if !records.is_empty() {
        failures.push(format!(
            "a cancelled mapped request must write zero log rows: {} observed",
            records.len()
        ));
    }

    let stored = crate::ai_gateway::storage::read_config().expect("read config");
    let provider = stored
        .providers
        .iter()
        .find(|provider| provider.id == "p-shared")
        .expect("the provider must persist");
    let mapping = provider
        .mappings
        .iter()
        .find(|mapping| mapping.local_model == "local-chat")
        .expect("the candidate mapping must persist");
    if mapping.auto_disabled {
        failures.push(format!("cancellation must not disable the mapping: {mapping:?}"));
    }
    if mapping.consecutive_failures != 0 {
        failures.push(format!(
            "cancellation must not count a mapping failure: {mapping:?}"
        ));
    }

    assert!(
        failures.is_empty(),
        "cancelled mapped request evidence mismatches: {failures:#?}"
    );
}

// ---------------------------------------------------------------------------
// Step 3 of `20261015-pool-only-reasoning` (REQ-005 / REQ-007, AC-009 / AC-012):
// the status surface exposes the same static reasoning capability metadata
// ---------------------------------------------------------------------------

/// AC-009 / AC-012: `ai_gateway_model_pools_status` reports the pool's static
/// support, default and five-row coverage independently of candidate readiness,
/// using the same keys as the discovery catalog. The 2/3 coverage counts only
/// candidates whose source identity currently resolves.
#[test]
fn pool_status_reports_static_reasoning_capability() {
    super::with_temp_home("pool-reasoning-status", |_home| {
        let high = reasoning(
            "openai_effort",
            Some(vec!["high"]),
            None,
            false,
            vec![enum_mapping("high", "high")],
        );
        let pool_value = json!({
            "id": "pool-status",
            "name": "Status",
            "model": "status-model",
            "enabled": true,
            "default_reasoning_effort": "auto",
            "candidates": [
                candidate("p-shared", "local-chat", "remote-chat", "chat_completions", Some(high.clone())),
                candidate("p-shared", "local-switch", "remote-switch", "chat_completions", Some(high.clone())),
                candidate("p-shared", "missing-local", "missing-upstream", "chat_completions", Some(high)),
            ],
        });
        write_raw_gateway_config(&config_with(vec![shared_provider()], vec![], vec![pool_value]));

        let statuses = ai_gateway_model_pools_status().expect("pool status");
        let value = serde_json::to_value(&statuses).expect("serialize pool status");
        let entry = value
            .as_array()
            .expect("status array")
            .iter()
            .find(|pool| pool["id"] == "pool-status")
            .unwrap_or_else(|| panic!("missing pool status row: {value}"));

        assert_eq!(
            entry["default_reasoning_effort"], "auto",
            "the status surface must expose the canonical default: {entry}"
        );
        assert_eq!(
            entry["reasoning_efforts"],
            json!(["high"]),
            "the status surface must expose the support set: {entry}"
        );
        let coverage = entry["reasoning_coverage"]
            .as_array()
            .unwrap_or_else(|| panic!("status must expose reasoning_coverage: {entry}"));
        assert_eq!(
            coverage
                .iter()
                .map(|row| row["effort"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["low", "medium", "high", "xhigh", "none"],
            "all five rows in canonical order: {entry}"
        );
        let high_row = coverage
            .iter()
            .find(|row| row["effort"] == "high")
            .expect("high coverage row");
        assert_eq!(high_row["mapped_candidates"], 2, "{entry}");
        assert_eq!(high_row["total_candidates"], 3, "{entry}");
    });
}
