use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileSharingNetwork {
    pub id: String,
    pub interface_name: String,
    pub address: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSharingStartInput {
    pub network_id: String,
    pub paths: Vec<String>,
    #[serde(default)]
    pub public_access: bool,
    #[serde(default)]
    pub password: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileSharingFile {
    pub id: String,
    pub name: String,
    pub source_path: String,
    pub size: u64,
    pub modified_at: i64,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FileSharingTransferState {
    InProgress,
    Completed,
    ClientDisconnected,
    Cancelled,
    Failed,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileSharingTransfer {
    pub id: String,
    pub file_id: String,
    pub file_name: String,
    pub client_address: String,
    pub state: FileSharingTransferState,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub bytes_sent: u64,
    pub response_bytes: u64,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileSharingSummary {
    pub active_transfers: u64,
    pub completed_transfers: u64,
    pub failed_transfers: u64,
    pub cancelled_transfers: u64,
    pub bytes_sent: u64,
    pub dropped_transfer_records: u64,
}

#[derive(Clone, Copy, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FileSharingPublicState {
    #[default]
    Disabled,
    Starting,
    Running,
    Error,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileSharingSnapshot {
    pub running: bool,
    pub session_id: Option<String>,
    pub address: Option<String>,
    pub port: Option<u16>,
    pub share_url: Option<String>,
    pub started_at: Option<i64>,
    pub stopped_at: Option<i64>,
    pub files: Vec<FileSharingFile>,
    pub transfers: Vec<FileSharingTransfer>,
    pub summary: FileSharingSummary,
    pub last_error: Option<String>,
    pub password_protected: bool,
    pub public_tunnel_id: Option<String>,
    pub public_state: FileSharingPublicState,
    pub public_share_url: Option<String>,
    pub public_error: Option<String>,
}

impl Default for FileSharingSnapshot {
    fn default() -> Self {
        Self {
            running: false,
            session_id: None,
            address: None,
            port: None,
            share_url: None,
            started_at: None,
            stopped_at: None,
            files: Vec::new(),
            transfers: Vec::new(),
            summary: FileSharingSummary::default(),
            last_error: None,
            password_protected: false,
            public_tunnel_id: None,
            public_state: FileSharingPublicState::Disabled,
            public_share_url: None,
            public_error: None,
        }
    }
}
