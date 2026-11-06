use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcpPingResult {
    pub host: String,
    pub port: u16,
    pub success: bool,
    pub latency_ms: Option<u64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcpPingPacket {
    pub session_id: String,
    pub seq: u32,
    pub host: String,
    pub port: u16,
    pub success: bool,
    pub latency_ms: Option<u64>,
    pub error: Option<String>,
    pub timestamp: u64,
}

static ACTIVE_PROBE_SESSIONS: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();

fn get_sessions() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    ACTIVE_PROBE_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn stop_tcp_ping_stream(session_id: &str) -> bool {
    let mut map = get_sessions().lock().unwrap();
    if let Some(flag) = map.remove(session_id) {
        flag.store(false, Ordering::SeqCst);
        true
    } else {
        false
    }
}

pub fn start_tcp_ping_stream(
    app: AppHandle,
    session_id: String,
    host: String,
    port: u16,
    interval_ms: u64,
    max_count: Option<u32>,
    timeout_ms: u64,
) {
    stop_tcp_ping_stream(&session_id);

    let running_flag = Arc::new(AtomicBool::new(true));
    {
        let mut map = get_sessions().lock().unwrap();
        map.insert(session_id.clone(), running_flag.clone());
    }

    let flag_clone = running_flag.clone();
    let sid = session_id.clone();

    tokio::spawn(async move {
        let mut seq: u32 = 1;
        let event_name = format!("tcp-ping-packet:{}", sid);
        let done_event = format!("tcp-ping-complete:{}", sid);

        while flag_clone.load(Ordering::SeqCst) {
            let res = execute_tcp_ping(&host, port, timeout_ms).await;
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;

            let packet = TcpPingPacket {
                session_id: sid.clone(),
                seq,
                host: host.clone(),
                port,
                success: res.success,
                latency_ms: res.latency_ms,
                error: res.error,
                timestamp: now,
            };

            let _ = app.emit(&event_name, packet);

            if let Some(max) = max_count {
                if seq >= max {
                    break;
                }
            }

            seq += 1;

            let sleep_interval = Duration::from_millis(interval_ms.max(200));
            let check_interval = Duration::from_millis(50);
            let mut elapsed = Duration::from_millis(0);

            while elapsed < sleep_interval {
                if !flag_clone.load(Ordering::SeqCst) {
                    break;
                }
                tokio::time::sleep(check_interval).await;
                elapsed += check_interval;
            }
        }

        let mut map = get_sessions().lock().unwrap();
        map.remove(&sid);
        let _ = app.emit(&done_event, ());
    });
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpIntelligenceResult {
    pub ip: String,
    pub is_private: bool,
    pub is_loopback: bool,
    pub classification: String,
}

pub fn analyze_ip(ip_str: &str) -> IpIntelligenceResult {
    match ip_str.parse::<IpAddr>() {
        Ok(addr) => {
            let is_loopback = addr.is_loopback();
            let is_private = match addr {
                IpAddr::V4(ipv4) => {
                    ipv4.is_private()
                        || ipv4.is_loopback()
                        || ipv4.is_link_local()
                        || ipv4.is_broadcast()
                }
                IpAddr::V6(ipv6) => ipv6.is_loopback() || ipv6.is_multicast(),
            };

            let classification = if is_loopback {
                "本机回环 (Loopback)".to_string()
            } else if is_private {
                "局域网私有保留地址 (RFC 1918)".to_string()
            } else {
                "公网可路由 IP (Public)".to_string()
            };

            IpIntelligenceResult {
                ip: ip_str.to_string(),
                is_private,
                is_loopback,
                classification,
            }
        }
        Err(_) => IpIntelligenceResult {
            ip: ip_str.to_string(),
            is_private: false,
            is_loopback: false,
            classification: "未知格式".to_string(),
        },
    }
}

pub async fn execute_tcp_ping(host: &str, port: u16, timeout_ms: u64) -> TcpPingResult {
    let addr = format!("{}:{}", host, port);
    let start = Instant::now();
    let duration = Duration::from_millis(timeout_ms);

    match tokio::time::timeout(duration, tokio::net::TcpStream::connect(&addr)).await {
        Ok(Ok(_stream)) => {
            let latency_ms = start.elapsed().as_millis() as u64;
            TcpPingResult {
                host: host.to_string(),
                port,
                success: true,
                latency_ms: Some(latency_ms),
                error: None,
            }
        }
        Ok(Err(err)) => TcpPingResult {
            host: host.to_string(),
            port,
            success: false,
            latency_ms: None,
            error: Some(err.to_string()),
        },
        Err(_) => TcpPingResult {
            host: host.to_string(),
            port,
            success: false,
            latency_ms: None,
            error: Some("握手超时 (Timeout)".to_string()),
        },
    }
}
