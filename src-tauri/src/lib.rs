mod ai_assistant;
mod ai_env;
mod ai_news;
mod ai_sessions;
mod api_gateway;
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
pub use api_gateway::{
    api_gateway_create_provider_from_template, api_gateway_delete_provider_model,
    api_gateway_delete_provider_template, api_gateway_fetch_models,
    api_gateway_provider_templates, api_gateway_request_logs,
    api_gateway_reset_provider_templates, api_gateway_restore_provider_model,
    api_gateway_sync_provider_template, api_gateway_upsert_provider_template,
    api_gateway_usage_retention_get, api_gateway_usage_retention_save, api_gateway_usage_stats,
};
