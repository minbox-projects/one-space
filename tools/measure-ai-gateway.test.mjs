// Contract tests for tools/measure-ai-gateway.mjs.
//
// The runner is Backend-owned. These tests define the public behavior it must
// provide: a built-ins-only CLI that compiles the measurement binary, runs one
// exact ignored Rust case per fresh process, runs the dedicated frontend
// measurement file, parses the stable metric lines, rejects incomplete
// evidence and applies the AC-010 comparison gates independently per category.
//
// Stable printed contract consumed from the child processes:
//
//   AGW_METRIC {json}
//     {
//       "case": "<required case id>",
//       "phase": "cold" | "warm",
//       "pass": <0 for cold, 1..5 for warm>,
//       "business_samples_ms": [<one finite latency per operation>, ...],
//       "business_wall_ms": <finite whole-pass wall time>,
//       "operations": <non-negative integer>,
//       "dimensions": { ... },
//       "counters": { "<name>": <non-negative integer>, ... }
//     }
//
//   AGW_UI_METRIC {json}
//     {
//       "case": "<required UI case id>",
//       "locale": "en" | "zh",
//       "phase": "cold" | "warm",
//       "pass": <0..5>,
//       "operations": <positive integer>,
//       "counters": { "invoke_calls": <non-negative integer>, ... },
//       "duration_ms": <finite non-negative number>,
//       "proxy": true,
//       "dimensions": { ... }
//     }
//
//   AGW_UI_RESOURCE {json}
//     { "cpu_ms": <finite>, "peak_rss_bytes": <finite>, "scope": "vitest-worker", "includes_preparation": true }
//
// The report emitted on stdout carries at least:
//   status, phase, environment, source, fixtures, sample_plan, commands,
//   cases[], frontend.ui_counts[], worker_resources.rust/frontend.
//
// Exit status is 0 only for a complete, comparable report. A malformed metric,
// a missing case/pass, a missing hard metric, a zero-test run command, a failed
// child command or changed fixture/profile/worker scope is incomplete.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { test } from "node:test";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const RUNNER_PATH = path.join(HERE, "measure-ai-gateway.mjs");

const REQUEST_GATE = "forwarding_chat_sync_32k_c1";
const QUERY_GATE = "usage_query_1k";
const UI_GATE = "ui_usage_ticks_visible";

function meta(id, category, operations, dimensions, extra = {}) {
  return { id, category, operations_per_pass: operations, dimensions, ...extra };
}

