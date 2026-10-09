//! Reusable source collection caches for AI usage statistics (REQ-005/AC-005).
//!
//! Two layers cooperate:
//!
//! * [`ToolScanCache`] caches one tool's whole [`ToolScan`] for a requested
//!   window. Within the 30-second freshness window an unchanged window is
//!   returned without touching any source, and the per-tool mutex is held for
//!   the whole collection so concurrent callers coalesce onto one collection.
//!   [`super::sessions_usage_clear_cache`] clears it (explicit refresh).
//! * [`UsageFileCache`] memoizes one source file's parsed [`UsageRecord`]s keyed
//!   by file identity (path + length + mtime) rather than a hash. A changed,
//!   truncated or deleted file is re-read on the next collection and only that
//!   file's contribution changes; unchanged files are reused. The same cache is
//!   consulted while a stale tool scan is rebuilt, so a 30-second expiry or an
//!   explicit refresh re-reads only what actually changed.
//!
//! The per-source file identity is deliberately length + mtime. Hashes are
//! never used, so a same-length edit with a distinct mtime is still detected.

use super::{system_time_to_epoch_millis, HistorySessionEntry, ToolScan, UsageRecord};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Freshness window for whole-tool collection reuse.
pub(in crate::ai_sessions) const USAGE_SCAN_CACHE_TTL: Duration = Duration::from_secs(30);

/// Identity of one source file: byte length and modification time. Unknown
/// metadata means the source must be parsed (never silently dropped).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ai_sessions) struct SourceFileMeta {
    pub(in crate::ai_sessions) len: u64,
    pub(in crate::ai_sessions) modified_ms: i64,
}

impl SourceFileMeta {
    pub(in crate::ai_sessions) fn for_path(path: &Path) -> Option<Self> {
        let metadata = fs::metadata(path).ok()?;
        let modified_ms = metadata
            .modified()
            .ok()
            .map(system_time_to_epoch_millis)
            .unwrap_or(0);
        Some(Self {
            len: metadata.len(),
            modified_ms,
        })
    }
}

#[derive(Debug)]
struct CachedToolScan {
    collected_at: Instant,
    start_ms: i64,
    end_ms: i64,
    scan: Arc<ToolScan>,
}

/// One tool's window-scoped scan cache. Entries are keyed by the resolved
/// source root so isolated HOMEs (and their tests) never share a collection,
/// while production keeps one root and one reusable scan per tool.
#[derive(Debug, Default)]
pub(in crate::ai_sessions) struct ToolScanCache {
    entries: Mutex<HashMap<PathBuf, CachedToolScan>>,
}

impl ToolScanCache {
    /// Returns the cached scan when it is within the freshness window and its
    /// window covers the request; otherwise the lock is held while `collect`
    /// runs so concurrent callers coalesce onto a single collection.
    #[cfg(test)]
    pub(in crate::ai_sessions) fn get_or_collect<F>(
        &self,
        start_ms: i64,
        end_ms: i64,
        collect: F,
    ) -> Arc<ToolScan>
    where
        F: FnOnce() -> ToolScan,
    {
        self.get_or_collect_keyed(Path::new(""), start_ms, end_ms, collect)
    }

    /// [`Self::get_or_collect`] for a specific resolved source root.
    pub(in crate::ai_sessions) fn get_or_collect_keyed<F>(
        &self,
        root: &Path,
        start_ms: i64,
        end_ms: i64,
        collect: F,
    ) -> Arc<ToolScan>
    where
        F: FnOnce() -> ToolScan,
    {
        let key = root.to_path_buf();
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(cached) = entries.get(&key) {
            if cached.collected_at.elapsed() < USAGE_SCAN_CACHE_TTL
                && cached.start_ms <= start_ms
                && cached.end_ms >= end_ms
            {
                super::record_usage_cache_hit();
                return cached.scan.clone();
            }
        }

        let scan = Arc::new(collect());
        entries.insert(
            key,
            CachedToolScan {
                collected_at: Instant::now(),
                start_ms,
                end_ms,
                scan: Arc::clone(&scan),
            },
        );
        scan
    }

    pub(in crate::ai_sessions) fn clear(&self) {
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        entries.clear();
    }
}

/// Per-tool scan caches for all four supported tools.
#[derive(Debug)]
struct UsageScanCaches {
    by_tool: HashMap<&'static str, ToolScanCache>,
}

impl UsageScanCaches {
    fn for_tool(&self, tool: &str) -> Option<&ToolScanCache> {
        self.by_tool.get(tool)
    }

    fn clear(&self) {
        for cache in self.by_tool.values() {
            cache.clear();
        }
    }
}

impl Default for UsageScanCaches {
    fn default() -> Self {
        Self {
            by_tool: super::USAGE_TOOLS
                .iter()
                .copied()
                .map(|tool| (tool, ToolScanCache::default()))
                .collect(),
        }
    }
}

