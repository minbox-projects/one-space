//! Process-owned interval and daily collection scheduler.
//!
//! The scheduler runs independently of any page. Interval mode owns one
//! persistent session per target and rearms when a collection finishes. Daily
//! mode claims a target's local date/time in SQLite before any network work and
//! uses a one-shot session per attempt. At most four collections run at once and
//! a target never has more than one in-flight collection.

use crate::server_monitor::storage::{ServerMonitorConfig, ServerMonitorStore};
use crate::server_monitor::transport::{CollectionError, MonitorConnector, MonitorSession};
use crate::server_monitor::types::{
    CollectionFailureKind, CollectionOutcome, MetricSnapshot, MonitorAuth, MonitorConnectRequest,
    MonitorSource, MonitorTransportErrorKind, ServerMonitorCustomDraft, ServerMonitorCustomRecord,
    ServerMonitorOutcome, ServerMonitorSample,
    ServerMonitorScheduleMode, ServerMonitorSettings, ServerMonitorSourceKind, ServerMonitorState,
    ServerMonitorTargetDraft, ServerMonitorTargetRecord, ServerMonitorTestResult, SessionLifetime,
};
pub(crate) use crate::server_monitor::types::ServerMonitorHistoryRange;
use chrono::{Datelike, Local, LocalResult, TimeZone};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::Duration;

const MAX_ACTIVE_COLLECTIONS: usize = 4;
const DAILY_SLOT_WINDOW_MS: i64 = 60_000;
const TICK_INTERVAL: Duration = Duration::from_secs(1);
const COLLECTION_LIMIT_ERROR: &str = "Server monitor collection limit reached";
const CANCELLED_MESSAGE: &str = "Collection was cancelled";

/// Event boundary for safe state publication.
pub(crate) trait ServerMonitorEventSink: Send + Sync {
    fn publish(&self, state: &ServerMonitorState);
    fn notify_failure(&self, target_id: &str, target_name: &str, error_message: &str);
}

#[cfg(test)]
pub(crate) struct NoopEventSink;

#[cfg(test)]
impl ServerMonitorEventSink for NoopEventSink {
    fn publish(&self, _state: &ServerMonitorState) {}
    fn notify_failure(&self, _target_id: &str, _target_name: &str, _error_message: &str) {}
}

struct TauriEventSink {
    app: tauri::AppHandle,
}

impl ServerMonitorEventSink for TauriEventSink {
    fn publish(&self, state: &ServerMonitorState) {
        use tauri::Emitter;
        let _ = self.app.emit("server-monitor-updated", state);
    }

    fn notify_failure(&self, target_id: &str, target_name: &str, error_message: &str) {
        let input = crate::messages::MessageCreateInput {
            source: "server_monitor".to_string(),
            category: "collect_failed".to_string(),
            severity: "error".to_string(),
            title: format!("服务器监控采集异常 - {}", target_name),
            summary: Some(format!("服务器 [{}] 监控指标采集失败", target_name)),
            detail: Some(error_message.to_string()),
            dedupe_key: Some(format!("server_monitor_error:{}", target_id)),
            target: Some(crate::messages::MessageTarget {
                tab: "more-tools".to_string(),
                section: Some("server-monitor".to_string()),
                entity_id: Some(target_id.to_string()),
            }),
            metadata: Some(serde_json::json!({
                "target_id": target_id,
                "target_name": target_name,
            })),
        };
        crate::messages::record_message_silent(&self.app, input);
    }
}

pub(crate) trait ServerMonitorClock: Send + Sync {
    fn now_ms(&self) -> i64;
}

struct SystemClock;

impl ServerMonitorClock for SystemClock {
    fn now_ms(&self) -> i64 {
        Local::now().timestamp_millis()
    }
}

struct Inner {
    config: ServerMonitorConfig,
    config_broken: bool,
    /// Persistent interval sessions, reused by later collections.
    sessions: HashMap<String, Arc<MonitorSession>>,
    /// Every owned session per target, including one-shot sessions.
    owned_sessions: HashMap<String, Vec<Arc<MonitorSession>>>,
    inflight: HashMap<String, Arc<InFlight>>,
    interval_due: HashMap<String, i64>,
    daily_armed_after_ms: HashMap<String, i64>,
    daily_queue: Vec<DailyQueueEntry>,
    last_queue_tick_ms: Option<i64>,
    active: usize,
    generation: HashMap<String, u64>,
    error: Option<String>,
}

/// A claimed daily slot waiting for a free collection slot inside its minute.
#[derive(Debug, Clone)]
struct DailyQueueEntry {
    target_id: String,
    slot_ms: i64,
}

struct InFlight {
    result: Mutex<Option<CollectionReport>>,
    ready: Condvar,
}

impl InFlight {
    fn new() -> Self {
        Self {
            result: Mutex::new(None),
            ready: Condvar::new(),
        }
    }

    fn complete(&self, report: CollectionReport) {
        if let Ok(mut result) = self.result.lock() {
            *result = Some(report);
        }
        self.ready.notify_all();
    }

    fn wait(&self) -> Option<CollectionReport> {
        let mut result = self.result.lock().ok()?;
        while result.is_none() {
            let (next, _) = self.ready.wait_timeout(result, Duration::from_millis(100)).ok()?;
            result = next;
        }
        result.clone()
    }
}

/// One completed collection used by scheduled work and manual refreshes.
#[derive(Debug, Clone)]
pub(crate) struct CollectionReport {
    pub snapshot: Option<MetricSnapshot>,
    pub outcome: ServerMonitorOutcome,
    pub failure_kind: Option<CollectionFailureKind>,
    pub error: Option<String>,
    pub persistence_error: Option<String>,
    pub cancelled: bool,
    pub collected_at_ms: i64,
    pub source_mode: ServerMonitorScheduleMode,
    pub expected_interval_seconds: Option<u64>,
}

impl CollectionReport {
    fn cancelled(collected_at_ms: i64, source_mode: ServerMonitorScheduleMode) -> Self {
        Self {
            snapshot: None,
            outcome: ServerMonitorOutcome::Failed,
            failure_kind: None,
            error: Some("Collection was cancelled".to_string()),
            persistence_error: None,
            cancelled: true,
            collected_at_ms,
            source_mode,
            expected_interval_seconds: None,
        }
    }
}

/// Process-owned runtime. Commands and the background tick share one instance.
pub(crate) struct ServerMonitorRuntime {
    store: Arc<ServerMonitorStore>,
    connector: Arc<dyn MonitorConnector>,
    events: Arc<dyn ServerMonitorEventSink>,
    clock: Arc<dyn ServerMonitorClock>,
    inner: Mutex<Inner>,
    started: AtomicBool,
    shutting_down: AtomicBool,
}

static GLOBAL: OnceLock<Mutex<Option<Arc<ServerMonitorRuntime>>>> = OnceLock::new();

