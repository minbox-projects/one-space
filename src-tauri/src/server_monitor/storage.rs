//! Encrypted target/settings configuration and local sample history.
//!
//! Configuration lives in `server_monitor.enc.json` under the active profile
//! application directory and is encrypted as one document with the application
//! master password. History and daily slot claims live in `server_monitor.db`.
//! All configuration mutation is serialised under one lock and published
//! atomically, so a failed write leaves the previous document intact.

use crate::server_monitor::types::{
    CollectionFailureKind, CpuMetric, DiskMetric, MemoryMetric, MetricSnapshot, NetworkMetric,
    ServerMonitorOutcome, ServerMonitorSample, ServerMonitorScheduleMode, ServerMonitorSettings,
};
pub(crate) use crate::server_monitor::types::SlotClaimOutcome;
use rusqlite::{params, Connection};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

static CONFIG_LOCK: Mutex<()> = Mutex::new(());
static DB_LOCK: Mutex<()> = Mutex::new(());

const CONFIG_FILE_NAME: &str = "server_monitor.enc.json";
const DATABASE_FILE_NAME: &str = "server_monitor.db";
const CONFIG_VERSION: u32 = 1;

/// Complete encrypted configuration document.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ServerMonitorConfig {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub settings: ServerMonitorSettings,
    #[serde(default)]
    pub targets: Vec<crate::server_monitor::types::ServerMonitorTargetRecord>,
}

fn default_version() -> u32 {
    CONFIG_VERSION
}

impl Default for ServerMonitorConfig {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            settings: ServerMonitorSettings::default(),
            targets: Vec::new(),
        }
    }
}

/// One profile-local store rooted at the profile application directory.
#[derive(Debug, Clone)]
pub(crate) struct ServerMonitorStore {
    root: PathBuf,
}

impl ServerMonitorStore {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub(crate) fn from_app_dir() -> Result<Self, String> {
        Ok(Self::new(crate::config::get_app_dir()?))
    }

    pub(crate) fn config_path(&self) -> PathBuf {
        self.root.join(CONFIG_FILE_NAME)
    }

    pub(crate) fn database_path(&self) -> PathBuf {
        self.root.join(DATABASE_FILE_NAME)
    }

    fn master_password() -> Result<String, String> {
        crate::crypto::get_or_init_master_password()
    }

    /// Loads the encrypted configuration. A missing document yields defaults;
    /// an unreadable or undecryptable document is reported and never replaced.
    pub(crate) fn load_config(&self) -> Result<ServerMonitorConfig, String> {
        let _guard = CONFIG_LOCK.lock().map_err(|error| error.to_string())?;
        self.load_config_unlocked()
    }

    fn load_config_unlocked(&self) -> Result<ServerMonitorConfig, String> {
        let path = self.config_path();
        if !path.exists() {
            return Ok(ServerMonitorConfig::default());
        }
        let encrypted = fs::read_to_string(&path)
            .map_err(|error| format!("Failed to read server monitor config: {error}"))?;
        if encrypted.trim().is_empty() {
            return Err("Server monitor config is empty".to_string());
        }
        let password = Self::master_password()?;
        let plaintext = crate::crypto::decrypt(encrypted.trim(), &password)
            .map_err(|error| format!("Failed to decrypt server monitor config: {error}"))?;
        let config: ServerMonitorConfig = serde_json::from_str(&plaintext)
            .map_err(|error| format!("Failed to parse server monitor config: {error}"))?;
        config
            .settings
            .validate()
            .map_err(|error| format!("Invalid server monitor config: {error}"))?;
        Ok(config)
    }

    /// Rewrites the complete encrypted document atomically after validation.
    /// An existing document that cannot be read or decrypted is never replaced.
    pub(crate) fn save_config(&self, config: &ServerMonitorConfig) -> Result<(), String> {
        let _guard = CONFIG_LOCK.lock().map_err(|error| error.to_string())?;
        config.settings.validate()?;
        if self.config_path().exists() {
            self.load_config_unlocked().map_err(|error| {
                format!("Refusing to overwrite unreadable server monitor config: {error}")
            })?;
        }
        let plaintext = serde_json::to_string(config)
            .map_err(|error| format!("Failed to serialize server monitor config: {error}"))?;
        let password = Self::master_password()?;
        let encrypted = crate::crypto::encrypt(&plaintext, &password)?;
        crate::atomic_write_string(&self.config_path(), &encrypted)
    }

