//! Ignored synthetic measurement test for REQ-005/AC-005 (plan Step 5).
//!
//! Named `core_workflows_perf*` and executed by `tools/measure-core-workflows.mjs`
//! with `--ignored --nocapture --test-threads=1`. It writes one request's three
//! attempts through the real [`UsageLogStore`] batch path per call and prints one
//! `CWF_METRIC ...` line per pass. Assertions are structural only; there is no
//! absolute wall-time threshold.

use crate::ai_gateway::{
    reset_usage_log_write_stats, usage_log_write_stats, UsageLogRecord, UsageLogStore,
    UsageLogWriteStats, UsageResult,
};
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

const REQUESTS: usize = 10_000;
const ATTEMPTS_PER_REQUEST: usize = 3;
const WARM_PASSES: usize = 5;

/// Removes the temporary database directory when the test ends.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "onespace-cwf-gateway-{}-{}",
            name,
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&path).expect("create temp dir");
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// One persisted attempt row of a request. Only the last attempt is terminal.
fn attempt_record(request: usize, attempt: usize, base_ms: i64) -> UsageLogRecord {
    UsageLogRecord {
        timestamp_ms: base_ms + request as i64,
        local_model: "local-perf".to_string(),
        upstream_model: "upstream-perf".to_string(),
        provider_id: "provider-perf".to_string(),
        provider_name: "Provider Perf".to_string(),
        result: UsageResult::Success,
        status: 200,
        input_tokens: 10,
        cache_read_tokens: 1,
        cache_write_tokens: 0,
        output_tokens: 5,
        total_tokens: 15,
        amount: Some(0.001),
        duration_ms: 5,
        error_message: None,
        terminal: attempt + 1 == ATTEMPTS_PER_REQUEST,
        reasoning_effort: None,
    }
}

/// Write `REQUESTS` request batches and return the raw wall time in ms.
fn run_pass(store: &UsageLogStore, base_ms: i64) -> f64 {
    let started = Instant::now();
    for request in 0..REQUESTS {
        let records = (0..ATTEMPTS_PER_REQUEST)
            .map(|attempt| attempt_record(request, attempt, base_ms))
            .collect::<Vec<_>>();
        store
            .append_batch(&records, 365)
            .expect("append one request batch");
    }
    started.elapsed().as_secs_f64() * 1000.0
}

fn print_metric(phase: &str, wall_ms: f64, stats: &UsageLogWriteStats) {
    // Leading newline detaches the metric from libtest's `test <name> ... `
    // banner, which otherwise glues to the first captured output line.
    println!(
        "\nCWF_METRIC dataset=gateway_logging phase={phase} wall_ms={wall_ms:.3} \
         db_opens={} transactions={} rows_written={} batch_writes={}",
        stats.db_opens, stats.transactions, stats.rows_written, stats.batch_writes
    );
}

#[test]
#[ignore = "synthetic REQ-005/AC-005 measurement; run via tools/measure-core-workflows.mjs"]
fn core_workflows_perf_gateway_logging() {
    let dir = TempDir::new("logging-perf");
    let store = UsageLogStore::at(dir.0.join("ai_gateway_usage.db"));
    let base_ms = chrono::Utc::now().timestamp_millis();

    reset_usage_log_write_stats();

    let cold_wall = run_pass(&store, base_ms);
    let cold = usage_log_write_stats();
    assert!(
        cold.rows_written >= (REQUESTS * ATTEMPTS_PER_REQUEST) as u64,
        "cold pass must write every attempt row"
    );
    assert!(
        cold.batch_writes >= REQUESTS as u64,
        "cold pass must issue one batch append per request"
    );
    print_metric("cold", cold_wall, &cold);

    let mut previous = cold;
    for _ in 0..WARM_PASSES {
        let warm_wall = run_pass(&store, base_ms);
        let current = usage_log_write_stats();
        assert!(
            current.db_opens >= previous.db_opens,
            "db_opens must never decrease across passes"
        );
        assert!(
            current.transactions >= previous.transactions,
            "transactions must never decrease across passes"
        );
        assert!(
            current.rows_written >= previous.rows_written,
            "rows_written must never decrease across passes"
        );
        assert!(
            current.batch_writes >= previous.batch_writes,
            "batch_writes must never decrease across passes"
        );
        print_metric("warm", warm_wall, &current);
        previous = current;
    }
}
