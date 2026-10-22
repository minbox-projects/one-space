use std::collections::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProbeType {
    Http,
    Tcp,
    Ping,
    Dns,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MonitorStatus {
    Up,
    Down,
    Degraded,
    Paused,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpProbeOptions {
    #[serde(default = "default_http_method")]
    pub method: String,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default = "default_expected_status")]
    pub expected_status: Vec<u16>,
    #[serde(default)]
    pub keyword: Option<String>,
    #[serde(default)]
    pub keyword_absent: Option<String>,
    #[serde(default)]
    pub ignore_ssl_errors: bool,
    #[serde(default)]
    pub warn_ssl_days_remaining: Option<u32>,
}

fn default_http_method() -> String {
    "GET".to_string()
}

fn default_expected_status() -> Vec<u16> {
    vec![200, 201, 204]
}

impl Default for HttpProbeOptions {
    fn default() -> Self {
        Self {
            method: default_http_method(),
            headers: HashMap::new(),
            body: None,
            expected_status: default_expected_status(),
            keyword: None,
            keyword_absent: None,
            ignore_ssl_errors: false,
            warn_ssl_days_remaining: Some(14),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TcpProbeOptions {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DnsProbeOptions {
    pub record_type: String, // A, AAAA, CNAME
    pub expected_value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertConfig {
    #[serde(default = "default_true")]
    pub system_notification: bool,
    #[serde(default)]
    pub sound_alert: bool,
    #[serde(default)]
    pub webhook_url: Option<String>,
}

fn default_true() -> bool {
    true
}

impl Default for AlertConfig {
    fn default() -> Self {
        Self {
            system_notification: true,
            sound_alert: false,
            webhook_url: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorTarget {
    pub id: String,
    pub name: String,
    pub probe_type: ProbeType,
    pub target: String,
    #[serde(default = "default_interval")]
    pub interval_sec: u32,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u32,
    #[serde(default = "default_retry_count")]
    pub retry_count: u8,
    #[serde(default = "default_recover_count")]
    pub recover_count: u8,
    #[serde(default = "default_true")]
    pub is_enabled: bool,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub http_options: Option<HttpProbeOptions>,
    #[serde(default)]
    pub tcp_options: Option<TcpProbeOptions>,
    #[serde(default)]
    pub dns_options: Option<DnsProbeOptions>,
    #[serde(default)]
    pub alert_config: AlertConfig,
    pub created_at: i64,
    pub updated_at: i64,
}

fn default_interval() -> u32 {
    30
}
fn default_timeout() -> u32 {
    5000
}
fn default_retry_count() -> u8 {
    2
}
fn default_recover_count() -> u8 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeartbeatRecord {
    pub id: Option<i64>,
    pub monitor_id: String,
    pub timestamp: i64,
    pub status: MonitorStatus,
    pub response_time_ms: u32,
    pub status_code: Option<u16>,
    pub error_message: Option<String>,
    pub ssl_days_remaining: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeartbeatBarPoint {
    pub timestamp: i64,
    pub status: MonitorStatus,
    pub response_time_ms: u32,
    pub status_code: Option<u16>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorIncident {
    pub id: i64,
    pub monitor_id: String,
    pub started_at: i64,
    pub resolved_at: Option<i64>,
    pub duration_sec: Option<i64>,
    pub cause: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorSummary {
    pub target: MonitorTarget,
    pub current_status: MonitorStatus,
    pub last_checked_at: Option<i64>,
    pub last_response_time_ms: Option<u32>,
    pub uptime_ratio_24h: f64,
    pub uptime_ratio_7d: f64,
    pub avg_response_time_24h_ms: u32,
    pub recent_heartbeats: Vec<HeartbeatBarPoint>,
    pub active_incident: Option<MonitorIncident>,
    pub ssl_days_remaining: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeTestResult {
    pub success: bool,
    pub status: MonitorStatus,
    pub response_time_ms: u32,
    pub status_code: Option<u16>,
    pub message: String,
    pub ssl_days_remaining: Option<i64>,
}
