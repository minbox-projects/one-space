use serde::{Deserialize, Serialize};

/// The four explicit model choices an operator submits for a Claude terminal
/// sync: the default model plus the haiku/sonnet/opus family aliases.
///
/// Each value is a local mapping id (the id Claude Code sends), never a vendor
/// id guessed from a name, and the same id may fill several slots. The struct is
/// shared by the configure/sync command payload and the model-only
/// terminal-target snapshot; it never carries a credential value.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClaudeModelSelection {
    #[serde(default)]
    pub default_model: String,
    #[serde(default)]
    pub haiku: String,
    #[serde(default)]
    pub sonnet: String,
    #[serde(default)]
    pub opus: String,
}

impl ClaudeModelSelection {
    /// Trim every choice and return `None` when any of the four is blank, so a
    /// partial or empty selection is rejected before any Claude write.
    pub(in crate::ai_gateway) fn normalized(&self) -> Option<Self> {
        let trim = |value: &str| {
            let value = value.trim();
            (!value.is_empty()).then(|| value.to_string())
        };
        Some(Self {
            default_model: trim(&self.default_model)?,
            haiku: trim(&self.haiku)?,
            sonnet: trim(&self.sonnet)?,
            opus: trim(&self.opus)?,
        })
    }
}

/// Ledger entry recording the last value written to a terminal provider record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TerminalSyncRecord {
    pub provider_id: String,
    pub tool: String,
    pub synced_key_id: String,
    pub synced_base_url: String,
    pub synced_at: u64,
}
