//! Process-owned automatic provider-template refresh scheduler.
//!
//! One scheduler runs per application process, independent of WebViews and of
//! the gateway listener. It reads the persisted interval (`0` disables the
//! schedule, `10`–`1440` arms it), runs exactly one immediate startup batch when
//! armed, and re-arms on a persisted interval save without an immediate batch.
//!
//! The automatic batch and the manual `ai_gateway_sync_provider_template`
//! command share a same-template operation guard: an automatic contender skips a
//! template another operation is already refreshing, while a manual contender
//! reuses the in-flight result instead of fetching again. Per-template failures
//! live in process memory only and are exposed through
//! [`failure_snapshot`] for the status command and the update event.

use super::templates::ProviderTemplateView;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, watch};

/// One per-template automatic-refresh failure in the process-memory store.
///
/// Serialized with its snake_case field names so the cross-stack JSON shape is
/// `{ "template_id": ..., "reason": ... }`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub struct TemplateAutoRefreshFailure {
    pub template_id: String,
    pub reason: String,
}

/// Snapshot of every current per-template automatic-refresh failure.
///
/// Serialized as `{ "failures": [ { template_id, reason } ] }`; the status
/// command and the update event carry the same shape.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub struct TemplateAutoRefreshStatus {
    pub failures: Vec<TemplateAutoRefreshFailure>,
}

// ---------------------------------------------------------------------------
// Failure store (process memory only)
// ---------------------------------------------------------------------------

fn failures() -> &'static Mutex<BTreeMap<String, String>> {
    static FAILURES: OnceLock<Mutex<BTreeMap<String, String>>> = OnceLock::new();
    FAILURES.get_or_init(|| Mutex::new(BTreeMap::new()))
}

/// Snapshot the current per-template failures in template-id order.
pub(in crate::ai_gateway) fn failure_snapshot() -> TemplateAutoRefreshStatus {
    let map = failures().lock().unwrap_or_else(|error| error.into_inner());
    TemplateAutoRefreshStatus {
        failures: map
            .iter()
            .map(|(template_id, reason)| TemplateAutoRefreshFailure {
                template_id: template_id.clone(),
                reason: reason.clone(),
            })
            .collect(),
    }
}

/// Record a failure for one template. Emits the update event only when the
/// reason actually changed.
pub(in crate::ai_gateway) fn set_template_failure(
    template_id: &str,
    reason: &str,
    app: Option<&AppHandle>,
) {
    let changed = {
        let mut map = failures().lock().unwrap_or_else(|error| error.into_inner());
        match map.get(template_id) {
            Some(existing) if existing == reason => false,
            _ => {
                map.insert(template_id.to_string(), reason.to_string());
                true
            }
        }
    };
    if changed {
        emit_updated(app);
    }
}

/// Clear the failure for one template. Emits the update event only when an
/// entry was actually removed.
pub(in crate::ai_gateway) fn clear_template_failure(template_id: &str, app: Option<&AppHandle>) {
    let changed = failures()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .remove(template_id)
        .is_some();
    if changed {
        emit_updated(app);
    }
}

/// Clear every recorded failure without emitting (used on test reset).
#[cfg(test)]
#[allow(dead_code)]
fn clear_all_failures() {
    failures()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clear();
}

/// Publish the current failure snapshot: record it on the test seam and emit
/// the update event through the supplied handle when one is present.
fn emit_updated(app: Option<&AppHandle>) {
    let status = failure_snapshot();
    #[cfg(test)]
    TEMPLATE_AUTO_REFRESH_EVENTS.with(|events| events.borrow_mut().push(status.clone()));
    if let Some(handle) = app {
        let _ = handle.emit(
            super::AI_GATEWAY_TEMPLATE_AUTO_REFRESH_UPDATED_EVENT,
            &status,
        );
    }
}

