use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{sleep, Duration};
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use log::info;

use super::card_builder::CardBuilder;
use super::client::TelegramClient;
use super::opencode_bridge::OpenCodeBridge;
use super::types::{
    ExternalOpenCodeSessionInfo, HostedOptionItem, HostedQuestionItem,
    SessionHostingDecisionMode, HostedSessionInfo,
};

/// 被托管会话的内部记录结构
struct HostedSessionRecord {
    info: HostedSessionInfo,
    cancel_flag: Arc<AtomicBool>,
}

/// 会话托管守护引擎
pub struct SessionSupervisor {
    hosted_sessions: Arc<RwLock<HashMap<String, HostedSessionRecord>>>,
    active_questions: Arc<RwLock<HashMap<String, HostedQuestionItem>>>,
    client: Arc<TelegramClient>,
    bridge: Arc<OpenCodeBridge>,
}

impl SessionSupervisor {
    pub fn new(client: Arc<TelegramClient>, bridge: Arc<OpenCodeBridge>) -> Self {
        Self {
            hosted_sessions: Arc::new(RwLock::new(HashMap::new())),
            active_questions: Arc::new(RwLock::new(HashMap::new())),
            client,
            bridge,
        }
    }

    /// 启动对目标会话的完全托管
    pub async fn start_hosting(
        &self,
        chat_id: i64,
        session: ExternalOpenCodeSessionInfo,
        mode: SessionHostingDecisionMode,
    ) -> Result<String, String> {
        let session_id = match session.session_id {
            Some(ref sid) => sid.clone(),
            None => return Err("该外部会话尚未分配 Session ID，无法启动托管".to_string()),
        };

        {
            let mut hosts = self.hosted_sessions.write().await;
            if let Some(existing) = hosts.get_mut(&session_id) {
                existing.info.mode = mode.clone();
                return Ok(format!("🔄 已更新会话 `{}` 的托管决策模式为 `{:?}`", session_id, mode));
            }

            let completed_todos = session.todos.iter().filter(|t| t.status == "completed").count();
            let initial_step = session.current_step.unwrap_or(0);

            let hosted_info = HostedSessionInfo {
                session_id: session_id.clone(),
                pid: session.pid,
                cwd: session.cwd.clone(),
                project_name: session.project_name.clone(),
                chat_id,
                mode: mode.clone(),
                started_at: chrono::Utc::now().timestamp(),
                last_observed_step: initial_step,
                completed_todos_count: completed_todos,
                total_todos_count: session.todos.len(),
                active_question_summary: None,
            };

            let cancel_flag = Arc::new(AtomicBool::new(false));
            hosts.insert(
                session_id.clone(),
                HostedSessionRecord {
                    info: hosted_info.clone(),
                    cancel_flag: cancel_flag.clone(),
                },
            );

            // 启动后台托管监听协程
            self.spawn_supervisor_loop(
                chat_id,
                session_id.clone(),
                session.cwd.clone(),
                session.project_name.clone(),
                session.pid,
                cancel_flag,
            );
        }

        // 向 Telegram 推送托管已接管卡片
        let (ack_text, markup) = CardBuilder::hosted_session_started_card(
            &session.project_name,
            session.pid,
            &session_id,
            &mode,
        );
        let _ = self.client.send_message(chat_id, &ack_text, Some(markup), Some("Markdown")).await;

        Ok(format!("🟢 会话 `{}` (PID: {}) 已成功交由 OneSpace + Telegram 完全托管", session_id, session.pid))
    }

    /// 停止指定会话的托管
    pub async fn stop_hosting(&self, session_id: &str) -> bool {
        let mut hosts = self.hosted_sessions.write().await;
        if let Some(record) = hosts.remove(session_id) {
            record.cancel_flag.store(true, Ordering::Relaxed);
            self.active_questions.write().await.remove(session_id);

            let text = format!("🛑 *已退出对会话 `{}` 的托管守护*\n\n会话已恢复为普通外部运行状态，可随时使用 `/sessions` 或 `/host` 重新接管。", session_id);
            let _ = self.client.send_message(record.info.chat_id, &text, None, Some("Markdown")).await;
            true
        } else {
            false
        }
    }

