use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use tokio::process::Child;
use tokio::task::JoinHandle;

/// A running remote log tail: the SSH child plus the task streaming its stdout.
pub(super) struct ActiveTail {
    pub child: Child,
    pub reader: JoinHandle<()>,
}

pub(super) fn get_active_tailers() -> &'static Mutex<HashMap<String, ActiveTail>> {
    static TAILERS: OnceLock<Mutex<HashMap<String, ActiveTail>>> = OnceLock::new();
    TAILERS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(super) fn get_active_transfers() -> &'static Mutex<HashMap<String, Child>> {
    static TRANSFERS: OnceLock<Mutex<HashMap<String, Child>>> = OnceLock::new();
    TRANSFERS.get_or_init(|| Mutex::new(HashMap::new()))
}
