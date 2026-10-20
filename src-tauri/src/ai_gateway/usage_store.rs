//! Path-scoped reusable SQLite connections for the gateway usage database.
//!
//! Every request appends one batch and every dashboard read runs several
//! statements, so opening and migrating the database per call dominated local
//! database work. This module owns one connection per database path for each
//! role:
//!
//! * writes and reads use separate connections, so a writer never serializes a
//!   reader and unrelated paths never share a lock;
//! * each path/role entry has its own mutex; only the short lookup of the shared
//!   entry map is global, never SQL or schema initialization for another path;
//! * a handle is reused only while the actual file identity (device and inode)
//!   still matches, so a replaced, restored or deleted database is reopened and
//!   initialized against the file that is really present;
//! * a read connection also reuses its prepared statements, while a write
//!   connection runs its request transaction on the same handle;
//! * there is no result cache and no generic pool.
//!
//! The file identity is deliberately metadata-only: ordinary writes keep the
//! inode, while a replacement or restore changes it.

use rusqlite::{CachedStatement, Connection};
#[cfg(test)]
use std::cell::RefCell;
use std::collections::HashMap;
#[cfg(test)]
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

struct WriteEntry {
    connection: Option<Connection>,
    identity: Option<(u64, u64)>,
}

struct QueryEntry {
    connection: Option<Connection>,
    identity: Option<(u64, u64)>,
    /// SQL text already prepared on this exact connection. Used only to count
    /// real cache misses for the test-only work counters.
    #[cfg(test)]
    prepared: RefCell<HashSet<String>>,
}

type SharedEntry<T> = Arc<Mutex<T>>;

/// Device and inode identity of the database file. A missing file yields `None`,
/// which never matches a cached identity and therefore forces a reopen. Inodes
/// are stable across ordinary writes but change when the file is replaced or
/// restored, which is exactly when the owned database must be reopened.
fn file_identity(path: &Path) -> Option<(u64, u64)> {
    let metadata = fs::metadata(path).ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Some((metadata.dev(), metadata.ino()))
    }
    #[cfg(not(unix))]
    {
        // No inode on this platform: the existence prune below still drops a
        // deleted database, and a present file is treated as one identity.
        let _ = metadata;
        Some((0, 0))
    }
}

fn write_entries() -> &'static Mutex<HashMap<PathBuf, SharedEntry<WriteEntry>>> {
    static ENTRIES: OnceLock<Mutex<HashMap<PathBuf, SharedEntry<WriteEntry>>>> = OnceLock::new();
    ENTRIES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn query_entries() -> &'static Mutex<HashMap<PathBuf, SharedEntry<QueryEntry>>> {
    static ENTRIES: OnceLock<Mutex<HashMap<PathBuf, SharedEntry<QueryEntry>>>> = OnceLock::new();
    ENTRIES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn shared_write_entry(path: &Path) -> SharedEntry<WriteEntry> {
    let mut entries = write_entries()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    // Drop descriptors whose database no longer exists so a long run (for
    // example the whole test binary) never accumulates open handles. The
    // removed entries are released after the shared map lock is gone, so
    // closing one path never holds up another path.
    let mut removed = Vec::new();
    entries.retain(|cached_path, entry| {
        if cached_path.exists() {
            true
        } else {
            removed.push(entry.clone());
            false
        }
    });
    let entry = entries
        .entry(path.to_path_buf())
        .or_insert_with(|| {
            Arc::new(Mutex::new(WriteEntry {
                connection: None,
                identity: None,
            }))
        })
        .clone();
    drop(entries);
    drop(removed);
    entry
}

fn shared_query_entry(path: &Path) -> SharedEntry<QueryEntry> {
    let mut entries = query_entries()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut removed = Vec::new();
    entries.retain(|cached_path, entry| {
        if cached_path.exists() {
            true
        } else {
            removed.push(entry.clone());
            false
        }
    });
    let entry = entries
        .entry(path.to_path_buf())
        .or_insert_with(|| {
            Arc::new(Mutex::new(QueryEntry {
                connection: None,
                identity: None,
                #[cfg(test)]
                prepared: RefCell::new(HashSet::new()),
            }))
        })
        .clone();
    drop(entries);
    drop(removed);
    entry
}

fn needs_open(connection: &Option<Connection>, identity: Option<(u64, u64)>, path: &Path) -> bool {
    connection.is_none() || identity.is_none() || identity != file_identity(path)
}

/// Runs `use_connection` on the reusable write connection for `path`, invoking
/// `open` (which creates the parent directory, initializes the schema and runs
/// migrations) only when the connection is absent or its database changed.
///
/// The entry lock is held for the whole operation because one SQLite connection
/// cannot be used concurrently. It is never the shared entry-map lock, so a slow
/// write on one path cannot block another path.
pub(in crate::ai_gateway) fn with_write_connection<T>(
    path: &Path,
    open: impl FnOnce(&Path) -> Result<Connection, String>,
    use_connection: impl FnOnce(&mut Connection) -> Result<T, String>,
) -> Result<T, String> {
    let entry = shared_write_entry(path);
    let mut entry = entry
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if needs_open(&entry.connection, entry.identity, path) {
        let connection = open(path)?;
        entry.identity = file_identity(path);
        entry.connection = Some(connection);
    }
    use_connection(
        entry
            .connection
            .as_mut()
            .expect("write connection present after open"),
    )
}