// The fixed representative inventory. It is not a Cartesian product: each
// family names the cases an operator can actually compare, while behavior
// families without a new timing stay in BEHAVIOR_CASES.
const CASE_META = {
  config_status_models_small: meta("config_status_models_small", "request", 200, {
    providers: 4,
    mappings_per_provider: 20,
    keys_per_provider: 2,
    operations: "status+models",
  }),
  config_status_models_large: meta("config_status_models_large", "request", 200, {
    providers: 100,
    mappings_per_provider: 100,
    keys_per_provider: 4,
    operations: "status+models",
  }),
  routing_small: meta("routing_small", "request", 200, {
    providers: 4,
    mappings_per_provider: 20,
    request_bytes: 32768,
  }),
  routing_large: meta("routing_large", "request", 200, {
    providers: 100,
    mappings_per_provider: 100,
    request_bytes: 32768,
  }),
  forwarding_chat_sync_32k_c1: meta("forwarding_chat_sync_32k_c1", "request", 200, {
    ingress: "chat",
    upstream: "chat",
    stream: false,
    request_bytes: 32768,
    concurrency: 1,
  }),
  forwarding_chat_sync_32k_c8: meta("forwarding_chat_sync_32k_c8", "request", 200, {
    ingress: "chat",
    upstream: "chat",
    stream: false,
    request_bytes: 32768,
    concurrency: 8,
  }),
  forwarding_chat_sync_32k_c32: meta("forwarding_chat_sync_32k_c32", "request", 200, {
    ingress: "chat",
    upstream: "chat",
    stream: false,
    request_bytes: 32768,
    concurrency: 32,
  }),
  forwarding_chat_sync_256k_c1: meta("forwarding_chat_sync_256k_c1", "request", 200, {
    ingress: "chat",
    upstream: "chat",
    stream: false,
    request_bytes: 262144,
    concurrency: 1,
  }),
  forwarding_responses_sync_32k_c1: meta(
    "forwarding_responses_sync_32k_c1",
    "request",
    200,
    { ingress: "responses", upstream: "responses", stream: false, request_bytes: 32768, concurrency: 1 },
  ),
  forwarding_messages_sync_32k_c1: meta(
    "forwarding_messages_sync_32k_c1",
    "request",
    200,
    { ingress: "messages", upstream: "messages", stream: false, request_bytes: 32768, concurrency: 1 },
  ),
  forwarding_chat_stream_64_c1: meta("forwarding_chat_stream_64_c1", "request", 200, {
    ingress: "chat",
    upstream: "chat",
    stream: true,
    events: 1000,
    fragment_bytes: 64,
    request_bytes: 32768,
    concurrency: 1,
  }),
  forwarding_chat_stream_4096_c1: meta("forwarding_chat_stream_4096_c1", "request", 200, {
    ingress: "chat",
    upstream: "chat",
    stream: true,
    events: 1000,
    fragment_bytes: 4096,
    request_bytes: 262144,
    concurrency: 1,
  }),
  forwarding_responses_stream_64_c1: meta(
    "forwarding_responses_stream_64_c1",
    "request",
    200,
    { ingress: "responses", upstream: "responses", stream: true, events: 1000, fragment_bytes: 64, concurrency: 1 },
  ),
  forwarding_messages_stream_64_c1: meta(
    "forwarding_messages_stream_64_c1",
    "request",
    200,
    { ingress: "messages", upstream: "messages", stream: true, events: 1000, fragment_bytes: 64, concurrency: 1 },
  ),
  direct_mock_chat_sync_32k: meta("direct_mock_chat_sync_32k", "request", 200, {
    ingress: "chat",
    upstream: "chat",
    stream: false,
    request_bytes: 32768,
  }),
  direct_mock_responses_sync_32k: meta("direct_mock_responses_sync_32k", "request", 200, {
    ingress: "responses",
    upstream: "responses",
    stream: false,
    request_bytes: 32768,
  }),
  direct_mock_messages_sync_32k: meta("direct_mock_messages_sync_32k", "request", 200, {
    ingress: "messages",
    upstream: "messages",
    stream: false,
    request_bytes: 32768,
  }),
  direct_mock_chat_stream_64: meta("direct_mock_chat_stream_64", "request", 200, {
    ingress: "chat",
    upstream: "chat",
    stream: true,
    events: 1000,
    fragment_bytes: 64,
  }),
  direct_mock_chat_stream_4096: meta("direct_mock_chat_stream_4096", "request", 200, {
    ingress: "chat",
    upstream: "chat",
    stream: true,
    events: 1000,
    fragment_bytes: 4096,
  }),
  direct_mock_responses_stream_64: meta("direct_mock_responses_stream_64", "request", 200, {
    ingress: "responses",
    upstream: "responses",
    stream: true,
    events: 1000,
    fragment_bytes: 64,
  }),
  direct_mock_messages_stream_64: meta("direct_mock_messages_stream_64", "request", 200, {
    ingress: "messages",
    upstream: "messages",
    stream: true,
    events: 1000,
    fragment_bytes: 64,
  }),
  conversion_chat_to_messages_sync: meta("conversion_chat_to_messages_sync", "request", 200, {
    ingress: "chat",
    upstream: "messages",
    stream: false,
    request_bytes: 32768,
  }),
  conversion_responses_to_messages_sync: meta(
    "conversion_responses_to_messages_sync",
    "request",
    200,
    { ingress: "responses", upstream: "messages", stream: false, request_bytes: 32768 },
  ),
  conversion_chat_to_messages_stream: meta(
    "conversion_chat_to_messages_stream",
    "request",
    200,
    { ingress: "chat", upstream: "messages", stream: true, events: 1000, fragment_bytes: 64 },
  ),
  conversion_responses_to_messages_stream: meta(
    "conversion_responses_to_messages_stream",
    "request",
    200,
    { ingress: "responses", upstream: "messages", stream: true, events: 1000, fragment_bytes: 64 },
  ),
  direct_mock_conversion_chat_to_messages_sync: meta(
    "direct_mock_conversion_chat_to_messages_sync",
    "request",
    200,
    { ingress: "chat", upstream: "messages", stream: false, request_bytes: 32768 },
  ),
  direct_mock_conversion_responses_to_messages_sync: meta(
    "direct_mock_conversion_responses_to_messages_sync",
    "request",
    200,
    { ingress: "responses", upstream: "messages", stream: false, request_bytes: 32768 },
  ),
  direct_mock_conversion_chat_to_messages_stream: meta(
    "direct_mock_conversion_chat_to_messages_stream",
    "request",
    200,
    { ingress: "chat", upstream: "messages", stream: true, events: 1000, fragment_bytes: 64 },
  ),
  direct_mock_conversion_responses_to_messages_stream: meta(
    "direct_mock_conversion_responses_to_messages_stream",
    "request",
    200,
    { ingress: "responses", upstream: "messages", stream: true, events: 1000, fragment_bytes: 64 },
  ),
  pool_decision_10x20: meta("pool_decision_10x20", "request", 200, {
    pools: 10,
    candidates_per_pool: 20,
    operations: "plan_pool_request",
  }),
  pool_chat_to_chat: meta("pool_chat_to_chat", "request", 200, {
    ingress: "chat",
    upstream: "chat",
    pool_candidates: 20,
  }),
  pool_chat_to_responses: meta("pool_chat_to_responses", "request", 200, {
    ingress: "chat",
    upstream: "responses",
    pool_candidates: 20,
  }),
  pool_chat_to_messages: meta("pool_chat_to_messages", "request", 200, {
    ingress: "chat",
    upstream: "messages",
    pool_candidates: 20,
  }),
  pool_responses_to_chat: meta("pool_responses_to_chat", "request", 200, {
    ingress: "responses",
    upstream: "chat",
    pool_candidates: 20,
  }),
  pool_responses_to_responses: meta("pool_responses_to_responses", "request", 200, {
    ingress: "responses",
    upstream: "responses",
    pool_candidates: 20,
  }),
  pool_responses_to_messages: meta("pool_responses_to_messages", "request", 200, {
    ingress: "responses",
    upstream: "messages",
    pool_candidates: 20,
  }),
  usage_log_append_10000x3: meta(
    "usage_log_append_10000x3",
    "storage",
    10000,
    { requests: 10000, attempts_per_request: 3, detail_rows: 30000 },
    { attempts_per_request: 3 },
  ),
  usage_stats_1k: meta("usage_stats_1k", "query", 200, {
    detail_rows: 1000,
    providers: 50,
    models: 200,
    days: 365,
    selectors: "today|yesterday|7d|15d|30d|all",
  }),
  usage_stats_100k: meta("usage_stats_100k", "query", 200, {
    detail_rows: 100000,
    providers: 50,
    models: 200,
    days: 365,
    selectors: "today|yesterday|7d|15d|30d|all",
  }),
  usage_query_1k: meta("usage_query_1k", "query", 200, {
    detail_rows: 1000,
    providers: 50,
    models: 200,
    days: 365,
    operations: "first+last+groups+filters+facets",
  }),
  usage_query_100k: meta("usage_query_100k", "query", 200, {
    detail_rows: 100000,
    providers: 50,
    models: 200,
    days: 365,
    operations: "first+last+groups+filters+facets",
  }),
  provider_history_366: meta("provider_history_366", "query", 200, {
    days: 366,
    coverage: "zero|unavailable|ongoing",
  }),
  provider_history_3650: meta("provider_history_3650", "query", 200, {
    days: 3650,
    coverage: "zero|unavailable|ongoing",
  }),
  template_views: meta("template_views", "management", 200, {
    builtin_templates: true,
    persisted_templates: 0,
  }),
};

