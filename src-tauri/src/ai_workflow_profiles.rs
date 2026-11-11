#[cfg(test)]
mod tests;

mod capabilities;
mod commands;
mod provisioning;
mod storage;
mod types;

#[cfg(test)]
pub use capabilities::get_model_sources;
pub use commands::*;
#[cfg(test)]
pub use provisioning::activate_profile;
#[cfg(test)]
pub use storage::{
    create_profile, delete_profile, get_active_models, get_profile_matrix, list_profiles,
    rename_profile, save_profile,
};
#[cfg(test)]
pub use types::{AgentMatrixRow, ModelEffort, SUPPORTED_ROLES};