/// A borrowed query connection with its prepared-statement cache.
pub(in crate::ai_gateway) struct QueryConnection<'a> {
    entry: &'a QueryEntry,
}

impl QueryConnection<'_> {
    /// The underlying SQLite connection, used for transactions and statements
    /// that must share the same snapshot.
    pub(in crate::ai_gateway) fn connection(&self) -> &Connection {
        self.entry
            .connection
            .as_ref()
            .expect("query connection present after open")
    }

    /// Prepares `sql` on this connection, reusing the connection's cached
    /// statement when the same SQL was used before.
    pub(in crate::ai_gateway) fn prepare(
        &self,
        sql: &str,
    ) -> Result<CachedStatement<'_>, String> {
        self.prepare_on(self.connection(), sql)
    }

    /// Prepares `sql` on a connection view that must stay tied to this query
    /// connection, such as an open transaction.
    pub(in crate::ai_gateway) fn prepare_on<'a>(
        &self,
        connection: &'a Connection,
        sql: &str,
    ) -> Result<CachedStatement<'a>, String> {
        let statement = connection.prepare_cached(sql).map_err(|error| error.to_string())?;
        self.note_preparation(sql);
        Ok(statement)
    }

    fn note_preparation(&self, sql: &str) {
        let _ = sql;
        #[cfg(test)]
        {
            if !self.entry.prepared.borrow_mut().insert(sql.to_string()) {
                return;
            }
        }
        record_query_statement_prepare();
    }
}

/// Runs `use_connection` on the reusable query connection for `path`, opening
/// and initializing the actual database only when the entry is absent or its
/// file identity changed. A deleted or replaced file therefore never keeps
/// returning rows from an obsolete handle, and prepared statements are reused
/// only on the connection that prepared them.
pub(in crate::ai_gateway) fn with_query_connection<T>(
    path: &Path,
    open: impl FnOnce(&Path) -> Result<Connection, String>,
    use_connection: impl FnOnce(&QueryConnection<'_>) -> Result<T, String>,
) -> Result<T, String> {
    let entry = shared_query_entry(path);
    let mut entry = entry
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if needs_open(&entry.connection, entry.identity, path) {
        let connection = open(path)?;
        // Dashboard reads issue several similarly shaped statements; keep
        // enough of them cached to avoid re-preparing the same SQL on the next
        // identical read. This is a prepared-statement cache, not a result
        // cache, and every statement still executes against current data.
        connection.set_prepared_statement_cache_capacity(256);
        entry.identity = file_identity(path);
        #[cfg(test)]
        entry.prepared.get_mut().clear();
        entry.connection = Some(connection);
        record_query_connection_open();
        record_query_initialization();
    }
    let query = QueryConnection { entry: &entry };
    use_connection(&query)
}

/// Real query-side work caused by the public read paths. Compiled only into test
/// builds and thread-local so parallel store tests remain independent.
#[cfg(test)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(in crate::ai_gateway) struct QueryWorkStats {
    pub connection_opens: u64,
    pub initializations: u64,
    pub statement_prepares: u64,
}

#[cfg(test)]
thread_local! {
    static QUERY_WORK_STATS: std::cell::Cell<QueryWorkStats> =
        std::cell::Cell::new(QueryWorkStats::default());
}

#[cfg(test)]
fn update_query_work_stats(update: impl FnOnce(&mut QueryWorkStats)) {
    QUERY_WORK_STATS.with(|cell| {
        let mut stats = cell.get();
        update(&mut stats);
        cell.set(stats);
    });
}

#[cfg(test)]
fn record_query_connection_open() {
    update_query_work_stats(|stats| stats.connection_opens += 1);
}
#[cfg(not(test))]
fn record_query_connection_open() {}

#[cfg(test)]
fn record_query_initialization() {
    update_query_work_stats(|stats| stats.initializations += 1);
}
#[cfg(not(test))]
fn record_query_initialization() {}

#[cfg(test)]
fn record_query_statement_prepare() {
    update_query_work_stats(|stats| stats.statement_prepares += 1);
}
#[cfg(not(test))]
fn record_query_statement_prepare() {}

/// Snapshot of the test-only query work counters on the current thread.
#[cfg(test)]
pub(in crate::ai_gateway) fn query_work_stats() -> QueryWorkStats {
    QUERY_WORK_STATS.with(|cell| cell.get())
}

/// Zero the test-only query work counters on the current thread.
#[cfg(test)]
pub(in crate::ai_gateway) fn reset_query_work_stats() {
    QUERY_WORK_STATS.with(|cell| cell.set(QueryWorkStats::default()));
}
