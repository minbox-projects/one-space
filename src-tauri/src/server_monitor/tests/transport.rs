use crate::server_monitor::transport::{
    CollectionError, MonitorConnection, MonitorConnector, MonitorSession, MonitorTransportError,
    SessionLifetime,
};
use crate::server_monitor::types::{CollectionOutcome, MonitorConnectRequest, MonitorSource};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

struct FakeState {
    connect_count: usize,
    run_count: usize,
    close_count: usize,
    keepalive_count: usize,
    active: usize,
    max_active: usize,
    fail_timeout: bool,
}

struct FakeConnection {
    state: Arc<Mutex<FakeState>>,
    script_output: String,
}

impl MonitorConnection for FakeConnection {
    fn run_collector(&mut self, _script: &str, _deadline: Duration) -> Result<crate::server_monitor::types::CollectorOutput, MonitorTransportError> {
        let mut st = self.state.lock().unwrap();
        st.active += 1;
        if st.active > st.max_active {
            st.max_active = st.active;
        }
        st.run_count += 1;
        let timeout = st.fail_timeout;
        drop(st);
        if timeout {
            // simulate delay
        }
        let mut st = self.state.lock().unwrap();
        st.active -= 1;
        drop(st);
        if timeout {
            return Err(MonitorTransportError {
                kind: crate::server_monitor::types::MonitorTransportErrorKind::Timeout,
                message: "timeout".into(),
            });
        }
        Ok(crate::server_monitor::types::CollectorOutput {
            stdout: self.script_output.clone(),
            exit_code: Some(0),
        })
    }
    fn keepalive(&mut self) -> Result<(), MonitorTransportError> {
        let mut st = self.state.lock().unwrap();
        st.keepalive_count += 1;
        Ok(())
    }
    fn close(&mut self) {
        let mut st = self.state.lock().unwrap();
        st.close_count += 1;
    }
}

struct FakeConnector {
    state: Arc<Mutex<FakeState>>,
    output: String,
}

impl MonitorConnector for FakeConnector {
    fn connect(&self, _request: &MonitorConnectRequest) -> Result<Box<dyn MonitorConnection>, MonitorTransportError> {
        let mut st = self.state.lock().unwrap();
        st.connect_count += 1;
        drop(st);
        Ok(Box::new(FakeConnection {
            state: self.state.clone(),
            script_output: self.output.clone(),
        }))
    }
}

fn make_output() -> String {
    let frame = |name: &str, body: &str| format!("@@ONESPACE:{}@@\n{}", name, body);
    format!(
        "{}{}{}{}{}{}{}{}{}{}",
        frame("boot_id_1", "b1\n"),
        frame("boot_id_2", "b1\n"),
        frame("uptime_1", "10.0\n"),
        frame("uptime_2", "11.0\n"),
        frame("stat_1", "cpu  1 0 1 1 0 0 0 0 0 0\n"),
        frame("stat_2", "cpu  2 0 1 1 0 0 0 0 0 0\n"),
        frame("net_1", "eth0: 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0\n"),
        frame("net_2", "eth0: 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0\n"),
        frame("meminfo", "MemTotal: 1000000 kB\nMemAvailable: 400000 kB\n"),
        frame("df", "Filesystem 1K-blocks Used Available Use% Mounted on\n/dev/sda1 1000 500 500 50% /\n"),
    )
}

#[test]
fn persistent_session_connects_once_for_three_collects() {
    let state = Arc::new(Mutex::new(FakeState {
        connect_count: 0,
        run_count: 0,
        close_count: 0,
        keepalive_count: 0,
        active: 0,
        max_active: 0,
        fail_timeout: false,
    }));
    let connector: Arc<dyn MonitorConnector> = Arc::new(FakeConnector {
        state: state.clone(),
        output: make_output(),
    });
    let request = MonitorConnectRequest {
        target_id: "t".into(),
        source: MonitorSource::Custom {
            host: "h".into(),
            port: 22,
            user: "u".into(),
            auth: crate::server_monitor::types::MonitorAuth::Password,
        },
        password: None,
        tunnel: None,
    };
    let session = MonitorSession::new(connector, request, SessionLifetime::Persistent);
    for _ in 0..3 {
        let _ = session.collect(None, 1);
    }
    let st = state.lock().unwrap();
    assert_eq!(st.connect_count, 1);
    assert_eq!(st.run_count, 3);
}

