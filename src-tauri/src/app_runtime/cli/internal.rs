use crate::app_store;

pub(in crate::app_runtime) const INTERNAL_CLI_RESOLVE_SESSION_COMMAND: &str =
    "__onespace_cli_resolve_session";
pub(in crate::app_runtime) const INTERNAL_CLI_CREATE_SESSION_COMMAND: &str =
    "__onespace_cli_create_session";
pub(in crate::app_runtime) const INTERNAL_CLI_RESUME_SESSION_COMMAND: &str =
    "__onespace_cli_resume_session";
pub(in crate::app_runtime) const INTERNAL_CLI_CLAUDE_PROFILE_SET_DEFAULT: &str =
    "__onespace_cli_claude_profile_set_default";
pub(in crate::app_runtime) const INTERNAL_CLI_GET_CLAUDE_CONFIG_DIR: &str =
    "__onespace_cli_get_claude_config_dir";
pub(in crate::app_runtime) const INTERNAL_CLI_LIST_CLAUDE_PROFILES: &str =
    "__onespace_cli_list_claude_profiles";
pub(in crate::app_runtime) const INTERNAL_CLI_ENV_LIST: &str = "__onespace_cli_env_list";
pub(in crate::app_runtime) const INTERNAL_CLI_ENV_USE: &str = "__onespace_cli_env_use";

pub(in crate::app_runtime) fn handle_internal_cli_command() -> bool {
    let mut args = std::env::args();
    let _ = args.next();
    let Some(command) = args.next() else {
        return false;
    };

    match command.as_str() {
        INTERNAL_CLI_RESUME_SESSION_COMMAND => {
            let args = args.collect::<Vec<_>>();
            match tauri::async_runtime::block_on(app_store::session_service::resume_cli_session(
                &args,
            )) {
                Ok((_, status)) => {
                    let code = status.code().unwrap_or_else(|| {
                        #[cfg(unix)]
                        {
                            use std::os::unix::process::ExitStatusExt;
                            128 + status.signal().unwrap_or(1)
                        }
                        #[cfg(not(unix))]
                        {
                            1
                        }
                    });
                    std::process::exit(code);
                }
                Err(error) => {
                    eprintln!("{}: {}", error.code, error.message);
                    std::process::exit(1);
                }
            }
        }
        INTERNAL_CLI_CREATE_SESSION_COMMAND => {
            let args = args.collect::<Vec<_>>();
            match tauri::async_runtime::block_on(app_store::session_service::create_cli_session(
                &args,
            )) {
                Ok((_, status)) => {
                    let code = status.code().unwrap_or_else(|| {
                        #[cfg(unix)]
                        {
                            use std::os::unix::process::ExitStatusExt;
                            128 + status.signal().unwrap_or(1)
                        }
                        #[cfg(not(unix))]
                        {
                            1
                        }
                    });
                    std::process::exit(code);
                }
                Err(error) => {
                    eprintln!("{}: {}", error.code, error.message);
                    std::process::exit(1);
                }
            }
        }
        INTERNAL_CLI_RESOLVE_SESSION_COMMAND => {
            let query = args.next().unwrap_or_default();
            match app_store::cli_lookup_session(&query) {
                Ok(Some(record)) => {
                    println!(
                        "{}\u{1f}{}\u{1f}{}\u{1f}{}",
                        record.tool, record.tool_session_id, record.working_dir, record.id
                    );
                    std::process::exit(0);
                }
                Ok(None) => {
                    eprintln!("Session not found: {}", query);
                    std::process::exit(1);
                }
                Err(err) => {
                    eprintln!("Failed to resolve session: {}", err);
                    std::process::exit(1);
                }
            }
        }
        INTERNAL_CLI_CLAUDE_PROFILE_SET_DEFAULT => {
            let query = args.next().unwrap_or_default();
            let _operation = match app_store::lock_service_provider_operation() {
                Ok(operation) => operation,
                Err(error) => {
                    eprintln!("Failed to lock providers: {}", error);
                    std::process::exit(1);
                }
            };
            let mut state = match app_store::load_service_providers_state() {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Failed to load providers: {}", e);
                    std::process::exit(1);
                }
            };
            let profile_id = match state.providers.iter().find(|provider| {
                provider.tool == "claude"
                    && (provider.id == query
                        || provider.name == query
                        || provider.code.as_deref() == Some(query.as_str()))
            }) {
                Some(p) => p.id.clone(),
                None => {
                    eprintln!("Claude profile not found: {query}");
                    std::process::exit(1);
                }
            };
            state.active.insert("claude".to_string(), profile_id);
            if let Err(e) = app_store::save_service_providers_internal(&state) {
                eprintln!("Failed to save providers: {}", e);
                std::process::exit(1);
            }
            std::process::exit(0);
        }
        INTERNAL_CLI_GET_CLAUDE_CONFIG_DIR => {
            let query = args.next().unwrap_or_default();
            let state = match crate::app_store::load_service_providers_state() {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Failed to load providers: {}", e);
                    std::process::exit(1);
                }
            };
            let provider = state.providers.iter().find(|provider| {
                provider.tool == "claude"
                    && (provider.id == query
                        || provider.name == query
                        || provider.code.as_deref() == Some(query.as_str()))
            });
            match provider {
                Some(_) => {
                    let dir = match crate::app_store::resolve_claude_profile_config_dir(&query) {
                        Ok(dir) => dir,
                        Err(e) => {
                            eprintln!("{}", e);
                            std::process::exit(1);
                        }
                    };
                    println!("{}", dir.to_string_lossy());
                    std::process::exit(0);
                }
                None => {
                    eprintln!("Claude profile not found: {query}");
                    std::process::exit(1);
                }
            }
        }
        INTERNAL_CLI_LIST_CLAUDE_PROFILES => {
            let state = match crate::app_store::load_service_providers_state() {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Failed to load providers: {}", e);
                    std::process::exit(1);
                }
            };
            println!("Claude Profiles:");
            println!("----------------");
            for provider in state
                .providers
                .iter()
                .filter(|provider| provider.tool == "claude")
            {
                let default_mark = if state.active.get("claude") == Some(&provider.id) {
                    " [default]"
                } else {
                    ""
                };
                let profile_ref = provider.code.as_deref().unwrap_or(provider.id.as_str());
                let config_dir = crate::claude_profiles::get_claude_profiles_dir()
                    .map(|dir| dir.join(profile_ref))
                    .map(|dir| dir.to_string_lossy().to_string())
                    .unwrap_or_else(|_| String::new());
                println!("  {} ({}){}", provider.name, profile_ref, default_mark);
                if provider.code.is_some() {
                    println!("    Code: {}", profile_ref);
                }
                println!("    Config Dir: {}", config_dir);
            }
            std::process::exit(0);
        }
        INTERNAL_CLI_ENV_LIST => {
            match app_store::provider_activation::cli_environment_listing() {
                Ok(listing) => println!("{}", listing),
                Err(error) => {
                    eprintln!("{}", error);
                    std::process::exit(1);
                }
            }
            std::process::exit(0);
        }
        INTERNAL_CLI_ENV_USE => {
            let tool = args.next().unwrap_or_default();
            let target = args.next().unwrap_or_default();
            let provider_id = match app_store::provider_activation::use_cli_environment(&tool, &target) {
                Ok(provider_id) => provider_id,
                Err(error) => {
                    eprintln!("{}", error);
                    std::process::exit(1);
                }
            };
            println!(
                "Switched {} to environment: {} ({})",
                tool, target, provider_id
            );
            std::process::exit(0);
        }
        _ => return false,
    }
}
