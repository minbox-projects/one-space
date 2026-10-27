use crate::ai_sessions::system_time_to_epoch_millis;
use std::fs;
use std::path::Path;

pub(in crate::ai_sessions) fn history_scan_due(
    path: &Path,
    min_updated_at_ms: Option<i64>,
) -> bool {
    let Some(min_updated_at_ms) = min_updated_at_ms else {
        record_history_source_scanned();
        return true;
    };
    let due = fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .map(system_time_to_epoch_millis)
        .map(|modified_at_ms| modified_at_ms + 2_000 >= min_updated_at_ms)
        .unwrap_or(true);
    if due {
        record_history_source_scanned();
    } else {
        record_history_source_skipped();
    }
    due
}

// ---------------------------------------------------------------------------
// Benchmark-only history-sync counters.
//
// Compiled only into test builds; the ignored `core_workflows_perf` measurement
// tests reset and read them. They record whether the incremental timestamp
// cursor let a native history source be scanned or skipped. `opencode` history
// pushes its cursor into SQL/JSON filters instead of calling `history_scan_due`
// and is therefore not counted here. They change no behavior.
// ---------------------------------------------------------------------------

/// Counters describing one incremental native-history pass.
#[cfg(test)]
#[allow(dead_code)] // consumed by the Test-owned `core_workflows_perf` measurement tests
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(in crate::ai_sessions) struct HistorySyncStats {
    pub sources_scanned: u64,
    pub sources_skipped: u64,
}

#[cfg(test)]
static HISTORY_SOURCES_SCANNED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(test)]
static HISTORY_SOURCES_SKIPPED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[cfg(test)]
fn record_history_source_scanned() {
    HISTORY_SOURCES_SCANNED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}
#[cfg(not(test))]
fn record_history_source_scanned() {}

#[cfg(test)]
fn record_history_source_skipped() {
    HISTORY_SOURCES_SKIPPED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}
#[cfg(not(test))]
fn record_history_source_skipped() {}

/// Snapshot of the benchmark-only history-sync counters.
#[cfg(test)]
#[allow(dead_code)] // consumed by the Test-owned `core_workflows_perf` measurement tests
pub(in crate::ai_sessions) fn history_sync_stats() -> HistorySyncStats {
    HistorySyncStats {
        sources_scanned: HISTORY_SOURCES_SCANNED.load(std::sync::atomic::Ordering::Relaxed),
        sources_skipped: HISTORY_SOURCES_SKIPPED.load(std::sync::atomic::Ordering::Relaxed),
    }
}

/// Zero the benchmark-only history-sync counters.
#[cfg(test)]
#[allow(dead_code)] // consumed by the Test-owned `core_workflows_perf` measurement tests
pub(in crate::ai_sessions) fn reset_history_sync_stats() {
    HISTORY_SOURCES_SCANNED.store(0, std::sync::atomic::Ordering::Relaxed);
    HISTORY_SOURCES_SKIPPED.store(0, std::sync::atomic::Ordering::Relaxed);
}
