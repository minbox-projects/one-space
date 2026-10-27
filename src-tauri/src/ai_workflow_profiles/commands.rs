use std::path::Path;

use super::capabilities::get_model_sources;
use super::provisioning::activate_profile;
use super::storage::{
    create_profile, delete_profile, get_active_models, get_profile_matrix, list_profiles,
    rename_profile, save_profile,
};
use super::types::{
    AgentMatrixRow, ModelSourcesResult, ProfileActivationReport, ProfileMatrix, ProfileSummary,
};

// Tauri commands

#[tauri::command]
pub fn ai_workflow_list_profiles(
    home_override: Option<String>,
) -> Result<Vec<ProfileSummary>, String> {
    list_profiles(home_override.as_deref().map(Path::new))
}

#[tauri::command]
pub fn ai_workflow_get_profile_matrix(
    name: String,
    home_override: Option<String>,
) -> Result<ProfileMatrix, String> {
    get_profile_matrix(&name, home_override.as_deref().map(Path::new))
}

#[tauri::command]
pub fn ai_workflow_get_active_models() -> Result<Option<ProfileMatrix>, String> {
    get_active_models(None)
}

#[tauri::command]
pub fn ai_workflow_get_model_sources(
    home_override: Option<String>,
) -> Result<ModelSourcesResult, String> {
    get_model_sources(home_override.as_deref().map(Path::new))
}

#[tauri::command]
pub fn ai_workflow_save_profile(
    name: String,
    matrix: Vec<AgentMatrixRow>,
    home_override: Option<String>,
) -> Result<(), String> {
    save_profile(&name, &matrix, home_override.as_deref().map(Path::new))
}

#[tauri::command]
pub fn ai_workflow_activate_profile(
    name: String,
    home_override: Option<String>,
) -> Result<ProfileActivationReport, String> {
    activate_profile(&name, home_override.as_deref().map(Path::new))
}

#[tauri::command]
pub fn ai_workflow_create_profile(
    name: String,
    copy_from: Option<String>,
    home_override: Option<String>,
) -> Result<(), String> {
    create_profile(&name, copy_from.as_deref(), home_override.as_deref().map(Path::new))
}

#[tauri::command]
pub fn ai_workflow_delete_profile(
    name: String,
    home_override: Option<String>,
) -> Result<(), String> {
    delete_profile(&name, home_override.as_deref().map(Path::new))
}

#[tauri::command]
pub fn ai_workflow_rename_profile(
    old_name: String,
    new_name: String,
    home_override: Option<String>,
) -> Result<(), String> {
    rename_profile(&old_name, &new_name, home_override.as_deref().map(Path::new))
}
