use super::doh::{query_all_doh, DnsCompareReport};
use super::tcp_ping::{
    analyze_ip,
    execute_tcp_ping,
    start_tcp_ping_stream,
    stop_tcp_ping_stream,
    IpIntelligenceResult,
    TcpPingResult,
};
use tauri::AppHandle;

#[tauri::command]
pub async fn dns_inspector_query(
    domain: String,
    record_type: String,
) -> Result<DnsCompareReport, String> {
    let clean_domain = domain.trim();
    if clean_domain.is_empty() {
        return Err("域名不能为空".to_string());
    }

    let clean_type = record_type.trim().to_uppercase();
    Ok(query_all_doh(clean_domain, &clean_type).await)
}

#[tauri::command]
pub async fn dns_inspector_tcp_ping(
    host: String,
    port: u16,
    timeout_ms: Option<u64>,
) -> Result<TcpPingResult, String> {
    let clean_host = host.trim();
    if clean_host.is_empty() {
        return Err("主机名或 IP 不能为空".to_string());
    }

    let timeout = timeout_ms.unwrap_or(3000);
    Ok(execute_tcp_ping(clean_host, port, timeout).await)
}

#[tauri::command]
pub async fn dns_inspector_tcp_ping_stream_start(
    app: AppHandle,
    session_id: String,
    host: String,
    port: u16,
    interval_ms: Option<u64>,
    max_count: Option<u32>,
    timeout_ms: Option<u64>,
) -> Result<bool, String> {
    let clean_host = host.trim().to_string();
    if clean_host.is_empty() {
        return Err("主机名或 IP 不能为空".to_string());
    }
    let interval = interval_ms.unwrap_or(1000).max(200);
    let timeout = timeout_ms.unwrap_or(3000);

    start_tcp_ping_stream(
        app,
        session_id,
        clean_host,
        port,
        interval,
        max_count,
        timeout,
    );

    Ok(true)
}

#[tauri::command]
pub async fn dns_inspector_tcp_ping_stream_stop(session_id: String) -> Result<bool, String> {
    Ok(stop_tcp_ping_stream(&session_id))
}

#[tauri::command]
pub async fn dns_inspector_ip_lookup(ip: String) -> Result<IpIntelligenceResult, String> {
    let clean_ip = ip.trim();
    if clean_ip.is_empty() {
        return Err("IP 地址不能为空".to_string());
    }

    Ok(analyze_ip(clean_ip))
}
