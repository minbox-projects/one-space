//! Wall-clock and deterministic test clock for usage logging.

use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(test)]
thread_local! {
    /// Deterministic test clock honored by [`now_millis`]. Set only through the
    /// public test seams so migration, retention and reporting share one instant.
    static USAGE_CLOCK_OVERRIDE_MS: std::cell::Cell<Option<i64>> = std::cell::Cell::new(None);
}

/// Current wall-clock time in epoch milliseconds, or the injected test clock
/// when one is armed on the current thread.
pub(in crate::ai_gateway) fn now_millis() -> i64 {
    #[cfg(test)]
    {
        if let Some(millis) = USAGE_CLOCK_OVERRIDE_MS.with(|cell| cell.get()) {
            return millis;
        }
    }
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

/// Pin the current thread's deterministic usage clock (test seam).
#[cfg(test)]
pub(in crate::ai_gateway) fn set_usage_clock_for_test(millis: i64) {
    USAGE_CLOCK_OVERRIDE_MS.with(|cell| cell.set(Some(millis)));
}

/// Clear the current thread's deterministic usage clock (test seam).
#[cfg(test)]
pub(in crate::ai_gateway) fn clear_usage_clock_for_test() {
    USAGE_CLOCK_OVERRIDE_MS.with(|cell| cell.set(None));
}
