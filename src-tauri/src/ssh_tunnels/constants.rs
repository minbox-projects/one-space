const SSH_TUNNELS_UPDATED_EVENT: &str = "ssh-tunnels-updated";
const SSH_TUNNEL_CONNECT_FAILED_EVENT: &str = "ssh-tunnel-connect-failed";
const SSH_TUNNEL_WINDOW_RECONNECT_START_EVENT: &str = "ssh-tunnel-window-reconnect-start";
const SSH_TUNNEL_WINDOW_RECONNECT_DONE_EVENT: &str = "ssh-tunnel-window-reconnect-done";
const PASSWORD_SECRET_PREFIX: &str = "onespace_ssh_tunnel_password:";
const LOCAL_BIND_HOST: &str = "127.0.0.1";
const REMOTE_BIND_HOST: &str = "127.0.0.1";
const DEFAULT_TUNNEL_GROUP_ID: &str = "default";
const DEFAULT_TUNNEL_GROUP_NAME: &str = "Default Group";
const SSH_IO_TIMEOUT: Duration = Duration::from_millis(1000);
const SSH_IO_RETRY_BACKOFF: Duration = Duration::from_millis(10);
const SSH_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const SSH_SESSION_POOL_MAX_IDLE: usize = 4;
const SSH_KEEPALIVE_INTERVAL_SECS: u32 = 30;
const RECONNECT_INITIAL_BACKOFF: Duration = Duration::from_secs(2);
const RECONNECT_MAX_BACKOFF: Duration = Duration::from_secs(60);
const RECONNECT_HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(30);
const RECONNECT_BACKOFF_STEP: Duration = Duration::from_millis(500);
const RECONNECT_RESUME_DELAY: Duration = Duration::from_secs(15);
const RECONNECT_RECONCILE_COOLDOWN: Duration = Duration::from_secs(20);
const SLEEP_RESUME_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);
const SLEEP_RESUME_GAP_THRESHOLD: Duration = Duration::from_secs(60);

/// Default SSH key files to try when no IdentityFile is specified
const DEFAULT_SSH_KEYS: &[&str] = &[
    "id_ed25519",
    "id_ed25519_sk",
    "id_ecdsa",
    "id_ecdsa_sk",
    "id_rsa",
];

/// Find existing default SSH key files in ~/.ssh/ using OpenSSH-like priority order.
fn find_default_ssh_keys() -> Vec<PathBuf> {
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
