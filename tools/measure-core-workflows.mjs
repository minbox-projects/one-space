#!/usr/bin/env node
// Baseline/final measurement harness for plan
// 20261009-core-workflows-cleanup-and-optimization, Step 5 (REQ-005/AC-005).
//
// It runs the ignored synthetic `core_workflows_perf` tests, parses the
// measurement lines those tests print, and emits one JSON report. It uses only
// Node built-ins and never invents a number: a dataset with no printed line is
// recorded as `not_measured`.
//
// ---------------------------------------------------------------------------
// Printed counter format (contract for the Test-authored perf tests)
// ---------------------------------------------------------------------------
// Each measurement pass prints exactly one line to stdout (println!), starting
// with the literal prefix `CWF_METRIC ` (one space). Only these lines are
// parsed; every other cargo/test line is ignored.
//
//   CWF_METRIC dataset=<id> phase=<cold|warm> wall_ms=<number> [<key>=<int> ...]
//
//   dataset  required; one of: usage_collection, gateway_logging, ssh_records
//   phase    required; `cold` (once) or `warm` (repeated, default 5 times)
//   wall_ms  optional float, the test's own raw wall time for that pass
//   <key>    any remaining `key=<non-negative integer>` pair is a counter,
//            e.g. source_reads=12000 parsed_entries=20000 cache_hits=0
//
// Tokens may appear in any order. A malformed token is skipped and reported as
// a warning; it never becomes a fabricated value.
//
// `UsageLogWriteStats` uses `db_opens` / `transactions` / `rows_written` /
// `batch_writes`; `UsageCollectionStats` uses `source_reads` / `parsed_entries`
// / `collection_calls` / `cache_hits`; `HistorySyncStats` uses
// `sources_scanned` / `sources_skipped`.
//
// ---------------------------------------------------------------------------
// Usage
// ---------------------------------------------------------------------------
//   node tools/measure-core-workflows.mjs [options]
//
//   --help              Print this help and exit 0.
//   --out <path>        Also write the JSON report to <path>.
//   --build             Compile the measurement test binary first
//                       (`cargo test ... --no-run`); off by default, so no
//                       full npm/cargo build is ever run unless requested.
//   --warm <n>          Expected warm passes per dataset (default 5).
//   --filter <name>     Restrict the run with an exact cargo-test filter
//                       (repeatable). Passed as `--exact <name>` after `--`.
//   --manifest <path>   Cargo manifest relative to the repo root
//                       (default src-tauri/Cargo.toml).
//   --timeout-ms <n>    Per-command timeout (default 900000).
//
// The default test command is exactly:
//   cargo test --manifest-path src-tauri/Cargo.toml --lib core_workflows_perf \
//     -- --ignored --nocapture --test-threads=1
//
// Exit status: non-zero only for a harness failure (bad arguments: 2, report
// write or unexpected error: 1). A missing cargo/test binary is reported as a
// `not_measured` dataset with exit 0.

import { spawnSync } from "node:child_process";
import { existsSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(HERE, "..");

const LINE_PREFIX = "CWF_METRIC ";
const PLAN_ID = "20261009-core-workflows-cleanup-and-optimization";
const DEFAULT_MANIFEST = "src-tauri/Cargo.toml";
const DEFAULT_WARM = 5;
const DEFAULT_TIMEOUT_MS = 900_000;
const OUTPUT_TAIL_CHARS = 4000;

// Dataset cases fixed by the frozen plan's measurement boundary.
const DATASETS = [
  {
    id: "usage_collection",
    dimensions: { tools: 4, sessions_per_tool: 1000, messages_per_session: 20 },
  },
  {
    id: "gateway_logging",
    dimensions: { requests: 10000, attempts_per_request: 3 },
  },
  {
    id: "ssh_records",
    dimensions: { saved_ssh_records: 100 },
  },
];

function printUsage() {
  process.stdout.write(USAGE_TEXT);
}

const USAGE_TEXT = `measure-core-workflows.mjs - Step 5 (REQ-005/AC-005) measurement harness

Usage:
  node tools/measure-core-workflows.mjs [options]

Options:
  --help              Print this help and exit 0.
  --out <path>        Also write the JSON report to <path>.
  --build             Compile the measurement test binary first
                      (cargo test ... --no-run); off by default.
  --warm <n>          Expected warm passes per dataset (default ${DEFAULT_WARM}).
  --filter <name>     Restrict the run with an exact cargo-test filter
                      (repeatable); passed as --exact <name> after --.
  --manifest <path>   Cargo manifest relative to the repo root
                      (default ${DEFAULT_MANIFEST}).
  --timeout-ms <n>    Per-command timeout in milliseconds (default ${DEFAULT_TIMEOUT_MS}).

Test command (default):
  cargo test --manifest-path ${DEFAULT_MANIFEST} --lib core_workflows_perf \\
    -- --ignored --nocapture --test-threads=1

Printed counter contract (Test-authored tests must print these lines):
  ${LINE_PREFIX}dataset=<id> phase=<cold|warm> wall_ms=<number> [<key>=<int> ...]

  dataset  one of: ${DATASETS.map((d) => d.id).join(", ")}
  phase    cold (once) or warm (five times by default)
  counters e.g. source_reads=0 parsed_entries=0 collection_calls=0 cache_hits=0
           or db_opens=0 transactions=0 rows_written=0 batch_writes=0
           or sources_scanned=0 sources_skipped=0

Exit status: non-zero only on harness failure (2 bad arguments, 1 write/other).
A missing cargo or test binary is reported as a not_measured dataset with exit 0.
`;

function fail(message) {
  process.stderr.write(`measure-core-workflows: ${message}\n`);
  process.exit(2);
}

function parsePositiveInt(value, flag) {
  const n = Number(value);
  if (!Number.isInteger(n) || n <= 0) {
    fail(`${flag} expects a positive integer, got "${value}"`);
  }
  return n;
}

function parseArgs(argv) {
  const opts = {
    help: false,
    out: null,
    build: false,
    warm: DEFAULT_WARM,
    filters: [],
    manifest: DEFAULT_MANIFEST,
    timeoutMs: DEFAULT_TIMEOUT_MS,
  };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    const next = () => {
      i += 1;
      if (i >= argv.length) fail(`missing value for ${arg}`);
      return argv[i];
    };
    switch (arg) {
      case "--help":
      case "-h":
        opts.help = true;
        break;
      case "--out":
        opts.out = next();
        break;
      case "--build":
        opts.build = true;
        break;
      case "--warm":
        opts.warm = parsePositiveInt(next(), "--warm");
        break;
      case "--filter":
        opts.filters.push(next());
        break;
      case "--manifest":
        opts.manifest = next();
        break;
      case "--timeout-ms":
        opts.timeoutMs = parsePositiveInt(next(), "--timeout-ms");
        break;
      default:
        fail(`unknown argument: ${arg}`);
    }
  }
  return opts;
}

