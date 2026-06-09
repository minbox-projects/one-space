use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HistoryEntry {
    pub timestamp: u64,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct AiProvider {
    pub id: String,
    pub name: String,
    pub tool: String, // "claude", "codex", "gemini", "opencode"
    pub api_key: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,

    // 通用模型字段
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    // Claude 专属模型路由映射
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claude_haiku_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claude_sonnet_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claude_opus_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claude_default_model: Option<String>, // ANTHROPIC_MODEL - 通用默认模型
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claude_reasoning_effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claude_model_mappings: Option<Vec<crate::app_store::ClaudeModelMapping>>,

    // Claude 高级选项
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dangerously_skip_permissions: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_all_memory_features: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_mcp: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_tools: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_session_turns: Option<u32>,

    // Codex 高级选项
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable_response_storage: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub personality: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wire_api: Option<String>,

    // Codex 新增配置参数
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_reasoning_effort: Option<String>, // "minimal" | "low" | "medium" | "high" | "xhigh"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_reasoning_summary: Option<String>, // "auto" | "concise" | "detailed" | "none"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_policy: Option<String>, // "untrusted" | "on-failure" | "on-request" | "never"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sandbox_mode: Option<String>, // "read-only" | "workspace-write"

    // Gemini 高级选项
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gemini_auth_type: Option<String>, // "gemini-api-key" or "oauth-personal"

    // Gemini 新增配置参数
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>, // "Default" | "GitHub Dark" | "Light"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vim_mode: Option<bool>, // Vim 键盘绑定
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_approval_mode: Option<String>, // "default" | "auto_edit" | "plan"

    // OpenCode 全局配置
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opencode_default_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opencode_default_agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opencode_sessions_dir: Option<String>,

    // OpenCode 新增配置参数
    #[serde(skip_serializing_if = "Option::is_none")]
    pub small_model: Option<String>, // 轻量任务模型
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u32>, // 请求超时 (毫秒)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub share_mode: Option<String>, // "manual" | "auto" | "disabled"

    // 是否同步到 CLI 配置文件 (针对 OpenCode)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_enabled: Option<bool>,

    // 供应商标识，作为 opencode.json 中的 key
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_key: Option<String>,

    // 历史记录
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history: Option<Vec<HistoryEntry>>,

    // 存储 OpenCode 特有的所有其他字段，确保 JSON 编辑时不丢失数据
    #[serde(flatten)]
    pub extra_fields: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct AiProvidersState {
    pub active_claude: Option<String>,
    pub active_codex: Option<String>,
    pub active_gemini: Option<String>,
    pub active_opencode: Option<String>,
    pub providers: Vec<AiProvider>,
    #[serde(default)]
    pub is_encrypted: bool,
}