    fn ensure_root(&self) -> Result<(), String> {
        fs::create_dir_all(&self.root)
            .map_err(|error| format!("Failed to create server monitor data directory: {error}"))
    }

    fn open_db(&self) -> Result<Connection, String> {
        self.ensure_root()?;
        let connection = Connection::open(self.database_path())
            .map_err(|error| format!("Failed to open server monitor database: {error}"))?;
        init_schema(&connection)?;
        Ok(connection)
    }

    /// Persists one normalized sample and returns its row id.
    pub(crate) fn append_sample(&self, sample: &ServerMonitorSample) -> Result<i64, String> {
        let _guard = DB_LOCK.lock().map_err(|error| error.to_string())?;
        let connection = self.open_db()?;
        let outcome = match sample.outcome {
            ServerMonitorOutcome::Success(_) => "success",
            ServerMonitorOutcome::Partial(_) => "partial",
            ServerMonitorOutcome::Failed => "failed",
        };
        let source_mode = schedule_mode_str(sample.source_mode);
        let cpu = json_column(&sample.cpu)?;
        let memory = json_column(&sample.memory)?;
        let disks = json_column(&sample.disks)?;
        let networks = serde_json::to_string(&sample.networks)
            .map_err(|error| format!("Failed to serialize network metrics: {error}"))?;
        let error_kind = sample.error_kind.map(failure_kind_str);
        connection
            .execute(
                "INSERT INTO server_monitor_samples (
                    target_id, collected_at_ms, outcome, sampling_window_seconds,
                    source_mode, expected_interval_seconds, cpu_json, memory_json,
                    disks_json, networks_json, network_interface, error_kind, error
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    sample.target_id,
                    sample.collected_at_ms,
                    outcome,
                    sample.sampling_window_seconds,
                    source_mode,
                    sample.expected_interval_seconds.map(|value| value as i64),
                    cpu,
                    memory,
                    disks,
                    networks,
                    sample.network_interface,
                    error_kind,
                    sample.error,
                ],
            )
            .map_err(|error| format!("Failed to persist server monitor sample: {error}"))?;
        Ok(connection.last_insert_rowid())
    }

    /// Reads samples in the requested window, clipped to the configured
    /// retention. Gaps and failed observations are returned as stored.
    pub(crate) fn history(
        &self,
        target_id: &str,
        from_ms: i64,
        to_ms: i64,
    ) -> Result<Vec<ServerMonitorSample>, String> {
        let _guard = DB_LOCK.lock().map_err(|error| error.to_string())?;
        let retention_days = self
            .load_config_unlocked()
            .map(|config| config.settings.retention_days)
            .unwrap_or(7);
        let retention_floor = to_ms.saturating_sub(i64::from(retention_days) * 86_400_000);
        let from_ms = from_ms.max(retention_floor);
        let connection = self.open_db()?;
        let mut statement = connection
            .prepare(
                "SELECT id, target_id, collected_at_ms, outcome, sampling_window_seconds,
                        source_mode, expected_interval_seconds, cpu_json, memory_json,
                        disks_json, networks_json, network_interface, error_kind, error
                 FROM server_monitor_samples
                 WHERE target_id = ?1 AND collected_at_ms >= ?2 AND collected_at_ms <= ?3
                 ORDER BY collected_at_ms ASC, id ASC",
            )
            .map_err(|error| format!("Failed to query server monitor history: {error}"))?;
        let rows = statement
            .query_map(params![target_id, from_ms, to_ms], |row| {
                let outcome: String = row.get(3)?;
                let source_mode: String = row.get(5)?;
                let networks_json: Option<String> = row.get(10)?;
                let networks: Vec<NetworkMetric> = networks_json
                    .as_deref()
                    .and_then(|value| serde_json::from_str(value).ok())
                    .unwrap_or_default();
                let network_interface: Option<String> = row.get(11)?;
                let network = network_interface.as_ref().and_then(|interface| {
                    networks
                        .iter()
                        .find(|metric| &metric.interface == interface)
                        .cloned()
                });
                let snapshot = || MetricSnapshot {
                    collected_at_ms: row.get(2).unwrap_or(0),
                    sampling_window_seconds: row.get(4).unwrap_or(0.0),
                    cpu: parse_json_column::<CpuMetric>(row.get(7).ok().flatten()),
                    memory: parse_json_column::<MemoryMetric>(row.get(8).ok().flatten()),
                    disks: parse_json_column::<Vec<DiskMetric>>(row.get(9).ok().flatten()),
                    network: network.clone(),
                    networks: networks.clone(),
                    network_interface: network_interface.clone(),
                };
                let monitor_outcome = match outcome.as_str() {
                    "success" => ServerMonitorOutcome::Success(snapshot()),
                    "partial" => ServerMonitorOutcome::Partial(snapshot()),
                    _ => ServerMonitorOutcome::Failed,
                };
                let error_kind: Option<String> = row.get(12)?;
                Ok(ServerMonitorSample {
                    id: Some(row.get(0)?),
                    target_id: row.get(1)?,
                    collected_at_ms: row.get(2)?,
                    outcome: monitor_outcome,
                    sampling_window_seconds: row.get(4)?,
                    source_mode: parse_schedule_mode(&source_mode),
                    expected_interval_seconds: row
                        .get::<_, Option<i64>>(6)?
                        .map(|value| value as u64),
                    cpu: parse_json_column::<CpuMetric>(row.get(7).ok().flatten()),
                    memory: parse_json_column::<MemoryMetric>(row.get(8).ok().flatten()),
                    disks: parse_json_column::<Vec<DiskMetric>>(row.get(9).ok().flatten()),
                    networks,
                    network_interface,
                    error_kind: error_kind.as_deref().and_then(parse_failure_kind),
                    error: row.get(13)?,
                })
            })
            .map_err(|error| format!("Failed to read server monitor history: {error}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("Failed to decode server monitor history: {error}"))
    }

    /// Claims one target's local-date/time slot. The first claim wins.
    pub(crate) fn claim_slot(
        &self,
        target_id: &str,
        local_date: &str,
        planned_time: &str,
        claimed_at_ms: i64,
    ) -> Result<SlotClaimOutcome, String> {
        let _guard = DB_LOCK.lock().map_err(|error| error.to_string())?;
        let connection = self.open_db()?;
        let inserted = connection
            .execute(
                "INSERT OR IGNORE INTO server_monitor_slot_claims
                    (target_id, local_date, planned_time, claimed_at_ms)
                 VALUES (?1, ?2, ?3, ?4)",
                params![target_id, local_date, planned_time, claimed_at_ms],
            )
            .map_err(|error| format!("Failed to claim server monitor slot: {error}"))?;
        Ok(if inserted == 1 {
            SlotClaimOutcome::Claimed
        } else {
            SlotClaimOutcome::AlreadyClaimed
        })
    }

    #[cfg(test)]
    pub(crate) fn has_claim(
        &self,
        target_id: &str,
        local_date: &str,
        planned_time: &str,
    ) -> Result<bool, String> {
        let _guard = DB_LOCK.lock().map_err(|error| error.to_string())?;
        let connection = self.open_db()?;
        connection
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM server_monitor_slot_claims
                    WHERE target_id = ?1 AND local_date = ?2 AND planned_time = ?3
                 )",
                params![target_id, local_date, planned_time],
                |row| row.get::<_, i64>(0),
            )
            .map(|value| value == 1)
            .map_err(|error| format!("Failed to read server monitor slot claim: {error}"))
    }

    /// Removes only one target's samples and claims.
    pub(crate) fn delete_target_data(&self, target_id: &str) -> Result<(), String> {
        let _guard = DB_LOCK.lock().map_err(|error| error.to_string())?;
        let connection = self.open_db()?;
        connection
            .execute(
                "DELETE FROM server_monitor_samples WHERE target_id = ?1",
                params![target_id],
            )
            .map_err(|error| format!("Failed to delete server monitor samples: {error}"))?;
        connection
            .execute(
                "DELETE FROM server_monitor_slot_claims WHERE target_id = ?1",
                params![target_id],
            )
            .map_err(|error| format!("Failed to delete server monitor slot claims: {error}"))?;
        Ok(())
    }

    /// Prunes old samples while keeping today's and future slot claims.
    pub(crate) fn prune(
        &self,
        retention_days: u32,
        now_ms: i64,
        today: &str,
    ) -> Result<(), String> {
        let _guard = DB_LOCK.lock().map_err(|error| error.to_string())?;
        let connection = self.open_db()?;
        let cutoff = now_ms.saturating_sub(i64::from(retention_days) * 86_400_000);
        connection
            .execute(
                "DELETE FROM server_monitor_samples WHERE collected_at_ms < ?1",
                params![cutoff],
            )
            .map_err(|error| format!("Failed to prune server monitor samples: {error}"))?;
        connection
            .execute(
                "DELETE FROM server_monitor_slot_claims WHERE local_date < ?1",
                params![today],
            )
            .map_err(|error| format!("Failed to prune server monitor slot claims: {error}"))?;
        Ok(())
    }
}

