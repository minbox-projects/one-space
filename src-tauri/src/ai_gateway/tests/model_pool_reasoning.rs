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

use super::{pool_key, pool_provider, read_raw_gateway_config, write_raw_gateway_config};
use crate::ai_gateway::commands::ai_gateway_upsert_model_pool;
use crate::ai_gateway::storage::{config_path, modify_config};
use crate::ai_gateway::{GatewayConfig, ModelPool, GATEWAY_CONFIG_SCHEMA_VERSION};
use serde_json::{json, Value};

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