    /// 停止特定 chat 的所有托管
    pub async fn stop_all_for_chat(&self, chat_id: i64) -> usize {
        let mut hosts = self.hosted_sessions.write().await;
        let mut to_remove = Vec::new();
        for (sid, rec) in hosts.iter() {
            if rec.info.chat_id == chat_id {
                rec.cancel_flag.store(true, Ordering::Relaxed);
                to_remove.push(sid.clone());
            }
        }
        let count = to_remove.len();
        for sid in to_remove {
            hosts.remove(&sid);
            self.active_questions.write().await.remove(&sid);
        }
        count
    }

    /// 列出当前正在托管的会话快照
    pub async fn list_hosted_sessions(&self, chat_id: Option<i64>) -> Vec<HostedSessionInfo> {
        let hosts = self.hosted_sessions.read().await;
        hosts.values()
            .filter(|r| chat_id.map(|cid| r.info.chat_id == cid).unwrap_or(true))
            .map(|r| r.info.clone())
            .collect()
    }

    /// 检查指定会话是否在托管中
    pub async fn is_session_hosted(&self, session_id: &str) -> bool {
        self.hosted_sessions.read().await.contains_key(session_id)
    }

    /// 获取指定会话当前阻塞的提问
    pub async fn get_active_question(&self, session_id: &str) -> Option<HostedQuestionItem> {
        self.active_questions.read().await.get(session_id).cloned()
    }

    /// 用户在 Telegram 点击提问选项后进行回答注入
    pub async fn handle_answer_question(
        &self,
        chat_id: i64,
        session_id: &str,
        option_index: usize,
    ) -> Result<String, String> {
        let (cwd, chosen_label) = {
            let mut q_map = self.active_questions.write().await;
            let q_item = match q_map.get(session_id) {
                Some(q) => q,
                None => return Err("未找到待应答的提问（可能已被处理或过期）".to_string()),
            };

            let opt = match q_item.options.iter().find(|o| o.index == option_index) {
                Some(o) => o.clone(),
                None => return Err(format!("未找到选项序号 {}", option_index)),
            };

            let hosts = self.hosted_sessions.read().await;
            let host_rec = match hosts.get(session_id) {
                Some(r) => r,
                None => return Err("该会话未处于托管状态".to_string()),
            };

            let cwd = host_rec.info.cwd.clone();
            let label = opt.label.clone();
            q_map.remove(session_id);
            (cwd, label)
        };

        // 将选中的选项内容注入已有会话继续执行
        self.bridge.execute_prompt_for_session(
            chat_id,
            PathBuf::from(&cwd),
            session_id,
            &chosen_label,
        ).await?;

        Ok(chosen_label)
    }

    /// 切换决策模式
    pub async fn toggle_decision_mode(&self, session_id: &str) -> Option<SessionHostingDecisionMode> {
        let mut hosts = self.hosted_sessions.write().await;
        if let Some(record) = hosts.get_mut(session_id) {
            let new_mode = match record.info.mode {
                SessionHostingDecisionMode::Manual => SessionHostingDecisionMode::AutoRecommended,
                SessionHostingDecisionMode::AutoRecommended => SessionHostingDecisionMode::Manual,
            };
            record.info.mode = new_mode.clone();
            Some(new_mode)
        } else {
            None
        }
    }