#[cfg(test)]
thread_local! {
    pub(in crate::ai_gateway) static TEMPLATE_AUTO_REFRESH_EVENTS:
        std::cell::RefCell<Vec<TemplateAutoRefreshStatus>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

// ---------------------------------------------------------------------------
// Same-template single-flight operation guard
// ---------------------------------------------------------------------------

/// Result shared by every same-template contender.
pub(in crate::ai_gateway) type TemplateSyncResult = Result<ProviderTemplateView, String>;

/// Shared state of one in-flight template operation. The completion result is
/// published through the watch channel so every follower observes it.
struct TemplateOp {
    tx: watch::Sender<Option<TemplateSyncResult>>,
}

fn template_ops() -> &'static Mutex<HashMap<(String, u64), Arc<TemplateOp>>> {
    static TEMPLATE_OPS: OnceLock<Mutex<HashMap<(String, u64), Arc<TemplateOp>>>> = OnceLock::new();
    TEMPLATE_OPS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Ownership decision returned by [`start_template_op`].
pub(in crate::ai_gateway) enum StartOutcome {
    /// No operation was running; the caller owns and must complete it.
    Owner(TemplateOpOwner),
    /// Another operation for the same template is running; the caller may wait.
    Follower(TemplateOpFollower),
}

/// Owner token of a same-template operation. Completing it publishes the result
/// and clears the guard entry.
pub(in crate::ai_gateway) struct TemplateOpOwner {
    template_id: String,
    generation: u64,
    op: Arc<TemplateOp>,
}

/// Follower token of a same-template operation. Awaiting it yields the owner's
/// result without fetching again.
pub(in crate::ai_gateway) struct TemplateOpFollower {
    _op: Arc<TemplateOp>,
    rx: watch::Receiver<Option<TemplateSyncResult>>,
}

impl TemplateOpOwner {
    /// Publish `result` to every follower, then clear the guard entry. The entry
    /// is removed only when it still points at this operation, so an exiting
    /// owner can never clear a replacement operation.
    pub(in crate::ai_gateway) fn finish(self, result: TemplateSyncResult) {
        let _ = self.op.tx.send(Some(result));
        let key = (self.template_id.clone(), self.generation);
        let mut guard = template_ops()
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(existing) = guard.get(&key) {
            if Arc::ptr_eq(existing, &self.op) {
                guard.remove(&key);
            }
        }
    }
}

impl TemplateOpFollower {
    /// Await and reuse the owner's in-flight result.
    pub(in crate::ai_gateway) async fn wait(mut self) -> TemplateSyncResult {
        if let Some(result) = self.rx.borrow().clone() {
            return result;
        }
        loop {
            if self.rx.changed().await.is_err() {
                return self.rx.borrow().clone().unwrap_or_else(|| {
                    Err("template sync operation ended without a result".to_string())
                });
            }
            if let Some(result) = self.rx.borrow().clone() {
                return result;
            }
        }
    }
}

/// Begin (or join) the same-template operation for `template_id` under the
/// current global proxy generation. A generation change starts an independent
/// operation instead of joining the previous transport's flight.
pub(in crate::ai_gateway) fn start_template_op(template_id: &str) -> StartOutcome {
    let generation = super::current_proxy_generation();
    let key = (template_id.to_string(), generation);
    let mut guard = template_ops()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if let Some(op) = guard.get(&key) {
        return StartOutcome::Follower(TemplateOpFollower {
            rx: op.tx.subscribe(),
            _op: op.clone(),
        });
    }
    let (tx, _rx) = watch::channel(None);
    let op = Arc::new(TemplateOp { tx });
    guard.insert(key, op.clone());
    StartOutcome::Owner(TemplateOpOwner {
        template_id: template_id.to_string(),
        generation,
        op,
    })
}

/// Emit one configuration-update event after an owned automatic template sync
/// succeeded, so an open AI Gateway page reloads the persisted configuration and
/// the refreshed template mappings. The supplied `AppHandle` is the emission
/// route because the scheduler runs even when the gateway listener never
/// captured a handle; in test builds the notification is also recorded on the
/// existing configuration-event seam.
fn notify_config_updated(app: Option<&AppHandle>) {
    #[cfg(test)]
    super::runtime_http::CONFIG_UPDATE_EVENTS.with(|events| {
        events
            .borrow_mut()
            .push(super::AI_GATEWAY_CONFIG_UPDATED_EVENT.to_string())
    });
    if let Some(handle) = app {
        let _ = handle.emit(super::AI_GATEWAY_CONFIG_UPDATED_EVENT, ());
    }
}

/// Update the failure store from one completed owner result.
fn finish_owner(
    owner: TemplateOpOwner,
    template_id: &str,
    app: Option<&AppHandle>,
    result: &TemplateSyncResult,
) {
    match result {
        Ok(_) => clear_template_failure(template_id, app),
        Err(reason) => set_template_failure(template_id, reason, app),
    }
    owner.finish(result.clone());
}

/// Outcome of one automatic contender.
pub(in crate::ai_gateway) enum AutomaticOutcome {
    /// This contender owned the operation and completed it. The payload is read
    /// by the batch test seam, so it is kept even while only tests consume it.
    #[allow(dead_code)]
    Ran(TemplateSyncResult),
    /// Another same-template operation was in flight, so this contender skipped.
    Skipped,
}

/// Run one manual template sync through the shared same-template guard. An
/// owner performs the operation under the generation captured at admission; a
/// follower reuses the in-flight result. A stale owner settles only its own
/// flight and never records a failure.
pub(in crate::ai_gateway) async fn sync_template_manual(
    app: Option<AppHandle>,
    template_id: &str,
) -> TemplateSyncResult {
    match start_template_op(template_id) {
        StartOutcome::Owner(owner) => {
            let generation = owner.generation;
            match super::commands::execute_template_sync_at_generation(
                app.clone(),
                template_id,
                generation,
            )
            .await
            {
                Ok(super::commands::TemplateSyncExecution::Applied(view)) => {
                    finish_owner(owner, template_id, app.as_ref(), &Ok(view.clone()));
                    Ok(view)
                }
                Ok(super::commands::TemplateSyncExecution::Stale) => {
                    let reason = super::commands::STALE_TEMPLATE_SYNC_REASON.to_string();
                    owner.finish(Err(reason.clone()));
                    Err(reason)
                }
                Err(error) => {
                    finish_owner(owner, template_id, app.as_ref(), &Err(error.clone()));
                    Err(error)
                }
            }
        }
        StartOutcome::Follower(follower) => follower.wait().await,
    }
}

/// Run one automatic contender through the shared same-template guard. A
/// follower is skipped instead of waiting. A stale owner neither records a
/// failure nor announces a configuration update.
pub(in crate::ai_gateway) async fn sync_template_automatic(
    app: Option<AppHandle>,
    template_id: &str,
) -> AutomaticOutcome {
    match start_template_op(template_id) {
        StartOutcome::Owner(owner) => {
            let generation = owner.generation;
            match super::commands::execute_template_sync_at_generation(
                app.clone(),
                template_id,
                generation,
            )
            .await
            {
                Ok(super::commands::TemplateSyncExecution::Applied(view)) => {
                    notify_config_updated(app.as_ref());
                    finish_owner(owner, template_id, app.as_ref(), &Ok(view.clone()));
                    AutomaticOutcome::Ran(Ok(view))
                }
                Ok(super::commands::TemplateSyncExecution::Stale) => {
                    let reason = super::commands::STALE_TEMPLATE_SYNC_REASON.to_string();
                    owner.finish(Err(reason.clone()));
                    AutomaticOutcome::Ran(Err(reason))
                }
                Err(error) => {
                    finish_owner(owner, template_id, app.as_ref(), &Err(error.clone()));
                    AutomaticOutcome::Ran(Err(error))
                }
            }
        }
        StartOutcome::Follower(_) => AutomaticOutcome::Skipped,
    }
}

// ---------------------------------------------------------------------------
// Batch execution
// ---------------------------------------------------------------------------

/// One batch runs at a time; a contender that finds a batch in flight is
/// skipped. The guard is released on drop, including on unwind.
struct BatchInFlightGuard;

static BATCH_IN_FLIGHT: AtomicBool = AtomicBool::new(false);

impl BatchInFlightGuard {
    fn acquire() -> Option<Self> {
        BATCH_IN_FLIGHT
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .ok()
            .map(|_| Self)
    }
}

impl Drop for BatchInFlightGuard {
    fn drop(&mut self) {
        BATCH_IN_FLIGHT.store(false, Ordering::SeqCst);
    }
}

/// Whether a shutdown has been requested; the scheduler loop and an in-flight
/// batch stop at the next template boundary.
fn scheduler_shutdown_requested() -> bool {
    SCHEDULER_SHUTDOWN.load(Ordering::SeqCst)
}

/// The eligible templates for one batch: the ordered template views that carry
/// a non-empty trimmed `models_url`.
pub(in crate::ai_gateway) fn eligible_template_ids(config: &super::GatewayConfig) -> Vec<String> {
    super::templates::provider_template_views(config)
        .unwrap_or_default()
        .into_iter()
        .filter(|view| {
            view.template
                .models_url
                .as_deref()
                .map(str::trim)
                .is_some_and(|url| !url.is_empty())
        })
        .map(|view| view.template.id)
        .collect()
}

/// Run one sequential automatic batch over the currently eligible templates.
/// Sequential in template order; a tick arriving during a batch is skipped.
pub(in crate::ai_gateway) async fn run_batch(app: Option<AppHandle>) {
    let Some(_guard) = BatchInFlightGuard::acquire() else {
        return;
    };
    let ids = match super::storage::read_config() {
        Ok(config) => eligible_template_ids(&config),
        Err(_) => return,
    };
    for template_id in ids {
        if scheduler_shutdown_requested() {
            break;
        }
        let _ = sync_template_automatic(app.clone(), &template_id).await;
    }
}

/// Test seam: run one batch over injected templates with an injected automatic
/// sync closure, sharing the production batch-in-flight guard.
#[cfg(test)]
#[allow(dead_code)]
pub(in crate::ai_gateway) async fn run_batch_with<S, Fut>(
    templates: Vec<String>,
    sync: S,
) -> Option<Vec<(String, AutomaticOutcome)>>
where
    S: Fn(&str) -> Fut,
    Fut: std::future::Future<Output = AutomaticOutcome>,
{
    let _guard = BatchInFlightGuard::acquire()?;
    let mut results = Vec::new();
    for template_id in templates {
        if scheduler_shutdown_requested() {
            break;
        }
        let outcome = sync(&template_id).await;
        results.push((template_id, outcome));
    }
    Some(results)
}

// ---------------------------------------------------------------------------
// Scheduler lifecycle
// ---------------------------------------------------------------------------

fn current_interval_minutes() -> u32 {
    super::storage::read_config()
        .map(|config| {
            super::normalize_template_auto_refresh_minutes(config.template_auto_refresh_minutes)
        })
        .unwrap_or(0)
}

fn interval_deadline(minutes: u32) -> Option<tokio::time::Instant> {
    if minutes == 0 {
        None
    } else {
        Some(tokio::time::Instant::now() + Duration::from_secs(minutes as u64 * 60))
    }
}

fn rearm_deadline() -> Option<tokio::time::Instant> {
    interval_deadline(current_interval_minutes())
}

async fn scheduler_loop(app: AppHandle, mut rearm: mpsc::UnboundedReceiver<()>) {
    let mut deadline = rearm_deadline();
    loop {
        if scheduler_shutdown_requested() {
            return;
        }
        match deadline {
            Some(at) => {
                tokio::select! {
                    _ = rearm.recv() => { deadline = rearm_deadline(); }
                    _ = tokio::time::sleep_until(at) => {
                        run_batch(Some(app.clone())).await;
                        deadline = rearm_deadline();
                    }
                }
            }
            None => {
                if rearm.recv().await.is_none() {
                    return;
                }
                deadline = rearm_deadline();
            }
        }
    }
}

/// Control handle of the installed process scheduler.
struct SchedulerControl {
    rearm: mpsc::UnboundedSender<()>,
}

/// The installed scheduler slot: `None` until the app setup installs it (or a
/// test seam installs a controlled one), and `None` again after a test reset.
/// The mutex-backed slot makes a re-installation a non-replacing no-op without
/// the impossibility of clearing a `OnceLock`.
fn scheduler_state() -> &'static Mutex<Option<SchedulerControl>> {
    static SCHEDULER: OnceLock<Mutex<Option<SchedulerControl>>> = OnceLock::new();
    SCHEDULER.get_or_init(|| Mutex::new(None))
}

