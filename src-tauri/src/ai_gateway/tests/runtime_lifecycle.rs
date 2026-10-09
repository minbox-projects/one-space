//! Behavior tests for the AI Gateway listener lifetime across an abnormal exit
//! and across a replacement (20261009-core-workflows-cleanup-and-optimization,
//! REQ-002 / AC-002).
//!
//! These tests use real loopback sockets through `start_server`/`stop_server`
//! and the `force_next_accept_failure` fault seam. They take the shared
//! `temp_home` lock so the process-wide listener slot and the fault notification
//! never run concurrently with another server test.

use super::{free_port, temp_home};
use crate::ai_gateway::runtime_http::{
    force_next_accept_failure, server_status, start_server, state_lock, stop_server,
};
use crate::ai_gateway::storage::write_config;
use crate::ai_gateway::types_config::GatewayConfig;
use std::time::{Duration, Instant};

/// Persist a listener configuration on `port` with the process scheduler
/// disabled, so starting the gateway never triggers template network work.
fn seed_listener_config(port: u16) {
    let mut config = GatewayConfig::default();
    config.enabled = true;
    config.port = port;
    config.template_auto_refresh_minutes = 0;
    write_config(&config).expect("seed listener config");
}

/// Bounded wait until the process-wide listener slot no longer holds a listener.
async fn wait_for_listener_slot_to_clear(label: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if let Ok(guard) = state_lock().try_lock() {
            if guard.is_none() {
                return;
            }
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for {label}");
}

/// Whether the process-wide listener slot currently holds a listener.
fn listener_slot_is_occupied() -> bool {
    state_lock()
        .try_lock()
        .map(|guard| guard.is_some())
        .unwrap_or(true)
}

/// AC-002: an abnormal accept-loop failure publishes the actual stopped status
/// and the same port can be restarted.
#[tokio::test]
async fn faulted_listener_reports_stopped_and_restarts_on_the_same_port() {
    let _home = temp_home("runtime-lifecycle-fault-restart");
    let port = free_port().await;
    let _ = stop_server().await;

    seed_listener_config(port);
    let started = start_server(None).await.expect("start the gateway listener");
    assert!(started.running, "the listener must report running after start");
    assert_eq!(started.port, port);

    // Force the accept loop to exit abnormally.
    force_next_accept_failure();
    wait_for_listener_slot_to_clear("the faulted listener to retire").await;

    let status = server_status().expect("read the stopped status");
    assert!(
        !status.running,
        "a faulted listener must report the actual stopped lifetime"
    );

    // The same port must be rebindable and report running again.
    let restarted = start_server(None)
        .await
        .expect("restart on the same port after the fault");
    assert!(restarted.running, "the same-port restart must run");
    assert_eq!(restarted.port, port);

    stop_server().await.expect("stop the restarted listener");
}

/// AC-002: an exiting older listener must not clear the replacement. Starting a
/// new generation on a different port retires the old slot entry and signals the
/// old listener while installing the new one; after the old task's late cleanup
/// the replacement must still own the slot and stay running.
#[tokio::test]
async fn replacement_listener_survives_the_old_listeners_late_cleanup() {
    let _home = temp_home("runtime-lifecycle-replacement");
    let port_a = free_port().await;
    let port_b = free_port().await;
    assert_ne!(port_a, port_b, "the replacement must use a distinct port");
    let _ = stop_server().await;

    seed_listener_config(port_a);
    let first = start_server(None).await.expect("start the old listener");
    assert!(first.running);
    assert_eq!(first.port, port_a);

    // The different-port start takes the old slot entry, signals the old
    // listener and installs a new generation under the same lock.
    seed_listener_config(port_b);
    let second = start_server(None)
        .await
        .expect("install the replacement listener");
    assert!(second.running);
    assert_eq!(second.port, port_b);

    // Let the old task finish its exit and late cleanup, then prove the
    // replacement still owns the listener slot.
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(
            listener_slot_is_occupied(),
            "the old listener's late cleanup must not clear the replacement"
        );
    }

    let final_status = server_status().expect("read the replacement status");
    assert!(
        final_status.running,
        "the replacement listener must stay running after the old cleanup"
    );
    assert_eq!(final_status.port, port_b);

    stop_server().await.expect("stop the replacement listener");
}