    /// 启动针对特定会话的后台监督循环
    fn spawn_supervisor_loop(
        &self,
        chat_id: i64,
        session_id: String,
        cwd: String,
        project_name: String,
        pid: u32,
        cancel_flag: Arc<AtomicBool>,
    ) {
        let client = self.client.clone();
        let bridge = self.bridge.clone();
        let hosted_sessions = self.hosted_sessions.clone();
        let active_questions = self.active_questions.clone();

        tokio::spawn(async move {
            let (db_path, log_path) = hosted_progress_paths();

            let mut last_step: u32 = 0;
            let mut known_completed_todos: HashSet<String> = HashSet::new();
            let mut handled_question_calls: HashSet<String> = HashSet::new();
            let mut consecutive_idle_checks = 0;

            // 初始化已知完成待办
            if let Some(snap) = inspect_session_db(&db_path, &session_id) {
                last_step = snap.cur_step;
                for (c, s) in snap.todos {
                    if s == "completed" {
                        known_completed_todos.insert(c);
                    }
                }
            }

            info!("[Supervisor] 针对会话 {} (PID: {}) 的托管守护协程已启动", session_id, pid);

            while !cancel_flag.load(Ordering::Relaxed) {
                sleep(Duration::from_millis(2000)).await;

                if cancel_flag.load(Ordering::Relaxed) {
                    break;
                }

                // 纯同步检查数据库快照，确保 Connection 不跨越任何 await 点
                let snap_opt = inspect_session_db(&db_path, &session_id);

                if let Some(snap) = snap_opt {
                    // A. 待办进展检测 (Todo Milestone Progression)
                    let mut new_done = Vec::new();
                    let total_cnt = snap.todos.len();
                    let mut completed_cnt = 0;

                    for (content, status) in &snap.todos {
                        if status == "completed" {
                            completed_cnt += 1;
                            if known_completed_todos.insert(content.clone()) {
                                new_done.push(content.clone());
                            }
                        }
                    }

                    // 更新托管快照中的待办进度
                    {
                        let mut hosts = hosted_sessions.write().await;
                        if let Some(rec) = hosts.get_mut(&session_id) {
                            rec.info.completed_todos_count = completed_cnt;
                            rec.info.total_todos_count = total_cnt;
                        }
                    }

                    // 推送新完成待办通知
                    for done_item in new_done {
                        let notice = CardBuilder::hosted_todo_completed_notice(
                            &project_name,
                            &done_item,
                            completed_cnt,
                            total_cnt,
                        );
                        let _ = client.send_message(chat_id, &notice, None, Some("Markdown")).await;
                    }

                    // B. 步骤跨越检测 (Step Progression)
                    if snap.cur_step > last_step {
                        let mut hosts = hosted_sessions.write().await;
                        if let Some(rec) = hosts.get_mut(&session_id) {
                            rec.info.last_observed_step = snap.cur_step;
                        }
                        last_step = snap.cur_step;
                    }

                    // C. 提问与决策检测 (Interactive Questioning & Decision)
                    if let Some((call_id, q_text, options)) = snap.pending_question {
                        if !handled_question_calls.contains(&call_id) {
                            let question_item = HostedQuestionItem {
                                call_id: call_id.clone(),
                                question_text: q_text.clone(),
                                options: options.clone(),
                            };

                            let current_mode = {
                                let hosts = hosted_sessions.read().await;
                                hosts.get(&session_id).map(|r| r.info.mode.clone()).unwrap_or(SessionHostingDecisionMode::Manual)
                            };

                            handled_question_calls.insert(call_id.clone());

                            // 检查是否命中自动推荐决策
                            let recommended_opt = options.iter().find(|o| o.is_recommended);
                            if current_mode == SessionHostingDecisionMode::AutoRecommended && recommended_opt.is_some() {
                                let rec = recommended_opt.unwrap();
                                info!("[Supervisor] 命中自动推荐决策: 会话 {} 自动采纳 `{}`", session_id, rec.label);

                                let _ = bridge.execute_prompt_for_session(
                                    chat_id,
                                    PathBuf::from(&cwd),
                                    &session_id,
                                    &rec.label,
                                ).await;

                                let auto_card = CardBuilder::hosted_auto_decision_card(
                                    &project_name,
                                    &q_text,
                                    &rec.label,
                                );
                                let _ = client.send_message(chat_id, &auto_card, None, Some("Markdown")).await;
                            } else {
                                // 人工决断：注册至待答复并推送带有选项按钮的卡片
                                active_questions.write().await.insert(session_id.clone(), question_item);

                                let (q_msg, markup) = CardBuilder::hosted_question_card(
                                    &project_name,
                                    &session_id,
                                    &q_text,
                                    &options,
                                );
                                let _ = client.send_message(chat_id, &q_msg, Some(markup), Some("Markdown")).await;
                            }
                        }
                    }
                }

                // D. 状态与完工检测 (Idle / Completion Detection)
                let is_busy = check_session_busy_state(&log_path, &session_id);
                if !is_busy {
                    consecutive_idle_checks += 1;
                } else {
                    consecutive_idle_checks = 0;
                }

                // 连续 2 次（约 4 秒）确认处于 Idle，判定会话圆满交付并结束托管
                if consecutive_idle_checks >= 2 {
                    info!("[Supervisor] 会话 {} (PID: {}) 已圆满结束执行，推送最终报告", session_id, pid);

                    let (started_at, total_steps) = {
                        let hosts = hosted_sessions.read().await;
                        hosts.get(&session_id)
                            .map(|r| (r.info.started_at, r.info.last_observed_step))
                            .unwrap_or((chrono::Utc::now().timestamp(), last_step))
                    };

                    let elapsed_secs = (chrono::Utc::now().timestamp() - started_at).max(0);
                    let elapsed_desc = format!("{} 分 {} 秒", elapsed_secs / 60, elapsed_secs % 60);

                    // 检索 Token 消耗
                    let (tokens_in, tokens_out) = if let Some(snap) = inspect_session_db(&db_path, &session_id) {
                        snap.tokens
                    } else {
                        (0, 0)
                    };

                    let (done_text, markup) = CardBuilder::hosted_session_completed_card(
                        &project_name,
                        pid,
                        &session_id,
                        &elapsed_desc,
                        total_steps,
                        tokens_in,
                        tokens_out,
                    );

                    let _ = client.send_message(chat_id, &done_text, Some(markup), Some("Markdown")).await;

                    hosted_sessions.write().await.remove(&session_id);
                    active_questions.write().await.remove(&session_id);
                    break;
                }
            }

            info!("[Supervisor] 会话 {} 的托管守护协程已正常退出", session_id);
        });
    }
}

