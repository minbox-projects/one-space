use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SUPPORTED_ROLES: [&str; 9] = [
    "backend",
    "documentation-maintainer",
    "file-explorer",
    "frontend",
    "git-operator",
    "researcher",
    "spec-review",
    "standards-review",
    "test",
];

pub const SUPPORTED_TOOLS: [&str; 3] = ["codex", "claude", "opencode"];

pub const VALID_EFFORTS: [&str; 6] = ["low", "medium", "high", "xhigh", "max", "ultra"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileSummary {
    pub name: String,
    pub active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ModelEffort {
    pub model: String,
    pub reasoning_effort: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct AgentMatrixRow {
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex: Option<ModelEffort>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude: Option<ModelEffort>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opencode: Option<ModelEffort>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ProfileMatrix {
    pub name: String,
    pub rows: Vec<AgentMatrixRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ModelCapability {
    #[serde(default)]
    pub efforts: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_effort: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ColumnModelSource {
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Exact host-local model ID -> declared raw capabilities. An absent key is unknown.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub model_capabilities: BTreeMap<String, ModelCapability>,
    /// Optional capability-read diagnostic, isolated from the column `error`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capability_warning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ModelSourcesResult {
    pub opencode: ColumnModelSource,
    pub codex: ColumnModelSource,
    pub claude: ColumnModelSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct AgentInstallationInfo {
    pub name: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct HostInstallation {
    pub host: String,
    pub agents_directory: String,
    #[serde(default)]
    pub agents: Vec<AgentInstallationInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ProfileActivationReport {
    pub active_profile: String,
    #[serde(default)]
    pub hosts: Vec<String>,
    #[serde(default)]
    pub installations: Vec<HostInstallation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}
