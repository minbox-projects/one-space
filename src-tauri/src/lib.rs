mod ai_assistant;
mod ai_env;
mod ai_news;
mod ai_sessions;
mod ai_workflow_profiles;
mod ai_gateway;
mod app_store;
mod assistant_mcp;
mod backup;
mod claude_profiles;
mod cli_probe;
mod cli_updates;
mod config;
mod config_conflict;
mod crypto;
mod file_sharing;
mod git;
mod managed_assets;
mod mcp_export;
mod mcp_runtime;
mod mcp_servers;
mod mcp_templates;
mod messages;
mod protocol_router;
mod proxy;
mod runtime_profiles;
mod secrets;
mod short_link;
mod skills;
mod ssh_tunnels;
mod storage;
mod subagents;
mod version_detect;
mod workflows;
mod workspaces;

mod app_runtime;

#[cfg(test)]
pub(crate) use app_runtime::lock_test_home_env;
pub use app_runtime::run;
pub(crate) use app_runtime::{
    atomic_write_string, get_data_dir, get_git_command, get_hostname, get_ssh_hosts,
    open_path_with_system,
};
// Usage statistics, request logs and retention commands. Each one is also
// registered in `app_runtime::run_app`'s `generate_handler!`.
pub use ai_gateway::{
    ai_gateway_create_provider_from_template, ai_gateway_delete_provider_model,
    ai_gateway_delete_provider_template, ai_gateway_provider_templates,
    ai_gateway_request_logs,
    ai_gateway_reset_provider_templates, ai_gateway_restore_provider_model,
    ai_gateway_sync_provider_template, ai_gateway_upsert_provider_template,
    ai_gateway_usage_retention_get, ai_gateway_usage_retention_save, ai_gateway_usage_stats,
};
pub use ai_workflow_profiles::{
    ai_workflow_activate_profile, ai_workflow_create_profile, ai_workflow_delete_profile,
    ai_workflow_get_model_sources, ai_workflow_get_profile_matrix, ai_workflow_list_profiles,
    ai_workflow_save_and_activate_profile, ai_workflow_save_profile,
};