/// 解析托管进度读取所用的 OpenCode 数据库与日志路径，
/// 与终端会话发现共用同一来源，确保 macOS 上也读取相同位置。
pub(super) fn hosted_progress_paths() -> (PathBuf, PathBuf) {
    (
        super::opencode_inspector::resolve_opencode_db_path(),
        super::opencode_inspector::resolve_opencode_log_path(),
    )
}

/// 数据库查询快照（确保均为 Send 类型）
pub(super) struct SessionInspectionSnapshot {
    pub(super) cur_step: u32,
    pub(super) todos: Vec<(String, String)>,
    pub(super) pending_question: Option<(String, String, Vec<HostedOptionItem>)>,
    pub(super) tokens: (i64, i64),
}

/// 纯同步查询 SQLite 数据库，避免 Connection 跨越 await 点
pub(super) fn inspect_session_db(
    db_path: &Path,
    session_id: &str,
) -> Option<SessionInspectionSnapshot> {
    let conn = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX | OpenFlags::SQLITE_OPEN_URI,
    ).ok()?;

    // 1. step
    let mut cur_step = 0;
    let step_sql = "SELECT count(*) FROM part WHERE session_id = ?1 AND json_extract(data, '$.type') = 'step-start'";
    if let Ok(mut stmt) = conn.prepare(step_sql) {
        if let Ok(c) = stmt.query_row([session_id], |row| row.get::<_, u32>(0)) {
            cur_step = c;
        }
    }

    // 2. todos
    let mut todos = Vec::new();
    let todos_sql = "SELECT content, status FROM todo WHERE session_id = ?1";
    if let Ok(mut stmt) = conn.prepare(todos_sql) {
        if let Ok(rows) = stmt.query_map([session_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        }) {
            for r in rows.flatten() {
                todos.push(r);
            }
        }
    }

    // 3. pending question
    let mut pending_question = None;
    let q_sql = "SELECT data FROM part \
                 WHERE session_id = ?1 \
                   AND json_extract(data, '$.type') = 'tool' \
                   AND json_extract(data, '$.tool') IN ('question', 'ask_question') \
                 ORDER BY time_created DESC LIMIT 1";

    if let Ok(mut stmt) = conn.prepare(q_sql) {
        if let Ok(data_str) = stmt.query_row([session_id], |row| row.get::<_, String>(0)) {
            if let Ok(v) = serde_json::from_str::<Value>(&data_str) {
                let state = v.get("state");
                let status = state.and_then(|s| s.get("status")).and_then(|st| st.as_str()).unwrap_or("");
                let call_id = v.get("callID").and_then(|c| c.as_str()).unwrap_or("").to_string();

                if (status == "running" || status == "pending" || status.is_empty()) && !call_id.is_empty() {
                    if let Some(questions_arr) = state.and_then(|s| s.get("input")).and_then(|i| i.get("questions")).and_then(|q| q.as_array()) {
                        if let Some(first_q) = questions_arr.first() {
                            let q_text = first_q.get("question").and_then(|q| q.as_str()).unwrap_or("请确认执行方案").to_string();
                            let mut options = Vec::new();

                            if let Some(opts_arr) = first_q.get("options").and_then(|o| o.as_array()) {
                                for (idx, opt_val) in opts_arr.iter().enumerate() {
                                    let label = opt_val.get("label").and_then(|l| l.as_str()).unwrap_or("").to_string();
                                    let desc = opt_val.get("description").and_then(|d| d.as_str()).map(|s| s.to_string());
                                    let is_rec = label.contains("(Recommended)")
                                        || label.contains("推荐")
                                        || desc.as_deref().map(|d| d.contains("推荐") || d.contains("Recommended")).unwrap_or(false);

                                    options.push(HostedOptionItem {
                                        index: idx + 1,
                                        label,
                                        description: desc,
                                        is_recommended: is_rec,
                                    });
                                }
                            }

                            pending_question = Some((call_id, q_text, options));
                        }
                    }
                }
            }
        }
    }

    // 4. tokens
    let mut tokens = (0i64, 0i64);
    let token_sql = "SELECT tokens_input, tokens_output FROM session WHERE id = ?1 UNION SELECT tokens_input, tokens_output FROM session_v2 WHERE id = ?1";
    if let Ok(mut stmt) = conn.prepare(token_sql) {
        if let Ok(row) = stmt.query_row([session_id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))) {
            tokens = row;
        }
    }

    Some(SessionInspectionSnapshot {
        cur_step,
        todos,
        pending_question,
        tokens,
    })
}

