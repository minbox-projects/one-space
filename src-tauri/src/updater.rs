//! Application-managed native updater.
//!
//! The official updater SDK owns check, signature verification and
//! installation. This module keeps the typed `tauri_plugin_updater::Update`
//! handles and the bytes returned after official verification, exposes them
//! through native commands, and resolves the current proxy snapshot each time
//! the SDK builds an HTTP client.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

/// Event carrying native updater download progress to the frontend facade.
pub const APP_UPDATER_DOWNLOAD_EVENT: &str = "app-updater-download-event";

/// Typed metadata for an available release.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateMetadata {
    /// Stable identifier for the checked update, used by later commands.
    pub id: String,
    pub version: String,
    pub current_version: String,
    pub date: Option<String>,
    pub body: Option<String>,
}

/// Download progress payload, mirroring the official plugin's event shape.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "event", content = "data")]
pub enum AppUpdaterDownloadEvent {
    #[serde(rename_all = "camelCase")]
    Started {
        content_length: Option<u64>,
    },
    #[serde(rename_all = "camelCase")]
    Progress {
        chunk_length: usize,
    },
    Finished,
}

struct AppUpdaterEntry {
    update: Update,
    verified_download: Option<Vec<u8>>,
}

type EntryMap = HashMap<String, AppUpdaterEntry>;

fn global_entries() -> &'static Arc<Mutex<EntryMap>> {
    static ENTRIES: OnceLock<Arc<Mutex<EntryMap>>> = OnceLock::new();
    ENTRIES.get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
}

fn lock_entries(entries: &Arc<Mutex<EntryMap>>) -> MutexGuard<'_, EntryMap> {
    entries.lock().unwrap_or_else(|error| error.into_inner())
}

/// Application-managed updater resources.
///
/// Entries are keyed by the stable id returned from `app_updater_check`. A
/// successful check replaces earlier entries, `app_updater_discard` disposes
/// one entry explicitly, and `app_updater_shutdown` clears all entries.
#[derive(Clone)]
pub struct AppUpdaterState {
    entries: Arc<Mutex<EntryMap>>,
}

impl Default for AppUpdaterState {
    fn default() -> Self {
        Self {
            entries: Arc::clone(global_entries()),
        }
    }
}

impl AppUpdaterState {
    pub fn new() -> Self {
        Self::default()
    }

    fn replace_all_with(&self, id: String, update: Update) {
        let mut entries = lock_entries(&self.entries);
        entries.clear();
        entries.insert(
            id,
            AppUpdaterEntry {
                update,
                verified_download: None,
            },
        );
    }

    fn clear_all(&self) {
        lock_entries(&self.entries).clear();
    }

    fn update_for(&self, id: &str) -> Option<Update> {
        lock_entries(&self.entries)
            .get(id)
            .map(|entry| entry.update.clone())
    }

    fn store_verified_download(&self, id: &str, bytes: Vec<u8>) {
        let mut entries = lock_entries(&self.entries);
        if let Some(entry) = entries.get_mut(id) {
            entry.verified_download = Some(bytes);
        }
    }

    fn verified_download_for(&self, id: &str) -> Option<(Update, Vec<u8>)> {
        let entries = lock_entries(&self.entries);
        let entry = entries.get(id)?;
        let bytes = entry.verified_download.clone()?;
        Some((entry.update.clone(), bytes))
    }

    fn discard(&self, id: &str) {
        lock_entries(&self.entries).remove(id);
    }
}

/// Releases all native updater resources. Safe to call repeatedly during
/// application shutdown.
pub fn app_updater_shutdown() {
    lock_entries(global_entries()).clear();
}

fn stable_update_id(update: &Update) -> String {
    format!(
        "{}:{}:{}",
        update.current_version, update.version, update.target
    )
}

fn emit_download_event(app: &AppHandle, payload: AppUpdaterDownloadEvent) {
    let _ = app.emit(APP_UPDATER_DOWNLOAD_EVENT, payload);
}

/// Applies the current global proxy policy to a fresh SDK client.
///
/// The closure passed to `configure_client` calls this on every client the SDK
/// builds, so it never captures a proxy snapshot. When no valid transport can
/// be prepared, the client stays explicitly direct instead of inheriting an
/// automatic proxy.
fn configure_current_transport(builder: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
    let Some(manager) = crate::proxy::PROXY_MANAGER.get() else {
        return builder.no_proxy();
    };
    let snapshot = manager.current_snapshot();
    if manager.prepare(snapshot.config.clone()).is_err() {
        return builder.no_proxy();
    }
    crate::proxy::apply_proxy_policy(builder, &snapshot)
        .unwrap_or_else(|_| reqwest::Client::builder().no_proxy())
}

