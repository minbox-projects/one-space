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

    #[test]
    fn test_long_term_failure_no_repeated_alerts() {
        let registry = StateMachineRegistry::new();
        let monitor_id = "long-fail-mon";
        let retry_count = 3;
        let recover_count = 2;

        let mut transitions = Vec::new();

        // 模拟连续 10 次失败心跳（每 30 秒一次，总共持续 300 秒异常）
        for i in 1..=10 {
            let record = HeartbeatRecord {
                id: None,
                monitor_id: monitor_id.to_string(),
                timestamp: 1000 + (i * 30),
                status: MonitorStatus::Down,
                response_time_ms: 1000,
                status_code: Some(500),
                error_message: Some("Internal Server Error".to_string()),
                ssl_days_remaining: None,
            };
            if let Some(t) = registry.process_heartbeat(&record, retry_count, recover_count) {
                transitions.push((i, t));
            }
        }

        // 严密断言：仅在第 3 次（达到 retry_count 门槛）产生恰好 1 次 BecameDown 告警！
        // 第 4 到第 10 次心跳绝对保持静默，绝不每 30 秒重复写消息或告警！
        assert_eq!(transitions.len(), 1, "There must be exactly 1 alert during sustained outage");
        assert_eq!(transitions[0].0, 3, "Alert must be triggered on the 3rd failure");
        assert!(matches!(transitions[0].1, StateTransition::BecameDown { .. }));

        // 接下来模拟服务恢复：连续 2 次成功心跳
        let mut recover_transitions = Vec::new();
        for i in 11..=15 {
            let record = HeartbeatRecord {
                id: None,
                monitor_id: monitor_id.to_string(),
                timestamp: 1000 + (i * 30),
                status: MonitorStatus::Up,
                response_time_ms: 50,
                status_code: Some(200),
                error_message: None,
                ssl_days_remaining: None,
            };
            if let Some(t) = registry.process_heartbeat(&record, retry_count, recover_count) {
                recover_transitions.push((i, t));
            }
        }

        // 严密断言：仅在第 12 次（连续成功 2 次达到 recover_count）产生恰好 1 次 BecameUp 恢复告警！
        // 第 13 到第 15 次心跳持续正常保持静默！
        assert_eq!(recover_transitions.len(), 1, "There must be exactly 1 recovery notification");
        assert_eq!(recover_transitions[0].0, 12, "Recovery must be triggered on the 2nd success");
        assert!(matches!(recover_transitions[0].1, StateTransition::BecameUp { .. }));
    }

    #[test]
    fn test_active_incident_hydration_prevents_duplicate_alert() {
        let registry = StateMachineRegistry::new();
        let monitor_id = "hydrated-mon";

        // 模拟应用启动：从数据库发现该监控项此前已处于持续故障中
        registry.hydrate_active_incident(monitor_id, 1000, "Historical server error");

        // 随后的探测再次失败，验证由于已存在 active incident，不产生新的 BecameDown 告警
        let record = HeartbeatRecord {
            id: None,
            monitor_id: monitor_id.to_string(),
            timestamp: 2000,
            status: MonitorStatus::Down,
            response_time_ms: 1000,
            status_code: Some(502),
            error_message: Some("Bad Gateway".to_string()),
            ssl_days_remaining: None,
        };
        let trans = registry.process_heartbeat(&record, 2, 2);
        assert!(trans.is_none(), "Existing incident must not generate duplicate alert on restart");
    }

    #[test]
    fn test_alert_cooldown_throttling() {
        use crate::uptime_monitor::alerter::{can_dispatch_alert, reset_alert_throttle, ALERT_COOLDOWN_SECONDS};

        reset_alert_throttle();
        let monitor_id = "cooldown-mon";
        let base_time = 1_700_000_000i64;

        // 第一次告警：允许发出
        assert!(can_dispatch_alert(monitor_id, "down", base_time));

        // 30 秒后再次收到告警请求（未达到 300 秒冷却时间）：必须被拦截！
        assert!(!can_dispatch_alert(monitor_id, "down", base_time + 30));

        // 299 秒后（仍处于冷却期内）：仍然拦截！
        assert!(!can_dispatch_alert(monitor_id, "down", base_time + ALERT_COOLDOWN_SECONDS - 1));

        // 301 秒后（超过冷却期）：允许重新发出
        assert!(can_dispatch_alert(monitor_id, "down", base_time + ALERT_COOLDOWN_SECONDS + 1));
    }
}
