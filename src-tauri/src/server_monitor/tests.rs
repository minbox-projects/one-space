//! Delegated behavior tests for frozen plan Step 1 (`20261026-server-monitor-ssh-schedules`).
//!
//! RED baseline: the product `server_monitor` module is added by the backend
//! implementation step. These tests target only the public boundary described in
//! the frozen plan/spec and will not compile until that module is wired.

mod metrics;
mod transport;