const REQUIRED_CASES = Object.values(CASE_META);
const REQUIRED_UI_CASES = [
  { id: "ui_all_tabs_en", locale: "en", operations: 1 },
  { id: "ui_all_tabs_zh", locale: "zh", operations: 1 },
  { id: UI_GATE, locale: "en", operations: 30 },
  { id: "ui_hidden_ticks", locale: "en", operations: 30 },
  { id: "ui_config_burst", locale: "en", operations: 10 },
  { id: "ui_history_3650", locale: "en", operations: 1 },
];

// Families whose behavior is covered by an existing named case rather than a
// new timing measurement. Every name must resolve in the current test tree.
const BEHAVIOR_CASES = {
  configuration_discovery_behavior: [
    "ai_gateway::tests::default_key_prefers_first_enabled_and_advances_on_disable",
    "ai_gateway::tests::auto_disabled_state_persists_and_separates_from_user_enabled",
    "ai_gateway::tests::resolve_model_for_protocol_prefers_matching_rows_and_rejects_other_protocols",
  ],
  forwarding_conversion_behavior: [
    "ai_gateway::tests::anthropic_conversion::chat_entry_converts_ordinary_conversation_to_anthropic_messages",
    "ai_gateway::tests::anthropic_conversion::responses_entry_converts_ordinary_conversation_to_anthropic_messages",
    "ai_gateway::tests::anthropic_conversion::converted_chat_stream_emits_incremental_text_and_tools_then_done_once",
  ],
  pool_continuation_behavior: [
    "ai_gateway::tests::pool_context_hold::chat_non_streaming_tool_chain_prefers_the_preceding_candidate",
    "ai_gateway::tests::pool_context_hold::isolated_hold_stays_eligible_at_the_idle_boundary_and_expires_beyond_it",
  ],
  stream_fragmentation_behavior: [
    "ai_gateway::tests::output_timing::observer_buffers_fragmented_crlf_and_multiline_events",
    "ai_gateway::tests::output_timing::observer_recognizes_a_final_unterminated_event_at_finish",
    "ai_gateway::tests::anthropic_conversion::converted_stream_without_trailing_blank_line_still_terminates_once",
  ],
  http_stream_behavior: [
    "ai_gateway::tests::pool_context_hold::a_mid_stream_disconnect_publishes_no_hold_and_plays_no_replay",
    "ai_gateway::tests::pool_context_hold::an_oversized_streaming_request_still_forwards_with_normal_routing",
  ],
  usage_storage_behavior: [
    "ai_gateway::tests::usage_store::one_request_batch_stores_all_attempts_and_exactly_one_terminal_row",
    "ai_gateway::tests::usage_store::retention_one_and_365_bound_detail_rows_and_keep_daily_summaries",
    "ai_gateway::tests::provider_usage::provider_history_calendar_classifies_retained_unavailable_and_covered_zero",
    "ai_gateway::tests::provider_usage::provider_history_multi_year_keeps_every_date",
  ],
  management_lifecycle_behavior: [
    "ai_gateway::tests::quota::provider_quota_fetches_on_miss_caches_success_and_forces_refresh",
    "ai_gateway::tests::quota::quota_command_path_is_read_only_and_uses_the_shared_cache",
    "ai_gateway::tests::go_usage::provider_fetches_on_miss_caches_success_and_forces_refresh",
    "ai_gateway::tests::auto_refresh::batch_runs_templates_sequentially_in_order",
    "ai_gateway::tests::auto_refresh::second_concurrent_batch_is_skipped_while_one_is_in_flight",
    "ai_gateway::tests::daily_reset::daily_reset_scanner_startup_periodic_catch_up_and_idempotent_stop",
    "ai_gateway::tests::runtime_lifecycle::faulted_listener_reports_stopped_and_restarts_on_the_same_port",
  ],
  terminal_sync_behavior: [
    "ai_gateway::tests::claude_terminal_sync::template_refresh_skips_invalid_claude_and_still_upserts_opencode_codex",
    "ai_gateway::tests::model_pool_terminal_sync::terminal_targets_mark_opencode_pending_when_pool_membership_changes",
  ],
  frontend_draft_navigation_behavior: [
    "src/components/AiGateway/ProviderDetailDialog.test.tsx::runtime_provider_merge_updates_auto_disabled_presentation_without_losing_unsaved_edits",
    "src/components/AiGateway/ModelPoolDialog.test.tsx::preserves the draft and surfaces an actionable error when a new source disappears on save",
    "src/components/AiGateway/UsageLogsPanel.test.tsx::返回控件回到同一服务商历史",
    "src/components/AiGateway/ProviderUsageHistoryPanel.test.tsx::长历史保留每一天的完整日期（图表与表格均不截断）",
  ],
};

let runner = null;
let runnerLoadError = null;
try {
  runner = await import(pathToFileURL(RUNNER_PATH).href);
} catch (error) {
  runnerLoadError = error;
}

function requireRunner() {
  assert.ok(
    runner,
    `tools/measure-ai-gateway.mjs is missing or unloadable: ${runnerLoadError?.message ?? "unknown error"}`,
  );
  return runner;
}

function filled(value, count) {
  return Array.from({ length: count }, () => value);
}

function boundarySamples(count, value, outlier) {
  return [...filled(value, count - 10), ...filled(outlier, 10)];
}