static SCHEDULER_SHUTDOWN: AtomicBool = AtomicBool::new(false);

/// Install the scheduler slot and its parked control loop exactly once.
/// Returns `false` without replacing or re-spawning anything when a scheduler
/// is already installed, so a second install never double-runs. `spawn` runs
/// synchronously and receives the re-arm receiver the installed loop owns.
fn install_scheduler_control<S>(spawn: S) -> bool
where
    S: FnOnce(mpsc::UnboundedReceiver<()>),
{
    let mut slot = scheduler_state()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if slot.is_some() {
        return false;
    }
    let (tx, rx) = mpsc::unbounded_channel();
    *slot = Some(SchedulerControl { rearm: tx });
    drop(slot);
    spawn(rx);
    true
}

/// Start the process scheduler exactly once. A second setup call installs
/// nothing and never replaces the running schedule. A positive persisted
/// interval arms the schedule and runs exactly one immediate startup batch;
/// `0` installs the parked control (so a later interval save can re-arm it) but
/// runs no batch.
pub(in crate::ai_gateway) fn start_scheduler(app: AppHandle) {
    let loop_app = app.clone();
    let installed = install_scheduler_control(move |rx| {
        tauri::async_runtime::spawn(scheduler_loop(loop_app, rx));
    });
    if installed && current_interval_minutes() != 0 {
        let batch_app = app.clone();
        tauri::async_runtime::spawn(async move { run_batch(Some(batch_app)).await });
    }
}