fn global_slot() -> &'static Mutex<Option<Arc<ServerMonitorRuntime>>> {
    GLOBAL.get_or_init(|| Mutex::new(None))
}

/// Installs the process runtime exactly once and starts its scheduler.
pub(crate) fn init_global(app: tauri::AppHandle) -> Result<Arc<ServerMonitorRuntime>, String> {
    let mut slot = global_slot().lock().map_err(|error| error.to_string())?;
    if let Some(runtime) = slot.as_ref() {
        return Ok(runtime.clone());
    }
    let store = Arc::new(ServerMonitorStore::from_app_dir()?);
    let connector: Arc<dyn MonitorConnector> =
        Arc::new(crate::ssh_tunnels::monitor_transport::SshMonitorConnector::new());
    let events: Arc<dyn ServerMonitorEventSink> = Arc::new(TauriEventSink { app });
    let runtime = ServerMonitorRuntime::new(store, connector, events);
    runtime.start();
    *slot = Some(runtime.clone());
    Ok(runtime)
}

pub(crate) fn global() -> Result<Arc<ServerMonitorRuntime>, String> {
    global_slot()
        .lock()
        .map_err(|error| error.to_string())?
        .clone()
        .ok_or_else(|| "Server monitor runtime is not initialized".to_string())
}

/// Stops the process runtime idempotently.
pub(crate) fn shutdown_global() {
    let runtime = global_slot()
        .lock()
        .ok()
        .and_then(|mut slot| slot.take());
    if let Some(runtime) = runtime {
        runtime.shutdown();
    }
}

impl ServerMonitorRuntime {
    pub(crate) fn new(
        store: Arc<ServerMonitorStore>,
        connector: Arc<dyn MonitorConnector>,
        events: Arc<dyn ServerMonitorEventSink>,
    ) -> Arc<Self> {
        Self::new_with_clock(store, connector, events, Arc::new(SystemClock))
    }

    pub(crate) fn new_with_clock(
        store: Arc<ServerMonitorStore>,
        connector: Arc<dyn MonitorConnector>,
        events: Arc<dyn ServerMonitorEventSink>,
        clock: Arc<dyn ServerMonitorClock>,
    ) -> Arc<Self> {
        let (config, config_broken) = match store.load_config() {
            Ok(config) => (config, false),
            Err(error) => {
                log::error!("Server monitor configuration could not be loaded: {error}");
                (ServerMonitorConfig::default(), true)
            }
        };
        Arc::new(Self {
            store,
            connector,
            events,
            clock,
            inner: Mutex::new(Inner {
                config,
                config_broken,
                sessions: HashMap::new(),
                owned_sessions: HashMap::new(),
                inflight: HashMap::new(),
                interval_due: HashMap::new(),
                daily_armed_after_ms: HashMap::new(),
                daily_queue: Vec::new(),
                last_queue_tick_ms: None,
                active: 0,
                generation: HashMap::new(),
                error: None,
            }),
            started: AtomicBool::new(false),
            shutting_down: AtomicBool::new(false),
        })
    }

    /// Starts the scheduler once and arms the initial interval collection.
    pub(crate) fn start(self: &Arc<Self>) {
        if self.started.swap(true, Ordering::SeqCst) {
            return;
        }
        if self.shutting_down.load(Ordering::SeqCst) {
            return;
        }
        let now = self.clock.now_ms();
        if let Ok(mut inner) = self.inner.lock() {
            let mode = inner.config.settings.schedule_mode;
            if inner.config.settings.auto_collect_enabled {
                match mode {
                    ServerMonitorScheduleMode::Interval => {
                        for id in scheduled_target_ids(&inner.config) {
                            inner.interval_due.insert(id, now);
                        }
                    }
                    ServerMonitorScheduleMode::Daily => {
                        for id in all_target_ids(&inner.config) {
                            inner.daily_armed_after_ms.insert(id, now);
                        }
                    }
                }
            }
            let retention = inner.config.settings.retention_days;
            drop(inner);
            let today = today_string(now);
            let _ = self.store.prune(retention, now, &today);
        }
        let runtime = self.clone();
        std::thread::spawn(move || {
            while !runtime.shutting_down.load(Ordering::SeqCst) {
                let now = runtime.clock.now_ms();
                let _ = runtime.tick(now);
                runtime.send_keepalives();
                std::thread::sleep(TICK_INTERVAL);
            }
        });
    }

    pub(crate) fn shutdown(&self) {
        self.shutting_down.store(true, Ordering::SeqCst);
        if let Ok(mut inner) = self.inner.lock() {
            let persistent = inner.sessions.drain().map(|(_, session)| session).collect::<Vec<_>>();
            let owned = inner
                .owned_sessions
                .drain()
                .flat_map(|(_, sessions)| sessions)
                .collect::<Vec<_>>();
            let mut ids = inner.inflight.keys().cloned().collect::<Vec<_>>();
            ids.extend(inner.config.targets.iter().map(|target| target.id.clone()));
            for id in ids {
                *inner.generation.entry(id).or_insert(0) += 1;
            }
            inner.interval_due.clear();
            inner.daily_armed_after_ms.clear();
            inner.daily_queue.clear();
            drop(inner);
            for session in persistent.into_iter().chain(owned) {
                session.invalidate();
            }
        }
    }

    fn send_keepalives(&self) {
        if self.shutting_down.load(Ordering::SeqCst) {
            return;
        }
        let sessions = match self.inner.lock() {
            Ok(inner) => inner.sessions.values().cloned().collect::<Vec<_>>(),
            Err(_) => return,
        };
        let now = std::time::Instant::now();
        for session in sessions {
            session.keepalive_if_due(now);
        }
    }

    /// Runs one scheduler tick. Claim write failures are returned to the caller.
    pub(crate) fn tick(self: &Arc<Self>, now_ms: i64) -> Result<(), String> {
        if self.shutting_down.load(Ordering::SeqCst) {
            return Ok(());
        }
        let (mode, auto_enabled, targets) = {
            let inner = self.inner.lock().map_err(|error| error.to_string())?;
            (
                inner.config.settings.schedule_mode,
                inner.config.settings.auto_collect_enabled,
                inner.config.targets.clone(),
            )
        };
        if !auto_enabled {
            return Ok(());
        }

        match mode {
            ServerMonitorScheduleMode::Interval => {
                let due = {
                    let inner = self.inner.lock().map_err(|error| error.to_string())?;
                    targets
                        .iter()
                        .filter(|target| {
                            target.enabled
                                && !target.blocked
                                && !inner.inflight.contains_key(&target.id)
                                && inner
                                    .interval_due
                                    .get(&target.id)
                                    .copied()
                                    .map(|value| value <= now_ms)
                                    .unwrap_or(false)
                        })
                        .cloned()
                        .collect::<Vec<_>>()
                };
                let interval_minutes = self.settings_snapshot().interval_minutes;
                let expected = Some(u64::from(interval_minutes * 60));
                for target in due {
                    match self.begin_collection(
                        target,
                        SessionLifetime::Persistent,
                        ServerMonitorScheduleMode::Interval,
                        expected,
                    ) {
                        Ok(_) => {}
                        Err(error) if error == COLLECTION_LIMIT_ERROR => break,
                        Err(error) => {
                            self.set_error(error);
                            break;
                        }
                    }
                }
            }
            ServerMonitorScheduleMode::Daily => {
                self.process_daily_queue(now_ms)?;
                self.collect_due_daily_targets(now_ms, &targets)?;
            }
        }
        Ok(())
    }