function backendSamples(id, phase) {
  const meta = CASE_META[id];
  if (id === REQUEST_GATE) {
    return phase === "baseline" ? filled(100, 200) : filled(79, 200);
  }
  if (id === QUERY_GATE) {
    return phase === "baseline" ? filled(100, 200) : filled(75, 200);
  }
  return filled(50, meta.operations_per_pass);
}

function duplicateCounter(id, phase) {
  if (id === REQUEST_GATE || id === QUERY_GATE) {
    return phase === "baseline" ? 200 : 100;
  }
  return 100;
}

function metric(id, phase, pass, samples = backendSamples(id, phase)) {
  return {
    case: id,
    phase,
    pass,
    business_samples_ms: samples,
    business_wall_ms: samples.reduce((total, value) => total + value, 0),
    operations: CASE_META[id].operations_per_pass,
    dimensions: { ...CASE_META[id].dimensions },
    counters: {
      duplicate_operations: duplicateCounter(id, phase),
      measured_operations: CASE_META[id].operations_per_pass,
    },
  };
}

function completeCase(id, phase) {
  return {
    case: id,
    category: CASE_META[id].category,
    status: "measured",
    operations_per_pass: CASE_META[id].operations_per_pass,
    attempts_per_request: CASE_META[id].attempts_per_request ?? null,
    dimensions: { ...CASE_META[id].dimensions },
    cold: metric(id, phase, 0),
    warm: Array.from({ length: 5 }, (_, index) => metric(id, phase, index + 1)),
  };
}

function uiCounter(id, phase) {
  if (id === UI_GATE) return phase === "baseline" ? 10 : 5;
  return 7;
}

function uiPass(id, phase, pass) {
  const meta = REQUIRED_UI_CASES.find((entry) => entry.id === id);
  return {
    case: id,
    locale: meta.locale,
    phase: pass === 0 ? "cold" : "warm",
    pass,
    operations: meta.operations,
    counters: {
      invoke_calls: uiCounter(id, phase),
      usage_stats_calls: id === UI_GATE ? meta.operations : 0,
    },
    duration_ms: 10 + pass,
    proxy: true,
    dimensions: { locale: meta.locale, fixture: "gateway-large-v1" },
  };
}

function environment() {
  return {
    os: "macos",
    cpu: "Apple M1 Max",
    architecture: "aarch64",
    rustc: "1.93.1",
    node: "v22.14.0",
    profile: "test",
  };
}

function fixtures() {
  return {
    profile: "ai-gateway-performance-v1",
    dimensions: Object.fromEntries(
      Object.values(CASE_META).map((entry) => [entry.id, { ...entry.dimensions }]),
    ),
  };
}

function commands() {
  return [
    { name: "rust_build", kind: "build", command: "cargo", args: [], exit_code: 0, duration_ms: 10, tests_run: 0 },
    { name: "rust_case", kind: "run", command: "cargo", args: [], exit_code: 0, duration_ms: 10, tests_run: 44 },
    { name: "frontend", kind: "run", command: "npm", args: [], exit_code: 0, duration_ms: 10, tests_run: 6 },
  ];
}

function workerResources() {
  return {
    rust: {
      cpu_ms: 1000,
      peak_rss_bytes: 100 * 1024 * 1024,
      scope: "rust-test-worker",
      includes_preparation: true,
    },
    frontend: {
      cpu_ms: 500,
      peak_rss_bytes: 200 * 1024 * 1024,
      scope: "vitest-worker",
      includes_preparation: true,
    },
  };
}

function report(phase) {
  return {
    status: "complete",
    phase,
    environment: environment(),
    source: { git_revision: "32a4c2f2f4ad05522cb0036a6cfe482c56969fa7" },
    fixtures: fixtures(),
    sample_plan: { cold: 1, warm: 5 },
    commands: commands(),
    cases: Object.keys(CASE_META).map((id) => completeCase(id, phase)),
    frontend: {
      ui_counts: REQUIRED_UI_CASES.flatMap((entry) =>
        Array.from({ length: 6 }, (_, pass) => uiPass(entry.id, phase, pass)),
      ),
    },
    worker_resources: workerResources(),
  };
}

function clone(value) {
  return structuredClone(value);
}

function replaceSamples(target, id, samples, counters = {}) {
  const entry = target.cases.find((item) => item.case === id);
  entry.cold.business_samples_ms = samples;
  entry.cold.business_wall_ms = samples.reduce((total, value) => total + value, 0);
  Object.assign(entry.cold.counters, counters);
  for (const warm of entry.warm) {
    warm.business_samples_ms = samples;
    warm.business_wall_ms = samples.reduce((total, value) => total + value, 0);
    Object.assign(warm.counters, counters);
  }
}

test("runner exports the fixed measurement and behavior inventory", () => {
  const loaded = requireRunner();
  for (const name of [
    "parseGatewayMetricLine",
    "parseUiMetricLine",
    "parseUiResourceLine",
    "validateGatewayReport",
    "compareGatewayReports",
  ]) {
    assert.equal(typeof loaded[name], "function", `${name} must be exported`);
  }
  assert.ok(Array.isArray(loaded.REQUIRED_CASES));
  assert.ok(Array.isArray(loaded.REQUIRED_UI_CASES));

  const cases = new Map(loaded.REQUIRED_CASES.map((entry) => [entry.id, entry]));
  for (const expected of REQUIRED_CASES) {
    const actual = cases.get(expected.id);
    assert.ok(actual, `required measurement case missing: ${expected.id}`);
    assert.equal(actual.category, expected.category, `category mismatch: ${expected.id}`);
    assert.equal(actual.operations_per_pass, expected.operations_per_pass);
  }
  const append = cases.get("usage_log_append_10000x3");
  assert.equal(append.operations_per_pass, 10000);
  assert.equal(append.attempts_per_request, 3);

  const ui = new Set(loaded.REQUIRED_UI_CASES.map((entry) => entry.id));
  for (const expected of REQUIRED_UI_CASES) {
    assert.ok(ui.has(expected.id), `required UI case missing: ${expected.id}`);
  }

  for (const [family, tests] of Object.entries(BEHAVIOR_CASES)) {
    assert.deepEqual(loaded.BEHAVIOR_CASES[family], tests, `behavior family drift: ${family}`);
  }
});

