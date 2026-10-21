mod ai_env;
mod ai_news;
mod ai_sessions;
mod ai_workflow_profiles;
mod ai_gateway;
mod app_store;
mod backup;
pub mod ble_lock;
mod claude_profiles;
mod cli_probe;
mod cli_updates;
mod config;
mod crypto;
mod file_sharing;
mod git;
pub mod http_client;
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
pub mod sftp;
mod short_link;
mod skills;
pub mod ssl_certificate;
mod ssh_tunnels;
mod storage;
mod subagents;
pub mod tunnel;
mod version_detect;
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
    ai_gateway_restore_provider_model,
    ai_gateway_sync_provider_template, ai_gateway_upsert_provider_template,
    ai_gateway_usage_retention_get, ai_gateway_usage_retention_save, ai_gateway_usage_stats,
};
pub use ai_workflow_profiles::{
    ai_workflow_activate_profile, ai_workflow_create_profile, ai_workflow_delete_profile,
    ai_workflow_get_model_sources, ai_workflow_get_profile_matrix, ai_workflow_list_profiles,
    ai_workflow_rename_profile, ai_workflow_save_profile,
};
pub use ssl_certificate::{
    ssl_certificate_create_order, ssl_certificate_delete, ssl_certificate_doh_precheck,
    ssl_certificate_export, ssl_certificate_list, ssl_certificate_verify_and_finalize,
    ssl_check_certificate, ssl_get_config, ssl_save_config, ssl_watchlist_add,
    ssl_watchlist_check_all, ssl_watchlist_list, ssl_watchlist_remove,
};

// Test-only discovery hook for the delegated `ble_lock` contract tests. The
// product `ble_lock` module is added by the backend implementation step; this
// include exists only so `cargo test ... ble_lock --lib` compiles and discovers
// the RED behavior tests before that module exists. It declares no product
// symbol and is absent from non-test builds.
#[cfg(test)]
#[path = "ble_lock/tests.rs"]
mod ble_lock_contract_tests;


