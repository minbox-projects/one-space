//! Serialized ownership of SSH monitor sessions.
//!
//! A session in persistent mode connects once and runs one command channel per
//! collection. One-shot mode connects, collects and disconnects every time.
//! Collection and keepalive share one mutex, so a single owned session never
//! performs two operations concurrently, and invalidation prevents an in-flight
//! result from being published.

use crate::server_monitor::metrics::build_collection;
use crate::server_monitor::types::{
    CollectionOutcome, CollectorOutput, MonitorConnectRequest,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub(crate) use crate::server_monitor::types::{MonitorTransportError, SessionLifetime};

/// Upper bound for one collector execution, including reading its framed output.
pub(crate) const COLLECTION_DEADLINE: Duration = Duration::from_secs(15);
/// Idle interval after which a persistent session sends an explicit keepalive.
pub(crate) const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(30);

/// Connection factory for monitor collections.
pub(crate) trait MonitorConnector: Send + Sync {
    fn connect(
        &self,
        request: &MonitorConnectRequest,
    ) -> Result<Box<dyn MonitorConnection>, MonitorTransportError>;
}

/// One owned SSH connection able to execute the fixed collector script.
pub(crate) trait MonitorConnection: Send {
    fn run_collector(
        &mut self,
        script: &str,
        deadline: Duration,
    ) -> Result<CollectorOutput, MonitorTransportError>;

    fn keepalive(&mut self) -> Result<(), MonitorTransportError>;

    fn close(&mut self);
}

/// Collection errors that prevent publishing a snapshot at all.
#[derive(Debug)]
pub(crate) enum CollectionError {
    Transport(MonitorTransportError),
    Cancelled,
}

impl std::fmt::Display for CollectionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CollectionError::Transport(error) => formatter.write_str(&error.message),
            CollectionError::Cancelled => formatter.write_str("Collection was cancelled"),
        }
    }
}

impl std::error::Error for CollectionError {}

struct SessionState {
    connection: Option<Box<dyn MonitorConnection>>,
    last_activity: Instant,
}

/// Owned monitor session with mode-specific connection lifetime.
pub(crate) struct MonitorSession {
    connector: Arc<dyn MonitorConnector>,
    request: MonitorConnectRequest,
    lifetime: SessionLifetime,
    state: Mutex<SessionState>,
    invalidated: AtomicBool,
}

impl MonitorSession {
    pub(crate) const COLLECTOR_SCRIPT: &'static str = include_str!("collector.sh");

    pub(crate) fn new(
        connector: Arc<dyn MonitorConnector>,
        request: MonitorConnectRequest,
        lifetime: SessionLifetime,
    ) -> Self {
        Self {
            connector,
            request,
            lifetime,
            state: Mutex::new(SessionState {
                connection: None,
                last_activity: Instant::now(),
            }),
            invalidated: AtomicBool::new(false),
        }
    }

    /// Runs one collection, reusing or creating the connection as configured.
    pub(crate) fn collect(
        &self,
        selected_interface: Option<&str>,
        collected_at_ms: i64,
    ) -> Result<CollectionOutcome, CollectionError> {
        if self.invalidated.load(Ordering::SeqCst) {
            return Err(CollectionError::Cancelled);
        }

        let mut state = self
            .state
            .lock()
            .map_err(|_| CollectionError::Cancelled)?;

        if self.invalidated.load(Ordering::SeqCst) {
            self.close_connection(&mut state);
            return Err(CollectionError::Cancelled);
        }

        let needs_connection = match self.lifetime {
            SessionLifetime::Persistent => state.connection.is_none(),
            SessionLifetime::OneShot => true,
        };
        if needs_connection {
            if self.lifetime == SessionLifetime::OneShot {
                self.close_connection(&mut state);
            }
            let connection = self
                .connector
                .connect(&self.request)
                .map_err(CollectionError::Transport)?;
            state.connection = Some(connection);
        }

        let run_result = match state.connection.as_mut() {
            Some(connection) => {
                connection.run_collector(Self::COLLECTOR_SCRIPT, COLLECTION_DEADLINE)
            }
            None => return Err(CollectionError::Cancelled),
        };
        state.last_activity = Instant::now();

        match run_result {
            Ok(output) => {
                if self.lifetime == SessionLifetime::OneShot {
                    self.close_connection(&mut state);
                }
                if self.invalidated.load(Ordering::SeqCst) {
                    self.close_connection(&mut state);
                    return Err(CollectionError::Cancelled);
                }
                Ok(build_collection(&output, selected_interface, collected_at_ms))
            }
            Err(error) => {
                // A failed or timed-out command invalidates this transport;
                // a later ordinary trigger may establish a fresh connection.
                self.close_connection(&mut state);
                Err(CollectionError::Transport(error))
            }
        }
    }

    /// Sends an explicit keepalive when the persistent session has been idle for
    /// at least the keepalive interval. Collection and keepalive never overlap.
    pub(crate) fn keepalive_if_due(&self, now: Instant) {
        if self.invalidated.load(Ordering::SeqCst) || self.lifetime != SessionLifetime::Persistent {
            return;
        }
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if self.invalidated.load(Ordering::SeqCst) {
            self.close_connection(&mut state);
            return;
        }
        let elapsed = now.saturating_duration_since(state.last_activity);
        if elapsed < KEEPALIVE_INTERVAL {
            return;
        }
        let result = state
            .connection
            .as_mut()
            .map(|connection| connection.keepalive());
        match result {
            Some(Ok(())) => state.last_activity = now,
            Some(Err(_)) => self.close_connection(&mut state),
            None => {}
        }
    }

    /// Prevents publishing from this generation and closes its owned resources.
    pub(crate) fn invalidate(&self) {
        self.invalidated.store(true, Ordering::SeqCst);
        if let Ok(mut state) = self.state.lock() {
            self.close_connection(&mut state);
        }
    }

    /// Closes the owned connection. Repeated calls are harmless.
    pub(crate) fn close(&self) {
        self.invalidate();
    }

    fn close_connection(&self, state: &mut SessionState) {
        if let Some(mut connection) = state.connection.take() {
            connection.close();
        }
    }
}

impl Drop for MonitorSession {
    fn drop(&mut self) {
        self.invalidated.store(true, Ordering::SeqCst);
        if let Ok(mut state) = self.state.lock() {
            if let Some(mut connection) = state.connection.take() {
                connection.close();
            }
        }
    }
}
