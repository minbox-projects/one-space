//! Shared value types for server resource collection.
//!
//! Optional resource fields carry a real absence: an unavailable measurement is
//! `None` and is never silently represented as zero. Persisted settings and
//! samples serialise without credentials; target records stay inside the
//! encrypted configuration store and are exposed through safe views.

use serde::{Deserialize, Serialize};

/// Raw framed output of one collector execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CollectorOutput {
    pub stdout: String,
    pub exit_code: Option<i32>,
}

/// How a monitor session owns its SSH connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionLifetime {
    /// Connect once and keep the session for later collections.
    Persistent,
    /// Connect for one collection and disconnect afterwards.
    OneShot,
}

/// Connection source for a monitor collection.
#[derive(Debug, Clone)]
pub(crate) enum MonitorSource {
    SavedHost {
        alias: String,
    },
    Custom {
        host: String,
        port: u16,
        user: String,
        auth: MonitorAuth,
    },
}

/// Authentication choice for a custom connection.
#[derive(Debug, Clone)]
pub(crate) enum MonitorAuth {
    Password,
    Key { path: String },
    Agent,
}

/// One collection's connection request.
#[derive(Debug, Clone)]
pub(crate) struct MonitorConnectRequest {
    #[allow(dead_code)]
    pub target_id: String,
    pub source: MonitorSource,
    pub password: Option<String>,
    pub tunnel: Option<ServerMonitorTunnelRecord>,
}

/// Classified SSH-side failure. Transport and timeout may be retried by a later
/// ordinary collection trigger; authentication, host-key and configuration
/// failures require explicit correction or retry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MonitorTransportErrorKind {
    Transport,
    Auth,
    HostKey,
    Config,
    Timeout,
}

#[derive(Debug, Clone)]
pub(crate) struct MonitorTransportError {
    pub kind: MonitorTransportErrorKind,
    pub message: String,
}

impl std::fmt::Display for MonitorTransportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for MonitorTransportError {}

/// Collection-level failure categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum CollectionFailureKind {
    NonLinux,
    Script,
    Timeout,
    NoUsableResources,
    Transport,
    Auth,
    HostKey,
    Config,
}

impl CollectionFailureKind {
    pub(crate) fn is_blocking(self) -> bool {
        matches!(
            self,
            CollectionFailureKind::Auth
                | CollectionFailureKind::HostKey
                | CollectionFailureKind::Config
        )
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CollectionFailure {
    pub kind: CollectionFailureKind,
    pub message: String,
}

/// CPU utilization over the collector's own approximately one-second window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CpuMetric {
    pub usage_percent: f64,
}

/// Memory snapshot derived from `/proc/meminfo`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MemoryMetric {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub used_percent: f64,
}

/// One local persistent mount reported by `df`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiskMetric {
    pub filesystem: String,
    pub mount_point: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub used_percent: f64,
}

/// Receive/transmit rate for one interface over the collector's sampling window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NetworkMetric {
    pub interface: String,
    pub rx_bytes_per_sec: f64,
    pub tx_bytes_per_sec: f64,
    pub rx_delta_bytes: u64,
    pub tx_delta_bytes: u64,
    pub elapsed_seconds: f64,
}

/// Available resources from one self-contained collection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MetricSnapshot {
    pub collected_at_ms: i64,
    /// Actual positive uptime delta measured by the collector, even when no
    /// network rate could be derived.
    #[serde(default)]
    pub sampling_window_seconds: f64,
    pub cpu: Option<CpuMetric>,
    pub memory: Option<MemoryMetric>,
    pub disks: Option<Vec<DiskMetric>>,
    pub network: Option<NetworkMetric>,
    #[serde(default)]
    pub networks: Vec<NetworkMetric>,
    #[serde(default)]
    pub network_interface: Option<String>,
}

/// Result of a completed collector execution.
#[derive(Debug, Clone)]
pub(crate) enum CollectionOutcome {
    Success(MetricSnapshot),
    Partial(MetricSnapshot),
    Failed(CollectionFailure),
}

/// Mutually exclusive collection schedules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ServerMonitorScheduleMode {
    Interval,
    Daily,
}

/// Connection source kind for a saved target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ServerMonitorSourceKind {
    SavedHost,
    Custom,
}

/// Authentication choice for a custom target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ServerMonitorAuthKind {
    Password,
    Key,
    Agent,
}

/// Whether a scheduling slot has already been attempted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SlotClaimOutcome {
    Claimed,
    AlreadyClaimed,
}

fn default_auto_collect_enabled() -> bool {
    true
}

fn default_schedule_mode() -> ServerMonitorScheduleMode {
    ServerMonitorScheduleMode::Interval
}

fn default_interval_minutes() -> u32 {
    5
}

fn default_daily_times() -> Vec<String> {
    vec!["09:00".to_string()]
}

