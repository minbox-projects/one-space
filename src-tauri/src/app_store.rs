use crate::{
    ai_env, ai_news, ai_sessions, config, git, mcp_servers, messages, secrets, storage, workspaces,
};
#[cfg(target_os = "macos")]
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::Emitter;
use uuid::Uuid;

include!("app_store/types.rs");
include!("app_store/storage_engine.rs");
include!("app_store/provider_ids.rs");
include!("app_store/sessions_state.rs");
include!("app_store/legacy_providers.rs");
include!("app_store/providers_storage.rs");
include!("app_store/command_types.rs");
include!("app_store/launcher_core.rs");
include!("app_store/provider_projection.rs");
include!("app_store/sync.rs");
include!("app_store/migration.rs");
include!("app_store/storage_commands.rs");
include!("app_store/service_provider_commands.rs");
include!("app_store/launcher_commands.rs");
include!("app_store/session_commands.rs");
include!("app_store/projection_sync_commands.rs");
include!("app_store/tests.rs");
