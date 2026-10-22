#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use crate::uptime_monitor::state_machine::{StateMachineRegistry, StateTransition};
    use crate::uptime_monitor::storage::{
        create_incident, get_active_incident, get_recent_heartbeats, get_uptime_metrics, init_db,
        insert_heartbeat, prune_old_records, resolve_incident,
    };
    use crate::uptime_monitor::types::{HeartbeatRecord, MonitorStatus};

    #[test]
    fn test_sqlite_heartbeats_and_metrics() {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        init_db(&conn).expect("init db");

        let monitor_id = "test-monitor-1";
        let base_time = 1_700_000_000i64;

        // 插入 10 条心跳：8 条 Up, 2 条 Down
        for i in 0..8 {
            let record = HeartbeatRecord {
                id: None,
                monitor_id: monitor_id.to_string(),
                timestamp: base_time + (i * 30),
                status: MonitorStatus::Up,
                response_time_ms: 30 + (i as u32 * 5),
                status_code: Some(200),
                error_message: None,
                ssl_days_remaining: Some(60),
            };
            insert_heartbeat(&conn, &record).expect("insert up heartbeat");
        }

        for i in 8..10 {
            let record = HeartbeatRecord {
                id: None,
                monitor_id: monitor_id.to_string(),
                timestamp: base_time + (i * 30),
                status: MonitorStatus::Down,
                response_time_ms: 1000,
                status_code: Some(500),
                error_message: Some("Internal Server Error".to_string()),
                ssl_days_remaining: None,
            };
            insert_heartbeat(&conn, &record).expect("insert down heartbeat");
        }

        let recent = get_recent_heartbeats(&conn, monitor_id, 5).expect("get recent");
        assert_eq!(recent.len(), 5);
        assert_eq!(recent.last().unwrap().timestamp, base_time + (9 * 30));
        assert_eq!(recent.last().unwrap().status, MonitorStatus::Down);

        let (ratio, avg_rt) = get_uptime_metrics(&conn, monitor_id, 86400, base_time + 300)
            .expect("get uptime metrics");
        assert_eq!(ratio, 80.0);
        assert!(avg_rt > 0);
    }

    #[test]
    fn test_incident_lifecycle() {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        init_db(&conn).expect("init db");

        let monitor_id = "test-incident-mon";
        let start_time = 1_700_000_000i64;

        let active = get_active_incident(&conn, monitor_id).expect("get active");
        assert!(active.is_none());

        let inc = create_incident(&conn, monitor_id, start_time, "Connection refused")
            .expect("create incident");
        assert_eq!(inc.started_at, start_time);
        assert!(inc.resolved_at.is_none());

        let active = get_active_incident(&conn, monitor_id).expect("get active").unwrap();
        assert_eq!(active.id, inc.id);

        let dup = create_incident(&conn, monitor_id, start_time + 10, "Another error")
            .expect("create dup incident");
        assert_eq!(dup.id, inc.id);

        let resolved = resolve_incident(&conn, monitor_id, start_time + 120)
            .expect("resolve incident")
            .unwrap();
        assert_eq!(resolved.resolved_at, Some(start_time + 120));
        assert_eq!(resolved.duration_sec, Some(120));

        let after_resolved = get_active_incident(&conn, monitor_id).expect("get active");
        assert!(after_resolved.is_none());
    }

    #[tokio::test]
    async fn test_real_probe_huanweiyun() {
        use crate::uptime_monitor::prober::probe_http;
        use crate::uptime_monitor::types::HttpProbeOptions;

        let opts = HttpProbeOptions::default();
        let res = probe_http("https://console.huanweiyun.com/", &opts, 10000).await;
        println!("PROBE RESULT: {:?}", res);
        assert!(res.success, "probe failed: {}", res.message);
        assert_eq!(res.status_code, Some(200));
    }

    #[test]
    fn test_prune_old_records() {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        init_db(&conn).expect("init db");

        let monitor_id = "test-prune";
        let now = 1_700_000_000i64;
        let old_time = now - (35 * 86400);

        let old_rec = HeartbeatRecord {
            id: None,
            monitor_id: monitor_id.to_string(),
            timestamp: old_time,
            status: MonitorStatus::Up,
            response_time_ms: 50,
            status_code: Some(200),
            error_message: None,
            ssl_days_remaining: None,
        };
        let new_rec = HeartbeatRecord {
            id: None,
            monitor_id: monitor_id.to_string(),
            timestamp: now,
            status: MonitorStatus::Up,
            response_time_ms: 50,
            status_code: Some(200),
            error_message: None,
            ssl_days_remaining: None,
        };

        insert_heartbeat(&conn, &old_rec).unwrap();
        insert_heartbeat(&conn, &new_rec).unwrap();

        let deleted = prune_old_records(&conn, 30, now).expect("prune");
        assert_eq!(deleted, 1);

        let remaining = get_recent_heartbeats(&conn, monitor_id, 10).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].timestamp, now);
    }

    #[test]
    fn test_state_machine_anti_flapping() {
        let registry = StateMachineRegistry::new();
        let monitor_id = "flapping-mon";

        let hb_fail_1 = HeartbeatRecord {
            id: None,
            monitor_id: monitor_id.to_string(),
            timestamp: 1000,
            status: MonitorStatus::Down,
            response_time_ms: 5000,
            status_code: None,
            error_message: Some("Timeout".to_string()),
            ssl_days_remaining: None,
        };

        // 第一次失败，retry_count 为 2，不应触发跃迁
        let trans1 = registry.process_heartbeat(&hb_fail_1, 2, 1);
        assert!(trans1.is_none());
        assert_ne!(registry.get_state(monitor_id).current_status, MonitorStatus::Down);

        // 第二次连续失败，达到阈值，触发 BecameDown
        let mut hb_fail_2 = hb_fail_1.clone();
        hb_fail_2.timestamp = 1030;
        let trans2 = registry.process_heartbeat(&hb_fail_2, 2, 1);
        assert!(matches!(trans2, Some(StateTransition::BecameDown { .. })));
        assert_eq!(registry.get_state(monitor_id).current_status, MonitorStatus::Down);

        // 成功 1 次（recover_count 为 1），触发 BecameUp
        let hb_ok = HeartbeatRecord {
            id: None,
            monitor_id: monitor_id.to_string(),
            timestamp: 1060,
            status: MonitorStatus::Up,
            response_time_ms: 35,
            status_code: Some(200),
            error_message: None,
            ssl_days_remaining: None,
        };
        let trans3 = registry.process_heartbeat(&hb_ok, 2, 1);
        assert!(matches!(trans3, Some(StateTransition::BecameUp { .. })));
        assert_eq!(registry.get_state(monitor_id).current_status, MonitorStatus::Up);
    }
}