fn default_retention_days() -> u32 {
    7
}

/// Persisted scheduling settings, shared by the tool and Settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ServerMonitorSettings {
    #[serde(default = "default_auto_collect_enabled")]
    pub auto_collect_enabled: bool,
    #[serde(default = "default_schedule_mode")]
    pub schedule_mode: ServerMonitorScheduleMode,
    #[serde(default = "default_interval_minutes")]
    pub interval_minutes: u32,
    #[serde(default = "default_daily_times")]
    pub daily_times: Vec<String>,
    #[serde(default = "default_retention_days")]
    pub retention_days: u32,
}

impl Default for ServerMonitorSettings {
    fn default() -> Self {
        Self {
            auto_collect_enabled: default_auto_collect_enabled(),
            schedule_mode: default_schedule_mode(),
            interval_minutes: default_interval_minutes(),
            daily_times: default_daily_times(),
            retention_days: default_retention_days(),
        }
    }
}

/// Returns true when `value` is exactly `HH:mm` in 24-hour form.
pub(crate) fn is_valid_hh_mm(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 5 || bytes[2] != b':' {
        return false;
    }
    if !bytes[0].is_ascii_digit()
        || !bytes[1].is_ascii_digit()
        || !bytes[3].is_ascii_digit()
        || !bytes[4].is_ascii_digit()
    {
        return false;
    }
    let hour = (bytes[0] - b'0') as u32 * 10 + (bytes[1] - b'0') as u32;
    let minute = (bytes[3] - b'0') as u32 * 10 + (bytes[4] - b'0') as u32;
    hour < 24 && minute < 60
}

impl ServerMonitorSettings {
    /// Strict validation without silent correction of any submitted value.
    pub(crate) fn validate(&self) -> Result<(), String> {
        if !(1..=1440).contains(&self.interval_minutes) {
            return Err("Interval must be an integer from 1 to 1440 minutes".to_string());
        }
        if !(1..=30).contains(&self.retention_days) {
            return Err("Retention must be an integer from 1 to 30 days".to_string());
        }
        if self.daily_times.is_empty() {
            return Err("At least one daily collection time is required".to_string());
        }
        let mut previous: Option<&str> = None;
        for time in &self.daily_times {
            if !is_valid_hh_mm(time) {
                return Err(format!("Invalid daily collection time: {time}"));
            }
            if let Some(previous) = previous {
                if previous == time {
                    return Err(format!("Duplicate daily collection time: {time}"));
                }
                if previous > time.as_str() {
                    return Err("Daily collection times must be sorted".to_string());
                }
            }
            previous = Some(time);
        }
        Ok(())
    }
}

/// Encrypted-record target. Passwords never leave the configuration store.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ServerMonitorCustomRecord {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub auth_kind: ServerMonitorAuthKind,
    #[serde(default)]
    pub key_path: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
}

/// Encrypted-record SSH tunnel / jump host configuration for targets without a public IP.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ServerMonitorTunnelRecord {
    pub enabled: bool,
    pub source_kind: ServerMonitorSourceKind,
    #[serde(default)]
    pub saved_host_name: Option<String>,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub auth_kind: Option<ServerMonitorAuthKind>,
    #[serde(default)]
    pub key_path: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
}

/// Safe view of SSH tunnel / jump host settings for commands and events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ServerMonitorTunnelView {
    pub enabled: bool,
    pub source_kind: ServerMonitorSourceKind,
    pub saved_host_name: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub user: Option<String>,
    pub auth_kind: Option<ServerMonitorAuthKind>,
    pub key_path: Option<String>,
    pub has_password: bool,
}

impl ServerMonitorTunnelRecord {
    pub(crate) fn view(&self) -> ServerMonitorTunnelView {
        ServerMonitorTunnelView {
            enabled: self.enabled,
            source_kind: self.source_kind,
            saved_host_name: self.saved_host_name.clone(),
            host: self.host.clone(),
            port: self.port,
            user: self.user.clone(),
            auth_kind: self.auth_kind,
            key_path: self.key_path.clone(),
            has_password: self
                .password
                .as_ref()
                .map(|password| !password.is_empty())
                .unwrap_or(false),
        }
    }
}

/// Encrypted-record target. Passwords never leave the configuration store.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ServerMonitorTargetRecord {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub source_kind: ServerMonitorSourceKind,
    #[serde(default)]
    pub saved_host_name: Option<String>,
    #[serde(default)]
    pub custom: Option<ServerMonitorCustomRecord>,
    #[serde(default)]
    pub tunnel: Option<ServerMonitorTunnelRecord>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    #[serde(default)]
    pub blocked: bool,
    #[serde(default)]
    pub last_error_kind: Option<CollectionFailureKind>,
    #[serde(default)]
    pub last_error: Option<String>,
    /// Completion time of this target's latest successful or partial result.
    #[serde(default)]
    pub last_success_at_ms: Option<i64>,
}