test("fixed child command catalog runs one exact case in a fresh process", () => {
  const { FIXED_COMMANDS } = requireRunner();
  assert.deepEqual(FIXED_COMMANDS.rust_build, [
    "test",
    "--manifest-path",
    "src-tauri/Cargo.toml",
    "--lib",
    "gateway_performance",
    "--no-run",
    "--message-format=json",
  ]);
  assert.deepEqual(FIXED_COMMANDS.rust_case, [
    "--ignored",
    "--exact",
    "--nocapture",
    "--test-threads=1",
  ]);
  assert.deepEqual(FIXED_COMMANDS.frontend, [
    "test",
    "--",
    "src/components/AiGateway/AiGateway.performance.test.tsx",
  ]);
});

test("parses one metric object per prefixed line and rejects malformed JSON", () => {
  const { parseGatewayMetricLine, parseUiMetricLine, parseUiResourceLine } = requireRunner();

  const parsed = parseGatewayMetricLine(`noise\nAGW_METRIC ${JSON.stringify(metric(REQUEST_GATE, "baseline", 0))}`);
  assert.equal(parsed.case, REQUEST_GATE);
  assert.equal(parsed.phase, "cold");
  assert.equal(parsed.business_samples_ms.length, 200);

  const ui = parseUiMetricLine(`AGW_UI_METRIC ${JSON.stringify(uiPass(UI_GATE, "baseline", 0))}`);
  assert.equal(ui.case, UI_GATE);
  assert.equal(ui.proxy, true);

  const resource = parseUiResourceLine(
    `AGW_UI_RESOURCE ${JSON.stringify({ cpu_ms: 1, peak_rss_bytes: 2, scope: "vitest-worker", includes_preparation: true })}`,
  );
  assert.equal(resource.scope, "vitest-worker");

  assert.equal(parseGatewayMetricLine("not a metric"), null);
  assert.throws(() => parseGatewayMetricLine("AGW_METRIC {not-json}"), /malformed|JSON|metric/i);
  assert.throws(() => parseUiMetricLine("AGW_UI_METRIC [1,2,3]"), /malformed|object|metric/i);
});

test("rejects missing cases, missing passes and malformed per-operation samples", () => {
  const { validateGatewayReport } = requireRunner();

  const missingCase = report("baseline");
  missingCase.cases = missingCase.cases.filter((entry) => entry.case !== "config_status_models_small");
  const missing = validateGatewayReport(missingCase);
  assert.equal(missing.status, "incomplete");
  assert.ok(missing.failures.some((failure) => /missing_case:config_status_models_small/.test(failure)));

  const missingWarm = report("baseline");
  missingWarm.cases.find((entry) => entry.case === REQUEST_GATE).warm.pop();
  const short = validateGatewayReport(missingWarm);
  assert.equal(short.status, "incomplete");
  assert.ok(short.failures.some((failure) => /sample_count|warm/.test(failure)));

  const malformed = report("baseline");
  malformed.cases.find((entry) => entry.case === REQUEST_GATE).cold.business_samples_ms[0] = "10";
  const malformedResult = validateGatewayReport(malformed);
  assert.equal(malformedResult.status, "incomplete");
  assert.ok(malformedResult.failures.some((failure) => /business_samples|sample/.test(failure)));

  const shortSamples = report("baseline");
  const gate = shortSamples.cases.find((entry) => entry.case === REQUEST_GATE);
  gate.cold.business_samples_ms = filled(100, 199);
  const shortResult = validateGatewayReport(shortSamples);
  assert.equal(shortResult.status, "incomplete");
  assert.ok(shortResult.failures.some((failure) => /sample_count|operations/.test(failure)));
});

test("enforces the operation floor and the 10,000-batch append case", () => {
  const { validateGatewayReport } = requireRunner();

  const tooFew = report("baseline");
  const target = tooFew.cases.find((entry) => entry.case === REQUEST_GATE);
  target.cold.operations = 199;
  for (const warm of target.warm) warm.operations = 199;
  const tooFewResult = validateGatewayReport(tooFew);
  assert.equal(tooFewResult.status, "incomplete");
  assert.ok(tooFewResult.failures.some((failure) => /operations_floor/.test(failure)));

  const append = report("baseline");
  const appendCase = append.cases.find((entry) => entry.case === "usage_log_append_10000x3");
  assert.equal(appendCase.cold.operations, 10000);
  assert.equal(appendCase.cold.business_samples_ms.length, 10000);
  assert.ok(
    !validateGatewayReport(append).failures.some((failure) =>
      /operations_floor:usage_log_append_10000x3/.test(failure),
    ),
  );
});

test("rejects failed children, zero-test run coverage and missing hard evidence", () => {
  const { validateGatewayReport } = requireRunner();

  const failed = report("baseline");
  failed.commands.find((entry) => entry.name === "frontend").exit_code = 1;
  const failedResult = validateGatewayReport(failed);
  assert.equal(failedResult.status, "incomplete");
  assert.ok(failedResult.failures.some((failure) => /child_failed/.test(failure)));

  const zeroTests = report("baseline");
  zeroTests.commands.find((entry) => entry.kind === "run" && entry.name === "frontend").tests_run = 0;
  const zeroResult = validateGatewayReport(zeroTests);
  assert.equal(zeroResult.status, "incomplete");
  assert.ok(zeroResult.failures.some((failure) => /zero_test|tests_run/.test(failure)));

  for (const [label, mutate] of [
    ["worker_resources", (value) => delete value.worker_resources.rust],
    ["environment", (value) => delete value.environment.cpu],
    ["source", (value) => delete value.source.git_revision],
    ["fixtures", (value) => delete value.fixtures.profile],
    ["sample_plan", (value) => delete value.sample_plan],
  ]) {
    const invalid = report("baseline");
    mutate(invalid);
    const result = validateGatewayReport(invalid);
    assert.equal(result.status, "incomplete", `${label} must be required`);
    assert.ok(result.failures.some((failure) => new RegExp(label).test(failure)));
  }
});