fn init_schema(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS server_monitor_samples (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                target_id TEXT NOT NULL,
                collected_at_ms INTEGER NOT NULL,
                outcome TEXT NOT NULL,
                sampling_window_seconds REAL NOT NULL,
                source_mode TEXT NOT NULL,
                expected_interval_seconds INTEGER,
                cpu_json TEXT,
                memory_json TEXT,
                disks_json TEXT,
                networks_json TEXT,
                network_interface TEXT,
                error_kind TEXT,
                error TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_server_monitor_samples_target_time
                ON server_monitor_samples(target_id, collected_at_ms DESC);
            CREATE TABLE IF NOT EXISTS server_monitor_slot_claims (
                target_id TEXT NOT NULL,
                local_date TEXT NOT NULL,
                planned_time TEXT NOT NULL,
                claimed_at_ms INTEGER NOT NULL,
                PRIMARY KEY (target_id, local_date, planned_time)
            );",
        )
        .map_err(|error| format!("Failed to initialize server monitor database: {error}"))
}

fn json_column<T: serde::Serialize>(value: &Option<T>) -> Result<Option<String>, String> {
    match value {
        Some(value) => serde_json::to_string(value)
            .map(Some)
            .map_err(|error| format!("Failed to serialize metric: {error}")),
        None => Ok(None),
    }
}