/// Request a schedule re-arm after a persisted interval save. Re-arming never
/// starts an immediate batch. A scheduler that was never started is a no-op.
pub(in crate::ai_gateway) fn request_rearm() {
    let slot = scheduler_state()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if let Some(control) = slot.as_ref() {
        let _ = control.rearm.send(());
    }
}

/// Stop the scheduler: set the shutdown flag and wake a parked loop. Idempotent
/// and safe when no scheduler runs.
pub(in crate::ai_gateway) fn stop_scheduler() {
    SCHEDULER_SHUTDOWN.store(true, Ordering::SeqCst);
    let slot = scheduler_state()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if let Some(control) = slot.as_ref() {
        let _ = control.rearm.send(());
    }
}

/// Test seam: install a scheduler whose startup batch runs through the injected
/// executor instead of the real network batch.
///
/// Semantics: a positive interval installs the schedule and runs `batch`
/// exactly once synchronously; a second install while started returns `false`
/// and keeps the existing schedule without running the new executor; interval
/// `0` installs nothing and never runs the executor. The installed control loop
/// only parks and drains re-arm signals, so a later `request_rearm` never runs
/// a batch. Returns whether this call installed the scheduler.
#[cfg(test)]
#[allow(dead_code)]
pub(in crate::ai_gateway) fn install_scheduler_for_tests_with<F: Fn()>(
    interval_minutes: u32,
    batch: F,
) -> bool {
    if interval_minutes == 0 {
        return false;
    }
    let installed = install_scheduler_control(|mut rx| {
        tauri::async_runtime::spawn(async move {
            loop {
                if scheduler_shutdown_requested() {
                    return;
                }
                if rx.recv().await.is_none() {
                    return;
                }
            }
        });
    });
    if installed {
        batch();
    }
    installed
}