test("a complete comparable baseline passes validation", () => {
  const { validateGatewayReport } = requireRunner();
  const validation = validateGatewayReport(report("baseline"));
  assert.equal(validation.status, "complete", validation.failures.join("\n"));
});

test("request and query gates are independent and each needs a positive baseline", () => {
  const { compareGatewayReports } = requireRunner();

  const baseline = report("baseline");
  const final = report("final");
  const pass = compareGatewayReports(baseline, final);
  assert.equal(pass.status, "complete", pass.failures?.join("\n"));
  assert.equal(pass.gates.request_improvement.passed, true);
  assert.equal(pass.gates.query_improvement.passed, true);
  assert.equal(pass.gates.ui_duplicate_work.passed, true);

  const requestMiss = clone(final);
  replaceSamples(requestMiss, REQUEST_GATE, filled(81, 200), { duplicate_operations: 200 });
  const requestMissResult = compareGatewayReports(baseline, requestMiss);
  assert.equal(requestMissResult.status, "incomplete");
  assert.ok(requestMissResult.failures.some((failure) => /request/i.test(failure)));

  const queryMiss = clone(final);
  replaceSamples(queryMiss, QUERY_GATE, filled(81, 200), { duplicate_operations: 200 });
  const queryMissResult = compareGatewayReports(baseline, queryMiss);
  assert.equal(queryMissResult.status, "incomplete");
  assert.ok(queryMissResult.failures.some((failure) => /query/i.test(failure)));

  const countOnly = clone(final);
  replaceSamples(countOnly, REQUEST_GATE, filled(100, 200), { duplicate_operations: 100 });
  const countResult = compareGatewayReports(baseline, countOnly);
  assert.equal(countResult.status, "complete", countResult.failures?.join("\n"));
  assert.equal(countResult.gates.request_improvement.mode, "duplicate_operations");

  const zeroBaseline = clone(baseline);
  replaceSamples(zeroBaseline, REQUEST_GATE, filled(100, 200), { duplicate_operations: 0 });
  const zeroFinal = clone(final);
  replaceSamples(zeroFinal, REQUEST_GATE, filled(100, 200), { duplicate_operations: 100 });
  const zeroResult = compareGatewayReports(zeroBaseline, zeroFinal);
  assert.equal(zeroResult.status, "incomplete");
  assert.ok(zeroResult.failures.some((failure) => /baseline|positive/i.test(failure)));
});

test("P95 and peak RSS compare against exact inclusive bounds", () => {
  const { compareGatewayReports } = requireRunner();
  const baseline = report("baseline");

  const p95Baseline = clone(baseline);
  replaceSamples(p95Baseline, "forwarding_responses_sync_32k_c1", filled(100, 200));
  const p95Pass = clone(report("final"));
  replaceSamples(p95Pass, "forwarding_responses_sync_32k_c1", boundarySamples(200, 50, 110));
  const p95PassResult = compareGatewayReports(p95Baseline, p95Pass);
  assert.equal(p95PassResult.status, "complete", p95PassResult.failures?.join("\n"));
  assert.equal(p95PassResult.gates.p95.passed, true);

  const p95Fail = clone(report("final"));
  replaceSamples(p95Fail, "forwarding_responses_sync_32k_c1", boundarySamples(200, 50, 111));
  const p95FailResult = compareGatewayReports(p95Baseline, p95Fail);
  assert.equal(p95FailResult.status, "incomplete");
  assert.ok(p95FailResult.failures.some((failure) => /p95/i.test(failure)));

  const rssPass = clone(report("final"));
  rssPass.worker_resources.rust.peak_rss_bytes = 110 * 1024 * 1024;
  const rssPassResult = compareGatewayReports(baseline, rssPass);
  assert.equal(rssPassResult.status, "complete", rssPassResult.failures?.join("\n"));

  const rssFail = clone(report("final"));
  rssFail.worker_resources.rust.peak_rss_bytes = 110 * 1024 * 1024 + 1;
  const rssFailResult = compareGatewayReports(baseline, rssFail);
  assert.equal(rssFailResult.status, "incomplete");
  assert.ok(rssFailResult.failures.some((failure) => /rss/i.test(failure)));
});

test("comparison rejects changed fixtures, profile or worker scope", () => {
  const { compareGatewayReports } = requireRunner();
  const baseline = report("baseline");

  const changedFixture = clone(report("final"));
  changedFixture.fixtures.dimensions[REQUEST_GATE].request_bytes = 65536;
  const changedCase = changedFixture.cases.find((entry) => entry.case === REQUEST_GATE);
  changedCase.dimensions.request_bytes = 65536;
  changedCase.cold.dimensions.request_bytes = 65536;
  for (const warm of changedCase.warm) warm.dimensions.request_bytes = 65536;
  const fixtureResult = compareGatewayReports(baseline, changedFixture);
  assert.equal(fixtureResult.status, "incomplete");
  assert.ok(fixtureResult.failures.some((failure) => /fixture|dimension/i.test(failure)));

  const changedProfile = clone(report("final"));
  changedProfile.fixtures.profile = "another-profile";
  const profileResult = compareGatewayReports(baseline, changedProfile);
  assert.equal(profileResult.status, "incomplete");
  assert.ok(profileResult.failures.some((failure) => /profile/i.test(failure)));

  const changedScope = clone(report("final"));
  changedScope.worker_resources.rust.scope = "another-worker";
  const scopeResult = compareGatewayReports(baseline, changedScope);
  assert.equal(scopeResult.status, "incomplete");
  assert.ok(scopeResult.failures.some((failure) => /scope|worker/i.test(failure)));

  const changedEnvironment = clone(report("final"));
  changedEnvironment.environment.cpu = "Another CPU";
  const environmentResult = compareGatewayReports(baseline, changedEnvironment);
  assert.equal(environmentResult.status, "incomplete");
  assert.ok(environmentResult.failures.some((failure) => /environment|cpu/i.test(failure)));
});

