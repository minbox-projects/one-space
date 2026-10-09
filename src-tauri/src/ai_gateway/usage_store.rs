//! Path-scoped reusable SQLite write connections for the gateway usage log
//! (REQ-005/AC-005).
//!
//! The gateway writes one request's attempts as a batch on every request. Opening
//! and migrating the database for each batch dominated the write path, so this
//! module owns one write connection per database path:
//!
//! * distinct paths stay isolated;
//! * the connection is initialized and migrated when its actual database is
//!   first opened;
//! * it is reopened when the owned file was replaced or restored (its identity
//!   changes) and dropped when the file no longer exists, so a long run cannot
//!   accumulate descriptors;
//! * there is no generic pool: the operation that needs the connection borrows
//!   it under the pool lock for its whole duration.
//!
//! Query connections may stay separate; only the write path uses this cache.

use rusqlite::Connection;
use std::collections::HashMap;
use std::fs;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

struct CachedConnection {
    connection: Connection,
    identity: Option<(u64, u64)>,
}

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

fn connections() -> &'static Mutex<HashMap<PathBuf, CachedConnection>> {
    static CONNECTIONS: OnceLock<Mutex<HashMap<PathBuf, CachedConnection>>> = OnceLock::new();
    CONNECTIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Borrows the cached write connection for one database path while its pool
/// lock is held. Dereferences to the underlying [`Connection`].
pub(in crate::ai_gateway) struct WriteConnection {
    guard: MutexGuard<'static, HashMap<PathBuf, CachedConnection>>,
    path: PathBuf,
}

impl Deref for WriteConnection {
    type Target = Connection;

    fn deref(&self) -> &Connection {
        &self
            .guard
            .get(&self.path)
            .expect("write connection present")
            .connection
    }
}

impl DerefMut for WriteConnection {
    fn deref_mut(&mut self) -> &mut Connection {
        &mut self
            .guard
            .get_mut(&self.path)
            .expect("write connection present")
            .connection
    }
}

/// Returns the reusable write connection for `path`, invoking `open` (which the
/// caller uses to create the parent directory, initialize the schema and run
/// migrations) only when the connection is absent or its database changed.
pub(in crate::ai_gateway) fn write_connection<F>(
    path: &Path,
    open: F,
) -> Result<WriteConnection, String>
where
    F: FnOnce(&Path) -> Result<Connection, String>,
{
    let mut guard = connections()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    // Drop descriptors whose database no longer exists so a long run (for
    // example the whole test binary) never accumulates open handles.
    guard.retain(|cached_path, _| cached_path.exists());

    let key = path.to_path_buf();
    let current_identity = file_identity(path);
    let needs_open = match guard.get(&key) {
        Some(cached) => cached.identity.is_none() || cached.identity != current_identity,
        None => true,
    };
    if needs_open {
        let connection = open(path)?;
        let identity = file_identity(path);
        guard.insert(
            key.clone(),
            CachedConnection {
                connection,
                identity,
            },
        );
    }
    Ok(WriteConnection { guard, path: key })
}