fn usage_scan_caches() -> &'static UsageScanCaches {
    static CACHES: OnceLock<UsageScanCaches> = OnceLock::new();
    CACHES.get_or_init(UsageScanCaches::default)
}

/// Clears every per-tool scan cache. Called by the explicit-refresh command.
pub(in crate::ai_sessions) fn clear_tool_scan_caches() {
    usage_scan_caches().clear();
}

/// Looks up the scan cache for `tool`, if the tool is supported.
pub(in crate::ai_sessions) fn tool_scan_cache(tool: &str) -> Option<&'static ToolScanCache> {
    usage_scan_caches().for_tool(tool)
}

// ---------------------------------------------------------------------------
// Per-source parsed-record reuse
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct CachedUsageRecords {
    meta: SourceFileMeta,
    records: Arc<Vec<UsageRecord>>,
}

/// Memoizes parsed usage records per source file by file identity.
#[derive(Debug, Default)]
pub(in crate::ai_sessions) struct UsageFileCache {
    entries: Mutex<HashMap<PathBuf, CachedUsageRecords>>,
}

impl UsageFileCache {
    /// Returns the file's cached records when its metadata is unchanged;
    /// otherwise parses it (recording the source read inside `parse`) and stores
    /// the successful result. A parse error is returned and never cached, so a
    /// later attempt retries. A file without metadata is always parsed.
    pub(in crate::ai_sessions) fn get_or_parse<F>(
        &self,
        path: &Path,
        parse: F,
    ) -> Result<Arc<Vec<UsageRecord>>, String>
    where
        F: FnOnce() -> Result<Vec<UsageRecord>, String>,
    {
        let Some(meta) = SourceFileMeta::for_path(path) else {
            return parse().map(Arc::new);
        };
        {
            let entries = self
                .entries
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Some(cached) = entries.get(path) {
                if cached.meta == meta {
                    super::record_usage_cache_hit();
                    return Ok(Arc::clone(&cached.records));
                }
            }
        }
        let records = Arc::new(parse()?);
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        entries.insert(
            path.to_path_buf(),
            CachedUsageRecords {
                meta,
                records: Arc::clone(&records),
            },
        );
        Ok(records)
    }

    pub(in crate::ai_sessions) fn clear(&self) {
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        entries.clear();
    }
}

fn usage_file_cache() -> &'static UsageFileCache {
    static CACHE: OnceLock<UsageFileCache> = OnceLock::new();
    CACHE.get_or_init(UsageFileCache::default)
}

/// Shared per-source usage parse cache.
pub(in crate::ai_sessions) fn shared_usage_file_cache() -> &'static UsageFileCache {
    usage_file_cache()
}

// ---------------------------------------------------------------------------
// Per-source history parse reuse
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct CachedHistoryEntry {
    meta: SourceFileMeta,
    entry: Option<Arc<HistorySessionEntry>>,
}

/// Memoizes one history source file's parsed entry by file identity. The
/// `dependency` discriminator folds in any external input the parse depends on
/// (for example a shared index file's identity) as plain metadata, so a changed
/// dependency forces a re-parse while an unchanged file is reused.
#[derive(Debug, Default)]
pub(in crate::ai_sessions) struct HistoryFileCache {
    entries: Mutex<HashMap<(PathBuf, String), CachedHistoryEntry>>,
}

impl HistoryFileCache {
    pub(in crate::ai_sessions) fn get_or_parse<F>(
        &self,
        path: &Path,
        dependency: &str,
        parse: F,
    ) -> Option<Arc<HistorySessionEntry>>
    where
        F: FnOnce() -> Option<HistorySessionEntry>,
    {
        let Some(meta) = SourceFileMeta::for_path(path) else {
            return parse().map(Arc::new);
        };
        let key = (path.to_path_buf(), dependency.to_string());
        {
            let entries = self
                .entries
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Some(cached) = entries.get(&key) {
                if cached.meta == meta {
                    super::record_usage_cache_hit();
                    return cached.entry.clone();
                }
            }
        }
        let entry = parse().map(Arc::new);
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        entries.insert(
            key,
            CachedHistoryEntry {
                meta,
                entry: entry.clone(),
            },
        );
        entry
    }

    pub(in crate::ai_sessions) fn clear(&self) {
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        entries.clear();
    }
}

fn history_file_cache() -> &'static HistoryFileCache {
    static CACHE: OnceLock<HistoryFileCache> = OnceLock::new();
    CACHE.get_or_init(HistoryFileCache::default)
}

/// Shared per-source history parse cache.
pub(in crate::ai_sessions) fn shared_history_file_cache() -> &'static HistoryFileCache {
    history_file_cache()
}

/// Clears both per-source caches. Called by the explicit-refresh command.
pub(in crate::ai_sessions) fn clear_source_file_caches() {
    usage_file_cache().clear();
    history_file_cache().clear();
}