test("UI gate needs a positive baseline and a 50% exact duplicate-work reduction", () => {
  const { compareGatewayReports } = requireRunner();
  const baseline = report("baseline");

  const pass = compareGatewayReports(baseline, report("final"));
  assert.equal(pass.status, "complete", pass.failures?.join("\n"));
  assert.equal(pass.gates.ui_duplicate_work.passed, true);

  const miss = clone(report("final"));
  for (const entry of miss.frontend.ui_counts.filter((item) => item.case === UI_GATE)) {
    entry.counters.invoke_calls = 6;
  }
  const missResult = compareGatewayReports(baseline, miss);
  assert.equal(missResult.status, "incomplete");
  assert.ok(missResult.failures.some((failure) => /ui/i.test(failure)));

  const zeroBaseline = clone(baseline);
  for (const entry of zeroBaseline.frontend.ui_counts.filter((item) => item.case === UI_GATE)) {
    entry.counters.invoke_calls = 0;
  }
  const zeroResult = compareGatewayReports(zeroBaseline, report("final"));
  assert.equal(zeroResult.status, "incomplete");
  assert.ok(zeroResult.failures.some((failure) => /ui|positive/i.test(failure)));

  const missingWarm = clone(report("final"));
  missingWarm.frontend.ui_counts = missingWarm.frontend.ui_counts.filter(
    (item) => !(item.case === UI_GATE && item.pass === 5),
  );
  const warmResult = compareGatewayReports(baseline, missingWarm);
  assert.equal(warmResult.status, "incomplete");
  assert.ok(warmResult.failures.some((failure) => /ui|sample_count|warm/i.test(failure)));
});

test("CLI help exits 0 and an incomplete run never exits 0", () => {
  requireRunner();

  const help = spawnSync(process.execPath, [RUNNER_PATH, "--help"], {
    encoding: "utf8",
    timeout: 15_000,
  });
  assert.equal(help.status, 0, help.stderr);
  assert.match(help.stdout, /AGW_METRIC|AGW_UI_METRIC/);

  const bad = spawnSync(
    process.execPath,
    [RUNNER_PATH, "--phase", "baseline", "--fixtures", "/nonexistent"],
    { encoding: "utf8", timeout: 15_000 },
  );
  assert.notEqual(bad.status, 0, "an invalid/incomplete invocation must not exit 0");
  assert.equal(bad.signal, null);
});

// AC-010 gate selection: any representative case in the category may prove the
// gain, while direct-mock controls stay separate and cannot satisfy a product
// request/query gate. The UI gate may choose any UI case and exact duplicate
// query/render/formatter counter with a positive baseline.
const REQUEST_LOCAL_CASES = REQUIRED_CASES.filter(
  (entry) => entry.category === "request" && !entry.id.startsWith("direct_mock_"),
);
const DIRECT_MOCK_CASES = REQUIRED_CASES.filter((entry) => entry.id.startsWith("direct_mock_"));
const QUERY_CASES = REQUIRED_CASES.filter((entry) => entry.category === "query");
const UI_COUNTERS = [
  "invoke_calls",
  "usage_stats_calls",
  "request_logs_calls",
  "config_calls",
  "provider_history_calls",
  "pool_status_calls",
  "usage_providers_calls",
  "catch_up_usage_stats_calls",
];

function measureAll(target, cases, samples, duplicateOperations) {
  for (const entry of cases) {
    replaceSamples(target, entry.id, filled(samples, entry.operations_per_pass), {
      duplicate_operations: duplicateOperations,
    });
  }
}

function gateNumber(gate, side, counter) {
  const names = [
    side,
    `${side}_value`,
    `${side}_count`,
    counter ? `${side}_${counter}` : null,
    `${side}_median_ms`,
    `${side}_duplicate_operations`,
  ].filter(Boolean);
  for (const name of names) {
    if (typeof gate?.[name] === "number") return gate[name];
  }
  return undefined;
}

function assertGateSelection(gate, { caseId, counter, baseline, final }) {
  assert.equal(gate?.passed, true, JSON.stringify(gate));
  assert.equal(gate?.case, caseId, JSON.stringify(gate));
  if (counter !== undefined) {
    assert.equal(gate?.counter, counter, JSON.stringify(gate));
  }
  assert.equal(gateNumber(gate, "baseline", counter), baseline, JSON.stringify(gate));
  assert.equal(gateNumber(gate, "final", counter), final, JSON.stringify(gate));
}

test("request gate selects any gateway-local request case meeting the threshold", () => {
  const { compareGatewayReports } = requireRunner();
  const baseline = report("baseline");
  const final = report("final");

  measureAll(baseline, REQUEST_LOCAL_CASES, 100, 200);
  measureAll(final, REQUEST_LOCAL_CASES, 100, 200);
  replaceSamples(baseline, REQUEST_GATE, filled(100, 200), { duplicate_operations: 200 });
  replaceSamples(final, REQUEST_GATE, filled(105, 200), { duplicate_operations: 200 });
  replaceSamples(baseline, "routing_small", filled(100, 200), { duplicate_operations: 200 });
  replaceSamples(final, "routing_small", filled(49.3, 200), { duplicate_operations: 200 });
  replaceSamples(final, "direct_mock_chat_sync_32k", filled(10, 200), {
    duplicate_operations: 100,
  });

  const result = compareGatewayReports(baseline, final);
  assert.equal(result.status, "complete", result.failures.join("\n"));
  assertGateSelection(result.gates.request_improvement, {
    caseId: "routing_small",
    baseline: 100,
    final: 49.3,
  });
});