    /// Starts already-claimed slots that were waiting for collection capacity.
    fn process_daily_queue(self: &Arc<Self>, now_ms: i64) -> Result<(), String> {
        let mut ready = Vec::new();
        let mut keep = Vec::new();
        {
            let mut inner = self.inner.lock().map_err(|error| error.to_string())?;
            if inner.config.settings.schedule_mode != ServerMonitorScheduleMode::Daily
                || !inner.config.settings.auto_collect_enabled
            {
                inner.daily_queue.clear();
                return Ok(());
            }
            if inner.last_queue_tick_ms == Some(now_ms) {
                return Ok(());
            }
            inner.last_queue_tick_ms = Some(now_ms);
            let queued = std::mem::take(&mut inner.daily_queue);
            for entry in queued {
                if now_ms < entry.slot_ms {
                    // A clock rollback must not start a future slot early.
                    keep.push(entry);
                    continue;
                }
                if now_ms >= entry.slot_ms + DAILY_SLOT_WINDOW_MS {
                    // The one-minute opportunity passed; the claim stays consumed.
                    continue;
                }
                let Some(target) = inner
                    .config
                    .targets
                    .iter()
                    .find(|target| target.id == entry.target_id)
                    .cloned()
                else {
                    continue;
                };
                if !target.enabled || target.blocked {
                    continue;
                }
                if inner.inflight.contains_key(&entry.target_id) {
                    keep.push(entry);
                    continue;
                }
                ready.push((entry, target));
            }
            inner.daily_queue = keep;
        }
        for (entry, target) in ready {
            match self.begin_collection(
                target,
                SessionLifetime::OneShot,
                ServerMonitorScheduleMode::Daily,
                None,
            ) {
                Ok(_) => {}
                Err(error) if error == COLLECTION_LIMIT_ERROR => {
                    if let Ok(mut inner) = self.inner.lock() {
                        inner.daily_queue.push(entry);
                    }
                }
                Err(error) => self.set_error(error),
            }
        }
        Ok(())
    }

    fn collect_due_daily_targets(
        self: &Arc<Self>,
        now_ms: i64,
        targets: &[ServerMonitorTargetRecord],
    ) -> Result<(), String> {
        let (times, armed, busy) = {
            let inner = self.inner.lock().map_err(|error| error.to_string())?;
            (
                inner.config.settings.daily_times.clone(),
                inner.daily_armed_after_ms.clone(),
                inner
                    .inflight
                    .keys()
                    .cloned()
                    .collect::<std::collections::HashSet<_>>(),
            )
        };
        let date = today_string(now_ms);
        for target in targets
            .iter()
            .filter(|target| target.enabled && !target.blocked)
        {
            let armed_after = armed.get(&target.id).copied();
            for planned_time in &times {
                let Some(slot_ms) = local_slot_ms(&date, planned_time) else {
                    // Nonexistent local times (for example a spring-forward gap)
                    // are skipped rather than shifted.
                    continue;
                };
                if now_ms < slot_ms || now_ms >= slot_ms + DAILY_SLOT_WINDOW_MS {
                    continue;
                }
                if !daily_slot_eligible(slot_ms, armed_after) {
                    continue;
                }
                if busy.contains(&target.id) {
                    // A manual collection already covers this slot. Claiming it
                    // prevents a second collection after the manual result lands.
                    self.claim_daily_slot(&target.id, &date, planned_time, now_ms)?;
                    break;
                }
                let claimed = self.claim_daily_slot(&target.id, &date, planned_time, now_ms)?;
                if claimed {
                    match self.begin_collection(
                        target.clone(),
                        SessionLifetime::OneShot,
                        ServerMonitorScheduleMode::Daily,
                        None,
                    ) {
                        Ok(_) => {}
                        Err(error) if error == COLLECTION_LIMIT_ERROR => {
                            if let Ok(mut inner) = self.inner.lock() {
                                inner.daily_queue.push(DailyQueueEntry {
                                    target_id: target.id.clone(),
                                    slot_ms,
                                });
                            }
                        }
                        Err(error) => self.set_error(error),
                    }
                }
                break;
            }
        }
        Ok(())
    }

    /// Claims one daily slot, publishing and returning storage failures.
    fn claim_daily_slot(
        &self,
        target_id: &str,
        date: &str,
        planned_time: &str,
        now_ms: i64,
    ) -> Result<bool, String> {
        match self
            .store
            .claim_slot(target_id, date, planned_time, now_ms)
        {
            Ok(crate::server_monitor::types::SlotClaimOutcome::Claimed) => Ok(true),
            Ok(crate::server_monitor::types::SlotClaimOutcome::AlreadyClaimed) => Ok(false),
            Err(error) => {
                self.set_error(error.clone());
                Err(error)
            }
        }
    }