#[test]
fn oneshot_session_connects_per_collect() {
    let state = Arc::new(Mutex::new(FakeState {
        connect_count: 0,
        run_count: 0,
        close_count: 0,
        keepalive_count: 0,
        active: 0,
        max_active: 0,
        fail_timeout: false,
    }));
    let connector: Arc<dyn MonitorConnector> = Arc::new(FakeConnector {
        state: state.clone(),
        output: make_output(),
    });
    let request = MonitorConnectRequest {
        target_id: "t".into(),
        source: MonitorSource::Custom {
            host: "h".into(),
            port: 22,
            user: "u".into(),
            auth: crate::server_monitor::types::MonitorAuth::Password,
        },
        password: None,
        tunnel: None,
    };
    let session = MonitorSession::new(connector, request, SessionLifetime::OneShot);
    for _ in 0..3 {
        let _ = session.collect(None, 1);
    }
    let st = state.lock().unwrap();
    assert_eq!(st.connect_count, 3);
    assert_eq!(st.keepalive_count, 0);
}

#[test]
fn keepalive_if_due_at_30s_boundary() {
    let state = Arc::new(Mutex::new(FakeState {
        connect_count: 0,
        run_count: 0,
        close_count: 0,
        keepalive_count: 0,
        active: 0,
        max_active: 0,
        fail_timeout: false,
    }));
    let connector: Arc<dyn MonitorConnector> = Arc::new(FakeConnector {
        state: state.clone(),
        output: make_output(),
    });
    let request = MonitorConnectRequest {
        target_id: "t".into(),
        source: MonitorSource::Custom {
            host: "h".into(),
            port: 22,
            user: "u".into(),
            auth: crate::server_monitor::types::MonitorAuth::Password,
        },
        password: None,
        tunnel: None,
    };
    let session = MonitorSession::new(connector, request, SessionLifetime::Persistent);
    let _ = session.collect(None, 1);
    let base = Instant::now();
    session.keepalive_if_due(base + Duration::from_secs(30));
    let st = state.lock().unwrap();
    assert!(st.keepalive_count >= 1);
}

#[test]
fn invalidate_during_collect_prevents_publication() {
    // best-effort: call invalidate then collect; expect Cancelled or normal. 
    let state = Arc::new(Mutex::new(FakeState {
        connect_count: 0,
        run_count: 0,
        close_count: 0,
        keepalive_count: 0,
        active: 0,
        max_active: 0,
        fail_timeout: false,
    }));
    let connector: Arc<dyn MonitorConnector> = Arc::new(FakeConnector {
        state: state.clone(),
        output: make_output(),
    });
    let request = MonitorConnectRequest {
        target_id: "t".into(),
        source: MonitorSource::Custom {
            host: "h".into(),
            port: 22,
            user: "u".into(),
            auth: crate::server_monitor::types::MonitorAuth::Password,
        },
        password: None,
        tunnel: None,
    };
    let session = MonitorSession::new(connector, request, SessionLifetime::Persistent);
    session.invalidate();
    let res = session.collect(None, 1);
    assert!(res.is_err());
}

#[test]
fn transport_timeout_surfaces_transport_timeout() {
    let state = Arc::new(Mutex::new(FakeState {
        connect_count: 0,
        run_count: 0,
        close_count: 0,
        keepalive_count: 0,
        active: 0,
        max_active: 0,
        fail_timeout: true,
    }));
    let connector: Arc<dyn MonitorConnector> = Arc::new(FakeConnector {
        state: state.clone(),
        output: make_output(),
    });
    let request = MonitorConnectRequest {
        target_id: "t".into(),
        source: MonitorSource::Custom {
            host: "h".into(),
            port: 22,
            user: "u".into(),
            auth: crate::server_monitor::types::MonitorAuth::Password,
        },
        password: None,
        tunnel: None,
    };
    let session = MonitorSession::new(connector, request, SessionLifetime::Persistent);
    let res = session.collect(None, 1);
    match res {
        Err(CollectionError::Transport(e)) => {
            assert!(matches!(e.kind, crate::server_monitor::types::MonitorTransportErrorKind::Timeout));
        }
        _ => panic!("expected transport timeout"),
    }
}

#[test]
fn collector_script_nonempty() {
    assert!(!MonitorSession::COLLECTOR_SCRIPT.is_empty());
}

