pub mod auth;
pub mod card_builder;
pub mod client;
pub mod commands;
pub mod opencode_bridge;
pub mod opencode_inspector;
pub mod plan_progress;
pub mod router;
pub mod session_inventory;
pub mod session_supervisor;
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
    router: RwLock<Option<Arc<CommandRouter>>>,
    status: RwLock<RemoteBotStatus>,
    running_loop: Mutex<Option<JoinHandle<()>>>,
    stop_signal: Arc<AtomicBool>,
}

impl RemoteBotManager {
    pub fn new(app_handle: AppHandle) -> Self {
        Self {
            app_handle,
            client: RwLock::new(None),
            router: RwLock::new(None),
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

                // 注册 Telegram 官方原生菜单按钮 (setMyCommands)
                let default_commands = vec![
                    BotCommand { command: "running".to_string(), description: "查看终端运行中的 OpenCode 会话进度".to_string() },
                    BotCommand { command: "host".to_string(), description: "完全托管运行中的会话 (交互/通知/决策)".to_string() },
                    BotCommand { command: "unhost".to_string(), description: "退出当前会话托管守护".to_string() },
                    BotCommand { command: "planning".to_string(), description: "启动需求调研与方案规划 (Planning)".to_string() },
                    BotCommand { command: "plans".to_string(), description: "查看工作区方案列表及状态".to_string() },
                    BotCommand { command: "progress".to_string(), description: "查看当前或指定方案的实施进度".to_string() },
                    BotCommand { command: "diff".to_string(), description: "查看最新代码改动对比或方案差异".to_string() },
                    BotCommand { command: "coding".to_string(), description: "直接实施指定的已规划方案".to_string() },
                    BotCommand { command: "projects".to_string(), description: "切换目标项目工作区".to_string() },
                    BotCommand { command: "status".to_string(), description: "查看主机状态、Git分支与系统信息".to_string() },
                    BotCommand { command: "cancel".to_string(), description: "中断当前正在执行的任务或会话".to_string() },
                    BotCommand { command: "help".to_string(), description: "查看使用说明与指令指南".to_string() },
                ];
                let client_cmd = client.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = client_cmd.set_my_commands(default_commands).await {
                        warn!("注册 Telegram 原生 Bot 菜单失败 (不影响核心通信): {}", e);
                    } else {
                        info!("Telegram 原生 Bot 菜单注册成功！");
                    }
                });
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
        {
            let mut router_lock = self.router.write().await;
            *router_lock = Some(router.clone());
        }

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

    pub async fn get_router(&self) -> Option<Arc<CommandRouter>> {
        self.router.read().await.clone()
    }

    pub async fn stop(&self) {
        self.stop_signal.store(true, Ordering::SeqCst);
        let mut loop_lock = self.running_loop.lock().await;
        if let Some(handle) = loop_lock.take() {
            handle.abort();
        }
        *self.router.write().await = None;
        let mut status = self.status.write().await;
        status.running = false;
    }
}