test("request gate rejects direct-mock-only gains when every gateway-local request case misses", () => {
  const { compareGatewayReports } = requireRunner();
  const baseline = report("baseline");
  const final = report("final");

  measureAll(baseline, REQUEST_LOCAL_CASES, 100, 200);
  measureAll(final, REQUEST_LOCAL_CASES, 100, 200);
  measureAll(baseline, DIRECT_MOCK_CASES, 100, 200);
  measureAll(final, DIRECT_MOCK_CASES, 10, 100);

  const result = compareGatewayReports(baseline, final);
  assert.equal(result.status, "incomplete");
  assert.equal(result.gates.request_improvement.passed, false);
  assert.ok(result.failures.some((failure) => /request/i.test(failure)));
  assert.ok(
    !String(result.gates.request_improvement.case ?? "").startsWith("direct_mock_"),
    "direct-mock controls must not satisfy the product request gate",
  );
  assert.ok(final.cases.some((entry) => entry.case === "direct_mock_chat_sync_32k"));
});

test("query gate selects any database query case meeting the threshold", () => {
  const { compareGatewayReports } = requireRunner();
  const baseline = report("baseline");
  const final = report("final");

  measureAll(baseline, QUERY_CASES, 100, 200);
  measureAll(final, QUERY_CASES, 100, 200);
  replaceSamples(baseline, QUERY_GATE, filled(100, 200), { duplicate_operations: 200 });
  replaceSamples(final, QUERY_GATE, filled(100, 200), { duplicate_operations: 200 });
  replaceSamples(baseline, "usage_stats_1k", filled(100, 200), { duplicate_operations: 200 });
  replaceSamples(final, "usage_stats_1k", filled(75, 200), { duplicate_operations: 200 });

  const result = compareGatewayReports(baseline, final);
  assert.equal(result.status, "complete", result.failures.join("\n"));
  assertGateSelection(result.gates.query_improvement, {
    caseId: "usage_stats_1k",
    baseline: 100,
    final: 75,
  });
});

test("query gate fails when every database query case misses", () => {
  const { compareGatewayReports } = requireRunner();
  const baseline = report("baseline");
  const final = report("final");

  measureAll(baseline, QUERY_CASES, 100, 200);
  measureAll(final, QUERY_CASES, 100, 200);

  const result = compareGatewayReports(baseline, final);
  assert.equal(result.status, "incomplete");
  assert.equal(result.gates.query_improvement.passed, false);
  assert.ok(result.failures.some((failure) => /query/i.test(failure)));
});

test("UI gate selects any positive exact duplicate counter across UI cases", () => {
  const { compareGatewayReports } = requireRunner();
  const baseline = report("baseline");
  const final = report("final");

  for (const entry of baseline.frontend.ui_counts) {
    if (entry.case === UI_GATE) {
      entry.counters.invoke_calls = 120;
      entry.counters.request_logs_calls = 30;
    }
  }
  for (const entry of final.frontend.ui_counts) {
    if (entry.case === UI_GATE) {
      entry.counters.invoke_calls = 90;
      entry.counters.request_logs_calls = 0;
    }
  }

  const requestPass = compareGatewayReports(baseline, final);
  assert.equal(requestPass.status, "complete", requestPass.failures.join("\n"));
  assertGateSelection(requestPass.gates.ui_duplicate_work, {
    caseId: UI_GATE,
    counter: "request_logs_calls",
    baseline: 30,
    final: 0,
  });

  const burstBaseline = clone(report("baseline"));
  const burstFinal = clone(report("final"));
  for (const entry of burstBaseline.frontend.ui_counts) {
    if (entry.case === UI_GATE) entry.counters.invoke_calls = 120;
    if (entry.case === "ui_config_burst") entry.counters.invoke_calls = 50;
  }
  for (const entry of burstFinal.frontend.ui_counts) {
    if (entry.case === UI_GATE) entry.counters.invoke_calls = 90;
    if (entry.case === "ui_config_burst") entry.counters.invoke_calls = 10;
  }

  const burstPass = compareGatewayReports(burstBaseline, burstFinal);
  assert.equal(burstPass.status, "complete", burstPass.failures.join("\n"));
  assertGateSelection(burstPass.gates.ui_duplicate_work, {
    caseId: "ui_config_burst",
    counter: "invoke_calls",
    baseline: 50,
    final: 10,
  });
});

test("UI gate fails when every positive exact duplicate counter misses or starts at zero", () => {
  const { compareGatewayReports } = requireRunner();
  const baseline = report("baseline");
  const final = report("final");

  for (const entry of baseline.frontend.ui_counts) {
    for (const counter of UI_COUNTERS) entry.counters[counter] = 10;
  }
  for (const entry of final.frontend.ui_counts) {
    for (const counter of UI_COUNTERS) entry.counters[counter] = 9;
  }
  for (const entry of baseline.frontend.ui_counts) {
    if (entry.case === "ui_hidden_ticks") entry.counters.invoke_calls = 0;
  }
  for (const entry of final.frontend.ui_counts) {
    if (entry.case === "ui_hidden_ticks") entry.counters.invoke_calls = 0;
  }

  const result = compareGatewayReports(baseline, final);
  assert.equal(result.status, "incomplete");
  assert.equal(result.gates.ui_duplicate_work.passed, false);
  assert.ok(result.failures.some((failure) => /ui/i.test(failure)));
});