function runCommand(command, args, timeoutMs) {
  const start = process.hrtime.bigint();
  const result = spawnSync(command, args, {
    encoding: "utf8",
    maxBuffer: 128 * 1024 * 1024,
    timeout: timeoutMs,
  });
  const wallMs = Number(process.hrtime.bigint() - start) / 1e6;
  return {
    command,
    args,
    exit_code: result.status,
    signal: result.signal ?? null,
    error: result.error ? String(result.error.message) : null,
    stdout: result.stdout ?? "",
    stderr: result.stderr ?? "",
    wall_ms: Number(wallMs.toFixed(3)),
  };
}

function tail(text) {
  if (!text) return "";
  return text.length > OUTPUT_TAIL_CHARS
    ? text.slice(text.length - OUTPUT_TAIL_CHARS)
    : text;
}

function versionOf(command, args) {
  try {
    const result = runCommand(command, args, 15000);
    if (result.error || result.exit_code !== 0) {
      return null;
    }
    return (result.stdout || result.stderr).trim().split(/\r?\n/)[0] || null;
  } catch {
    return null;
  }
}

function collectEnvironment() {
  const cpus = os.cpus();
  return {
    os: `${os.type()} ${os.release()}`,
    platform: process.platform,
    arch: os.arch(),
    cpu_model: cpus.length > 0 ? cpus[0].model : null,
    cpu_count: cpus.length,
    node_version: process.version,
    rustc: versionOf("rustc", ["--version"]),
    cargo: versionOf("cargo", ["--version"]),
  };
}

function checkArtifacts() {
  return {
    rust_target_dir_present: existsSync(path.join(REPO_ROOT, "src-tauri", "target")),
    frontend_dist_present: existsSync(path.join(REPO_ROOT, "dist", "index.html")),
  };
}

function parseMeasurementLines(text) {
  const records = [];
  const warnings = [];
  for (const rawLine of text.split(/\r?\n/)) {
    if (!rawLine.startsWith(LINE_PREFIX)) continue;
    const tokens = rawLine.slice(LINE_PREFIX.length).trim().split(/\s+/).filter(Boolean);
    const record = {
      dataset: null,
      phase: null,
      reported_wall_ms: null,
      counters: {},
      line: rawLine.trim(),
    };
    for (const token of tokens) {
      const eq = token.indexOf("=");
      if (eq <= 0) {
        warnings.push(`ignored token "${token}"`);
        continue;
      }
      const key = token.slice(0, eq);
      const value = token.slice(eq + 1);
      if (key === "dataset") {
        record.dataset = value;
      } else if (key === "phase") {
        record.phase = value;
      } else if (key === "wall_ms") {
        const n = Number(value);
        if (Number.isFinite(n)) {
          record.reported_wall_ms = n;
        } else {
          warnings.push(`ignored non-numeric wall_ms="${value}"`);
        }
      } else {
        const n = Number(value);
        if (Number.isInteger(n) && n >= 0) {
          record.counters[key] = n;
        } else {
          warnings.push(`ignored non-integer counter ${key}="${value}"`);
        }
      }
    }
    if (!record.dataset || !record.phase) {
      warnings.push(`ignored CWF_METRIC line missing dataset/phase: ${rawLine.trim()}`);
      continue;
    }
    records.push(record);
  }
  return { records, warnings };
}

