//! Runtime state, events, clocks and snapshots shared by the scheduler and
//! collector.

use crate::server_monitor::storage::{ServerMonitorConfig, ServerMonitorStore};
use crate::server_monitor::transport::{MonitorConnector, MonitorSession};
use crate::server_monitor::types::{
    CollectionFailureKind, MetricSnapshot, ServerMonitorOutcome, ServerMonitorSample,
    ServerMonitorScheduleMode, ServerMonitorSettings, ServerMonitorState, ServerMonitorTargetRecord,
};
use chrono::{Datelike, Local, LocalResult, TimeZone};
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::Duration;

pub(super) const CANCELLED_MESSAGE: &str = "Collection was cancelled";

/// Event boundary for safe state publication.
pub(crate) trait ServerMonitorEventSink: Send + Sync {
    fn publish(&self, state: &ServerMonitorState);
    fn notify_failure(&self, target_id: &str, target_name: &str, error_message: &str);
    fn notify_alert(&self, target_id: &str, target_name: &str, alert_reasons: &[String]);
}

#[cfg(test)]
pub(crate) struct NoopEventSink;

#[cfg(test)]
impl ServerMonitorEventSink for NoopEventSink {
    fn publish(&self, _state: &ServerMonitorState) {}
    fn notify_failure(&self, _target_id: &str, _target_name: &str, _error_message: &str) {}
    fn notify_alert(&self, _target_id: &str, _target_name: &str, _alert_reasons: &[String]) {}
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

