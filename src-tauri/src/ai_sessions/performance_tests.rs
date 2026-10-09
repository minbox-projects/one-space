//! Ignored synthetic measurement tests for REQ-005/AC-005 (plan Step 5).
//!
//! These tests are named `core_workflows_perf*` and are executed by
//! `tools/measure-core-workflows.mjs` with `--ignored --nocapture
//! --test-threads=1`. Each pass prints one `CWF_METRIC ...` line; the harness
//! parses those lines and never invents a number. They assert only structural
//! correctness (counts are non-zero and counters never decrease), never an
//! absolute wall-time threshold.
//!
//! All fixtures are synthetic and live under a temporary HOME; no real user
//! data, credentials or live CLI calls are involved.

use super::{
    reset_usage_collection_stats, sessions_usage_clear_cache, sessions_usage_tool_stats,
    usage_collection_stats, UsageCollectionStats,
};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

const USAGE_TOOLS: [&str; 4] = ["claude", "codex", "antigravity", "opencode"];
const SESSIONS_PER_TOOL: usize = 1000;
const MESSAGES_PER_SESSION: usize = 20;
const WARM_PASSES: usize = 5;

/// Process-wide HOME isolation. The usage collectors for claude/codex/opencode
/// resolve `dirs::home_dir()` (the process `$HOME`), while antigravity prefers
/// the thread-local override; set both to the same temporary root and hold the
/// shared HOME lock so no other test observes the mutation.
struct TempHome {
    root: PathBuf,
    original_home: Option<String>,
    _guard: crate::config::test_home::TestHomeGuard,
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl Drop for TempHome {
    fn drop(&mut self) {
        match self.original_home.take() {
            Some(home) => std::env::set_var("HOME", home),
            None => std::env::remove_var("HOME"),
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn isolated_temp_home(name: &str) -> TempHome {
    let lock = crate::lock_test_home_env();
    let root = std::env::temp_dir().join(format!(
        "onespace-cwf-usage-{}-{}",
        name,
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create temp home");
    let original_home = std::env::var("HOME").ok();
    std::env::set_var("HOME", &root);
    let guard = crate::config::test_home::TestHomeGuard::set(&root);
    TempHome {
        root,
        original_home,
        _guard: guard,
        _lock: lock,
    }
}

fn seed_all_tools(home: &Path) {
    seed_claude(home);
    seed_codex(home);
    seed_antigravity(home);
    seed_opencode(home);
}

/// 1000 sessions x 20 assistant usage lines under `.claude/projects`.
fn seed_claude(home: &Path) {
    let dir = home.join(".claude").join("projects").join("cwf");
    fs::create_dir_all(&dir).expect("create claude fixture dir");
    let timestamp = chrono::Local::now().to_rfc3339();
    for session in 0..SESSIONS_PER_TOOL {
        let mut content = String::new();
        for message in 0..MESSAGES_PER_SESSION {
            let line = json!({
                "type": "assistant",
                "sessionId": format!("claude-{session}"),
                "timestamp": &timestamp,
                "message": {
                    "model": "claude-opus-4-6",
                    "usage": {
                        "input_tokens": 10 + message,
                        "output_tokens": 5 + message,
                        "cache_read_input_tokens": 1
                    }
                }
            });
            content.push_str(&line.to_string());
            content.push('\n');
        }
        fs::write(dir.join(format!("session-{session}.jsonl")), content)
            .expect("write claude fixture");
    }
}

/// 1000 sessions x 20 token-count events under `.codex/sessions`.
fn seed_codex(home: &Path) {
    let dir = home.join(".codex").join("sessions").join("cwf");
    fs::create_dir_all(&dir).expect("create codex fixture dir");
    let timestamp = chrono::Local::now().to_rfc3339();
    for session in 0..SESSIONS_PER_TOOL {
        let mut content = String::new();
        content.push_str(
            &json!({
                "type": "session_meta",
                "payload": { "id": format!("codex-{session}"), "timestamp": &timestamp }
            })
            .to_string(),
        );
        content.push('\n');
        content.push_str(
            &json!({
                "type": "turn_context",
                "payload": { "model": "gpt-5-codex" }
            })
            .to_string(),
        );
        content.push('\n');
        for message in 0..MESSAGES_PER_SESSION {
            content.push_str(
                &json!({
                    "type": "event_msg",
                    "timestamp": &timestamp,
                    "payload": {
                        "type": "token_count",
                        "info": {
                            "last_token_usage": {
                                "input_tokens": 10 + message,
                                "cached_input_tokens": 1,
                                "output_tokens": 5 + message,
                                "total_tokens": 15 + 2 * message
                            }
                        }
                    }
                })
                .to_string(),
            );
            content.push('\n');
        }
        fs::write(dir.join(format!("rollout-{session}.jsonl")), content)
            .expect("write codex fixture");
    }
}

/// 1000 legacy Antigravity session files x 20 messages under `.gemini/tmp`.
fn seed_antigravity(home: &Path) {
    let timestamp = chrono::Local::now().to_rfc3339();
    for session in 0..SESSIONS_PER_TOOL {
        let dir = home
            .join(".gemini")
            .join("tmp")
            .join(format!("rollout-{session}"))
            .join("chats");
        fs::create_dir_all(&dir).expect("create antigravity fixture dir");
        let messages = (0..MESSAGES_PER_SESSION)
            .map(|message| {
                json!({
                    "tokens": { "input": 10 + message, "output": 5 + message, "cached": 1, "total": 0 },
                    "model": "gemini-pro",
                    "timestamp": &timestamp
                })
            })
            .collect::<Vec<_>>();
        let content = json!({
            "sessionId": format!("anti-{session}"),
            "messages": messages
        });
        fs::write(dir.join(format!("session-{session}.json")), content.to_string())
            .expect("write antigravity fixture");
    }
}

/// One SQLite v2 OpenCode database with 1000 sessions x 20 usage messages.
///
/// The single-database source keeps the fixture bounded while still exercising
/// the real OpenCode collection path (SQLite window filtering).
fn seed_opencode(home: &Path) {
    let dir = home.join(".local").join("share").join("opencode");
    fs::create_dir_all(&dir).expect("create opencode fixture dir");
    let connection =
        rusqlite::Connection::open(dir.join("opencode.db")).expect("create opencode db");
    connection
        .execute_batch(
            "CREATE TABLE session_v2 (id TEXT PRIMARY KEY, time_archived INTEGER);
             CREATE TABLE session_message (
                 id TEXT PRIMARY KEY,
                 session_id TEXT NOT NULL,
                 time_created INTEGER NOT NULL,
                 data TEXT NOT NULL
             );",
        )
        .expect("create opencode schema");
    let now_ms = chrono::Local::now().timestamp_millis();
    let transaction = connection
        .unchecked_transaction()
        .expect("begin opencode fixture transaction");
    for session in 0..SESSIONS_PER_TOOL {
        let session_id = format!("oc-{session}");
        transaction
            .execute(
                "INSERT INTO session_v2 (id, time_archived) VALUES (?1, NULL)",
                [&session_id],
            )
            .expect("insert opencode session");
        for message in 0..MESSAGES_PER_SESSION {
            let data = json!({
                "role": "assistant",
                "modelID": "opencode-model",
                "tokens": { "input": 10 + message, "output": 5 + message, "cache": { "read": 1 } }
            })
            .to_string();
            transaction
                .execute(
                    "INSERT INTO session_message (id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![
                        format!("{session_id}-msg-{message}"),
                        session_id,
                        now_ms,
                        data
                    ],
                )
                .expect("insert opencode message");
        }
    }
    transaction.commit().expect("commit opencode fixture");
}

/// Run one full four-tool collection pass and return its raw wall time in ms.
fn run_pass() -> f64 {
    let started = Instant::now();
    for tool in USAGE_TOOLS {
        let stats = sessions_usage_tool_stats(tool.to_string(), Some(7))
            .unwrap_or_else(|error| panic!("{tool} usage collection failed: {error}"));
        assert_eq!(
            stats.source_status, "available",
            "{tool} synthetic source must be available"
        );
        assert!(
            stats.summary.calls > 0,
            "{tool} synthetic source must parse at least one in-window call"
        );
    }
    started.elapsed().as_secs_f64() * 1000.0
}

fn print_metric(phase: &str, wall_ms: f64, stats: &UsageCollectionStats) {
    // Leading newline detaches the metric from libtest's `test <name> ... `
    // banner, which otherwise glues to the first captured output line.
    println!(
        "\nCWF_METRIC dataset=usage_collection phase={phase} wall_ms={wall_ms:.3} \
         source_reads={} parsed_entries={} collection_calls={} cache_hits={}",
        stats.source_reads, stats.parsed_entries, stats.collection_calls, stats.cache_hits
    );
}

#[test]
#[ignore = "synthetic REQ-005/AC-005 measurement; run via tools/measure-core-workflows.mjs"]
fn core_workflows_perf_usage_collection() {
    let home = isolated_temp_home("usage-perf");
    seed_all_tools(&home.root);

    sessions_usage_clear_cache();
    reset_usage_collection_stats();

    let cold_wall = run_pass();
    let cold = usage_collection_stats();
    assert!(
        cold.collection_calls >= USAGE_TOOLS.len() as u64,
        "one cold pass collects every supported tool"
    );
    assert!(cold.source_reads > 0, "cold pass must read synthetic sources");
    assert!(
        cold.parsed_entries >= (SESSIONS_PER_TOOL * MESSAGES_PER_SESSION) as u64,
        "cold pass must parse at least one full tool's synthetic entries"
    );
    print_metric("cold", cold_wall, &cold);

    let mut previous = cold;
    for _ in 0..WARM_PASSES {
        let warm_wall = run_pass();
        let current = usage_collection_stats();
        assert!(
            current.source_reads >= previous.source_reads,
            "source_reads must never decrease across passes"
        );
        assert!(
            current.parsed_entries >= previous.parsed_entries,
            "parsed_entries must never decrease across passes"
        );
        assert!(
            current.collection_calls >= previous.collection_calls,
            "collection_calls must never decrease across passes"
        );
        assert!(
            current.cache_hits >= previous.cache_hits,
            "cache_hits must never decrease across passes"
        );
        print_metric("warm", warm_wall, &current);
        previous = current;
    }

    sessions_usage_clear_cache();
}
