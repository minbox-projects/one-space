#[cfg(test)]
mod tests;

mod capabilities;
mod commands;
mod provisioning;
mod storage;
mod types;

pub use capabilities::get_model_sources;
pub use commands::*;
pub use provisioning::activate_profile;
pub use storage::{
    create_profile, delete_profile, get_active_models, get_profile_matrix, list_profiles,
    rename_profile, save_profile, validate_profile_name,
};
pub use types::{
    AgentInstallationInfo, AgentMatrixRow, ColumnModelSource, HostInstallation, ModelCapability,
    ModelEffort, ModelSourcesResult, ProfileActivationReport, ProfileMatrix, ProfileSummary,
    SUPPORTED_ROLES, SUPPORTED_TOOLS, VALID_EFFORTS,
};
