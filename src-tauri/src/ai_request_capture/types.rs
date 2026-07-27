use serde::{Deserialize, Serialize};

pub const CAPTURE_BODY_LIMIT_BYTES: usize = 2 * 1024 * 1024;
pub const CAPTURE_RETENTION_SECONDS: i64 = 7 * 24 * 60 * 60;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiRequestCaptureConfig {
    pub enabled: bool,
    pub port: u16,
    pub upstream_base_url: String,
}

impl Default for AiRequestCaptureConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            port: 17688,
            upstream_base_url: String::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiRequestCaptureValidationError {
    pub field: String,
    pub message: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiRequestCaptureStatus {
    pub running: bool,
    pub listen_address: String,
    pub port: u16,
    pub last_error: Option<String>,
}

impl AiRequestCaptureStatus {
    pub fn stopped(port: u16) -> Self {
        Self {
            running: false,
            listen_address: "127.0.0.1".to_string(),
            port,
            last_error: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiRequestCaptureConfigApplyResult {
    pub config: AiRequestCaptureConfig,
    pub status: AiRequestCaptureStatus,
    #[serde(default)]
    pub validation_errors: Vec<AiRequestCaptureValidationError>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureState {
    InProgress,
    Completed,
    Rejected,
    UpstreamError,
    RequestTransferError,
    ResponseTransferError,
    ClientDisconnected,
    Interrupted,
}

impl CaptureState {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
            Self::Rejected => "rejected",
            Self::UpstreamError => "upstream_error",
            Self::RequestTransferError => "request_transfer_error",
            Self::ResponseTransferError => "response_transfer_error",
            Self::ClientDisconnected => "client_disconnected",
            Self::Interrupted => "interrupted",
        }
    }
    pub(crate) fn parse(value: &str) -> Result<Self, String> {
        match value {
            "in_progress" => Ok(Self::InProgress),
            "completed" => Ok(Self::Completed),
            "rejected" => Ok(Self::Rejected),
            "upstream_error" => Ok(Self::UpstreamError),
            "request_transfer_error" => Ok(Self::RequestTransferError),
            "response_transfer_error" => Ok(Self::ResponseTransferError),
            "client_disconnected" => Ok(Self::ClientDisconnected),
            "interrupted" => Ok(Self::Interrupted),
            _ => Err(format!("unknown capture state: {value}")),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiRequestCaptureHeader {
    pub name: String,
    pub values: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapturedBody {
    #[serde(skip)]
    pub data: Vec<u8>,
    #[serde(rename = "data")]
    pub display: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoding: Option<String>,
    pub captured_bytes: u64,
    pub total_bytes: u64,
    pub truncated: bool,
}

impl CapturedBody {
    pub fn from_bytes(mut data: Vec<u8>, total_bytes: u64) -> Self {
        let truncated = data.len() > CAPTURE_BODY_LIMIT_BYTES || total_bytes > data.len() as u64;
        data.truncate(CAPTURE_BODY_LIMIT_BYTES);
        Self {
            captured_bytes: data.len() as u64,
            total_bytes,
            truncated,
            data,
            display: String::new(),
            encoding: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct CaptureStart {
    pub id: String,
    pub started_at: i64,
    pub http_version: String,
    pub method: String,
    pub request_path_and_query: String,
    pub upstream_url: String,
    pub request_headers: Vec<AiRequestCaptureHeader>,
    pub request_body: CapturedBody,
    pub provider: Option<String>,
    pub model: Option<String>,
}

#[derive(Clone, Debug)]
pub struct CaptureFinish {
    pub completed_at: i64,
    pub state: CaptureState,
    pub response_status: Option<u16>,
    pub response_headers: Vec<AiRequestCaptureHeader>,
    pub response_body: CapturedBody,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureListQuery {
    pub search: Option<String>,
    pub method: Option<String>,
    #[serde(default)]
    pub states: Vec<CaptureState>,
    pub provider: Option<String>,
    pub model: Option<String>,
    #[serde(default)]
    pub page: u32,
    #[serde(default)]
    pub page_size: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiRequestCaptureListItem {
    pub id: String,
    pub started_at: i64,
    pub completed_at: Option<i64>,
    pub state: CaptureState,
    pub method: String,
    pub request_path_and_query: String,
    pub upstream_url: String,
    pub response_status: Option<u16>,
    pub duration_ms: Option<i64>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    #[serde(skip_serializing)]
    pub request_body: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiRequestCaptureListResult {
    pub items: Vec<AiRequestCaptureListItem>,
    pub total: u64,
    pub page: u32,
    pub page_size: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiRequestCaptureDetail {
    pub id: String,
    pub started_at: i64,
    pub completed_at: Option<i64>,
    pub state: CaptureState,
    pub http_version: String,
    pub method: String,
    pub request_path_and_query: String,
    pub upstream_url: String,
    pub request_headers: Vec<AiRequestCaptureHeader>,
    pub request_body: CapturedBody,
    pub response_status: Option<u16>,
    pub response_headers: Vec<AiRequestCaptureHeader>,
    pub response_body: CapturedBody,
    pub duration_ms: Option<i64>,
    pub error: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiRequestCaptureClearResult {
    pub cleared: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiRequestCaptureExportInput {
    pub query: CaptureListQuery,
    pub output_path: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiRequestCaptureExportResult {
    pub output_path: String,
    pub exported: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiRequestCaptureCurlResult {
    pub command: String,
    pub complete: bool,
    pub warning: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureRecoveryResult {
    pub interrupted: u64,
    pub deleted: u64,
}