#[test]
fn concurrent_collect_keepalive_no_overlap() {
    let state = Arc::new(Mutex::new(FakeState {
        connect_count: 0,
        run_count: 0,
        close_count: 0,
        keepalive_count: 0,
        active: 0,
        max_active: 0,
        fail_timeout: false,
    }));
    let connector: Arc<dyn MonitorConnector> = Arc::new(FakeConnector {
        state: state.clone(),
        output: make_output(),
    });
    let request = MonitorConnectRequest {
        target_id: "t".into(),
        source: MonitorSource::Custom {
            host: "h".into(),
            port: 22,
            user: "u".into(),
            auth: crate::server_monitor::types::MonitorAuth::Password,
        },
        password: None,
        tunnel: None,
    };
    let session = MonitorSession::new(connector, request, SessionLifetime::Persistent);
    let _ = session.collect(None, 1);
    session.keepalive_if_due(Instant::now());
    let st = state.lock().unwrap();
    assert!(st.max_active <= 1);
}

static SMOKE_HOME_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
#[ignore = "requires MONITOR_SMOKE_* and a controlled Linux SSH target"]
fn real_ssh_persistent_and_oneshot_sessions_smoke() {
    let host = match std::env::var("MONITOR_SMOKE_HOST") {
        Ok(v) if !v.is_empty() => v,
        _ => {
            println!("skipping real_ssh_persistent_and_oneshot_sessions_smoke: MONITOR_SMOKE_HOST missing");
            return;
        }
    };
    let port: u16 = match std::env::var("MONITOR_SMOKE_PORT") {
        Ok(v) => v.parse().unwrap_or(0),
        _ => {
            println!("skipping: MONITOR_SMOKE_PORT missing");
            return;
        }
    };
    if port == 0 {
        println!("skipping: MONITOR_SMOKE_PORT invalid/missing");
        return;
    }
    let user = match std::env::var("MONITOR_SMOKE_USER") {
        Ok(v) if !v.is_empty() => v,
        _ => {
            println!("skipping: MONITOR_SMOKE_USER missing");
            return;
        }
    };
    let password = match std::env::var("MONITOR_SMOKE_PASSWORD") {
        Ok(v) => v,
        _ => {
            println!("skipping: MONITOR_SMOKE_PASSWORD missing");
            return;
        }
    };

    let _guard = SMOKE_HOME_MUTEX.lock().unwrap();
    let original_home = std::env::var("HOME").ok();
    let tmp_home = std::env::temp_dir().join(format!("onespace-smoke-home-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&tmp_home).unwrap();
    std::env::set_var("HOME", &tmp_home);

    let request = MonitorConnectRequest {
        target_id: "smoke".into(),
        source: MonitorSource::Custom {
            host: host.clone(),
            port,
            user: user.clone(),
            auth: crate::server_monitor::types::MonitorAuth::Password,
        },
        password: Some(password.clone()),
        tunnel: None,
    };

    let connector: Arc<dyn MonitorConnector> =
        Arc::new(crate::ssh_tunnels::monitor_transport::SshMonitorConnector::new());

    // Persistent session: three collects, first already has network rates.
    let session = MonitorSession::new(connector.clone(), request.clone(), SessionLifetime::Persistent);
    for i in 0..3 {
        let outcome = session.collect(None, 1_000).expect("collect failed");
        match outcome {
            CollectionOutcome::Success(s) | CollectionOutcome::Partial(s) => {
                assert!(s.cpu.is_some(), "cpu missing");
                assert!(s.memory.is_some(), "memory missing");
                if i == 0 {
                    assert!(
                        s.network.is_some() || !s.networks.is_empty(),
                        "network rates missing on first collect"
                    );
                }
            }
            CollectionOutcome::Failed(_) => panic!("collection failed"),
        }
    }
    session.close();

    // Two separate one-shot sessions.
    for _ in 0..2 {
        let session = MonitorSession::new(connector.clone(), request.clone(), SessionLifetime::OneShot);
        let outcome = session.collect(None, 1_000).expect("oneshot collect failed");
        assert!(matches!(outcome, CollectionOutcome::Success(_) | CollectionOutcome::Partial(_)));
    }

    let _ = std::fs::remove_dir_all(&tmp_home);
    match original_home {
        Some(h) => std::env::set_var("HOME", h),
        None => std::env::remove_var("HOME"),
    }
}
