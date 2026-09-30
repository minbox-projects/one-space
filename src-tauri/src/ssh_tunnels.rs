mod commands;
mod constants;
mod forwarding_runtime;
mod reconnect;
#[cfg(test)]
mod tests;
mod types_state;
mod validation_ssh_config;

pub(in crate::ssh_tunnels) use commands::*;
pub(in crate::ssh_tunnels) use constants::*;
pub(in crate::ssh_tunnels) use forwarding_runtime::*;
pub(in crate::ssh_tunnels) use types_state::*;
pub(in crate::ssh_tunnels) use validation_ssh_config::*;

pub use commands::*;
pub use reconnect::{start_sleep_resume_monitor, start_system_wake_observer};

/// Starts the periodic SSH-tunnel watchdog.
///
/// The `forwarding_runtime` glob re-export only widens to
/// `pub(in crate::ssh_tunnels)`, so the public entry point is exposed here.
pub fn start_tunnel_watchdog(app: tauri::AppHandle) {
    forwarding_runtime::start_tunnel_watchdog(app);
}
pub use types_state::*;
