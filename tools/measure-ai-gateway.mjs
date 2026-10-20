#!/usr/bin/env node
// Whole-gateway baseline/final measurement runner for the AI Gateway.
//
// The runner uses only Node built-ins. It compiles the ignored Rust
// `gateway_performance` test binary once, runs one exact case per fresh child
// process serially, runs the dedicated frontend measurement file once, parses
// the stable metric lines and emits one JSON report. A malformed metric, a
// missing case or pass, a failed child, a zero-test run or a missing hard
// resource makes the report `incomplete` and the process exit non-zero.
//
// Stable child output:
//   AGW_METRIC {json}
//   AGW_UI_METRIC {json}
//   AGW_UI_RESOURCE {json}
//
// Process-cold means a fresh process-owned state, not an operating-system cold
// disk cache. Rust CPU/RSS are read from `/usr/bin/time -l` around the actual
// worker process and therefore include that process's preparation. Frontend
// CPU/RSS come from the vitest worker's own resource line. No request body,
// header or credential value is printed or saved.

import { spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(HERE, "..");
const DEFAULT_TIMEOUT_MS = 900_000;
const MAX_BUFFER = 256 * 1024 * 1024;
const METRIC_PREFIX = "AGW_METRIC ";
const UI_METRIC_PREFIX = "AGW_UI_METRIC ";
const UI_RESOURCE_PREFIX = "AGW_UI_RESOURCE ";
const FIXTURE_PROFILE = "ai-gateway-performance-v1";
const REVISION_ENV = "AI_GATEWAY_MEASUREMENT_REVISION";

function meta(id, category, operations, dimensions, extra = {}) {
  return { id, category, operations_per_pass: operations, dimensions, ...extra };
}

// Fixed representative inventory. It names comparable workloads rather than a
// Cartesian product; behavior-only families live in BEHAVIOR_CASES.
export const REQUIRED_CASES = [
  meta("config_status_models_small", "request", 200, {
    providers: 4,
    mappings_per_provider: 20,
    keys_per_provider: 2,
    operations: "status+models",
  }),
  meta("config_status_models_large", "request", 200, {
    providers: 100,
    mappings_per_provider: 100,
    keys_per_provider: 4,
    operations: "status+models",
  }),
  meta("routing_small", "request", 200, {
    providers: 4,
    mappings_per_provider: 20,
    request_bytes: 32768,
  }),
  meta("routing_large", "request", 200, {
    providers: 100,
    mappings_per_provider: 100,
    request_bytes: 32768,
  }),
  meta("forwarding_chat_sync_32k_c1", "request", 200, {
    ingress: "chat",
    upstream: "chat",
    stream: false,
    request_bytes: 32768,
    concurrency: 1,
  }),
  meta("forwarding_chat_sync_32k_c8", "request", 200, {
    ingress: "chat",
    upstream: "chat",
    stream: false,
    request_bytes: 32768,
    concurrency: 8,
  }),
  meta("forwarding_chat_sync_32k_c32", "request", 200, {
    ingress: "chat",
    upstream: "chat",
    stream: false,
    request_bytes: 32768,
    concurrency: 32,
  }),
  meta("forwarding_chat_sync_256k_c1", "request", 200, {
    ingress: "chat",
    upstream: "chat",
    stream: false,
    request_bytes: 262144,
    concurrency: 1,
  }),
  meta("forwarding_responses_sync_32k_c1", "request", 200, {
    ingress: "responses",
    upstream: "responses",
    stream: false,
    request_bytes: 32768,
    concurrency: 1,
  }),
  meta("forwarding_messages_sync_32k_c1", "request", 200, {
    ingress: "messages",
    upstream: "messages",
    stream: false,
    request_bytes: 32768,
    concurrency: 1,
  }),
  meta("forwarding_chat_stream_64_c1", "request", 200, {
    ingress: "chat",
    upstream: "chat",
    stream: true,
    events: 1000,
    fragment_bytes: 64,
    request_bytes: 32768,
    concurrency: 1,
  }),
  meta("forwarding_chat_stream_4096_c1", "request", 200, {
    ingress: "chat",
    upstream: "chat",
    stream: true,
    events: 1000,
    fragment_bytes: 4096,
    request_bytes: 262144,
    concurrency: 1,
  }),
  meta("forwarding_responses_stream_64_c1", "request", 200, {
    ingress: "responses",
    upstream: "responses",
    stream: true,
    events: 1000,
    fragment_bytes: 64,
    concurrency: 1,
  }),
  meta("forwarding_messages_stream_64_c1", "request", 200, {
    ingress: "messages",
    upstream: "messages",
    stream: true,
    events: 1000,
    fragment_bytes: 64,
    concurrency: 1,
  }),
  meta("direct_mock_chat_sync_32k", "request", 200, {
    ingress: "chat",
    upstream: "chat",
    stream: false,
    request_bytes: 32768,
  }),
  meta("direct_mock_responses_sync_32k", "request", 200, {
    ingress: "responses",
    upstream: "responses",
    stream: false,
    request_bytes: 32768,
  }),
  meta("direct_mock_messages_sync_32k", "request", 200, {
    ingress: "messages",
    upstream: "messages",
    stream: false,
    request_bytes: 32768,
  }),
  meta("direct_mock_chat_stream_64", "request", 200, {
    ingress: "chat",
    upstream: "chat",
    stream: true,
    events: 1000,
    fragment_bytes: 64,
  }),
  meta("direct_mock_chat_stream_4096", "request", 200, {
    ingress: "chat",
    upstream: "chat",
    stream: true,
    events: 1000,
    fragment_bytes: 4096,
  }),
  meta("direct_mock_responses_stream_64", "request", 200, {
    ingress: "responses",
    upstream: "responses",
    stream: true,
    events: 1000,
    fragment_bytes: 64,
  }),
  meta("direct_mock_messages_stream_64", "request", 200, {
    ingress: "messages",
    upstream: "messages",
    stream: true,
    events: 1000,
    fragment_bytes: 64,
  }),
  meta("conversion_chat_to_messages_sync", "request", 200, {
    ingress: "chat",
    upstream: "messages",
    stream: false,
    request_bytes: 32768,
  }),
  meta("conversion_responses_to_messages_sync", "request", 200, {
    ingress: "responses",
    upstream: "messages",
    stream: false,
    request_bytes: 32768,
  }),
  meta("conversion_chat_to_messages_stream", "request", 200, {
    ingress: "chat",
    upstream: "messages",
    stream: true,
    events: 1000,
    fragment_bytes: 64,
  }),
  meta("conversion_responses_to_messages_stream", "request", 200, {
    ingress: "responses",
    upstream: "messages",
    stream: true,
    events: 1000,
    fragment_bytes: 64,
  }),
  meta("direct_mock_conversion_chat_to_messages_sync", "request", 200, {
    ingress: "chat",
    upstream: "messages",
    stream: false,
    request_bytes: 32768,
  }),
  meta("direct_mock_conversion_responses_to_messages_sync", "request", 200, {
    ingress: "responses",
    upstream: "messages",
    stream: false,
    request_bytes: 32768,
  }),
  meta("direct_mock_conversion_chat_to_messages_stream", "request", 200, {
    ingress: "chat",
    upstream: "messages",
    stream: true,
    events: 1000,
    fragment_bytes: 64,
  }),
  meta("direct_mock_conversion_responses_to_messages_stream", "request", 200, {
    ingress: "responses",
    upstream: "messages",
    stream: true,
    events: 1000,
    fragment_bytes: 64,
  }),
  meta("pool_decision_10x20", "request", 200, {
    pools: 10,
    candidates_per_pool: 20,
    operations: "plan_pool_request",
  }),
  meta("pool_chat_to_chat", "request", 200, {
    ingress: "chat",
    upstream: "chat",
    pool_candidates: 20,
  }),
  meta("pool_chat_to_responses", "request", 200, {
    ingress: "chat",
    upstream: "responses",
    pool_candidates: 20,
  }),
  meta("pool_chat_to_messages", "request", 200, {
    ingress: "chat",
    upstream: "messages",
    pool_candidates: 20,
  }),
  meta("pool_responses_to_chat", "request", 200, {
    ingress: "responses",
    upstream: "chat",
    pool_candidates: 20,
  }),
  meta("pool_responses_to_responses", "request", 200, {
    ingress: "responses",
    upstream: "responses",
    pool_candidates: 20,
  }),
  meta("pool_responses_to_messages", "request", 200, {
    ingress: "responses",
    upstream: "messages",
    pool_candidates: 20,
  }),
  meta(
    "usage_log_append_10000x3",
    "storage",
    10000,
    { requests: 10000, attempts_per_request: 3, detail_rows: 30000 },
    { attempts_per_request: 3 },
  ),
  meta("usage_stats_1k", "query", 200, {
    detail_rows: 1000,
    providers: 50,
    models: 200,
    days: 365,
    selectors: "today|yesterday|7d|15d|30d|all",
  }),
  meta("usage_stats_100k", "query", 200, {
    detail_rows: 100000,
    providers: 50,
    models: 200,
    days: 365,
    selectors: "today|yesterday|7d|15d|30d|all",
  }),
  meta("usage_query_1k", "query", 200, {
    detail_rows: 1000,
    providers: 50,
    models: 200,
    days: 365,
    operations: "first+last+groups+filters+facets",
  }),
  meta("usage_query_100k", "query", 200, {
    detail_rows: 100000,
    providers: 50,
    models: 200,
    days: 365,
    operations: "first+last+groups+filters+facets",
  }),
  meta("provider_history_366", "query", 200, {
    days: 366,
    coverage: "zero|unavailable|ongoing",
  }),
  meta("provider_history_3650", "query", 200, {
    days: 3650,
    coverage: "zero|unavailable|ongoing",
  }),
  meta("template_views", "management", 200, {
    builtin_templates: true,
    persisted_templates: 0,
  }),
];

const CASE_BY_ID = new Map(REQUIRED_CASES.map((entry) => [entry.id, entry]));

export const REQUIRED_UI_CASES = [
  { id: "ui_all_tabs_en", locale: "en", operations: 1 },
  { id: "ui_all_tabs_zh", locale: "zh", operations: 1 },
  { id: "ui_usage_ticks_visible", locale: "en", operations: 30 },
  { id: "ui_hidden_ticks", locale: "en", operations: 30 },
  { id: "ui_config_burst", locale: "en", operations: 10 },
  { id: "ui_history_3650", locale: "en", operations: 1 },
];

// Behavior families covered by existing named tests. This runner records them
// as referenced, not executed; the Test role owns executing them.
export const BEHAVIOR_CASES = {
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

export const FIXED_COMMANDS = {
  rust_build: [
    "test",
    "--manifest-path",
    "src-tauri/Cargo.toml",
    "--lib",
    "gateway_performance",
    "--no-run",
    "--message-format=json",
  ],
  rust_case: ["--ignored", "--exact", "--nocapture", "--test-threads=1"],
  frontend: [
    "test",
    "--",
    "src/components/AiGateway/AiGateway.performance.test.tsx",
  ],
};

function findPrefixedLine(text, prefix) {
  if (typeof text !== "string") return null;
  for (const line of text.split(/\r?\n/)) {
    if (line.startsWith(prefix)) return line;
  }
  return null;
}

function parseMetricObject(text, prefix, label) {
  const line = findPrefixedLine(text, prefix);
  if (line === null) return null;
  const raw = line.slice(prefix.length).trim();
  let value;
  try {
    value = JSON.parse(raw);
  } catch (error) {
    throw new Error(`malformed ${label}: ${error.message}`);
  }
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`malformed ${label}: expected a JSON object`);
  }
  return value;
}

