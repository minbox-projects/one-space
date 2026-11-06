pub mod auth;
pub mod client;
pub mod commands;
pub mod opencode_bridge;
pub mod router;
pub mod storage;
pub mod types;

#[cfg(test)]
mod tests;

pub use commands::*;
pub use types::*;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock as StdRwLock};
use std::time::Duration;
use log::{error, info, warn};
use tauri::AppHandle;
use tokio::sync::{Mutex, RwLock};
use tauri::async_runtime::JoinHandle;

use client::TelegramClient;
use router::CommandRouter;
use storage::load_config;

static GLOBAL_BOT_MANAGER: StdRwLock<Option<Arc<RemoteBotManager>>> = StdRwLock::new(None);

pub struct RemoteBotManager {
    #[allow(dead_code)]
    app_handle: AppHandle,
    client: RwLock<Option<Arc<TelegramClient>>>,
    status: RwLock<RemoteBotStatus>,
    running_loop: Mutex<Option<JoinHandle<()>>>,
    stop_signal: Arc<AtomicBool>,
}

impl RemoteBotManager {
    pub fn new(app_handle: AppHandle) -> Self {
        Self {
            app_handle,
            client: RwLock::new(None),
            status: RwLock::new(RemoteBotStatus::default()),
            running_loop: Mutex::new(None),
            stop_signal: Arc::new(AtomicBool::new(false)),
        }
    }

    pub async fn init_global(app_handle: AppHandle) {
        let manager = Arc::new(Self::new(app_handle));
        if let Ok(mut global) = GLOBAL_BOT_MANAGER.write() {
            *global = Some(manager.clone());
        }
        manager.reload().await;
    }

    pub fn global() -> Option<Arc<RemoteBotManager>> {
        GLOBAL_BOT_MANAGER.read().ok().and_then(|guard| guard.clone())
    }

    pub async fn get_status(&self) -> RemoteBotStatus {
        self.status.read().await.clone()
    }

    /// 热重载或重新启动长轮询
    pub async fn reload(&self) {
        let config = match load_config() {
            Ok(c) => c,
            Err(e) => {
                let mut status = self.status.write().await;
                status.last_error = Some(format!("加载配置失败: {}", e));
                return;
            }
        };

        // 先停止旧任务
        self.stop().await;

        if !config.enabled || config.bot_token.trim().is_empty() {
            let mut status = self.status.write().await;
            status.running = false;
            status.bot_username = None;
            status.bot_name = None;
            status.last_error = None;
            info!("Remote Bot 未启用或未配置 Token，保持空闲状态");
            return;
        }

        let client = Arc::new(TelegramClient::new(config.bot_token.trim()));
        
        // 测试连接并获取 Bot 用户名
        match client.get_me().await {
            Ok(user) => {
                let mut status = self.status.write().await;
                status.running = true;
                status.bot_username = user.username.clone();
                status.bot_name = Some(user.first_name.clone());
                status.last_error = None;
                info!("Remote Bot 连通成功: @{:?} ({})", user.username, user.first_name);
            }
            Err(e) => {
                let mut status = self.status.write().await;
                status.running = false;
                status.last_error = Some(format!("连接 Telegram 失败: {}", e));
                error!("Remote Bot 验证 Token 失败: {}", e);
                return;
            }
        }

        {
            let mut client_lock = self.client.write().await;
            *client_lock = Some(client.clone());
        }

        self.stop_signal.store(false, Ordering::SeqCst);
        let stop_flag = self.stop_signal.clone();
        let bridge = Arc::new(opencode_bridge::OpenCodeBridge::new(client.clone()));
        let router = Arc::new(CommandRouter::new(client.clone(), bridge.clone()));

        // 启动长轮询任务
        let handle = tauri::async_runtime::spawn(async move {
            info!("Telegram Long-Polling 轮询协程启动...");
            let mut offset: Option<i64> = None;

            while !stop_flag.load(Ordering::Relaxed) {
                match client.get_updates(offset, 25).await {
                    Ok(updates) => {
                        for update in updates {
                            let next_offset = update.update_id + 1;
                            offset = Some(match offset {
                                Some(curr) => curr.max(next_offset),
                                None => next_offset,
                            });

                            if let Err(err) = router.dispatch_update(update).await {
                                warn!("处理 Telegram Update 出错: {}", err);
                            }
                        }
                    }
                    Err(e) => {
                        // 遇到网络中断或错误，稍微等待后重试，避免狂刷连接
                        warn!("Telegram 轮询遇到错误: {}, 3秒后重试...", e);
                        tokio::time::sleep(Duration::from_secs(3)).await;
                    }
                }
            }
            info!("Telegram Long-Polling 轮询协程已安全退出。");
        });

        let mut loop_lock = self.running_loop.lock().await;
        *loop_lock = Some(handle);
    }

    pub async fn stop(&self) {
        self.stop_signal.store(true, Ordering::SeqCst);
        let mut loop_lock = self.running_loop.lock().await;
        if let Some(handle) = loop_lock.take() {
            handle.abort();
        }
        let mut status = self.status.write().await;
        status.running = false;
    }
}