    fn notify_alert(&self, target_id: &str, target_name: &str, alert_reasons: &[String]) {
        let summary_text = alert_reasons.join(", ");
        let detail_text = format!(
            "服务器 [{}] 检测到以下资源使用率超过告警阈值:\n{}",
            target_name,
            alert_reasons
                .iter()
                .map(|r| format!("- {}", r))
                .collect::<Vec<_>>()
                .join("\n")
        );
        let input = crate::messages::MessageCreateInput {
            source: "server_monitor".to_string(),
            category: "resource_alert".to_string(),
            severity: "warning".to_string(),
            title: format!("服务器监控告警 - {}", target_name),
            summary: Some(format!("服务器 [{}] 出现资源告警: {}", target_name, summary_text)),
            detail: Some(detail_text),
            dedupe_key: Some(format!("server_monitor_alert:{}", target_id)),
            target: Some(crate::messages::MessageTarget {
                tab: "more-tools".to_string(),
                section: Some("server-monitor".to_string()),
                entity_id: Some(target_id.to_string()),
            }),
            metadata: Some(serde_json::json!({
                "target_id": target_id,
                "target_name": target_name,
                "reasons": alert_reasons,
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

pub(super) fn evaluate_alert(
    snapshot: &MetricSnapshot,
    settings: &ServerMonitorSettings,
) -> (bool, Vec<String>) {
    if !settings.alert_enabled {
        return (false, Vec::new());
    }
    let mut reasons = Vec::new();
    if let Some(ref cpu) = snapshot.cpu {
        if cpu.usage_percent >= settings.cpu_threshold_percent as f64 {
            reasons.push(format!(
                "CPU 使用率 {:.1}% (阈值 {}%)",
                cpu.usage_percent, settings.cpu_threshold_percent
            ));
        }
    }
    if let Some(ref memory) = snapshot.memory {
        if memory.used_percent >= settings.memory_threshold_percent as f64 {
            reasons.push(format!(
                "内存使用率 {:.1}% (阈值 {}%)",
                memory.used_percent, settings.memory_threshold_percent
            ));
        }
    }
    if let Some(ref disks) = snapshot.disks {
        for disk in disks {
            if disk.used_percent >= settings.disk_threshold_percent as f64 {
                reasons.push(format!(
                    "磁盘 [{}] 使用率 {:.1}% (阈值 {}%)",
                    disk.mount_point, disk.used_percent, settings.disk_threshold_percent
                ));
            }
        }
    }
    let has_alert = !reasons.is_empty();
    (has_alert, reasons)
}

pub(super) struct Inner {
    pub(super) config: ServerMonitorConfig,
    pub(super) config_broken: bool,
    /// Persistent interval sessions, reused by later collections.
    pub(super) sessions: HashMap<String, Arc<MonitorSession>>,
    /// Every owned session per target, including one-shot sessions.
    pub(super) owned_sessions: HashMap<String, Vec<Arc<MonitorSession>>>,
    pub(super) inflight: HashMap<String, Arc<InFlight>>,
    pub(super) interval_due: HashMap<String, i64>,
    pub(super) daily_armed_after_ms: HashMap<String, i64>,
    pub(super) daily_queue: Vec<DailyQueueEntry>,
    pub(super) last_queue_tick_ms: Option<i64>,
    pub(super) active: usize,
    pub(super) generation: HashMap<String, u64>,
    pub(super) error: Option<String>,
}

/// A claimed daily slot waiting for a free collection slot inside its minute.
#[derive(Debug, Clone)]
pub(super) struct DailyQueueEntry {
    pub(super) target_id: String,
    pub(super) slot_ms: i64,
}

pub(super) struct InFlight {
    result: Mutex<Option<CollectionReport>>,
    ready: Condvar,
}

impl InFlight {
    pub(super) fn new() -> Self {
        Self {
            result: Mutex::new(None),
            ready: Condvar::new(),
        }
    }

    pub(super) fn complete(&self, report: CollectionReport) {
        if let Ok(mut result) = self.result.lock() {
            *result = Some(report);
        }
        self.ready.notify_all();
    }

    pub(super) fn wait(&self) -> Option<CollectionReport> {
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
    pub(super) fn cancelled(collected_at_ms: i64, source_mode: ServerMonitorScheduleMode) -> Self {
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
    pub(super) store: Arc<ServerMonitorStore>,
    pub(super) connector: Arc<dyn MonitorConnector>,
    pub(super) events: Arc<dyn ServerMonitorEventSink>,
    pub(super) clock: Arc<dyn ServerMonitorClock>,
    pub(super) inner: Mutex<Inner>,
    pub(super) started: AtomicBool,
    pub(super) shutting_down: AtomicBool,
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
        let (mut config, config_broken) = match store.load_config() {
            Ok(config) => (config, false),
            Err(error) => {
                log::error!("Server monitor configuration could not be loaded: {error}");
                (ServerMonitorConfig::default(), true)
            }
        };
        let mut alert_state_changed = false;
        for target in &mut config.targets {
            if let Ok(Some(snapshot)) = store.latest_snapshot(&target.id) {
                let (has_alert, reasons) = evaluate_alert(&snapshot, &config.settings);
                if target.has_alert != has_alert || target.alert_reasons != reasons {
                    target.has_alert = has_alert;
                    target.alert_reasons = reasons;
                    alert_state_changed = true;
                }
            }
        }
        if alert_state_changed && !config_broken {
            let _ = store.save_config(&config);
        }
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

    pub(super) fn set_error(&self, message: String) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.error = Some(message);
        }
        self.publish();
    }

    pub(super) fn clear_error(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.error = None;
        }
    }

    pub(super) fn publish(&self) {
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
        range: super::ServerMonitorHistoryRange,
    ) -> Result<Vec<ServerMonitorSample>, String> {
        let to_ms = self.clock.now_ms();
        let from_ms = to_ms.saturating_sub(range.window_ms());
        self.store.history(target_id, from_ms, to_ms)
    }

    pub(crate) fn get_settings(&self) -> Result<ServerMonitorSettings, String> {
        self.inner
            .lock()
            .map(|inner| inner.config.settings.clone())
            .map_err(|error| error.to_string())
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

pub(super) fn report_sample(target_id: &str, report: &CollectionReport) -> ServerMonitorSample {
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
pub(super) fn invalidate_target_sessions(inner: &mut Inner, target_id: &str) {
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
        ServerMonitorScheduleMode::Interval => inner.interval_due.get(&target.id).copied(),
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
                    if slot <= now_ms || !daily_slot_eligible(slot, Some(armed_after)) {
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
pub(super) fn daily_slot_eligible(slot_ms: i64, armed_after_ms: Option<i64>) -> bool {
    match armed_after_ms {
        Some(armed_after) => slot_ms > armed_after,
        None => true,
    }
}

pub(super) fn today_string(now_ms: i64) -> String {
    Local
        .timestamp_millis_opt(now_ms)
        .single()
        .map(|time| time.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "1970-01-01".to_string())
}

pub(super) fn local_slot_ms(date: &str, time: &str) -> Option<i64> {
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
