use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::{watch, Semaphore};

use super::alerter::dispatch_transition_alert;
use super::prober::execute_probe;
use super::state_machine::{StateMachineRegistry, StateTransition};
use super::storage::{
    create_incident, get_active_incident, get_recent_heartbeats, get_uptime_metrics,
    insert_heartbeat, load_monitors, open_uptime_db, prune_old_records, resolve_incident,
};
use super::types::{MonitorStatus, MonitorSummary};

static SCHEDULER: OnceLock<Arc<UptimeScheduler>> = OnceLock::new();
static APP_HANDLE: OnceLock<std::sync::Mutex<Option<tauri::AppHandle>>> = OnceLock::new();

fn app_handle_slot() -> &'static std::sync::Mutex<Option<tauri::AppHandle>> {
    APP_HANDLE.get_or_init(|| std::sync::Mutex::new(None))
}

pub fn capture_app_handle(app: tauri::AppHandle) {
    let mut guard = app_handle_slot()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    *guard = Some(app);
}

pub fn captured_app_handle() -> Option<tauri::AppHandle> {
    app_handle_slot()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone()
}

pub struct UptimeScheduler {
    state_machine: Arc<StateMachineRegistry>,
    semaphore: Arc<Semaphore>,
    #[allow(dead_code)]
    stop_tx: watch::Sender<bool>,
}

impl UptimeScheduler {
    pub fn init_global(app: tauri::AppHandle) -> Arc<Self> {
        capture_app_handle(app);
        Self::global()
    }

    pub fn global() -> Arc<Self> {
        SCHEDULER
            .get_or_init(|| {
                let (stop_tx, stop_rx) = watch::channel(false);
                let scheduler = Arc::new(Self {
                    state_machine: Arc::new(StateMachineRegistry::new()),
                    semaphore: Arc::new(Semaphore::new(10)),
                    stop_tx,
                });
                scheduler.clone().start_background_loop(stop_rx);
                scheduler
            })
            .clone()
    }

    pub fn state_machine(&self) -> Arc<StateMachineRegistry> {
        self.state_machine.clone()
    }

    fn start_background_loop(self: Arc<Self>, mut stop_rx: watch::Receiver<bool>) {
        tauri::async_runtime::spawn(async move {
            // 启动时自动从 SQLite 中读取正在进行中的 active incident 预热状态机，防止应用重启引起的假新事件/重复警报
            if let Ok(conn) = open_uptime_db() {
                if let Ok(monitors) = load_monitors() {
                    for m in monitors {
                        if let Ok(Some(inc)) = get_active_incident(&conn, &m.id) {
                            self.state_machine.hydrate_active_incident(&m.id, inc.started_at, &inc.cause);
                        }
                    }
                }
            }

            let mut prune_timer = tokio::time::interval(Duration::from_secs(3600));
            let mut tick_timer = tokio::time::interval(Duration::from_secs(5));

            loop {
                tokio::select! {
                    _ = stop_rx.changed() => {
                        if *stop_rx.borrow() {
                            break;
                        }
                    }
                    _ = prune_timer.tick() => {
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs() as i64;
                        if let Ok(conn) = open_uptime_db() {
                            let _ = prune_old_records(&conn, 30, now);
                        }
                    }
                    _ = tick_timer.tick() => {
                        self.run_scheduled_probes().await;
                    }
                }
            }
        });
    }

    async fn run_scheduled_probes(&self) {
        let monitors = match load_monitors() {
            Ok(m) => m,
            Err(_) => return,
        };

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        for target in monitors {
            if !target.is_enabled {
                continue;
            }

            let state = self.state_machine.get_state(&target.id);
            let last_check = state.last_checked_at.unwrap_or(0);
            let interval = target.interval_sec as i64;

            if now - last_check >= interval {
                let sem = self.semaphore.clone();
                let sm = self.state_machine.clone();
                tauri::async_runtime::spawn(async move {
                    let _permit = match sem.acquire().await {
                        Ok(p) => p,
                        Err(_) => return,
                    };

                    let record = execute_probe(&target).await;

                    let maybe_transition = sm.process_heartbeat(
                        &record,
                        target.retry_count,
                        target.recover_count,
                    );

                    // 写入数据库与处理 incident（Connection 局限在独立同步作用域内）
                    let (should_alert, duration_sec) = if let Ok(conn) = open_uptime_db() {
                        let _ = insert_heartbeat(&conn, &record);
                        match &maybe_transition {
                            Some(StateTransition::BecameDown { monitor_id, cause, timestamp }) => {
                                let _ = create_incident(&conn, monitor_id, *timestamp, cause);
                                (true, None)
                            }
                            Some(StateTransition::BecameUp { monitor_id, timestamp }) => {
                                let dur = resolve_incident(&conn, monitor_id, *timestamp)
                                    .ok()
                                    .flatten()
                                    .and_then(|i| i.duration_sec);
                                (true, dur)
                            }
                            None => (false, None),
                        }
                    } else {
                        (false, None)
                    };

                    // 异步告警分发（纯 async，无未锁定的 Connection）
                    if should_alert {
                        if let Some(ref transition) = maybe_transition {
                            dispatch_transition_alert(&target, transition, duration_sec).await;
                        }
                    }
                });
            }
        }
    }

    pub fn get_all_summaries(&self) -> Result<Vec<MonitorSummary>, String> {
        let monitors = load_monitors()?;
        let conn = open_uptime_db()?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let mut summaries = Vec::new();

        for target in monitors {
            let state = self.state_machine.get_state(&target.id);
            let recent_heartbeats = get_recent_heartbeats(&conn, &target.id, 60).unwrap_or_default();

            let (uptime_24h, avg_rt_24h) =
                get_uptime_metrics(&conn, &target.id, 86400, now).unwrap_or((100.0, 0));
            let (uptime_7d, _) =
                get_uptime_metrics(&conn, &target.id, 7 * 86400, now).unwrap_or((100.0, 0));

            let active_incident = get_active_incident(&conn, &target.id).unwrap_or(None);

            let current_status = if !target.is_enabled {
                MonitorStatus::Paused
            } else if state.current_status == MonitorStatus::Unknown {
                recent_heartbeats
                    .last()
                    .map(|h| h.status)
                    .unwrap_or(MonitorStatus::Unknown)
            } else {
                state.current_status
            };

            summaries.push(MonitorSummary {
                target,
                current_status,
                last_checked_at: state.last_checked_at,
                last_response_time_ms: state.last_response_time_ms,
                uptime_ratio_24h: uptime_24h,
                uptime_ratio_7d: uptime_7d,
                avg_response_time_24h_ms: avg_rt_24h,
                recent_heartbeats,
                active_incident,
                ssl_days_remaining: None,
            });
        }

        Ok(summaries)
    }
}