/// Safe target representation for commands and events.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ServerMonitorTargetView {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub source_kind: ServerMonitorSourceKind,
    pub saved_host_name: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub user: Option<String>,
    pub auth_kind: Option<ServerMonitorAuthKind>,
    pub key_path: Option<String>,
    pub has_password: bool,
    pub tunnel: Option<ServerMonitorTunnelView>,
    pub blocked: bool,
    pub last_error_kind: Option<CollectionFailureKind>,
    pub last_error: Option<String>,
    pub last_success_at_ms: Option<i64>,
    pub collecting: bool,
    pub next_collection_at_ms: Option<i64>,
}

impl ServerMonitorTargetRecord {
    pub(crate) fn view(
        &self,
        collecting: bool,
        next_collection_at_ms: Option<i64>,
    ) -> ServerMonitorTargetView {
        let custom = self.custom.as_ref();
        ServerMonitorTargetView {
            id: self.id.clone(),
            name: self.name.clone(),
            enabled: self.enabled,
            source_kind: self.source_kind,
            saved_host_name: self.saved_host_name.clone(),
            host: custom.map(|value| value.host.clone()),
            port: custom.map(|value| value.port),
            user: custom.map(|value| value.user.clone()),
            auth_kind: custom.map(|value| value.auth_kind),
            key_path: custom.and_then(|value| value.key_path.clone()),
            has_password: custom
                .and_then(|value| value.password.as_ref())
                .map(|password| !password.is_empty())
                .unwrap_or(false),
            tunnel: self.tunnel.as_ref().map(|tunnel| tunnel.view()),
            blocked: self.blocked,
            last_error_kind: self.last_error_kind,
            last_error: self.last_error.clone(),
            last_success_at_ms: self.last_success_at_ms,
            collecting,
            next_collection_at_ms,
        }
    }
}

/// Serializable, credential-free monitor state.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ServerMonitorState {
    pub settings: ServerMonitorSettings,
    pub targets: Vec<ServerMonitorTargetView>,
    pub next_collection_at_ms: Option<i64>,
    pub last_success_at_ms: Option<i64>,
    /// Safe text for the latest configuration, scheduling or storage failure.
    #[serde(default)]
    pub error: Option<String>,
}

/// Outcome stored with every normalized sample.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ServerMonitorOutcome {
    Success(MetricSnapshot),
    Partial(MetricSnapshot),
    Failed,
}

/// One persisted collection sample.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ServerMonitorSample {
    pub id: Option<i64>,
    pub target_id: String,
    pub collected_at_ms: i64,
    pub outcome: ServerMonitorOutcome,
    pub sampling_window_seconds: f64,
    pub source_mode: ServerMonitorScheduleMode,
    pub expected_interval_seconds: Option<u64>,
    pub cpu: Option<CpuMetric>,
    pub memory: Option<MemoryMetric>,
    pub disks: Option<Vec<DiskMetric>>,
    pub networks: Vec<NetworkMetric>,
    pub network_interface: Option<String>,
    pub error_kind: Option<CollectionFailureKind>,
    pub error: Option<String>,
}

/// History window requested by the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ServerMonitorHistoryRange {
    TwentyFourHours,
    SevenDays,
}

impl ServerMonitorHistoryRange {
    pub(crate) fn window_ms(self) -> i64 {
        match self {
            ServerMonitorHistoryRange::TwentyFourHours => 24 * 60 * 60 * 1000,
            ServerMonitorHistoryRange::SevenDays => 7 * 24 * 60 * 60 * 1000,
        }
    }
}

/// Draft submitted by the add/edit dialog.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ServerMonitorCustomDraft {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub auth_kind: ServerMonitorAuthKind,
    #[serde(default)]
    pub key_path: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
}

/// Draft submitted by the add/edit dialog for SSH tunnel / jump host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ServerMonitorTunnelDraft {
    pub enabled: bool,
    pub source_kind: ServerMonitorSourceKind,
    #[serde(default)]
    pub saved_host_name: Option<String>,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub auth_kind: Option<ServerMonitorAuthKind>,
    #[serde(default)]
    pub key_path: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
}

/// Draft submitted by the add/edit dialog.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ServerMonitorTargetDraft {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub enabled: bool,
    pub source_kind: ServerMonitorSourceKind,
    #[serde(default)]
    pub saved_host_name: Option<String>,
    #[serde(default)]
    pub custom: Option<ServerMonitorCustomDraft>,
    #[serde(default)]
    pub tunnel: Option<ServerMonitorTunnelDraft>,
}

/// Result of one temporary draft connection test.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ServerMonitorTestResult {
    pub succeeded: bool,
    pub partial: bool,
    pub error_kind: Option<CollectionFailureKind>,
    pub error: Option<String>,
    pub snapshot: Option<MetricSnapshot>,
}