function normalizePhase(record) {
  if (!record || typeof record !== "object") return record;
  if (record.pass === 0) record.phase = "cold";
  else if (Number.isInteger(record.pass) && record.pass > 0) record.phase = "warm";
  return record;
}

export function parseGatewayMetricLine(text) {
  return normalizePhase(parseMetricObject(text, METRIC_PREFIX, "AGW_METRIC"));
}

export function parseUiMetricLine(text) {
  return normalizePhase(parseMetricObject(text, UI_METRIC_PREFIX, "AGW_UI_METRIC"));
}

export function parseUiResourceLine(text) {
  return parseMetricObject(text, UI_RESOURCE_PREFIX, "AGW_UI_RESOURCE");
}

function isObject(value) {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

function isFiniteNumber(value) {
  return typeof value === "number" && Number.isFinite(value);
}

function isNonNegativeNumber(value) {
  return isFiniteNumber(value) && value >= 0;
}

function isNonNegativeInteger(value) {
  return Number.isInteger(value) && value >= 0;
}

function isNonEmptyString(value) {
  return typeof value === "string" && value.trim().length > 0;
}

function stableStringify(value) {
  if (Array.isArray(value)) return `[${value.map(stableStringify).join(",")}]`;
  if (isObject(value)) {
    return `{${Object.keys(value)
      .sort()
      .map((key) => `${JSON.stringify(key)}:${stableStringify(value[key])}`)
      .join(",")}}`;
  }
  return JSON.stringify(value);
}

function validateMetric(metric, meta, pass, prefix, failures) {
  if (!isObject(metric)) {
    failures.push(`${prefix}:sample_count:${meta.id}:${pass}`);
    return;
  }
  if (metric.case !== meta.id) {
    failures.push(`${prefix}:metric_case:${meta.id}:${pass}`);
  }
  if (metric.pass !== pass) {
    failures.push(`${prefix}:sample_count:${meta.id}:${pass}`);
  }
  if (!Number.isInteger(metric.operations) || metric.operations < meta.operations_per_pass) {
    failures.push(`${prefix}:operations_floor:${meta.id}`);
  }
  const samples = metric.business_samples_ms;
  if (!Array.isArray(samples)) {
    failures.push(`${prefix}:business_samples:${meta.id}:${pass}`);
  } else {
    if (metric.operations !== samples.length) {
      failures.push(`${prefix}:sample_count:${meta.id}:${pass}`);
    }
    for (const sample of samples) {
      if (!isNonNegativeNumber(sample)) {
        failures.push(`${prefix}:business_samples:${meta.id}:${pass}`);
        break;
      }
    }
  }
  if (!isNonNegativeNumber(metric.business_wall_ms)) {
    failures.push(`${prefix}:business_wall:${meta.id}:${pass}`);
  }
  if (!isObject(metric.dimensions)) {
    failures.push(`${prefix}:dimension:${meta.id}:${pass}`);
  }
  if (!isObject(metric.counters)) {
    failures.push(`${prefix}:counter:${meta.id}:${pass}`);
  } else {
    for (const [name, value] of Object.entries(metric.counters)) {
      // Child metrics carry measured numbers (wall time may be fractional) and
      // typed metadata such as `stream` or `pool`; none of it is invented.
      const valid =
        isNonNegativeNumber(value) ||
        typeof value === "boolean" ||
        (typeof value === "string" && value.length > 0);
      if (!valid) {
        failures.push(`${prefix}:counter:${meta.id}:${pass}:${name}`);
        break;
      }
    }
  }
}

function validateCase(entry, meta, failures) {
  if (!isObject(entry)) {
    failures.push(`missing_case:${meta.id}`);
    return;
  }
  if (entry.case !== meta.id) {
    failures.push(`metric_case:${meta.id}`);
  }
  if (entry.operations_per_pass !== meta.operations_per_pass) {
    failures.push(`operations_floor:${meta.id}`);
  }
  if (meta.attempts_per_request !== undefined) {
    if (entry.attempts_per_request !== meta.attempts_per_request) {
      failures.push(`attempts_per_request:${meta.id}`);
    }
  }
  const cold = entry.cold;
  const warm = entry.warm;
  if (!isObject(cold)) {
    failures.push(`sample_count:${meta.id}:cold`);
  } else {
    validateMetric(cold, meta, 0, "case", failures);
  }
  if (!Array.isArray(warm)) {
    failures.push(`sample_count:${meta.id}:warm`);
  } else {
    if (warm.length !== 5) {
      failures.push(`sample_count:${meta.id}:warm`);
    }
    for (let pass = 1; pass <= 5; pass += 1) {
      const metric = warm.find((item) => isObject(item) && item.pass === pass);
      if (!metric) {
        failures.push(`sample_count:${meta.id}:warm:${pass}`);
      } else {
        validateMetric(metric, meta, pass, "case", failures);
      }
    }
  }
}

function validateUi(report, failures) {
  const counts = report.frontend?.ui_counts;
  if (!Array.isArray(counts)) {
    failures.push("ui_missing");
    return;
  }
  for (const expected of REQUIRED_UI_CASES) {
    const entries = counts.filter((entry) => isObject(entry) && entry.case === expected.id);
    if (entries.length !== 6) {
      failures.push(`ui_sample_count:${expected.id}:warm`);
    }
    for (let pass = 0; pass <= 5; pass += 1) {
      const entry = entries.find((item) => item.pass === pass);
      if (!entry) {
        failures.push(`ui_sample_count:${expected.id}:warm:${pass}`);
        continue;
      }
      if (entry.locale !== expected.locale) {
        failures.push(`ui_locale:${expected.id}:${pass}`);
      }
      if (!isNonNegativeInteger(entry.operations) || entry.operations !== expected.operations) {
        failures.push(`ui_operations:${expected.id}:${pass}`);
      }
      if (!isObject(entry.counters) || !isNonNegativeNumber(entry.counters.invoke_calls)) {
        failures.push(`ui_counter:${expected.id}:${pass}`);
      }
      if (!isNonNegativeNumber(entry.duration_ms)) {
        failures.push(`ui_duration:${expected.id}:${pass}`);
      }
      if (entry.proxy !== true) {
        failures.push(`ui_proxy:${expected.id}:${pass}`);
      }
      if (!isObject(entry.dimensions)) {
        failures.push(`ui_dimension:${expected.id}:${pass}`);
      }
    }
  }
}

export function validateGatewayReport(report) {
  const failures = [];
  if (!isObject(report)) {
    return { status: "incomplete", failures: ["report_missing"] };
  }

  const environment = report.environment;
  if (!isObject(environment)) {
    failures.push("environment_missing");
  } else {
    for (const key of ["os", "cpu", "architecture", "node", "rustc", "profile"]) {
      if (!isNonEmptyString(environment[key])) {
        failures.push(`environment_missing:${key}`);
      }
    }
  }

  if (!isObject(report.source) || !isNonEmptyString(report.source.git_revision)) {
    failures.push("source_missing:git_revision");
  }

  if (!isObject(report.fixtures)) {
    failures.push("fixtures_missing");
  } else {
    if (!isNonEmptyString(report.fixtures.profile)) {
      failures.push("fixtures_missing:profile");
    }
    if (!isObject(report.fixtures.dimensions)) {
      failures.push("fixtures_missing:dimensions");
    }
  }

  if (!isObject(report.sample_plan)) {
    failures.push("sample_plan_missing");
  } else {
    if (report.sample_plan.cold !== 1) failures.push("sample_plan_cold");
    if (report.sample_plan.warm !== 5) failures.push("sample_plan_warm");
  }

  if (!Array.isArray(report.commands) || report.commands.length === 0) {
    failures.push("commands_missing");
  } else {
    for (const command of report.commands) {
      if (!isObject(command)) {
        failures.push("commands_missing:entry");
        continue;
      }
      const name = isNonEmptyString(command.name) ? command.name : "unnamed";
      if (command.exit_code !== 0) {
        failures.push(`child_failed:${name}`);
      }
      if (command.kind === "run") {
        if (!Number.isInteger(command.tests_run) || command.tests_run <= 0) {
          failures.push(`zero_test:${name}`);
        }
      }
    }
  }

  const resources = report.worker_resources;
  if (!isObject(resources)) {
    failures.push("worker_resources_missing");
  } else {
    for (const key of ["rust", "frontend"]) {
      const resource = resources[key];
      if (!isObject(resource)) {
        failures.push(`worker_resources_missing:${key}`);
        continue;
      }
      if (!isNonNegativeNumber(resource.cpu_ms)) {
        failures.push(`worker_resources_missing:${key}:cpu_ms`);
      }
      if (!isFiniteNumber(resource.peak_rss_bytes) || resource.peak_rss_bytes <= 0) {
        failures.push(`worker_resources_missing:${key}:peak_rss_bytes`);
      }
      if (!isNonEmptyString(resource.scope)) {
        failures.push(`worker_resources_missing:${key}:scope`);
      }
      if (resource.includes_preparation !== true) {
        failures.push(`worker_resources_missing:${key}:includes_preparation`);
      }
    }
  }

  if (!Array.isArray(report.cases)) {
    failures.push("cases_missing");
  } else {
    const byId = new Map(
      report.cases.filter((entry) => isObject(entry)).map((entry) => [entry.case, entry]),
    );
    for (const meta of REQUIRED_CASES) {
      const entry = byId.get(meta.id);
      if (!entry) {
        failures.push(`missing_case:${meta.id}`);
      } else {
        validateCase(entry, meta, failures);
      }
    }
    for (const entry of report.cases) {
      if (isObject(entry) && !CASE_BY_ID.has(entry.case)) {
        failures.push(`unknown_case:${entry.case}`);
      }
    }
  }

  validateUi(report, failures);

  if (Array.isArray(report.run_failures)) {
    for (const failure of report.run_failures) {
      if (isNonEmptyString(failure)) failures.push(failure);
    }
  }

  return { status: failures.length === 0 ? "complete" : "incomplete", failures };
}

function allSamples(entry) {
  const samples = [];
  if (isObject(entry?.cold) && Array.isArray(entry.cold.business_samples_ms)) {
    samples.push(...entry.cold.business_samples_ms);
  }
  if (Array.isArray(entry?.warm)) {
    for (const metric of entry.warm) {
      if (isObject(metric) && Array.isArray(metric.business_samples_ms)) {
        samples.push(...metric.business_samples_ms);
      }
    }
  }
  return samples.filter(isFiniteNumber);
}

function counterValues(entry, name) {
  const values = [];
  const push = (metric) => {
    if (isObject(metric?.counters) && isFiniteNumber(metric.counters[name])) {
      values.push(metric.counters[name]);
    }
  };
  push(entry?.cold);
  if (Array.isArray(entry?.warm)) entry.warm.forEach(push);
  return values;
}

function median(values) {
  const sorted = values.filter(isFiniteNumber).sort((a, b) => a - b);
  if (sorted.length === 0) return null;
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 1
    ? sorted[middle]
    : (sorted[middle - 1] + sorted[middle]) / 2;
}

function percentile(values, fraction) {
  const sorted = values.filter(isFiniteNumber).sort((a, b) => a - b);
  if (sorted.length === 0) return null;
  const index = Math.min(sorted.length - 1, Math.floor(fraction * sorted.length));
  return sorted[index];
}

function caseById(report, id) {
  if (!Array.isArray(report?.cases)) return null;
  return report.cases.find((entry) => isObject(entry) && entry.case === id) ?? null;
}

function compareImprovement(label, id, baseline, final, failures) {
  const baselineCase = caseById(baseline, id);
  const finalCase = caseById(final, id);
  const baselineMedian = median(allSamples(baselineCase));
  const finalMedian = median(allSamples(finalCase));
  const baselineDuplicate = median(counterValues(baselineCase, "duplicate_operations"));
  const finalDuplicate = median(counterValues(finalCase, "duplicate_operations"));

  const wallImprovement =
    baselineMedian !== null && baselineMedian > 0 && finalMedian !== null
      ? ((baselineMedian - finalMedian) / baselineMedian) * 100
      : null;
  const duplicateReduction =
    baselineDuplicate !== null && baselineDuplicate > 0 && finalDuplicate !== null
      ? ((baselineDuplicate - finalDuplicate) / baselineDuplicate) * 100
      : null;

  const wallPass = wallImprovement !== null && wallImprovement >= 20;
  const duplicatePass = duplicateReduction !== null && duplicateReduction >= 50;
  const passed = wallPass || duplicatePass;
  const mode = wallPass ? "business_wall" : duplicatePass ? "duplicate_operations" : "none";

  if (!passed) {
    failures.push(
      `${label}_improvement: no >=20% business median improvement and no positive-baseline duplicate-operation reduction >=50%`,
    );
  }

  return {
    passed,
    mode,
    case: id,
    baseline_median_ms: baselineMedian,
    final_median_ms: finalMedian,
    improvement_percent: wallImprovement,
    baseline_duplicate_operations: baselineDuplicate,
    final_duplicate_operations: finalDuplicate,
    duplicate_reduction_percent: duplicateReduction,
  };
}

function compareUiGate(baseline, final, failures) {
  const gateId = "ui_usage_ticks_visible";
  const baselineEntries = (baseline?.frontend?.ui_counts ?? []).filter(
    (entry) => isObject(entry) && entry.case === gateId,
  );
  const finalEntries = (final?.frontend?.ui_counts ?? []).filter(
    (entry) => isObject(entry) && entry.case === gateId,
  );
  const baselineCalls = median(
    baselineEntries.map((entry) => entry.counters?.invoke_calls).filter(isFiniteNumber),
  );
  const finalCalls = median(
    finalEntries.map((entry) => entry.counters?.invoke_calls).filter(isFiniteNumber),
  );
  const reduction =
    baselineCalls !== null && baselineCalls > 0 && finalCalls !== null
      ? ((baselineCalls - finalCalls) / baselineCalls) * 100
      : null;
  const passed = reduction !== null && reduction >= 50;
  if (!passed) {
    failures.push(
      "ui_duplicate_work: requires a positive baseline invoke count and a >=50% exact duplicate-work reduction",
    );
  }
  return {
    passed,
    mode: "duplicate_work",
    counter: "invoke_calls",
    case: gateId,
    baseline_invoke_calls: baselineCalls,
    final_invoke_calls: finalCalls,
    reduction_percent: reduction,
  };
}

function compareEnvironment(baseline, final, failures) {
  const baselineEnvironment = isObject(baseline?.environment) ? baseline.environment : {};
  const finalEnvironment = isObject(final?.environment) ? final.environment : {};
  const keys = new Set([...Object.keys(baselineEnvironment), ...Object.keys(finalEnvironment)]);
  for (const key of keys) {
    if (stableStringify(baselineEnvironment[key]) !== stableStringify(finalEnvironment[key])) {
      failures.push(`environment_mismatch:${key}`);
    }
  }
}

function compareFixtures(baseline, final, failures) {
  if (
    baseline?.sample_plan?.cold !== final?.sample_plan?.cold ||
    baseline?.sample_plan?.warm !== final?.sample_plan?.warm
  ) {
    failures.push("sample_plan_mismatch");
  }
  if (baseline?.fixtures?.profile !== final?.fixtures?.profile) {
    failures.push("profile_mismatch");
  }
  if (
    stableStringify(baseline?.fixtures?.dimensions) !==
    stableStringify(final?.fixtures?.dimensions)
  ) {
    failures.push("fixture_dimensions_mismatch");
  }
  for (const meta of REQUIRED_CASES) {
    const baselineCase = caseById(baseline, meta.id);
    const finalCase = caseById(final, meta.id);
    if (!baselineCase || !finalCase) continue;
    if (stableStringify(baselineCase.dimensions) !== stableStringify(finalCase.dimensions)) {
      failures.push(`fixture_dimensions_mismatch:${meta.id}`);
    }
  }
}

function compareWorkerScopes(baseline, final, failures) {
  for (const key of ["rust", "frontend"]) {
    const baselineScope = baseline?.worker_resources?.[key]?.scope;
    const finalScope = final?.worker_resources?.[key]?.scope;
    if (stableStringify(baselineScope) !== stableStringify(finalScope)) {
      failures.push(`worker_scope_mismatch:${key}`);
    }
  }
}

function compareP95(baseline, final, failures) {
  let passed = true;
  for (const meta of REQUIRED_CASES) {
    const baselineSamples = allSamples(caseById(baseline, meta.id));
    const finalSamples = allSamples(caseById(final, meta.id));
    if (baselineSamples.length === 0 || finalSamples.length === 0) continue;
    const baselineP95 = percentile(baselineSamples, 0.95);
    const finalP95 = percentile(finalSamples, 0.95);
    if (baselineP95 === null || finalP95 === null) continue;
    const bound = baselineP95 + Math.max(0.1 * Math.abs(baselineP95), 1);
    if (finalP95 > bound) {
      passed = false;
      failures.push(
        `p95_regression:${meta.id}: baseline_p95=${baselineP95} final_p95=${finalP95} bound=${bound}`,
      );
    }
  }
  return { passed };
}

function compareRss(baseline, final, failures) {
  let passed = true;
  for (const key of ["rust", "frontend"]) {
    const baselineRss = baseline?.worker_resources?.[key]?.peak_rss_bytes;
    const finalRss = final?.worker_resources?.[key]?.peak_rss_bytes;
    if (!isFiniteNumber(baselineRss) || !isFiniteNumber(finalRss)) continue;
    const bound = baselineRss + Math.max(0.1 * baselineRss, 8 * 1024 * 1024);
    if (finalRss > bound) {
      passed = false;
      failures.push(
        `rss_regression:${key}: baseline=${baselineRss} final=${finalRss} bound=${bound}`,
      );
    }
  }
  return { passed };
}

export function compareGatewayReports(baseline, final) {
  const failures = [];
  if (baseline?.status !== "complete") {
    failures.push("baseline_status_not_complete");
  }
  const baselineValidation = validateGatewayReport(baseline);
  const finalValidation = validateGatewayReport(final);
  for (const failure of baselineValidation.failures) failures.push(`baseline_${failure}`);
  for (const failure of finalValidation.failures) failures.push(`final_${failure}`);

  compareEnvironment(baseline, final, failures);
  compareFixtures(baseline, final, failures);
  compareWorkerScopes(baseline, final, failures);

  const requestImprovement = compareImprovement(
    "request",
    "forwarding_chat_sync_32k_c1",
    baseline,
    final,
    failures,
  );
  const queryImprovement = compareImprovement(
    "query",
    "usage_query_1k",
    baseline,
    final,
    failures,
  );
  const uiDuplicateWork = compareUiGate(baseline, final, failures);
  const p95 = compareP95(baseline, final, failures);
  const rss = compareRss(baseline, final, failures);
  const cpu = {
    rust_baseline_ms: baseline?.worker_resources?.rust?.cpu_ms ?? null,
    rust_final_ms: final?.worker_resources?.rust?.cpu_ms ?? null,
    frontend_baseline_ms: baseline?.worker_resources?.frontend?.cpu_ms ?? null,
    frontend_final_ms: final?.worker_resources?.frontend?.cpu_ms ?? null,
  };

  return {
    status: failures.length === 0 ? "complete" : "incomplete",
    failures,
    gates: {
      request_improvement: requestImprovement,
      query_improvement: queryImprovement,
      ui_duplicate_work: uiDuplicateWork,
      p95,
      rss,
      cpu,
    },
  };
}

function runCommand(command, args, options = {}) {
  const started = process.hrtime.bigint();
  const result = spawnSync(command, args, {
    cwd: options.cwd ?? REPO_ROOT,
    encoding: "utf8",
    maxBuffer: MAX_BUFFER,
    timeout: options.timeoutMs ?? DEFAULT_TIMEOUT_MS,
  });
  const durationMs = Number(process.hrtime.bigint() - started) / 1e6;
  return {
    command,
    args,
    exit_code: typeof result.status === "number" ? result.status : 1,
    signal: result.signal ?? null,
    error: result.error ? String(result.error.message) : null,
    stdout: result.stdout ?? "",
    stderr: result.stderr ?? "",
    duration_ms: Number(durationMs.toFixed(3)),
  };
}

function versionOf(command, args) {
  const result = runCommand(command, args, { timeoutMs: 15_000 });
  if (result.error || result.exit_code !== 0) return null;
  return (result.stdout || result.stderr).trim().split(/\r?\n/)[0] || null;
}

function collectEnvironment() {
  const cpus = os.cpus();
  return {
    os: `${os.type()} ${os.release()}`,
    platform: process.platform,
    architecture: os.arch(),
    cpu: cpus.length > 0 ? cpus[0].model : null,
    cpu_count: cpus.length,
    node: process.version,
    npm: versionOf("npm", ["--version"]),
    rustc: versionOf("rustc", ["--version"]),
    cargo: versionOf("cargo", ["--version"]),
    profile: "test",
    time: process.platform === "darwin" ? "/usr/bin/time -l" : null,
  };
}

function parseCompilerExecutable(stdout) {
  let preferred = null;
  let fallback = null;
  for (const line of stdout.split(/\r?\n/)) {
    const trimmed = line.trim();
    if (!trimmed.startsWith("{")) continue;
    let message;
    try {
      message = JSON.parse(trimmed);
    } catch {
      continue;
    }
    if (
      message.reason === "compiler-artifact" &&
      typeof message.executable === "string" &&
      message.executable.length > 0 &&
      message.profile?.test === true
    ) {
      if (message.target?.name === "onespace_lib") preferred = message.executable;
      else fallback = message.executable;
    }
  }
  return preferred ?? fallback;
}

function parseTimeOutput(stderr) {
  const cpu = stderr.match(/^\s*([\d.]+)\s+real\s+([\d.]+)\s+user\s+([\d.]+)\s+sys/m);
  const rss = stderr.match(/^\s*(\d+)\s+maximum resident set size/m);
  if (!cpu || !rss) return null;
  const cpuMs = (Number(cpu[2]) + Number(cpu[3])) * 1000;
  const peakRss = Number(rss[1]);
  if (!Number.isFinite(cpuMs) || cpuMs < 0 || !Number.isFinite(peakRss) || peakRss <= 0) {
    return null;
  }
  return { cpu_ms: cpuMs, peak_rss_bytes: peakRss };
}

function parseRustPassed(output) {
  let passed = 0;
  for (const match of output.matchAll(/test result:\s+\w+\.\s+(\d+) passed/g)) {
    passed += Number(match[1]);
  }
  return passed;
}

function stripAnsi(text) {
  return text.replace(/\u001b\[[0-9;]*m/g, "");
}

function parseFrontendTestsRun(output) {
  const match = stripAnsi(output).match(/\bTests\s+(\d+)\s+passed\b/);
  if (!match) return 0;
  return Number(match[1]);
}

function summarizeSamples(samples, operations, wallMs) {
  return {
    operations,
    p50_ms: median(samples),
    p95_ms: percentile(samples, 0.95),
    throughput_ops_per_s:
      wallMs !== null && wallMs > 0 ? Number((operations / (wallMs / 1000)).toFixed(3)) : null,
    business_wall_ms: wallMs,
  };
}

function summarizeMetric(metric) {
  if (!isObject(metric) || !Array.isArray(metric.business_samples_ms)) return null;
  const samples = metric.business_samples_ms.filter(isFiniteNumber);
  const operations = Number.isInteger(metric.operations) ? metric.operations : samples.length;
  const wallMs = isFiniteNumber(metric.business_wall_ms) ? metric.business_wall_ms : null;
  return summarizeSamples(samples, operations, wallMs);
}

function collectPrefixed(text, prefix, parser) {
  const records = [];
  const failures = [];
  for (const line of text.split(/\r?\n/)) {
    if (!line.startsWith(prefix)) continue;
    try {
      const parsed = parser(line);
      if (parsed) records.push(parsed);
    } catch (error) {
      failures.push(`malformed_metric:${error.message}`);
    }
  }
  return { records, failures };
}

function buildCaseEntry(meta, metrics, runFailures) {
  const byPass = new Map();
  for (const metric of metrics) {
    if (metric.case !== meta.id) {
      runFailures.push(`metric_case:${meta.id}`);
      continue;
    }
    if (!Number.isInteger(metric.pass) || metric.pass < 0 || metric.pass > 5) {
      runFailures.push(`metric_pass:${meta.id}`);
      continue;
    }
    if (byPass.has(metric.pass)) {
      runFailures.push(`duplicate_metric:${meta.id}:${metric.pass}`);
      continue;
    }
    byPass.set(metric.pass, metric);
  }
  const cold = byPass.get(0) ?? null;
  const warm = [];
  for (let pass = 1; pass <= 5; pass += 1) {
    const metric = byPass.get(pass);
    if (metric) warm.push(metric);
  }
  return {
    case: meta.id,
    category: meta.category,
    status: cold && warm.length === 5 ? "measured" : "incomplete",
    operations_per_pass: meta.operations_per_pass,
    attempts_per_request: meta.attempts_per_request ?? null,
    dimensions: (cold ?? warm[0])?.dimensions ?? { ...meta.dimensions },
    cold,
    warm,
    summary: (() => {
      const metrics = [cold, ...warm].filter(isObject);
      const samples = metrics.flatMap((metric) =>
        Array.isArray(metric.business_samples_ms)
          ? metric.business_samples_ms.filter(isFiniteNumber)
          : [],
      );
      const operations = metrics.reduce(
        (total, metric) =>
          total + (Number.isInteger(metric.operations) ? metric.operations : 0),
        0,
      );
      const wallMs = metrics.reduce(
        (total, metric) =>
          total + (isFiniteNumber(metric.business_wall_ms) ? metric.business_wall_ms : 0),
        0,
      );
      return summarizeSamples(samples, operations, wallMs);
    })(),
    summaries: {
      cold: summarizeMetric(cold),
      warm: warm.map(summarizeMetric),
    },
  };
}

function buildFixtureDimensions() {
  return Object.fromEntries(
    REQUIRED_CASES.map((entry) => [entry.id, { ...entry.dimensions }]),
  );
}

function validateReportAndEmit(report, phase, baselinePath) {
  let comparison = null;
  if (phase === "final") {
    const resolvedBaseline = path.resolve(REPO_ROOT, baselinePath);
    if (!existsSync(resolvedBaseline)) {
      report.run_failures = [...(report.run_failures ?? []), `baseline_missing:${baselinePath}`];
    } else {
      try {
        const baseline = JSON.parse(readFileSync(resolvedBaseline, "utf8"));
        comparison = compareGatewayReports(baseline, report);
        report.comparison = comparison;
      } catch (error) {
        report.run_failures = [
          ...(report.run_failures ?? []),
          `baseline_unreadable:${error.message}`,
        ];
      }
    }
  }

  const finalValidation = validateGatewayReport(report);
  if (comparison) {
    report.status = comparison.status;
    report.failures = comparison.failures;
  } else {
    report.status = finalValidation.status;
    report.failures = finalValidation.failures;
  }
  if (finalValidation.status === "incomplete" && report.status === "complete") {
    report.status = "incomplete";
    report.failures = finalValidation.failures;
  }
  process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
  process.exitCode = report.status === "complete" ? 0 : 1;
}

function usageText() {
  return `measure-ai-gateway.mjs - whole-gateway baseline/final measurement runner

Usage:
  node tools/measure-ai-gateway.mjs --phase baseline --revision <sha>
  node tools/measure-ai-gateway.mjs --phase final --baseline docs/ai-gateway-performance-baseline.json --revision <sha>

Options:
  --help                 Print this help and exit 0.
  --phase <baseline|final>
                         Required measurement phase.
  --baseline <path>      Required read-only baseline JSON for --phase final.
  --revision <sha>       Source revision supplied by Git Operator. The
                         ${REVISION_ENV} environment variable is used when the
                         flag is omitted; no Git command is run.
  --timeout-ms <n>       Per-child timeout (default ${DEFAULT_TIMEOUT_MS}).

Child contract:
  AGW_METRIC {"case":...,"phase":...,"pass":...}
  AGW_UI_METRIC {"case":...,"locale":...,"pass":...}
  AGW_UI_RESOURCE {"cpu_ms":...,"peak_rss_bytes":...}
`;
}

function parseArgs(argv) {
  const options = {
    help: false,
    phase: null,
    baseline: null,
    revision: null,
    timeoutMs: DEFAULT_TIMEOUT_MS,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    const next = () => {
      index += 1;
      if (index >= argv.length) throw new Error(`missing value for ${arg}`);
      return argv[index];
    };
    switch (arg) {
      case "--help":
      case "-h":
        options.help = true;
        break;
      case "--phase":
        options.phase = next();
        break;
      case "--baseline":
        options.baseline = next();
        break;
      case "--revision":
        options.revision = next();
        break;
      case "--timeout-ms":
        options.timeoutMs = Number(next());
        break;
      default:
        throw new Error(`unknown argument: ${arg}`);
    }
  }
  if (options.help) return options;
  if (options.phase !== "baseline" && options.phase !== "final") {
    throw new Error("--phase must be baseline or final");
  }
  if (options.phase === "final" && !isNonEmptyString(options.baseline)) {
    throw new Error("--phase final requires --baseline <path>");
  }
  if (!Number.isInteger(options.timeoutMs) || options.timeoutMs <= 0) {
    throw new Error("--timeout-ms expects a positive integer");
  }
  return options;
}

function main() {
  const harnessStarted = process.hrtime.bigint();
  let options;
  try {
    options = parseArgs(process.argv.slice(2));
  } catch (error) {
    process.stderr.write(`measure-ai-gateway: ${error.message}\n`);
    process.stdout.write(
      `${JSON.stringify(
        {
          status: "incomplete",
          phase: null,
          harness: "tools/measure-ai-gateway.mjs",
          failures: [`invalid_arguments:${error.message}`],
        },
        null,
        2,
      )}\n`,
    );
    process.exitCode = 2;
    return;
  }

  if (options.help) {
    process.stdout.write(usageText());
    process.exitCode = 0;
    return;
  }

  const environment = collectEnvironment();
  const revision =
    (isNonEmptyString(options.revision) ? options.revision.trim() : null) ??
    (isNonEmptyString(process.env[REVISION_ENV]) ? process.env[REVISION_ENV].trim() : null);

  const runFailures = [];
  const build = runCommand("cargo", FIXED_COMMANDS.rust_build, {
    timeoutMs: options.timeoutMs,
  });
  const executable = build.exit_code === 0 ? parseCompilerExecutable(build.stdout) : null;
  if (build.exit_code !== 0) runFailures.push("rust_build_failed");
  if (!executable) runFailures.push("rust_test_executable_missing");

  const cases = [];
  const rustResources = [];
  const rustCaseRuns = [];
  let rustTestsRun = 0;
  let rustWallMs = 0;
  let rustExitCode = build.exit_code === 0 ? 0 : 1;

  for (const meta of REQUIRED_CASES) {
    const testName = `ai_gateway::tests::gateway_performance::gateway_performance_${meta.id}`;
    if (!executable) {
      rustCaseRuns.push({
        case: meta.id,
        test: testName,
        exit_code: 1,
        duration_ms: 0,
        tests_run: 0,
        worker_resource: null,
      });
      cases.push(buildCaseEntry(meta, [], runFailures));
      continue;
    }
    const timeArgs = ["-l", executable, ...FIXED_COMMANDS.rust_case, testName];
    const run = runCommand("/usr/bin/time", timeArgs, { timeoutMs: options.timeoutMs });
    rustWallMs += run.duration_ms;
    if (run.exit_code !== 0) {
      rustExitCode = 1;
      runFailures.push(`rust_case_failed:${meta.id}`);
    }
    const parsed = collectPrefixed(
      run.stdout,
      METRIC_PREFIX,
      parseGatewayMetricLine,
    );
    runFailures.push(...parsed.failures);
    const passed = parseRustPassed(`${run.stdout}\n${run.stderr}`);
    rustTestsRun += passed;
    if (passed !== 1) {
      runFailures.push(`zero_test:${meta.id}`);
    }
    const time = parseTimeOutput(run.stderr);
    if (!time) {
      runFailures.push(`missing_worker_resource:${meta.id}`);
    } else {
      rustResources.push(time);
    }
    rustCaseRuns.push({
      case: meta.id,
      test: testName,
      exit_code: run.exit_code,
      duration_ms: run.duration_ms,
      tests_run: passed,
      worker_resource: time,
    });
    cases.push(buildCaseEntry(meta, parsed.records, runFailures));
  }

  const rustWorker =
    rustResources.length === 0
      ? null
      : {
          cpu_ms: rustResources.reduce((total, resource) => total + resource.cpu_ms, 0),
          peak_rss_bytes: Math.max(...rustResources.map((resource) => resource.peak_rss_bytes)),
          scope: "rust-test-worker-aggregate",
          includes_preparation: true,
          processes: rustResources.length,
        };

  const frontendRun = runCommand("npm", FIXED_COMMANDS.frontend, {
    timeoutMs: options.timeoutMs,
  });
  const frontendOutput = `${frontendRun.stdout}\n${frontendRun.stderr}`;
  const uiParsed = collectPrefixed(frontendOutput, UI_METRIC_PREFIX, parseUiMetricLine);
  const resourceParsed = collectPrefixed(frontendOutput, UI_RESOURCE_PREFIX, parseUiResourceLine);
  runFailures.push(...uiParsed.failures, ...resourceParsed.failures);
  const frontendTestsRun = parseFrontendTestsRun(frontendOutput);
  if (frontendRun.exit_code !== 0) runFailures.push("frontend_failed");
  const uiCounts = [];
  for (const expected of REQUIRED_UI_CASES) {
    const entries = uiParsed.records.filter((entry) => entry.case === expected.id);
    for (let pass = 0; pass <= 5; pass += 1) {
      const entry = entries.find((item) => item.pass === pass);
      if (entry) uiCounts.push(entry);
    }
  }
  const frontendResource = resourceParsed.records[0] ?? null;

  const commands = [
    {
      name: "rust_build",
      kind: "build",
      command: "cargo",
      args: FIXED_COMMANDS.rust_build,
      exit_code: build.exit_code,
      duration_ms: build.duration_ms,
      tests_run: 0,
    },
    {
      name: "rust_case",
      kind: "run",
      command: "/usr/bin/time",
      args: ["-l", executable ?? "<missing-executable>", ...FIXED_COMMANDS.rust_case, "gateway_performance"],
      exit_code: rustExitCode,
      duration_ms: Number(rustWallMs.toFixed(3)),
      tests_run: rustTestsRun,
      case_runs: rustCaseRuns,
    },
    {
      name: "frontend",
      kind: "run",
      command: "npm",
      args: FIXED_COMMANDS.frontend,
      exit_code: frontendRun.exit_code,
      duration_ms: frontendRun.duration_ms,
      tests_run: frontendTestsRun,
    },
  ];

  const report = {
    status: "incomplete",
    phase: options.phase,
    harness: "tools/measure-ai-gateway.mjs",
    generated_at: new Date().toISOString(),
    environment,
    source: {
      git_revision: revision,
      supplied_by: revision
        ? isNonEmptyString(options.revision)
          ? "command_line"
          : "environment"
        : "missing",
    },
    fixtures: {
      profile: FIXTURE_PROFILE,
      dimensions: buildFixtureDimensions(),
    },
    sample_plan: {
      cold: 1,
      warm: 5,
      cold_scope: "fresh_process_owned_state_not_os_disk_cache",
    },
    commands,
    timing: {
      build_ms: build.duration_ms,
      rust_worker_wall_ms: Number(rustWallMs.toFixed(3)),
      frontend_wall_ms: frontendRun.duration_ms,
      harness_ms: Number((Number(process.hrtime.bigint() - harnessStarted) / 1e6).toFixed(3)),
    },
    cases,
    frontend: { ui_counts: uiCounts },
    worker_resources: {
      rust: rustWorker,
      frontend: frontendResource,
    },
    behavior: {
      execution: "not_executed_by_measurement_runner",
      note: "Named behavior cases are referenced for the Test role; this runner does not execute them.",
      inventory: BEHAVIOR_CASES,
    },
    run_failures: runFailures,
  };

  validateReportAndEmit(report, options.phase, options.baseline);
}

const entryPoint = process.argv[1]
  ? pathToFileURL(path.resolve(process.argv[1])).href
  : null;
if (entryPoint === import.meta.url) {
  main();
}


