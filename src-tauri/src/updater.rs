//! Application-managed native updater.
//!
//! The official updater SDK owns check, signature verification and
//! installation. This module keeps the typed `tauri_plugin_updater::Update`
//! handles and the bytes returned after official verification, exposes them
//! through native commands, and resolves the current proxy snapshot each time
//! the SDK builds an HTTP client.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::Duration;

use semver::Version;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

/// Event carrying native updater download progress to the frontend facade.
pub const APP_UPDATER_DOWNLOAD_EVENT: &str = "app-updater-download-event";

const GITHUB_RELEASES_URL: &str =
    "https://api.github.com/repos/minbox-projects/one-space/releases?per_page=100";
const BETA_LOOKUP_TIMEOUT: Duration = Duration::from_secs(10);
const LATEST_MANIFEST_ASSET: &str = "latest.json";

#[derive(Deserialize)]
struct GithubRelease {
    #[serde(default)]
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    assets: Vec<GithubReleaseAsset>,
}

#[derive(Deserialize)]
struct GithubReleaseAsset {
    #[serde(default)]
    name: String,
}

#[derive(Deserialize)]
struct UpdaterPluginConfig {
    #[serde(default)]
    endpoints: Vec<url::Url>,
}

/// Parses a release tag into the semantic version it represents.
///
/// Accepted forms are `vX.Y.Z` and `vX.Y.Z-beta.N`; build metadata and other
/// prerelease identifiers are not release tags for this application.
fn release_version(tag: &str) -> Option<Version> {
    let version = Version::parse(tag.strip_prefix('v')?).ok()?;
    if !version.build.is_empty() {
        return None;
    }
    if !version.pre.is_empty() {
        let beta_number = version.pre.as_str().strip_prefix("beta.")?;
        if beta_number.is_empty() || !beta_number.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
    }
    Some(version)
}

/// Selects the highest non-draft release that publishes `latest.json`.
///
/// The GitHub releases response is a JSON array. Entries without a valid
/// release tag or without a `latest.json` asset are skipped, and semantic
/// version precedence ranks a stable release above its prereleases. A
/// malformed body or a non-array body is an error.
pub(crate) fn select_beta_release_tag(releases_json: &str) -> Result<Option<String>, String> {
    let body: serde_json::Value = serde_json::from_str(releases_json).map_err(|e| e.to_string())?;
    let releases = body
        .as_array()
        .ok_or_else(|| "GitHub releases response is not an array".to_string())?;

    let mut best: Option<(Version, String)> = None;
    for release in releases {
        let Ok(release) = serde_json::from_value::<GithubRelease>(release.clone()) else {
            continue;
        };
        if release.draft {
            continue;
        }
        if !release
            .assets
            .iter()
            .any(|asset| asset.name == LATEST_MANIFEST_ASSET)
        {
            continue;
        }
        let Some(version) = release_version(&release.tag_name) else {
            continue;
        };
        if best
            .as_ref()
            .map_or(true, |(current, _)| version > *current)
        {
            best = Some((version, release.tag_name));
        }
    }

    Ok(best.map(|(_, tag)| tag))
}

/// Builds the update manifest URL for a valid release tag.
pub(crate) fn beta_release_endpoint(tag: &str) -> Result<url::Url, String> {
    if release_version(tag).is_none() {
        return Err(format!("Invalid release tag: {tag}"));
    }
    url::Url::parse(&format!(
        "https://github.com/minbox-projects/one-space/releases/download/{tag}/{LATEST_MANIFEST_ASSET}"
    ))
    .map_err(|e| e.to_string())
}

/// Orders the endpoints for an update check: the selected beta endpoint first
/// when beta updates are enabled, followed by the configured stable endpoints.
pub(crate) fn updater_endpoint_plan(
    receive_beta_updates: bool,
    beta_endpoint: Option<url::Url>,
    stable_endpoints: Vec<url::Url>,
) -> Vec<url::Url> {
    let mut endpoints = Vec::new();
    if receive_beta_updates {
        if let Some(beta_endpoint) = beta_endpoint {
            endpoints.push(beta_endpoint);
        }
    }
    endpoints.extend(stable_endpoints);
    endpoints
}

