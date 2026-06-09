use std::path::PathBuf;
use std::time::Duration;

pub(in crate::ssh_tunnels) const SSH_TUNNELS_UPDATED_EVENT: &str = "ssh-tunnels-updated";
pub(in crate::ssh_tunnels) const SSH_TUNNEL_CONNECT_FAILED_EVENT: &str =
    "ssh-tunnel-connect-failed";
pub(in crate::ssh_tunnels) const SSH_TUNNEL_WINDOW_RECONNECT_START_EVENT: &str =
    "ssh-tunnel-window-reconnect-start";
pub(in crate::ssh_tunnels) const SSH_TUNNEL_WINDOW_RECONNECT_DONE_EVENT: &str =
    "ssh-tunnel-window-reconnect-done";
pub(in crate::ssh_tunnels) const PASSWORD_SECRET_PREFIX: &str = "onespace_ssh_tunnel_password:";
pub(in crate::ssh_tunnels) const LOCAL_BIND_HOST: &str = "127.0.0.1";
pub(in crate::ssh_tunnels) const REMOTE_BIND_HOST: &str = "127.0.0.1";
pub(in crate::ssh_tunnels) const DEFAULT_TUNNEL_GROUP_ID: &str = "default";
pub(in crate::ssh_tunnels) const DEFAULT_TUNNEL_GROUP_NAME: &str = "Default Group";
pub(in crate::ssh_tunnels) const SSH_IO_TIMEOUT: Duration = Duration::from_millis(1000);
pub(in crate::ssh_tunnels) const SSH_IO_RETRY_BACKOFF: Duration = Duration::from_millis(10);
pub(in crate::ssh_tunnels) const SSH_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
pub(in crate::ssh_tunnels) const SSH_SESSION_POOL_MAX_IDLE: usize = 4;
pub(in crate::ssh_tunnels) const SSH_KEEPALIVE_INTERVAL_SECS: u32 = 30;
pub(in crate::ssh_tunnels) const RECONNECT_INITIAL_BACKOFF: Duration = Duration::from_secs(2);
pub(in crate::ssh_tunnels) const RECONNECT_MAX_BACKOFF: Duration = Duration::from_secs(60);
pub(in crate::ssh_tunnels) const RECONNECT_HEALTH_CHECK_INTERVAL: Duration =
    Duration::from_secs(30);
pub(in crate::ssh_tunnels) const RECONNECT_BACKOFF_STEP: Duration = Duration::from_millis(500);
pub(in crate::ssh_tunnels) const RECONNECT_RESUME_DELAY: Duration = Duration::from_secs(15);
pub(in crate::ssh_tunnels) const RECONNECT_RECONCILE_COOLDOWN: Duration = Duration::from_secs(20);
pub(in crate::ssh_tunnels) const SLEEP_RESUME_HEARTBEAT_INTERVAL: Duration =
    Duration::from_secs(15);
pub(in crate::ssh_tunnels) const SLEEP_RESUME_GAP_THRESHOLD: Duration = Duration::from_secs(60);

/// Default SSH key files to try when no IdentityFile is specified
pub(in crate::ssh_tunnels) const DEFAULT_SSH_KEYS: &[&str] = &[
    "id_ed25519",
    "id_ed25519_sk",
    "id_ecdsa",
    "id_ecdsa_sk",
    "id_rsa",
];

/// Find existing default SSH key files in ~/.ssh/ using OpenSSH-like priority order.
pub(in crate::ssh_tunnels) fn find_default_ssh_keys() -> Vec<PathBuf> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let ssh_dir = home.join(".ssh");
    DEFAULT_SSH_KEYS
        .iter()
        .map(|key_name| ssh_dir.join(key_name))
        .filter(|path| path.exists())
        .collect()
}
