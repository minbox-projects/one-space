use serde::{Deserialize, Serialize};

/// Which OpenAI-compatible endpoint family an upstream provider exposes.
///
/// The relay accepts `/chat/completions` and `/responses` from clients. A
/// provider's `protocol` is the default its mapping rows inherit; each row may
/// pin its own, so candidates are selected by a row's effective protocol and a
/// provider is not filtered by its own protocol alone.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum UpstreamProtocol {
    #[default]
    ChatCompletions,
    Responses,
    AnthropicMessages,
}

impl UpstreamProtocol {
    /// Canonical upstream path suffix for this protocol.
    pub fn endpoint_path(self) -> &'static str {
        match self {
            Self::ChatCompletions => "/chat/completions",
            Self::Responses => "/responses",
            Self::AnthropicMessages => "/messages",
        }
    }
}

/// How the gateway reaches a provider's upstream endpoint.
///
/// `direct` forwards from the application process with the existing HTTP client.
/// `opencode_zen_sidecar` forwards through the bundled loopback sidecar so the
/// upstream TLS connection uses the sidecar's client profile. The value is
/// declared by the provider's bound template and is authoritative: URL, name,
/// icon and other stored fields never select a transport.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TransportStrategy {
    #[default]
    Direct,
    OpencodeZenSidecar,
}

/// Model-synchronization strategy declared by a provider template.
///
/// `generic` keeps today's model-list parsing and protocol-inheritance
/// fallback; `opencode_go` restricts a sync to model ids that the template
/// explicitly declares as Chat Completions or Responses (or that a future
/// source payload declares with a supported endpoint), never guessing a
/// protocol for undeclared or Messages-only ids.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SyncStrategy {
    #[default]
    Generic,
    OpencodeGo,
}

/// Quota/usage monitoring strategy declared by a provider template.
///
/// `none` performs no provider-specific monitor query, `commandcode` uses the
/// fixed CommandCode credits endpoint and `opencode_go` uses the fixed OpenCode
/// Go usage endpoint. The strategy is authoritative over any URL heuristic.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum QuotaStrategy {
    #[serde(rename = "none")]
    #[default]
    NoQuota,
    #[serde(rename = "commandcode")]
    Commandcode,
    #[serde(rename = "opencode_go")]
    OpencodeGo,
}

/// Error-handling strategy declared by a provider template. Only the existing
/// generic classification exists in this release.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ErrorStrategy {
    #[default]
    Generic,
}
