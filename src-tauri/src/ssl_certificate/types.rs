use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CertificateNode {
    pub subject: String,
    pub subject_cn: Option<String>,
    pub issuer: String,
    pub issuer_cn: Option<String>,
    pub issuer_org: Option<String>,
    pub not_before: i64,
    pub not_after: i64,
    pub days_remaining: i64,
    pub sans: Vec<String>,
    pub serial_number: String,
    pub signature_algorithm: String,
    pub public_key_algorithm: String,
    pub sha256_fingerprint: String,
    pub is_root: bool,
    pub is_ca: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SslCheckResult {
    pub domain: String,
    pub port: u16,
    pub ip_address: Option<String>,
    pub handshake_success: bool,
    pub tls_version: Option<String>,
    pub cipher_suite: Option<String>,
    pub is_valid: bool,
    pub validation_error: Option<String>,
    pub days_remaining: i64,
    pub is_expired: bool,
    pub is_expiring_soon: bool,
    pub domain_matches: bool,
    pub certificates: Vec<CertificateNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SslStatus {
    Valid,
    ExpiringSoon,
    Expired,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SslWatchItem {
    pub id: String,
    pub domain: String,
    pub port: u16,
    pub alias: Option<String>,
    pub enabled: bool,
    pub alert_days_threshold: u32,
    pub last_check_at: Option<i64>,
    pub last_days_remaining: Option<i64>,
    pub last_status: Option<SslStatus>,
    pub last_error: Option<String>,
    pub issuer_cn: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_notified_date: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SslWatchItemInput {
    pub domain: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub alias: Option<String>,
    #[serde(default = "default_threshold")]
    pub alert_days_threshold: u32,
}

fn default_port() -> u16 {
    443
}

fn default_threshold() -> u32 {
    30
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SslChallengeItem {
    pub domain: String,
    pub record_name: String,
    pub record_type: String,
    pub record_value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SslOrderRequest {
    pub domain: String,
    #[serde(default)]
    pub san_domains: Vec<String>,
    pub email: Option<String>,
    #[serde(default)]
    pub use_staging: bool,
    #[serde(default = "default_key_type")]
    pub key_type: String,
    pub cloudflare_api_token: Option<String>,
}

fn default_key_type() -> String {
    "ecc_p256".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SslOrderResponse {
    pub order_id: String,
    pub challenges: Vec<SslChallengeItem>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SslIssuedCertificate {
    pub id: String,
    pub primary_domain: String,
    pub san_domains: Vec<String>,
    pub issuer: String,
    pub issued_at: i64,
    pub expires_at: i64,
    pub days_remaining: i64,
    pub key_type: String,
    pub cert_pem: String,
    pub chain_pem: String,
    pub fullchain_pem: String,
    pub privkey_pem: Option<String>,
    pub nginx_config: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SslDohPrecheckResult {
    pub resolved: bool,
    pub found_values: Vec<String>,
}