/// Test seam: whether a process scheduler is currently installed.
#[cfg(test)]
#[allow(dead_code)]
pub(in crate::ai_gateway) fn scheduler_started() -> bool {
    scheduler_state()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .is_some()
}

/// Test seam: reset all process-memory auto-refresh state between tests. Taking
/// the scheduler slot drops its re-arm sender, which stops a parked control loop
/// and lets a later test install a fresh controlled scheduler.
#[cfg(test)]
#[allow(dead_code)]
pub(in crate::ai_gateway) fn reset_auto_refresh_for_tests() {
    clear_all_failures();
    template_ops()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clear();
    scheduler_state()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .take();
    BATCH_IN_FLIGHT.store(false, Ordering::SeqCst);
    SCHEDULER_SHUTDOWN.store(false, Ordering::SeqCst);
    TEMPLATE_AUTO_REFRESH_EVENTS.with(|events| events.borrow_mut().clear());
}


#[cfg(test)]
mod proxy_switch_named {
    use super::{start_template_op, StartOutcome};
    use crate::ai_gateway::commands::{
        execute_template_sync_at_generation, TemplateSyncExecution,
    };
    use crate::ai_gateway::templates::ProviderTemplateView;
    use crate::ai_gateway::types_config::{ProviderTemplate, ProviderTemplateState};
    use crate::proxy::test_support::{HttpProxyRecorder, SERIAL_GUARD};
    use std::ffi::OsString;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    const TEMPLATE_ID: &str = "proxy-generation-flight-template";

