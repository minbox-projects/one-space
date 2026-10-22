use std::collections::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomResponseConfig {
    #[serde(default = "default_status_code")]
    pub status_code: u16,
    #[serde(default = "default_content_type")]
    pub content_type: String,
    #[serde(default = "default_response_body")]
    pub body: String,
    #[serde(default)]
    pub delay_ms: u32,
}

fn default_status_code() -> u16 {
    200
}
fn default_content_type() -> String {
    "application/json".to_string()
}
fn default_response_body() -> String {
    "{\"success\":true}".to_string()
}

impl Default for CustomResponseConfig {
    fn default() -> Self {
        Self {
            status_code: default_status_code(),
            content_type: default_content_type(),
            body: default_response_body(),
            delay_ms: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ForwardConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub target_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebhookEndpoint {
    pub id: String,
    pub name: String,
    pub path: String, // 相对路径，如 "/hook/github"
    #[serde(default = "default_true")]
    pub is_active: bool,
    #[serde(default)]
    pub custom_response: CustomResponseConfig,
    #[serde(default)]
    pub forward_config: Option<ForwardConfig>,
    pub created_at: i64,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedSignature {
    pub provider: String, // "GitHub", "Stripe", "Feishu", etc.
    pub header_name: String,
    pub header_value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapturedWebhookRequest {
    pub id: String,
    pub endpoint_id: String,
    pub endpoint_name: String,
    pub timestamp: i64,
    pub method: String,
    pub path: String,
    pub query_params: HashMap<String, String>,
    pub headers: HashMap<String, String>,
    pub client_ip: String,
    pub body: String,
    pub body_format: String, // "json", "form", "xml", "raw"
    pub is_truncated: bool,
    pub detected_signature: Option<DetectedSignature>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplayRequest {
    pub request_id: String,
    pub target_url: String,
    #[serde(default)]
    pub override_headers: Option<HashMap<String, String>>,
    #[serde(default)]
    pub override_body: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplayResult {
    pub status_code: u16,
    pub duration_ms: u64,
    pub headers: HashMap<String, String>,
    pub response_body: String,
    pub executed_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebhookServerStatus {
    pub is_running: bool,
    pub port: u16,
    pub active_endpoints_count: usize,
    pub captured_requests_count: usize,
}
