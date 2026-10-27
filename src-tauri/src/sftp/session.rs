use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use tokio::process::Child;

pub(super) fn get_active_tailers() -> &'static Mutex<HashMap<String, Child>> {
    static TAILERS: OnceLock<Mutex<HashMap<String, Child>>> = OnceLock::new();
    TAILERS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(super) fn get_active_transfers() -> &'static Mutex<HashMap<String, Child>> {
    static TRANSFERS: OnceLock<Mutex<HashMap<String, Child>>> = OnceLock::new();
    TRANSFERS.get_or_init(|| Mutex::new(HashMap::new()))
}
