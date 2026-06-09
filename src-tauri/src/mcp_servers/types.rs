#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MCPServerTransport {
    Stdio,
    Http,
    Sse,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MCPServer {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_key: Option<String>,
    pub description: Option<String>,

    // 传输方式
    pub transport: MCPServerTransport,

    // Stdio 方式
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,

    // HTTP/SSE 方式
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_url: Option<String>,

    // 环境敏感信息（加密存储）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,

    // 高级配置
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trust: Option<bool>,

    // 关联的供应商 ID 列表
    pub linked_provider_ids: Vec<String>,

    // 元数据
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct MCPServersState {
    pub servers: Vec<MCPServer>,
    #[serde(default)]
    pub is_encrypted: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
struct MCPLocalInstallState {
    #[serde(default)]
    pub model_switches: HashMap<String, MCPModelSwitchState>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ApiMeta {
    pub revision: u64,
    pub ts: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ApiOk<T> {
    pub ok: bool,
    pub data: T,
    pub meta: ApiMeta,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MCPUpdateStatus {
    UpToDate,
    Updatable,
    FloatingLatest,
    Unsupported,
    CheckFailed,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MCPUpdateInfo {
    pub server_id: String,
    pub package_name: Option<String>,
    pub current_version: Option<String>,
    pub latest_version: Option<String>,
    pub status: MCPUpdateStatus,
    pub message: Option<String>,
    pub checked_at: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct MCPUpdatesState {
    pub status: String,
    pub last_error: Option<String>,
    pub last_checked_at: Option<u64>,
    #[serde(default)]
    pub items: Vec<MCPUpdateInfo>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MCPModel {
    Claude,
    Codex,
    Gemini,
    Opencode,
}

impl FromStr for MCPModel {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "claude" => Ok(Self::Claude),
            "codex" => Ok(Self::Codex),
            "gemini" => Ok(Self::Gemini),
            "opencode" => Ok(Self::Opencode),
            _ => Err(format!("Unsupported MCP model: {}", value)),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct MCPModelSwitchState {
    pub claude: bool,
    pub codex: bool,
    pub gemini: bool,
    pub opencode: bool,
}

#[derive(Debug, Default)]
struct ModelKeysets {
    claude: HashSet<String>,
    codex: HashSet<String>,
    gemini: HashSet<String>,
    opencode: HashSet<String>,
}

#[derive(Debug, Default, Clone)]
struct LocalModelConfigs {
    claude: HashMap<String, MCPServer>,
    codex: HashMap<String, MCPServer>,
    gemini: HashMap<String, MCPServer>,
    opencode: HashMap<String, MCPServer>,
}

impl LocalModelConfigs {
    fn keysets(&self) -> ModelKeysets {
        ModelKeysets {
            claude: self.claude.keys().cloned().collect(),
            codex: self.codex.keys().cloned().collect(),
            gemini: self.gemini.keys().cloned().collect(),
            opencode: self.opencode.keys().cloned().collect(),
        }
    }
}
