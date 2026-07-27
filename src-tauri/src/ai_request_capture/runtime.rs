use super::{
    database_path_in, validation_errors, AiRequestCaptureConfig, AiRequestCaptureStatus,
    CaptureStore,
};
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Emitter};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

struct ActiveRuntime {
    shutdown: oneshot::Sender<()>,
    task: JoinHandle<()>,
}

struct RuntimeState {
    status: AiRequestCaptureStatus,
    active: Option<ActiveRuntime>,
}

impl Default for RuntimeState {
    fn default() -> Self {
        Self {
            status: AiRequestCaptureStatus::stopped(17688),
            active: None,
        }
    }
}

fn state() -> &'static Mutex<RuntimeState> {
    static STATE: OnceLock<Mutex<RuntimeState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(RuntimeState::default()))
}

pub(crate) fn current_status() -> AiRequestCaptureStatus {
    state()
        .lock()
        .map(|state| state.status.clone())
        .unwrap_or_else(|_| {
            failed_status(
                17688,
                "capture runtime state lock is unavailable".to_string(),
            )
        })
}

pub(crate) fn set_recovery_status(status: AiRequestCaptureStatus) -> AiRequestCaptureStatus {
    replace_status(status)
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) async fn start_in(
    app_dir: &Path,
    config: AiRequestCaptureConfig,
) -> AiRequestCaptureStatus {
    start(app_dir, config, None).await
}

pub(crate) async fn start(
    app_dir: &Path,
    config: AiRequestCaptureConfig,
    app: Option<AppHandle>,
) -> AiRequestCaptureStatus {
    let enabled_config = AiRequestCaptureConfig {
        enabled: true,
        ..config.clone()
    };
    if let Some(error) = validation_errors(&enabled_config).first() {
        return publish_status(failed_status(config.port, error.message.clone()), &app);
    }

    let existing = match state().lock() {
        Ok(mut state) => {
            if state.active.is_some() && state.status.running && state.status.port == config.port {
                let status = state.status.clone();
                drop(state);
                return publish_status(status, &app);
            }
            state.active.take()
        }
        Err(_) => {
            return publish_status(
                failed_status(
                    config.port,
                    "capture runtime state lock is unavailable".to_string(),
                ),
                &app,
            );
        }
    };
    stop_active(existing).await;

    let store = match CaptureStore::open(database_path_in(app_dir)) {
        Ok(store) => store,
        Err(error) => return publish_status(failed_status(config.port, error), &app),
    };
    let listener = match TcpListener::bind(("127.0.0.1", config.port)).await {
        Ok(listener) => listener,
        Err(error) => {
            return publish_status(
                failed_status(
                    config.port,
                    format!("failed to bind 127.0.0.1:{}: {error}", config.port),
                ),
                &app,
            );
        }
    };
    let (shutdown, mut shutdown_rx) = oneshot::channel();
    let handler_config = config.clone();
    let handler_app = app.clone();
    let task = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut shutdown_rx => break,
                accepted = listener.accept() => {
                    let Ok((stream, _)) = accepted else { break };
                    let config = handler_config.clone();
                    let store = store.clone();
                    let app = handler_app.clone();
                    tokio::spawn(async move {
                        let service = service_fn(move |request| {
                            super::proxy::forward(request, config.clone(), store.clone(), app.clone())
                        });
                        let _ = hyper::server::conn::http1::Builder::new()
                            .serve_connection(TokioIo::new(stream), service)
                            .await;
                    });
                }
            }
        }
    });
    let status = AiRequestCaptureStatus {
        running: true,
        listen_address: "127.0.0.1".to_string(),
        port: config.port,
        last_error: None,
    };
    let stored = match state().lock() {
        Ok(mut state) => {
            state.status = status.clone();
            state.active = Some(ActiveRuntime { shutdown, task });
            true
        }
        Err(_) => {
            let _ = shutdown.send(());
            false
        }
    };
    if stored {
        publish_status(status, &app)
    } else {
        failed_status(
            config.port,
            "capture runtime state lock is unavailable".to_string(),
        )
    }
}

pub(crate) async fn apply_config(
    app_dir: &Path,
    config: AiRequestCaptureConfig,
    app: Option<AppHandle>,
) -> AiRequestCaptureStatus {
    if config.enabled {
        start(app_dir, config, app).await
    } else {
        stop_with_port(config.port, app).await
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) async fn stop() -> AiRequestCaptureStatus {
    stop_with_port(current_status().port, None).await
}

pub(crate) async fn stop_with_port(port: u16, app: Option<AppHandle>) -> AiRequestCaptureStatus {
    let active = match state().lock() {
        Ok(mut state) => state.active.take(),
        Err(_) => {
            return publish_status(
                failed_status(
                    port,
                    "capture runtime state lock is unavailable".to_string(),
                ),
                &app,
            );
        }
    };
    stop_active(active).await;
    publish_status(AiRequestCaptureStatus::stopped(port), &app)
}

#[cfg_attr(test, allow(dead_code))]
pub(crate) fn request_shutdown() {
    if let Ok(mut state) = state().lock() {
        if let Some(active) = state.active.take() {
            let _ = active.shutdown.send(());
        }
        state.status.running = false;
    }
}

async fn stop_active(active: Option<ActiveRuntime>) {
    if let Some(active) = active {
        let _ = active.shutdown.send(());
        let _ = active.task.await;
    }
}

fn publish_status(
    status: AiRequestCaptureStatus,
    app: &Option<AppHandle>,
) -> AiRequestCaptureStatus {
    let status = replace_status(status);
    if let Some(app) = app {
        let _ = app.emit("ai-request-capture-status-update", &status);
    }
    status
}

fn replace_status(status: AiRequestCaptureStatus) -> AiRequestCaptureStatus {
    match state().lock() {
        Ok(mut state) => {
            state.status = status.clone();
            status
        }
        Err(_) => failed_status(
            status.port,
            "capture runtime state lock is unavailable".to_string(),
        ),
    }
}

fn failed_status(port: u16, error: String) -> AiRequestCaptureStatus {
    AiRequestCaptureStatus {
        running: false,
        listen_address: "127.0.0.1".to_string(),
        port,
        last_error: Some(error),
    }
}
