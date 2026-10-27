//! Process-owned interval and daily collection scheduler.
//!
//! The scheduler runs independently of any page. Interval mode owns one
//! persistent session per target and rearms when a collection finishes. Daily
//! mode claims a target's local date/time in SQLite before any network work and
//! uses a one-shot session per attempt. At most four collections run at once and
//! a target never has more than one in-flight collection.

mod collector;
mod scheduler;
mod state;

pub(crate) use crate::server_monitor::types::ServerMonitorHistoryRange;
#[allow(unused_imports)]
pub(crate) use state::{
    global, init_global, shutdown_global, CollectionReport, ServerMonitorClock,
    ServerMonitorEventSink, ServerMonitorRuntime,
};

#[cfg(test)]
pub(crate) use state::NoopEventSink;
