//! Shared value types for server resource collection.
//!
//! Optional resource fields carry a real absence: an unavailable measurement is
//! `None` and is never silently represented as zero.

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
    pub target_id: String,
    pub source: MonitorSource,
    pub password: Option<String>,
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
    Script,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone)]
pub(crate) struct CollectionFailure {
    pub kind: CollectionFailureKind,
    pub message: String,
}

/// CPU utilization over the collector's own approximately one-second window.
#[derive(Debug, Clone)]
pub(crate) struct CpuMetric {
    pub usage_percent: f64,
}

/// Memory snapshot derived from `/proc/meminfo`.
#[derive(Debug, Clone)]
pub(crate) struct MemoryMetric {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub used_percent: f64,
}

/// One local persistent mount reported by `df`.
#[derive(Debug, Clone)]
pub(crate) struct DiskMetric {
    pub filesystem: String,
    pub mount_point: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub used_percent: f64,
}

/// Receive/transmit rate for one interface over the collector's sampling window.
#[derive(Debug, Clone)]
pub(crate) struct NetworkMetric {
    pub interface: String,
    pub rx_bytes_per_sec: f64,
    pub tx_bytes_per_sec: f64,
    pub rx_delta_bytes: u64,
    pub tx_delta_bytes: u64,
    pub elapsed_seconds: f64,
}

/// Available resources from one self-contained collection.
#[derive(Debug, Clone)]
pub(crate) struct MetricSnapshot {
    pub collected_at_ms: i64,
    pub cpu: Option<CpuMetric>,
    pub memory: Option<MemoryMetric>,
    pub disks: Option<Vec<DiskMetric>>,
    pub network: Option<NetworkMetric>,
}

/// Result of a completed collector execution.
#[derive(Debug, Clone)]
pub(crate) enum CollectionOutcome {
    Success(MetricSnapshot),
    Partial(MetricSnapshot),
    Failed(CollectionFailure),
}