/// 辅助：检查日志中该 session 的最新 busy 状态
fn check_session_busy_state(log_path: &Path, session_id: &str) -> bool {
    use std::fs::File;
    use std::io::{Read, Seek, SeekFrom};

    let mut file = match File::open(log_path) {
        Ok(f) => f,
        Err(_) => return false,
    };

    let metadata = match file.metadata() {
        Ok(m) => m,
        Err(_) => return false,
    };

    let file_len = metadata.len();
    if file_len == 0 {
        return false;
    }

    let read_size = std::cmp::min(file_len, 256 * 1024) as usize;
    let seek_pos = file_len.saturating_sub(read_size as u64);
    if file.seek(SeekFrom::Start(seek_pos)).is_err() {
        return false;
    }

    let mut buffer = vec![0u8; read_size];
    if file.read_exact(&mut buffer).is_err() {
        return false;
    }

    let text = String::from_utf8_lossy(&buffer);
    let mut last_status: Option<bool> = None;

    for line in text.lines() {
        if line.contains(session_id) {
            if line.contains("type=session.status") {
                if line.contains("status=busy") {
                    last_status = Some(true);
                } else if line.contains("status=idle") {
                    last_status = Some(false);
                }
            } else if line.contains("type=session.idle") {
                last_status = Some(false);
            }
        }
    }

    last_status.unwrap_or(false)
}
