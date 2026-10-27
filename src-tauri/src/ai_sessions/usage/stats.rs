// ---------------------------------------------------------------------------
// Benchmark-only usage-collection counters.
//
// Compiled only into test builds; the ignored `core_workflows_perf` measurement
// tests reset and read them. They never run in production and change no
// behavior. `cache_hits` counts reuse of the per-tool scan cache (all four
// tools) and of the per-source parse cache.
// ---------------------------------------------------------------------------

/// Counters describing one or more usage-collection passes.
#[cfg(test)]
#[allow(dead_code)] // consumed by the Test-owned `core_workflows_perf` measurement tests
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UsageCollectionStats {
    pub source_reads: u64,
    pub parsed_entries: u64,
    pub collection_calls: u64,
    pub cache_hits: u64,
}

#[cfg(test)]
static USAGE_SOURCE_READS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(test)]
static USAGE_PARSED_ENTRIES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(test)]
static USAGE_COLLECTION_CALLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(test)]
static USAGE_CACHE_HITS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[cfg(test)]
pub(super) fn record_usage_source_read() {
    USAGE_SOURCE_READS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}
#[cfg(not(test))]
pub(super) fn record_usage_source_read() {}

#[cfg(test)]
pub(super) fn record_usage_parsed_entry() {
    USAGE_PARSED_ENTRIES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}
#[cfg(not(test))]
pub(super) fn record_usage_parsed_entry() {}

#[cfg(test)]
pub(super) fn record_usage_collection_call() {
    USAGE_COLLECTION_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}
#[cfg(not(test))]
pub(super) fn record_usage_collection_call() {}

#[cfg(test)]
pub(in crate::ai_sessions) fn record_usage_cache_hit() {
    USAGE_CACHE_HITS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}
#[cfg(not(test))]
pub(in crate::ai_sessions) fn record_usage_cache_hit() {}

/// Snapshot of the benchmark-only usage-collection counters.
#[cfg(test)]
#[allow(dead_code)] // consumed by the Test-owned `core_workflows_perf` measurement tests
pub fn usage_collection_stats() -> UsageCollectionStats {
    UsageCollectionStats {
        source_reads: USAGE_SOURCE_READS.load(std::sync::atomic::Ordering::Relaxed),
        parsed_entries: USAGE_PARSED_ENTRIES.load(std::sync::atomic::Ordering::Relaxed),
        collection_calls: USAGE_COLLECTION_CALLS.load(std::sync::atomic::Ordering::Relaxed),
        cache_hits: USAGE_CACHE_HITS.load(std::sync::atomic::Ordering::Relaxed),
    }
}

/// Zero the benchmark-only usage-collection counters.
#[cfg(test)]
#[allow(dead_code)] // consumed by the Test-owned `core_workflows_perf` measurement tests
pub fn reset_usage_collection_stats() {
    USAGE_SOURCE_READS.store(0, std::sync::atomic::Ordering::Relaxed);
    USAGE_PARSED_ENTRIES.store(0, std::sync::atomic::Ordering::Relaxed);
    USAGE_COLLECTION_CALLS.store(0, std::sync::atomic::Ordering::Relaxed);
    USAGE_CACHE_HITS.store(0, std::sync::atomic::Ordering::Relaxed);
}