/// Checks endpoints sequentially and returns the first available update.
///
/// A missing update or an error moves on to the next endpoint. When no
/// endpoint provides an update, any endpoint that reported no update makes the
/// result `Ok(None)`; the last error is returned only when every endpoint
/// failed.
pub(crate) async fn check_endpoints_in_order<T, F, Fut>(
    endpoints: Vec<url::Url>,
    mut check: F,
) -> Result<Option<T>, String>
where
    F: FnMut(url::Url) -> Fut,
    Fut: std::future::Future<Output = Result<Option<T>, String>>,
{
    let mut last_error = None;
    let mut saw_no_update = false;
    for endpoint in endpoints {
        match check(endpoint).await {
            Ok(Some(value)) => return Ok(Some(value)),
            Ok(None) => saw_no_update = true,
            Err(error) => last_error = Some(error),
        }
    }

    if saw_no_update {
        return Ok(None);
    }
    match last_error {
        Some(error) => Err(error),
        None => Ok(None),
    }
}

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
    #[cfg(test)]
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
    crate::proxy::apply_current_proxy_to_builder(builder)
        .unwrap_or_else(|_| reqwest::Client::builder().no_proxy())
}

/// Returns the stable endpoints declared by the updater plugin configuration.
fn configured_stable_endpoints(app: &AppHandle) -> Result<Vec<url::Url>, String> {
    let plugin_config = app
        .config()
        .plugins
        .0
        .get("updater")
        .cloned()
        .ok_or_else(|| "Updater plugin configuration is missing".to_string())?;
    let config: UpdaterPluginConfig = serde_json::from_value(plugin_config)
        .map_err(|_| "Updater plugin endpoints are invalid".to_string())?;
    if config.endpoints.is_empty() {
        return Err("Updater plugin has no stable endpoint".to_string());
    }
    Ok(config.endpoints)
}

/// Resolves the update manifest of the highest eligible GitHub release.
///
/// The request uses the current managed proxy and is bounded by a fixed
/// timeout; a failed lookup or an empty result simply leaves the check on the
/// stable channel.
async fn selected_beta_endpoint() -> Result<Option<url::Url>, String> {
    let client = crate::proxy::apply_current_proxy_to_builder(
        reqwest::Client::builder()
            .user_agent("OneSpace")
            .timeout(BETA_LOOKUP_TIMEOUT),
    )
    .map_err(|_| "Failed to prepare beta update transport".to_string())?
    .build()
    .map_err(|_| "Failed to prepare beta update transport".to_string())?;

    let response = client
        .get(GITHUB_RELEASES_URL)
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "GitHub releases request failed with status {}",
            response.status()
        ));
    }

    let body = response.text().await.map_err(|error| error.to_string())?;
    let tag = select_beta_release_tag(&body)?;
    tag.map(|tag| beta_release_endpoint(&tag)).transpose()
}

/// Checks one endpoint through the current managed proxy transport.
async fn check_endpoint(app: AppHandle, endpoint: url::Url) -> Result<Option<Update>, String> {
    let updater = app
        .updater_builder()
        .endpoints(vec![endpoint])
        .map_err(|error| error.to_string())?
        .configure_client(configure_current_transport)
        .build()
        .map_err(|error| error.to_string())?;
    updater.check().await.map_err(|error| error.to_string())
}

/// Checks the configured endpoint through the current managed proxy transport.
#[tauri::command]
pub async fn app_updater_check(app: AppHandle) -> Result<Option<AppUpdateMetadata>, String> {
    let prepared = crate::proxy::apply_current_proxy_to_builder(reqwest::Client::builder())
        .map_err(|_| "Failed to prepare updater transport".to_string())?;
    prepared
        .build()
        .map_err(|_| "Failed to prepare updater transport".to_string())?;

    let receive_beta_updates = crate::config::get_config()
        .map(|config| config.receive_beta_updates_enabled())
        .unwrap_or(false);

    let update = if receive_beta_updates {
        let beta_endpoint = selected_beta_endpoint().await.ok().flatten();
        let stable_endpoints = configured_stable_endpoints(&app).unwrap_or_default();
        let endpoints = updater_endpoint_plan(true, beta_endpoint, stable_endpoints);
        check_endpoints_in_order(endpoints, |endpoint| {
            let app = app.clone();
            async move { check_endpoint(app, endpoint).await }
        })
        .await?
    } else {
        let updater = app
            .updater_builder()
            .configure_client(configure_current_transport)
            .build()
            .map_err(|error| error.to_string())?;
        updater.check().await.map_err(|error| error.to_string())?
    };

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
