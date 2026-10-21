use std::sync::mpsc;
use std::sync::{Mutex, OnceLock};
use std::thread::{spawn, JoinHandle};
use std::time::Duration;

use chrono::Local;
use tauri::{AppHandle, Emitter};

use super::config::load_config;
use super::watchlist::check_all_watchlist;

pub const SSL_WATCHLIST_UPDATED_EVENT: &str = "ssl-watchlist-updated";

struct SslSchedulerControl {
    stop: mpsc::Sender<()>,
    join: Option<JoinHandle<()>>,
}

fn scheduler_slot() -> &'static Mutex<Option<SslSchedulerControl>> {
    static SCHEDULER: OnceLock<Mutex<Option<SslSchedulerControl>>> = OnceLock::new();
    SCHEDULER.get_or_init(|| Mutex::new(None))
}

pub fn start_ssl_scheduler(app: AppHandle) -> bool {
    let mut slot = scheduler_slot()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if slot.is_some() {
        return false;
    }

    let (stop, stop_rx) = mpsc::channel();
    let loop_app = app.clone();

    let join = spawn(move || {
        let mut last_triggered_slot: Option<String> = None;
        loop {
            match stop_rx.recv_timeout(Duration::from_secs(30)) {
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    let now = Local::now();
                    let current_hh_mm = now.format("%H:%M").to_string();
                    let today_date = now.format("%Y-%m-%d").to_string();
                    let current_slot = format!("{} {}", today_date, current_hh_mm);

                    let config = load_config().unwrap_or_default();
                    if !config.auto_check_enabled {
                        continue;
                    }

                    if config.check_times.contains(&current_hh_mm) {
                        if last_triggered_slot.as_deref() != Some(&current_slot) {
                            last_triggered_slot = Some(current_slot);
                            let _ = tauri::async_runtime::block_on(check_all_watchlist());
                            let _ = loop_app.emit(SSL_WATCHLIST_UPDATED_EVENT, ());
                        }
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) | Ok(()) => return,
            }
        }
    });

    *slot = Some(SslSchedulerControl {
        stop,
        join: Some(join),
    });

    true
}

pub fn stop_ssl_scheduler() {
    let control = scheduler_slot()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take();
    if let Some(control) = control {
        let _ = control.stop.send(());
        if let Some(join) = control.join {
            let _ = join.join();
        }
    }
}
