use tauri::AppHandle;
use crate::tunnel::{
    start_ephemeral_cloudflare_tunnel, supervisor::get_status,
    EPHEMERAL_TUNNEL_ID_PREFIX, TunnelRuntimeStatus,
};

pub const REALTIME_TUNNEL_EVENT: &str = "realtime-tunnel-status";

fn realtime_tunnel_id(local_port: u16) -> String {
    format!("{}realtime-{}", EPHEMERAL_TUNNEL_ID_PREFIX, local_port)
}

#[tauri::command]
pub async fn realtime_tunnel_start(
    app: AppHandle,
    local_port: u16,
) -> Result<TunnelRuntimeStatus, String> {
    if local_port == 0 {
        return Err("本地端口不能为 0".to_string());
    }

    let tunnel_id = realtime_tunnel_id(local_port);
    let target_url = format!("http://127.0.0.1:{}", local_port);

    start_ephemeral_cloudflare_tunnel(
        tunnel_id,
        target_url,
        REALTIME_TUNNEL_EVENT.to_string(),
        Some(app),
    )
    .await
}

#[tauri::command]
pub async fn realtime_tunnel_stop(
    app: AppHandle,
    local_port: u16,
) -> Result<TunnelRuntimeStatus, String> {
    let tunnel_id = realtime_tunnel_id(local_port);
    let stopped = crate::tunnel::supervisor::stop_tunnel(&tunnel_id, Some(app)).await?;
    Ok(stopped)
}

#[tauri::command]
pub async fn realtime_tunnel_status(local_port: u16) -> Result<TunnelRuntimeStatus, String> {
    let tunnel_id = realtime_tunnel_id(local_port);
    Ok(get_status(&tunnel_id))
}