    fn view(synced_at: u64) -> ProviderTemplateView {
        ProviderTemplateView {
            template: ProviderTemplate {
                id: TEMPLATE_ID.to_string(),
                name: "Proxy generation flight".to_string(),
                ..ProviderTemplate::default()
            },
            synced_at: Some(synced_at),
            source: "test".to_string(),
            from_snapshot: false,
        }
    }

    fn port_of(url: &str) -> u16 {
        url.trim_start_matches("http://")
            .split(':')
            .nth(1)
            .expect("recorder URL has a port")
            .parse()
            .expect("recorder URL port is numeric")
    }

    fn proxy_config(url: &str) -> crate::config::ProxyConfig {
        crate::config::ProxyConfig {
            proxy_enabled: true,
            proxy_type: "http".to_string(),
            proxy_host: "127.0.0.1".to_string(),
            proxy_port: port_of(url),
            proxy_username: None,
            proxy_password: None,
            check_interval: 15,
        }
    }

    /// REQ-004 / AC-006: same-template flights are generation-scoped. A caller
    /// admitted after a proxy switch owns a new flight, while a same-generation
    /// caller joins the running one. A late old-generation finish settles only
    /// its own flight and never replaces the newer generation's entry.
    #[tokio::test(flavor = "current_thread")]
    async fn gateway_template_refresh_flight_separates_generations_and_gates_late_result() {
        let _guard = SERIAL_GUARD.lock().unwrap_or_else(|error| error.into_inner());
        let _home = crate::lock_test_home_env();
        let proxy_a = HttpProxyRecorder::new("A").await;
        let proxy_b = HttpProxyRecorder::new("B").await;
        let manager = crate::proxy::PROXY_MANAGER
            .get_or_init(|| Arc::new(crate::proxy::ProxyManager::new()));

        manager.publish(
            manager
                .prepare(Some(proxy_config(&proxy_a.url())))
                .expect("proxy A prepares"),
        );
        let generation_a = manager.generation();

        let owner_a = match start_template_op(TEMPLATE_ID) {
            StartOutcome::Owner(owner) => owner,
            StartOutcome::Follower(_) => {
                panic!("no flight should exist for the template under generation A")
            }
        };

        manager.publish(
            manager
                .prepare(Some(proxy_config(&proxy_b.url())))
                .expect("proxy B prepares"),
        );
        let generation_b = manager.generation();
        assert_ne!(
            generation_a, generation_b,
            "publishing a different proxy endpoint must advance the generation"
        );

        let owner_b = match start_template_op(TEMPLATE_ID) {
            StartOutcome::Owner(owner) => owner,
            StartOutcome::Follower(_) => panic!(
                "a new-generation template operation must own a fresh flight instead of joining the old one"
            ),
        };

        // Same generation as B: this contender must follow B, not fetch again.
        let follower_b = match start_template_op(TEMPLATE_ID) {
            StartOutcome::Follower(follower) => follower,
            StartOutcome::Owner(_) => {
                panic!("a same-generation template contender must follow the in-flight operation")
            }
        };

        owner_b.finish(Ok(view(2)));
        let followed = follower_b
            .wait()
            .await
            .expect("the generation B follower must observe the owner's result");
        assert_eq!(
            followed.synced_at,
            Some(2),
            "the follower must receive the generation B owner's result"
        );

        // After B finishes, a new B-generation operation owns a fresh flight.
        let owner_b2 = match start_template_op(TEMPLATE_ID) {
            StartOutcome::Owner(owner) => owner,
            StartOutcome::Follower(_) => panic!(
                "a completed flight must not be reused; the next same-generation operation must own"
            ),
        };

        // A settles late. Its own flight is gone, but it must not clear B2.
        owner_a.finish(Ok(view(1)));

        let follower_b2 = match start_template_op(TEMPLATE_ID) {
            StartOutcome::Follower(follower) => follower,
            StartOutcome::Owner(_) => panic!(
                "the late old-generation finish must not clear the newer operation's entry"
            ),
        };
        owner_b2.finish(Ok(view(3)));
        let followed_b2 = follower_b2
            .wait()
            .await
            .expect("the generation B second follower must observe its owner's result");
        assert_eq!(
            followed_b2.synced_at,
            Some(3),
            "a late old-generation result must not replace the current generation's result"
        );
    }