/// Checks the configured endpoint through the current managed proxy transport.
#[tauri::command]
pub async fn app_updater_check(app: AppHandle) -> Result<Option<AppUpdateMetadata>, String> {
    let prepared = crate::proxy::apply_current_proxy_to_builder(reqwest::Client::builder())
        .map_err(|_| "Failed to prepare updater transport".to_string())?;
    prepared
        .build()
        .map_err(|_| "Failed to prepare updater transport".to_string())?;

    let updater = app
        .updater_builder()
        .configure_client(configure_current_transport)
        .build()
        .map_err(|error| error.to_string())?;

    let update = updater.check().await.map_err(|error| error.to_string())?;
    let state = app.state::<AppUpdaterState>();

    match update {
        Some(update) => {
            let id = stable_update_id(&update);
            let metadata = AppUpdateMetadata {
                id: id.clone(),
                version: update.version.clone(),
                current_version: update.current_version.clone(),
                date: update.date.map(|date| {
                    let offset = date.offset().whole_seconds();
                    let sign = if offset < 0 { '-' } else { '+' };
                    let absolute = offset.unsigned_abs();
                    format!(
                        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}{}{:02}:{:02}",
                        date.year(),
                        u8::from(date.month()),
                        date.day(),
                        date.hour(),
                        date.minute(),
                        date.second(),
                        sign,
                        absolute / 3600,
                        (absolute % 3600) / 60,
                    )
                }),
                body: update.body.clone(),
            };
            state.replace_all_with(id, update);
            Ok(Some(metadata))
        }
        None => {
            state.clear_all();
            Ok(None)
        }
    }
}

/// Downloads the selected release through the current managed proxy transport.
///
/// The returned bytes are stored only after the official updater verifies the
/// signature.
#[tauri::command]
pub async fn app_updater_download(app: AppHandle, update_id: String) -> Result<(), String> {
    let update = app
        .state::<AppUpdaterState>()
        .update_for(&update_id)
        .ok_or_else(|| "Update is no longer available".to_string())?;

    let mut first_chunk = true;
    let bytes = update
        .download(
            |chunk_length, content_length| {
                if first_chunk {
                    first_chunk = false;
                    emit_download_event(&app, AppUpdaterDownloadEvent::Started { content_length });
                }
                emit_download_event(&app, AppUpdaterDownloadEvent::Progress { chunk_length });
            },
            || {
                emit_download_event(&app, AppUpdaterDownloadEvent::Finished);
            },
        )
        .await
        .map_err(|error| error.to_string())?;

    app.state::<AppUpdaterState>()
        .store_verified_download(&update_id, bytes);
    Ok(())
}

/// Installs a previously downloaded and verified release.
#[tauri::command]
pub async fn app_updater_install(app: AppHandle, update_id: String) -> Result<(), String> {
    let (update, bytes) = app
        .state::<AppUpdaterState>()
        .verified_download_for(&update_id)
        .ok_or_else(|| "Update has not been downloaded".to_string())?;

    update.install(bytes).map_err(|error| error.to_string())
}

/// Disposes one updater entry without installing it.
#[tauri::command]
pub fn app_updater_discard(app: AppHandle, update_id: String) {
    app.state::<AppUpdaterState>().discard(&update_id);
}

/// Builds the updater against controlled endpoints and public key for tests.
#[cfg(test)]
pub(crate) const TEST_UPDATER_TARGET: &str = "test-target";

#[cfg(test)]
pub(crate) fn test_check_builder<R: tauri::Runtime>(
    app: &AppHandle<R>,
    endpoint: url::Url,
    pubkey: &str,
) -> Result<tauri_plugin_updater::Updater, String> {
    app.updater_builder()
        .target(TEST_UPDATER_TARGET)
        .endpoints(vec![endpoint])
        .map_err(|error| error.to_string())?
        .pubkey(pubkey)
        .configure_client(configure_current_transport)
        .build()
        .map_err(|error| error.to_string())
}

#[cfg(test)]
pub(crate) fn test_metadata_from_update(update: &Update) -> AppUpdateMetadata {
    AppUpdateMetadata {
        id: stable_update_id(update),
        version: update.version.clone(),
        current_version: update.current_version.clone(),
        date: update.date.map(|date| {
            let offset = date.offset().whole_seconds();
            let sign = if offset < 0 { '-' } else { '+' };
            let absolute = offset.unsigned_abs();
            format!(
                "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}{}{:02}:{:02}",
                date.year(),
                u8::from(date.month()),
                date.day(),
                date.hour(),
                date.minute(),
                date.second(),
                sign,
                absolute / 3600,
                (absolute % 3600) / 60,
            )
        }),
        body: update.body.clone(),
    }
}

#[cfg(test)]
pub(crate) fn test_install_verified_with_mock(
    state: &AppUpdaterState,
    update_id: &str,
    mock_install: impl FnOnce(&Update, &[u8]) -> Result<(), String>,
) -> Result<(), String> {
    let (update, bytes) = state
        .verified_download_for(update_id)
        .ok_or_else(|| "Update has not been downloaded".to_string())?;
    mock_install(&update, &bytes)
}

#[cfg(test)]
mod tests;