    fn set_error(&self, message: String) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.error = Some(message);
        }
        self.publish();
    }

    fn clear_error(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.error = None;
        }
    }

    fn settings_snapshot(&self) -> ServerMonitorSettings {
        self.inner
            .lock()
            .map(|inner| inner.config.settings.clone())
            .unwrap_or_default()
    }

    fn begin_collection(
        self: &Arc<Self>,
        target: ServerMonitorTargetRecord,
        lifetime: SessionLifetime,
        mode: ServerMonitorScheduleMode,
        expected_interval_seconds: Option<u64>,
    ) -> Result<Arc<InFlight>, String> {
        if self.shutting_down.load(Ordering::SeqCst) {
            return Err("Server monitor runtime is shutting down".to_string());
        }

        let (handle, session, generation, collected_at_ms) = {
            let mut inner = self.inner.lock().map_err(|error| error.to_string())?;
            if !inner
                .config
                .targets
                .iter()
                .any(|candidate| candidate.id == target.id)
            {
                return Err("Server monitor target not found".to_string());
            }
            if let Some(existing) = inner.inflight.get(&target.id) {
                return Ok(existing.clone());
            }
            if inner.active >= MAX_ACTIVE_COLLECTIONS {
                return Err(COLLECTION_LIMIT_ERROR.to_string());
            }
            let request = connect_request(&target)?;
            let generation = inner.generation.get(&target.id).copied().unwrap_or(0);
            let session = match lifetime {
                SessionLifetime::Persistent => inner
                    .sessions
                    .get(&target.id)
                    .cloned()
                    .unwrap_or_else(|| {
                        Arc::new(MonitorSession::new(
                            self.connector.clone(),
                            request.clone(),
                            SessionLifetime::Persistent,
                        ))
                    }),
                SessionLifetime::OneShot => Arc::new(MonitorSession::new(
                    self.connector.clone(),
                    request.clone(),
                    SessionLifetime::OneShot,
                )),
            };
            if lifetime == SessionLifetime::Persistent {
                inner.sessions.insert(target.id.clone(), session.clone());
            }
            let owned = inner.owned_sessions.entry(target.id.clone()).or_default();
            if !owned
                .iter()
                .any(|existing| Arc::ptr_eq(existing, &session))
            {
                owned.push(session.clone());
            }
            inner.active += 1;
            let handle = Arc::new(InFlight::new());
            inner.inflight.insert(target.id.clone(), handle.clone());
            (handle, session, generation, self.clock.now_ms())
        };

        let runtime = self.clone();
        let worker_handle = handle.clone();
        let worker_session = session.clone();
        std::thread::spawn(move || {
            let report = runtime.collect_and_store(
                &session,
                lifetime,
                mode,
                expected_interval_seconds,
                collected_at_ms,
            );
            runtime.finish_collection(
                &target.id,
                generation,
                worker_handle,
                report,
                &worker_session,
            );
        });
        Ok(handle)
    }

    fn collect_and_store(
        self: &Arc<Self>,
        session: &Arc<MonitorSession>,
        lifetime: SessionLifetime,
        mode: ServerMonitorScheduleMode,
        expected_interval_seconds: Option<u64>,
        collected_at_ms: i64,
    ) -> CollectionReport {
        if self.shutting_down.load(Ordering::SeqCst) {
            return CollectionReport::cancelled(collected_at_ms, mode);
        }
        let outcome = session.collect(None, collected_at_ms);
        let report = match outcome {
            Ok(CollectionOutcome::Success(snapshot)) => CollectionReport {
                snapshot: Some(snapshot.clone()),
                outcome: ServerMonitorOutcome::Success(snapshot),
                failure_kind: None,
                error: None,
                persistence_error: None,
                cancelled: false,
                collected_at_ms,
                source_mode: mode,
                expected_interval_seconds,
            },
            Ok(CollectionOutcome::Partial(snapshot)) => CollectionReport {
                snapshot: Some(snapshot.clone()),
                outcome: ServerMonitorOutcome::Partial(snapshot),
                failure_kind: None,
                error: None,
                persistence_error: None,
                cancelled: false,
                collected_at_ms,
                source_mode: mode,
                expected_interval_seconds,
            },
            Ok(CollectionOutcome::Failed(failure)) => CollectionReport {
                snapshot: None,
                outcome: ServerMonitorOutcome::Failed,
                failure_kind: Some(failure.kind),
                error: Some(failure.message),
                persistence_error: None,
                cancelled: false,
                collected_at_ms,
                source_mode: mode,
                expected_interval_seconds,
            },
            Err(CollectionError::Cancelled) => {
                CollectionReport::cancelled(collected_at_ms, mode)
            }
            Err(CollectionError::Transport(error)) => {
                let kind = transport_failure_kind(error.kind);
                CollectionReport {
                    snapshot: None,
                    outcome: ServerMonitorOutcome::Failed,
                    failure_kind: Some(kind),
                    error: Some(error.message),
                    persistence_error: None,
                    cancelled: false,
                    collected_at_ms,
                    source_mode: mode,
                    expected_interval_seconds,
                }
            }
        };
        if lifetime == SessionLifetime::OneShot {
            session.close();
        }
        report
    }

    fn finish_collection(
        self: &Arc<Self>,
        target_id: &str,
        generation: u64,
        handle: Arc<InFlight>,
        mut report: CollectionReport,
        session: &Arc<MonitorSession>,
    ) {
        let now = self.clock.now_ms();
        let mut prune: Option<(u32, i64, String)> = None;
        let mut failure_notification: Option<(String, String, String)> = None;

        if let Ok(mut inner) = self.inner.lock() {
            inner.active = inner.active.saturating_sub(1);
            inner.inflight.remove(target_id);
            let mut owned_empty = false;
            if let Some(sessions) = inner.owned_sessions.get_mut(target_id) {
                sessions.retain(|owned| !Arc::ptr_eq(owned, session));
                owned_empty = sessions.is_empty();
            }
            if owned_empty {
                inner.owned_sessions.remove(target_id);
            }
            let current_generation = inner.generation.get(target_id).copied().unwrap_or(0);
            let accepted = !report.cancelled
                && !self.shutting_down.load(Ordering::SeqCst)
                && current_generation == generation;
            if accepted {
                inner.error = None;
                let sample = report_sample(target_id, &report);
                if let Err(error) = self.store.append_sample(&sample) {
                    inner.error = Some(error.clone());
                    report.persistence_error = Some(error);
                }
                let mut config = inner.config.clone();
                if let Some(target) = config
                    .targets
                    .iter_mut()
                    .find(|target| target.id == target_id)
                {
                    if let Some(kind) = report.failure_kind {
                        target.blocked = kind.is_blocking();
                        target.last_error_kind = Some(kind);
                        target.last_error = report.error.clone();
                        let error_msg = report
                            .error
                            .clone()
                            .unwrap_or_else(|| "Unknown collection failure".to_string());
                        failure_notification =
                            Some((target.id.clone(), target.name.clone(), error_msg));
                    } else if report.snapshot.is_some() {
                        target.blocked = false;
                        target.last_error_kind = None;
                        target.last_error = None;
                        target.last_success_at_ms = Some(report.collected_at_ms);
                    }
                    target.updated_at_ms = now;
                }
                match self.store.save_config(&config) {
                    Ok(()) => inner.config = config,
                    Err(error) => {
                        log::error!("Server monitor status could not be persisted: {error}");
                        inner.error = Some(error.clone());
                        report.persistence_error = Some(error);
                    }
                }
                let mode = inner.config.settings.schedule_mode;
                let auto_enabled = inner.config.settings.auto_collect_enabled;
                let blocked = inner
                    .config
                    .targets
                    .iter()
                    .find(|target| target.id == target_id)
                    .map(|target| target.blocked)
                    .unwrap_or(false);
                if mode == ServerMonitorScheduleMode::Interval && auto_enabled && !blocked {
                    let interval_ms = i64::from(inner.config.settings.interval_minutes) * 60_000;
                    inner
                        .interval_due
                        .insert(target_id.to_string(), now + interval_ms);
                } else {
                    inner.interval_due.remove(target_id);
                }
                let retention = inner.config.settings.retention_days;
                prune = Some((retention, now, today_string(now)));
            } else {
                // A stale generation, deleted target or shutdown result must not
                // publish, persist or let a manual refresh observe success.
                inner.sessions.remove(target_id);
                report.snapshot = None;
                report.outcome = ServerMonitorOutcome::Failed;
                report.failure_kind = None;
                report.error = Some(CANCELLED_MESSAGE.to_string());
                report.cancelled = true;
            }
        }
        if let Some((target_id, target_name, error_msg)) = failure_notification {
            self.events.notify_failure(&target_id, &target_name, &error_msg);
        }
        handle.complete(report);
        if let Some((retention, now_ms, today)) = prune {
            let _ = self.store.prune(retention, now_ms, &today);
        }
        self.publish();
    }

    fn publish(&self) {
        self.events.publish(&self.snapshot());
    }

    pub(crate) fn configuration_error(&self) -> Option<String> {
        self.inner
            .lock()
            .ok()
            .filter(|inner| inner.config_broken)
            .map(|_| "Server monitor configuration is unreadable; fix or remove it first".to_string())
    }

    pub(crate) fn snapshot(&self) -> ServerMonitorState {
        let now = self.clock.now_ms();
        let inner = match self.inner.lock() {
            Ok(inner) => inner,
            Err(_) => {
                return ServerMonitorState {
                    settings: ServerMonitorSettings::default(),
                    targets: Vec::new(),
                    next_collection_at_ms: None,
                    last_success_at_ms: None,
                    error: Some("Server monitor state is unavailable".to_string()),
                }
            }
        };
        let error = if inner.config_broken {
            Some("Server monitor configuration is unreadable; fix or remove it first".to_string())
        } else {
            inner.error.clone()
        };
        let targets = inner
            .config
            .targets
            .iter()
            .map(|target| {
                target.view(
                    inner.inflight.contains_key(&target.id),
                    target_next_collection_at_ms(&inner, target, now),
                )
            })
            .collect::<Vec<_>>();
        let last_success_at_ms = inner
            .config
            .targets
            .iter()
            .filter_map(|target| target.last_success_at_ms)
            .max();
        let next_collection_at_ms = targets
            .iter()
            .filter_map(|target| target.next_collection_at_ms)
            .min();
        ServerMonitorState {
            settings: inner.config.settings.clone(),
            targets,
            next_collection_at_ms,
            last_success_at_ms,
            error,
        }
    }

    pub(crate) fn history(
        &self,
        target_id: &str,
        range: ServerMonitorHistoryRange,
    ) -> Result<Vec<ServerMonitorSample>, String> {
        let to_ms = self.clock.now_ms();
        let from_ms = to_ms.saturating_sub(range.window_ms());
        self.store.history(target_id, from_ms, to_ms)
    }

    pub(crate) fn upsert_target(
        self: &Arc<Self>,
        draft: ServerMonitorTargetDraft,
    ) -> Result<ServerMonitorState, String> {
        let now = self.clock.now_ms();
        let mut inner = self.inner.lock().map_err(|error| error.to_string())?;
        if inner.config_broken {
            return Err("Server monitor configuration is unreadable; fix or remove it first".to_string());
        }
        let mut config = inner.config.clone();
        let existing_index = draft
            .id
            .as_ref()
            .and_then(|id| config.targets.iter().position(|target| &target.id == id));
        let existing = existing_index.map(|index| config.targets[index].clone());

        let id = draft
            .id
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let name = draft.name.trim().to_string();
        let custom = draft
            .custom
            .as_ref()
            .map(|custom| build_custom_record(custom, existing.as_ref()));
        let record = ServerMonitorTargetRecord {
            id: id.clone(),
            name,
            enabled: draft.enabled,
            source_kind: draft.source_kind,
            saved_host_name: draft
                .saved_host_name
                .as_ref()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            custom,
            created_at_ms: existing
                .as_ref()
                .map(|record| record.created_at_ms)
                .unwrap_or(now),
            updated_at_ms: now,
            // Any accepted edit clears the previous error and retries explicitly.
            blocked: false,
            last_error_kind: None,
            last_error: None,
            last_success_at_ms: existing
                .as_ref()
                .and_then(|record| record.last_success_at_ms),
        };
        validate_target(&record)?;
        if config
            .targets
            .iter()
            .enumerate()
            .any(|(index, target)| Some(index) != existing_index && target.id == record.id)
        {
            return Err("Target id must be unique".to_string());
        }

        let source_changed = existing
            .as_ref()
            .map(|previous| !same_source(previous, &record))
            .unwrap_or(false);
        let was_disabled = existing
            .as_ref()
            .map(|previous| !previous.enabled)
            .unwrap_or(true);
        let was_blocked = existing
            .as_ref()
            .map(|previous| previous.blocked)
            .unwrap_or(false);
        let disabling = existing
            .as_ref()
            .map(|previous| previous.enabled && !record.enabled)
            .unwrap_or(false);
        if let Some(index) = existing_index {
            config.targets[index] = record.clone();
        } else {
            config.targets.push(record.clone());
        }
        self.store.save_config(&config)?;

        inner.config = config;
        inner.error = None;
        let reschedule =
            existing.is_none() || source_changed || was_disabled || was_blocked || disabling;
        if source_changed || disabling {
            *inner.generation.entry(id.clone()).or_insert(0) += 1;
            invalidate_target_sessions(&mut inner, &id);
            inner
                .daily_queue
                .retain(|entry| entry.target_id != id);
        }
        let mode = inner.config.settings.schedule_mode;
        let auto_enabled = inner.config.settings.auto_collect_enabled;
        if disabling || !record.enabled {
            inner.interval_due.remove(&id);
            inner.daily_armed_after_ms.remove(&id);
        } else {
            match mode {
                ServerMonitorScheduleMode::Interval if auto_enabled => {
                    if reschedule {
                        inner.interval_due.insert(id.clone(), now);
                    }
                }
                ServerMonitorScheduleMode::Interval => {
                    inner.interval_due.remove(&id);
                }
                ServerMonitorScheduleMode::Daily if auto_enabled => {
                    if reschedule {
                        inner.daily_armed_after_ms.insert(id.clone(), now);
                    }
                }
                ServerMonitorScheduleMode::Daily => {}
            }
        }
        drop(inner);
        self.publish();
        Ok(self.snapshot())
    }

    pub(crate) fn delete_target(
        self: &Arc<Self>,
        target_id: &str,
    ) -> Result<ServerMonitorState, String> {
        let mut inner = self.inner.lock().map_err(|error| error.to_string())?;
        if inner.config_broken {
            return Err("Server monitor configuration is unreadable; fix or remove it first".to_string());
        }
        let mut config = inner.config.clone();
        let before = config.targets.len();
        config.targets.retain(|target| target.id != target_id);
        if config.targets.len() == before {
            return Ok(self.snapshot());
        }
        self.store.save_config(&config)?;
        inner.config = config;
        inner.error = None;
        inner.interval_due.remove(target_id);
        inner.daily_armed_after_ms.remove(target_id);
        inner
            .daily_queue
            .retain(|entry| entry.target_id != target_id);
        inner.inflight.remove(target_id);
        *inner.generation.entry(target_id.to_string()).or_insert(0) += 1;
        invalidate_target_sessions(&mut inner, target_id);
        drop(inner);
        self.store.delete_target_data(target_id)?;
        self.publish();
        Ok(self.snapshot())
    }

    pub(crate) fn set_target_enabled(
        self: &Arc<Self>,
        target_id: &str,
        enabled: bool,
    ) -> Result<ServerMonitorState, String> {
        let now = self.clock.now_ms();
        let mut inner = self.inner.lock().map_err(|error| error.to_string())?;
        if inner.config_broken {
            return Err("Server monitor configuration is unreadable; fix or remove it first".to_string());
        }
        let mut config = inner.config.clone();
        let Some(target) = config
            .targets
            .iter_mut()
            .find(|target| target.id == target_id)
        else {
            return Err("Server monitor target not found".to_string());
        };
        target.enabled = enabled;
        target.updated_at_ms = now;
        if enabled {
            // Resuming is an explicit retry: clear the prior block state.
            target.blocked = false;
            target.last_error_kind = None;
            target.last_error = None;
        }
        self.store.save_config(&config)?;
        inner.config = config;
        inner.error = None;
        if enabled {
            let mode = inner.config.settings.schedule_mode;
            let auto_enabled = inner.config.settings.auto_collect_enabled;
            inner
                .daily_queue
                .retain(|entry| entry.target_id != target_id);
            match mode {
                ServerMonitorScheduleMode::Interval if auto_enabled => {
                    inner.interval_due.insert(target_id.to_string(), now);
                }
                ServerMonitorScheduleMode::Interval => {
                    inner.interval_due.remove(target_id);
                }
                ServerMonitorScheduleMode::Daily if auto_enabled => {
                    inner
                        .daily_armed_after_ms
                        .insert(target_id.to_string(), now);
                }
                ServerMonitorScheduleMode::Daily => {}
            }
        } else {
            inner.interval_due.remove(target_id);
            inner.daily_armed_after_ms.remove(target_id);
            inner
                .daily_queue
                .retain(|entry| entry.target_id != target_id);
            *inner.generation.entry(target_id.to_string()).or_insert(0) += 1;
            invalidate_target_sessions(&mut inner, target_id);
        }
        drop(inner);
        self.publish();
        Ok(self.snapshot())
    }

    pub(crate) fn refresh_target(
        self: &Arc<Self>,
        target_id: &str,
    ) -> Result<ServerMonitorState, String> {
        let now = self.clock.now_ms();
        let (target, lifetime, mode, expected, daily_slot) = {
            let inner = self.inner.lock().map_err(|error| error.to_string())?;
            let target = inner
                .config
                .targets
                .iter()
                .find(|target| target.id == target_id)
                .cloned()
                .ok_or_else(|| "Server monitor target not found".to_string())?;
            let settings = inner.config.settings.clone();
            let persistent = settings.schedule_mode == ServerMonitorScheduleMode::Interval
                && settings.auto_collect_enabled
                && target.enabled;
            let daily_slot = if settings.schedule_mode == ServerMonitorScheduleMode::Daily
                && settings.auto_collect_enabled
                && target.enabled
            {
                let date = today_string(now);
                let armed_after = inner
                    .daily_armed_after_ms
                    .get(&target.id)
                    .copied();
                settings.daily_times.iter().find_map(|planned_time| {
                    let slot_ms = local_slot_ms(&date, planned_time)?;
                    if now < slot_ms || now >= slot_ms + DAILY_SLOT_WINDOW_MS {
                        return None;
                    }
                    if !daily_slot_eligible(slot_ms, armed_after) {
                        return None;
                    }
                    Some((date.clone(), planned_time.clone()))
                })
            } else {
                None
            };
            (
                target,
                if persistent {
                    SessionLifetime::Persistent
                } else {
                    SessionLifetime::OneShot
                },
                settings.schedule_mode,
                match settings.schedule_mode {
                    ServerMonitorScheduleMode::Interval => {
                        Some(u64::from(settings.interval_minutes * 60))
                    }
                    ServerMonitorScheduleMode::Daily => None,
                },
                daily_slot,
            )
        };
        if let Some((date, planned_time)) = daily_slot {
            // A manual collection colliding with a due daily slot claims that
            // slot and its result is the one attempt for the target/date/time.
            self.claim_daily_slot(target_id, &date, &planned_time, now)?;
        }
        let handle = self.begin_collection(target, lifetime, mode, expected)?;
        let report = handle
            .wait()
            .ok_or_else(|| "Server monitor collection did not complete".to_string())?;
        if let Some(error) = report.persistence_error {
            return Err(error);
        }
        if report.cancelled {
            return Err("Server monitor collection was cancelled".to_string());
        }
        self.clear_error();
        self.publish();
        Ok(self.snapshot())
    }

    pub(crate) fn test_connection(
        &self,
        draft: ServerMonitorTargetDraft,
    ) -> Result<ServerMonitorTestResult, String> {
        let now = self.clock.now_ms();
        let existing = {
            let inner = self.inner.lock().map_err(|error| error.to_string())?;
            draft
                .id
                .as_ref()
                .filter(|id| !id.trim().is_empty())
                .and_then(|id| {
                    inner
                        .config
                        .targets
                        .iter()
                        .find(|target| &target.id == id)
                        .cloned()
                })
        };
        // A draft test resolves the stored password for an existing target and
        // validates the submitted connection fields without persisting anything.
        let record = draft_record(&draft, existing.as_ref(), now)?;
        validate_target(&record)?;
        let request = connect_request(&record)?;
        let session = MonitorSession::new(self.connector.clone(), request, SessionLifetime::OneShot);
        let outcome = session.collect(None, now);
        session.close();
        Ok(match outcome {
            Ok(CollectionOutcome::Success(snapshot)) => ServerMonitorTestResult {
                succeeded: true,
                partial: false,
                error_kind: None,
                error: None,
                snapshot: Some(snapshot),
            },
            Ok(CollectionOutcome::Partial(snapshot)) => ServerMonitorTestResult {
                succeeded: true,
                partial: true,
                error_kind: None,
                error: None,
                snapshot: Some(snapshot),
            },
            Ok(CollectionOutcome::Failed(failure)) => ServerMonitorTestResult {
                succeeded: false,
                partial: false,
                error_kind: Some(failure.kind),
                error: Some(failure.message),
                snapshot: None,
            },
            Err(CollectionError::Transport(error)) => ServerMonitorTestResult {
                succeeded: false,
                partial: false,
                error_kind: Some(transport_failure_kind(error.kind)),
                error: Some(error.message),
                snapshot: None,
            },
            Err(CollectionError::Cancelled) => ServerMonitorTestResult {
                succeeded: false,
                partial: false,
                error_kind: None,
                error: Some("Connection test was cancelled".to_string()),
                snapshot: None,
            },
        })
    }

    pub(crate) fn get_settings(&self) -> Result<ServerMonitorSettings, String> {
        self.inner
            .lock()
            .map(|inner| inner.config.settings.clone())
            .map_err(|error| error.to_string())
    }

    pub(crate) fn save_settings(
        self: &Arc<Self>,
        settings: ServerMonitorSettings,
    ) -> Result<ServerMonitorState, String> {
        settings.validate()?;
        let now = self.clock.now_ms();
        let mut inner = self.inner.lock().map_err(|error| error.to_string())?;
        if inner.config_broken {
            return Err("Server monitor configuration is unreadable; fix or remove it first".to_string());
        }
        let mut config = inner.config.clone();
        let previous = config.settings.clone();
        config.settings = settings.clone();
        self.store.save_config(&config)?;
        inner.config = config;
        inner.error = None;
        let mode_changed = previous.schedule_mode != settings.schedule_mode;
        let auto_disabled = previous.auto_collect_enabled && !settings.auto_collect_enabled;
        let auto_enabled = !previous.auto_collect_enabled && settings.auto_collect_enabled;
        let interval_changed = previous.interval_minutes != settings.interval_minutes;
        if mode_changed || auto_disabled || !settings.auto_collect_enabled {
            let persistent = inner
                .sessions
                .drain()
                .map(|(_, session)| session)
                .collect::<Vec<_>>();
            let owned = inner
                .owned_sessions
                .drain()
                .flat_map(|(_, sessions)| sessions)
                .collect::<Vec<_>>();
            inner.interval_due.clear();
            inner.daily_armed_after_ms.clear();
            inner.daily_queue.clear();
            for id in all_target_ids(&inner.config) {
                *inner.generation.entry(id).or_insert(0) += 1;
            }
            drop(inner);
            for session in persistent.into_iter().chain(owned) {
                session.invalidate();
            }
            let mut inner = self.inner.lock().map_err(|error| error.to_string())?;
            if settings.auto_collect_enabled {
                match settings.schedule_mode {
                    ServerMonitorScheduleMode::Interval => {
                        for id in scheduled_target_ids(&inner.config) {
                            inner.interval_due.insert(id, now);
                        }
                    }
                    ServerMonitorScheduleMode::Daily => {
                        for id in all_target_ids(&inner.config) {
                            inner.daily_armed_after_ms.insert(id, now);
                        }
                    }
                }
            }
            drop(inner);
        } else {
            match settings.schedule_mode {
                ServerMonitorScheduleMode::Interval => {
                    if auto_enabled {
                        for id in scheduled_target_ids(&inner.config) {
                            inner.interval_due.insert(id, now);
                        }
                    } else if interval_changed {
                        let next = now + i64::from(settings.interval_minutes) * 60_000;
                        for id in scheduled_target_ids(&inner.config) {
                            inner.interval_due.insert(id, next);
                        }
                    }
                }
                ServerMonitorScheduleMode::Daily => {
                    if settings.auto_collect_enabled {
                        for id in all_target_ids(&inner.config) {
                            inner.daily_armed_after_ms.insert(id, now);
                        }
                    }
                    inner.daily_queue.clear();
                }
            }
            drop(inner);
        }
        let today = today_string(now);
        let _ = self.store.prune(settings.retention_days, now, &today);
        self.publish();
        Ok(self.snapshot())
    }

    /// Returns after in-flight work has drained or the bounded wait elapses.
    #[cfg(test)]
    pub(crate) fn wait_for_idle(&self) {
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        loop {
            let idle = self
                .inner
                .lock()
                .map(|inner| inner.active == 0 && inner.inflight.is_empty())
                .unwrap_or(true);
            if idle || std::time::Instant::now() >= deadline {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

fn report_sample(target_id: &str, report: &CollectionReport) -> ServerMonitorSample {
    let snapshot = report.snapshot.as_ref();
    let sampling_window_seconds = snapshot
        .map(|snapshot| snapshot.sampling_window_seconds)
        .unwrap_or(0.0);
    ServerMonitorSample {
        id: None,
        target_id: target_id.to_string(),
        collected_at_ms: report.collected_at_ms,
        outcome: match &report.outcome {
            ServerMonitorOutcome::Success(snapshot) => {
                ServerMonitorOutcome::Success(snapshot.clone())
            }
            ServerMonitorOutcome::Partial(snapshot) => {
                ServerMonitorOutcome::Partial(snapshot.clone())
            }
            ServerMonitorOutcome::Failed => ServerMonitorOutcome::Failed,
        },
        sampling_window_seconds,
        source_mode: report.source_mode,
        expected_interval_seconds: report.expected_interval_seconds,
        cpu: snapshot.and_then(|snapshot| snapshot.cpu.clone()),
        memory: snapshot.and_then(|snapshot| snapshot.memory.clone()),
        disks: snapshot.and_then(|snapshot| snapshot.disks.clone()),
        networks: snapshot
            .map(|snapshot| snapshot.networks.clone())
            .unwrap_or_default(),
        network_interface: snapshot.and_then(|snapshot| snapshot.network_interface.clone()),
        error_kind: report.failure_kind,
        error: report.error.clone(),
    }
}

/// Invalidates and closes every session currently owned for one target.
fn invalidate_target_sessions(inner: &mut Inner, target_id: &str) {
    if let Some(session) = inner.sessions.remove(target_id) {
        session.invalidate();
    }
    if let Some(sessions) = inner.owned_sessions.remove(target_id) {
        for session in sessions {
            session.invalidate();
        }
    }
}

/// Scheduled time for one target, or `None` when the target is not scheduled.
fn target_next_collection_at_ms(
    inner: &Inner,
    target: &ServerMonitorTargetRecord,
    now_ms: i64,
) -> Option<i64> {
    if !inner.config.settings.auto_collect_enabled || !target.enabled || target.blocked {
        return None;
    }
    match inner.config.settings.schedule_mode {
        ServerMonitorScheduleMode::Interval => {
            inner.interval_due.get(&target.id).copied()
        }
        ServerMonitorScheduleMode::Daily => {
            let armed_after = inner
                .daily_armed_after_ms
                .get(&target.id)
                .copied()
                .unwrap_or(i64::MIN);
            let now = Local.timestamp_millis_opt(now_ms).single()?;
            let today = now.date_naive();
            let mut best: Option<i64> = None;
            for offset in 0..=1 {
                let date = today + chrono::Duration::days(offset);
                let date_text = date.format("%Y-%m-%d").to_string();
                for time in &inner.config.settings.daily_times {
                    let Some(slot) = local_slot_ms(&date_text, time) else {
                        continue;
                    };
                    if slot <= now_ms
                        || !daily_slot_eligible(slot, Some(armed_after))
                    {
                        continue;
                    }
                    if best.map(|value| slot < value).unwrap_or(true) {
                        best = Some(slot);
                    }
                }
            }
            best
        }
    }
}

/// A slot runs only when it starts strictly after the target was armed, so
/// arming at or after a slot's start waits for the next scheduled time instead
/// of replaying a slot that already started.
fn daily_slot_eligible(slot_ms: i64, armed_after_ms: Option<i64>) -> bool {
    match armed_after_ms {
        Some(armed_after) => slot_ms > armed_after,
        None => true,
    }
}

fn scheduled_target_ids(config: &ServerMonitorConfig) -> Vec<String> {
    config
        .targets
        .iter()
        .filter(|target| target.enabled && !target.blocked)
        .map(|target| target.id.clone())
        .collect()
}

fn all_target_ids(config: &ServerMonitorConfig) -> Vec<String> {
    config
        .targets
        .iter()
        .map(|target| target.id.clone())
        .collect()
}

fn build_custom_record(
    draft: &ServerMonitorCustomDraft,
    existing: Option<&ServerMonitorTargetRecord>,
) -> ServerMonitorCustomRecord {
    let preserved_password = existing
        .and_then(|record| record.custom.as_ref())
        .and_then(|custom| custom.password.clone())
        .filter(|password| !password.is_empty());
    let password = if draft.auth_kind == crate::server_monitor::types::ServerMonitorAuthKind::Password
    {
        draft
            .password
            .clone()
            .filter(|password| !password.is_empty())
            .or(preserved_password)
    } else {
        None
    };
    ServerMonitorCustomRecord {
        host: draft.host.trim().to_string(),
        port: draft.port,
        user: draft.user.trim().to_string(),
        auth_kind: draft.auth_kind,
        key_path: draft
            .key_path
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        password,
    }
}

fn draft_record(
    draft: &ServerMonitorTargetDraft,
    existing: Option<&ServerMonitorTargetRecord>,
    now: i64,
) -> Result<ServerMonitorTargetRecord, String> {
    Ok(ServerMonitorTargetRecord {
        id: draft
            .id
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        name: draft.name.trim().to_string(),
        enabled: draft.enabled,
        source_kind: draft.source_kind,
        saved_host_name: draft
            .saved_host_name
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        custom: draft
            .custom
            .as_ref()
            .map(|custom| build_custom_record(custom, existing)),
        created_at_ms: now,
        updated_at_ms: now,
        blocked: false,
        last_error_kind: None,
        last_error: None,
        last_success_at_ms: existing.and_then(|record| record.last_success_at_ms),
    })
}

fn validate_target(record: &ServerMonitorTargetRecord) -> Result<(), String> {
    let name = record.name.trim();
    if name.is_empty() || name.chars().count() > 80 {
        return Err("Server name must be 1 to 80 characters".to_string());
    }
    if record.id.trim().is_empty() {
        return Err("Server id is required".to_string());
    }
    match record.source_kind {
        ServerMonitorSourceKind::SavedHost => {
            if record
                .saved_host_name
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_none()
            {
                return Err("Select an existing SSH host".to_string());
            }
        }
        ServerMonitorSourceKind::Custom => {
            let custom = record
                .custom
                .as_ref()
                .ok_or_else(|| "Custom connection details are required".to_string())?;
            if custom.host.trim().is_empty() {
                return Err("SSH host is required".to_string());
            }
            if custom.user.trim().is_empty() {
                return Err("SSH user is required".to_string());
            }
            if custom.port == 0 {
                return Err("SSH port must be from 1 to 65535".to_string());
            }
            match custom.auth_kind {
                crate::server_monitor::types::ServerMonitorAuthKind::Key => {
                    if custom
                        .key_path
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .is_none()
                    {
                        return Err("Key authentication requires a key path".to_string());
                    }
                }
                crate::server_monitor::types::ServerMonitorAuthKind::Password => {
                    if custom
                        .password
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .is_none()
                    {
                        return Err("Password authentication requires a password".to_string());
                    }
                }
                crate::server_monitor::types::ServerMonitorAuthKind::Agent => {
                    return Err("Agent authentication requires a saved SSH host".to_string());
                }
            }
        }
    }
    Ok(())
}

fn same_source(left: &ServerMonitorTargetRecord, right: &ServerMonitorTargetRecord) -> bool {
    if left.source_kind != right.source_kind || left.saved_host_name != right.saved_host_name {
        return false;
    }
    match (&left.custom, &right.custom) {
        (None, None) => true,
        (Some(left), Some(right)) => {
            left.host == right.host
                && left.port == right.port
                && left.user == right.user
                && left.auth_kind == right.auth_kind
                && left.key_path == right.key_path
                && left.password == right.password
        }
        _ => false,
    }
}

fn connect_request(target: &ServerMonitorTargetRecord) -> Result<MonitorConnectRequest, String> {
    let source = match target.source_kind {
        ServerMonitorSourceKind::SavedHost => MonitorSource::SavedHost {
            alias: target
                .saved_host_name
                .clone()
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| "Saved SSH host alias is missing".to_string())?,
        },
        ServerMonitorSourceKind::Custom => {
            let custom = target
                .custom
                .as_ref()
                .ok_or_else(|| "Custom connection details are missing".to_string())?;
            let auth = match custom.auth_kind {
                crate::server_monitor::types::ServerMonitorAuthKind::Password => {
                    MonitorAuth::Password
                }
                crate::server_monitor::types::ServerMonitorAuthKind::Key => MonitorAuth::Key {
                    path: custom.key_path.clone().unwrap_or_default(),
                },
                crate::server_monitor::types::ServerMonitorAuthKind::Agent => MonitorAuth::Agent,
            };
            MonitorSource::Custom {
                host: custom.host.clone(),
                port: custom.port,
                user: custom.user.clone(),
                auth,
            }
        }
    };
    Ok(MonitorConnectRequest {
        target_id: target.id.clone(),
        source,
        password: target
            .custom
            .as_ref()
            .and_then(|custom| custom.password.clone()),
    })
}

fn transport_failure_kind(kind: MonitorTransportErrorKind) -> CollectionFailureKind {
    match kind {
        MonitorTransportErrorKind::Transport => CollectionFailureKind::Transport,
        MonitorTransportErrorKind::Auth => CollectionFailureKind::Auth,
        MonitorTransportErrorKind::HostKey => CollectionFailureKind::HostKey,
        MonitorTransportErrorKind::Config => CollectionFailureKind::Config,
        MonitorTransportErrorKind::Timeout => CollectionFailureKind::Timeout,
    }
}

fn today_string(now_ms: i64) -> String {
    Local
        .timestamp_millis_opt(now_ms)
        .single()
        .map(|time| time.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "1970-01-01".to_string())
}

fn local_slot_ms(date: &str, time: &str) -> Option<i64> {
    let date = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    let mut parts = time.split(':');
    let hour = parts.next()?.parse::<u32>().ok()?;
    let minute = parts.next()?.parse::<u32>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    match Local.with_ymd_and_hms(date.year(), date.month(), date.day(), hour, minute, 0) {
        LocalResult::Single(value) => Some(value.timestamp_millis()),
        LocalResult::Ambiguous(earlier, _) => Some(earlier.timestamp_millis()),
        LocalResult::None => None,
    }
}