fn parse_json_column<T: serde::de::DeserializeOwned>(value: Option<String>) -> Option<T> {
    value.and_then(|value| serde_json::from_str(&value).ok())
}

fn schedule_mode_str(mode: ServerMonitorScheduleMode) -> &'static str {
    match mode {
        ServerMonitorScheduleMode::Interval => "interval",
        ServerMonitorScheduleMode::Daily => "daily",
    }
}

fn parse_schedule_mode(value: &str) -> ServerMonitorScheduleMode {
    if value == "daily" {
        ServerMonitorScheduleMode::Daily
    } else {
        ServerMonitorScheduleMode::Interval
    }
}

fn failure_kind_str(kind: CollectionFailureKind) -> &'static str {
    match kind {
        CollectionFailureKind::NonLinux => "nonLinux",
        CollectionFailureKind::Script => "script",
        CollectionFailureKind::Timeout => "timeout",
        CollectionFailureKind::NoUsableResources => "noUsableResources",
        CollectionFailureKind::Transport => "transport",
        CollectionFailureKind::Auth => "auth",
        CollectionFailureKind::HostKey => "hostKey",
        CollectionFailureKind::Config => "config",
    }
}

fn parse_failure_kind(value: &str) -> Option<CollectionFailureKind> {
    Some(match value {
        "nonLinux" => CollectionFailureKind::NonLinux,
        "script" => CollectionFailureKind::Script,
        "timeout" => CollectionFailureKind::Timeout,
        "noUsableResources" => CollectionFailureKind::NoUsableResources,
        "transport" => CollectionFailureKind::Transport,
        "auth" => CollectionFailureKind::Auth,
        "hostKey" => CollectionFailureKind::HostKey,
        "config" => CollectionFailureKind::Config,
        _ => return None,
    })
}