    struct TempHome {
        dir: PathBuf,
        saved_home: Option<OsString>,
        _override: crate::config::test_home::TestHomeGuard,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl TempHome {
        fn new() -> Self {
            let lock = crate::lock_test_home_env();
            let dir = std::env::temp_dir().join(format!(
                "onespace-stale-template-sync-{}",
                uuid::Uuid::new_v4()
            ));
            std::fs::create_dir_all(&dir).expect("create stale template home");
            let override_guard = crate::config::test_home::TestHomeGuard::set(dir.clone());
            let saved_home = std::env::var_os("HOME");
            std::env::set_var("HOME", &dir);
            Self {
                dir,
                saved_home,
                _override: override_guard,
                _lock: lock,
            }
        }
    }

    impl Drop for TempHome {
        fn drop(&mut self) {
            match self.saved_home.take() {
                Some(home) => std::env::set_var("HOME", home),
                None => std::env::remove_var("HOME"),
            }
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    /// REQ-004 / AC-006: the generation captured at operation start is rechecked
    /// inside the serialized configuration mutation. A body fetched before a
    /// proxy switch is discarded without publishing a template or changing the
    /// persisted configuration.
    #[tokio::test(flavor = "current_thread")]
    async fn stale_template_sync_is_refused_and_leaves_config_unchanged() {
        let _guard = SERIAL_GUARD.lock().unwrap_or_else(|error| error.into_inner());
        let home = TempHome::new();
        let template_id = "stale-generation-template";
        let proxy_a = HttpProxyRecorder::new("A").await;
        let proxy_b = HttpProxyRecorder::new("B").await;
        let manager = crate::proxy::PROXY_MANAGER
            .get_or_init(|| Arc::new(crate::proxy::ProxyManager::new()));

        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind stale template source");
        let source_url = format!(
            "http://{}/models",
            listener.local_addr().expect("template source address")
        );
        let (hit_tx, hit_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept template fetch");
            let mut buffer = [0u8; 2048];
            let _ = stream.read(&mut buffer).await;
            let _ = hit_tx.send(());
            let _ = release_rx.await;
            let body = r#"{"data":[{"id":"m1","name":"M1"}]}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes()).await;
        });

        let template = ProviderTemplate {
            id: template_id.to_string(),
            name: "Stale generation template".to_string(),
            models_url: Some(source_url),
            source: "https://template.example.test/models".to_string(),
            ..ProviderTemplate::default()
        };
        let config = crate::ai_gateway::GatewayConfig {
            provider_templates: vec![ProviderTemplateState {
                template_id: template_id.to_string(),
                template: Some(template),
                synced_at: None,
                source: None,
            }],
            ..crate::ai_gateway::GatewayConfig::default()
        };
        crate::ai_gateway::storage::write_config(&config)
            .expect("seed the stale template configuration");

        manager.publish(
            manager
                .prepare(Some(proxy_config(&proxy_a.url())))
                .expect("proxy A prepares"),
        );
        let generation_a = manager.generation();

        let owned_template_id = template_id.to_string();
        let mut call = tokio::spawn(async move {
            execute_template_sync_at_generation(None, &owned_template_id, generation_a).await
        });
        tokio::time::timeout(Duration::from_secs(5), hit_rx)
            .await
            .expect("the template sync must fetch its source")
            .expect("the template source must signal the request");

        manager.publish(
            manager
                .prepare(Some(proxy_config(&proxy_b.url())))
                .expect("proxy B prepares"),
        );
        release_tx.send(()).expect("release the template source");

        let outcome = tokio::time::timeout(Duration::from_secs(5), &mut call)
            .await
            .expect("the stale template sync must settle")
            .expect("the template sync task must not panic")
            .expect("the template source must succeed");
        assert!(
            matches!(outcome, TemplateSyncExecution::Stale),
            "a body fetched under an older proxy generation must be refused as stale"
        );
        server.await.expect("template source task must complete");

        let stored = crate::ai_gateway::storage::read_config()
            .expect("the stale sync must leave a readable configuration");
        let state = stored
            .provider_templates
            .iter()
            .find(|state| state.template_id == template_id)
            .expect("the seeded template state must remain");
        assert_eq!(
            state.synced_at, None,
            "a stale template sync must not record a successful sync time"
        );
        assert!(
            state
                .template
                .as_ref()
                .expect("the template remains present")
                .models
                .is_empty(),
            "a stale template sync must not publish fetched models"
        );

        drop(home);
    }
}