function groupDatasets(records, warmExpected) {
  const buckets = new Map();
  for (const dataset of DATASETS) {
    buckets.set(dataset.id, {
      id: dataset.id,
      dimensions: dataset.dimensions,
      cold: null,
      warm: [],
      unmatched: [],
    });
  }
  for (const record of records) {
    let bucket = buckets.get(record.dataset);
    if (!bucket) {
      bucket = { id: record.dataset, dimensions: null, cold: null, warm: [], unmatched: [] };
      buckets.set(record.dataset, bucket);
    }
    if (record.phase === "cold") {
      if (bucket.cold === null) {
        bucket.cold = record;
      } else {
        bucket.unmatched.push(record);
      }
    } else if (record.phase === "warm") {
      bucket.warm.push(record);
    } else {
      bucket.unmatched.push(record);
    }
  }
  return [...buckets.values()].map((bucket) => {
    const measured = bucket.cold !== null || bucket.warm.length > 0;
    return {
      id: bucket.id,
      dimensions: bucket.dimensions,
      status: measured ? "measured" : "not_measured",
      missing_reason: measured
        ? null
        : "no CWF_METRIC line for this dataset (perf test absent or failed)",
      cold: bucket.cold,
      warm: bucket.warm,
      warm_count_expected: warmExpected,
      warm_count_observed: bucket.warm.length,
      unmatched: bucket.unmatched,
    };
  });
}

function emit(report, outPath, exitCode) {
  const text = `${JSON.stringify(report, null, 2)}\n`;
  process.stdout.write(text);
  if (outPath) {
    try {
      writeFileSync(path.resolve(REPO_ROOT, outPath), text, "utf8");
    } catch (error) {
      process.stderr.write(`measure-core-workflows: cannot write ${outPath}: ${error.message}\n`);
      process.exit(1);
    }
  }
  process.exit(exitCode);
}

function main() {
  const opts = parseArgs(process.argv.slice(2));
  if (opts.help) {
    printUsage();
    process.exit(0);
  }

  const manifestPath = path.resolve(REPO_ROOT, opts.manifest);
  const report = {
    plan_id: PLAN_ID,
    step: 5,
    requirement: "REQ-005",
    acceptance_criteria: "AC-005",
    generated_at: new Date().toISOString(),
    harness: "tools/measure-core-workflows.mjs",
    measurement_format: `${LINE_PREFIX}dataset=<id> phase=<cold|warm> wall_ms=<number> [<key>=<int> ...]`,
    environment: collectEnvironment(),
    artifacts: checkArtifacts(),
    build: { requested: opts.build, ran: false, exit_code: null, wall_ms: null, error: null },
    command: null,
    parse_warnings: [],
    datasets: DATASETS.map((dataset) => ({
      id: dataset.id,
      dimensions: dataset.dimensions,
      status: "not_measured",
      missing_reason: "harness did not run",
      cold: null,
      warm: [],
      warm_count_expected: opts.warm,
      warm_count_observed: 0,
      unmatched: [],
    })),
    exit_reason: null,
  };

  if (!existsSync(manifestPath)) {
    report.datasets.forEach((dataset) => {
      dataset.missing_reason = `cargo manifest not found: ${manifestPath}`;
    });
    report.exit_reason = "manifest_missing";
    emit(report, opts.out, 1);
    return;
  }

  const cargoProbe = runCommand("cargo", ["--version"], 15000);
  if (cargoProbe.error) {
    report.datasets.forEach((dataset) => {
      dataset.missing_reason = `cargo unavailable: ${cargoProbe.error}`;
    });
    report.command = cargoProbe;
    report.exit_reason = "cargo_missing";
    emit(report, opts.out, 0);
    return;
  }

  if (opts.build) {
    const build = runCommand(
      "cargo",
      ["test", "--manifest-path", manifestPath, "--lib", "core_workflows_perf", "--no-run"],
      opts.timeoutMs,
    );
    report.build = {
      requested: true,
      ran: true,
      exit_code: build.exit_code,
      wall_ms: build.wall_ms,
      error: build.error,
    };
  }

  const libtestArgs = ["--ignored", "--nocapture", "--test-threads=1"];
  for (const filter of opts.filters) {
    libtestArgs.push("--exact", filter);
  }
  const testArgs = [
    "test",
    "--manifest-path",
    manifestPath,
    "--lib",
    "core_workflows_perf",
    "--",
    ...libtestArgs,
  ];
  const test = runCommand("cargo", testArgs, opts.timeoutMs);
  report.command = {
    command: test.command,
    args: test.args,
    exit_code: test.exit_code,
    signal: test.signal,
    error: test.error,
    wall_ms: test.wall_ms,
    stdout_tail: tail(test.stdout),
    stderr_tail: tail(test.stderr),
  };

  const { records, warnings } = parseMeasurementLines(`${test.stdout}\n${test.stderr}`);
  report.parse_warnings = warnings;
  report.datasets = groupDatasets(records, opts.warm);
  report.exit_reason = "measured";

  emit(report, opts.out, 0);
}

main();
