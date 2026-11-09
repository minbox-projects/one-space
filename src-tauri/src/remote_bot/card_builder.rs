use super::opencode_inspector;
use super::plan_progress::{PlanExecutionStatus, PlanProgressInfo};
use super::session_inventory::{
    IdentifierResolution, PlanExecutionState, SessionEntry, SessionProgressDetail,
};
use super::types::{ExternalOpenCodeSessionInfo, InlineKeyboardButton, InlineKeyboardMarkup};

/// Telegram 移动端高可读性卡片构建器
pub struct CardBuilder;

impl CardBuilder {
    // =========================================================================
    // 基础排版原语 (Primitives)
    // =========================================================================

    /// 生成原生引用块 (Telegram Markdown 原生 Blockquote)
    pub fn quote(text: &str) -> String {
        text.lines()
            .map(|l| if l.is_empty() { ">".to_string() } else { format!("> {}", l) })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// 将键值对列表渲染为树形引用卡片块
    pub fn quote_tree(items: &[(&str, &str)]) -> String {
        if items.is_empty() {
            return String::new();
        }
        if items.len() == 1 {
            return format!("> ▫️ {}: {}", items[0].0, items[0].1);
        }

        let mut lines = Vec::new();
        for (i, (key, val)) in items.iter().enumerate() {
            let prefix = if i == items.len() - 1 { "└─" } else { "├─" };
            lines.push(format!("> {} {}: {}", prefix, key, val));
        }
        lines.join("\n")
    }

    /// 提取内联按钮标签（安全长度截断，避免在手机屏幕上被省略号截断）
    /// - single_column (单列按键): max_chars = 14
    /// - multi_column (双列/多列并排): max_chars = 8
    pub fn format_button_label(num: Option<usize>, full_text: &str, max_chars: usize) -> String {
        let num_emojis = ["0️⃣", "1️⃣", "2️⃣", "3️⃣", "4️⃣", "5️⃣", "6️⃣", "7️⃣", "8️⃣", "9️⃣", "🔟"];
        let badge = match num {
            Some(n) if n <= 10 => format!("{} ", num_emojis[n]),
            Some(_) => "🔘 ".to_string(),
            None => String::new(),
        };

        let mut cleaned = full_text.trim();

        // 移除前导序号标记，例如 "1、", "1. ", "[1] ", "(1) " 等
        if let Ok(prefix_re) = regex::Regex::new(r"^(?:\[?(\d{1,2})[\]\.、\)]|\((\d{1,2})\))\s*") {
            if let Some(m) = prefix_re.find(cleaned) {
                cleaned = cleaned[m.end()..].trim();
            }
        }

        // 移除常见前缀词
        for prefix in &["推荐：", "推荐:", "【推荐】", "[推荐]", "推荐 - ", "推荐-", "建议：", "建议:"] {
            if let Some(stripped) = cleaned.strip_prefix(prefix) {
                cleaned = stripped.trim();
            }
        }

        // 遇到解释性分隔符截取前半部
        let separators = [" —— ", "——", " — ", "—", " - ", "：", ": ", " (", "（", "，", ", ", "；", "; "];
        for sep in &separators {
            if let Some(pos) = cleaned.find(sep) {
                let part = cleaned[..pos].trim();
                if !part.is_empty() {
                    cleaned = part;
                    break;
                }
            }
        }

        cleaned = cleaned.trim_matches('*').trim_matches('`').trim_matches('_').trim();

        let label = if cleaned.chars().count() > max_chars {
            let truncated: String = cleaned.chars().take(max_chars - 1).collect();
            format!("{}…", truncated)
        } else if cleaned.is_empty() {
            "选择".to_string()
        } else {
            cleaned.to_string()
        };

        format!("{}{}", badge, label)
    }

    // =========================================================================
    // 1. 接入与鉴权类卡片 (3 个)
    // =========================================================================

    /// 卡片 1: 首绑成功欢迎卡片
    pub fn welcome_card(chat_id: i64, hostname: &str) -> String {
        format!(
            "🎉 *OneSpace 移动工作台*\n\
            > ✨ *首位管理员已成功绑定*\n\
            > ├─ 管理员 ID: `{}`\n\
            > ├─ 本地主机: `{}`\n\
            > └─ 通信状态: `🟢 活跃连接 (Long-Polling)`\n\n\
            📌 *常用快捷指令*\n\
            • `/status` ── 查看 Mac 状态与 Git 变更\n\
            • `/projects` ── 切换当前目标项目\n\
            • `/planning <需求>` ── 唤起需求规划与方案制定\n\
            • `/plans` ── 查看工作区方案进度\n\n\
            _💡 提示：平时直接发送文字，机器人将直接基于代码库为您分析与解答。_",
            chat_id,
            hostname
        )
    }

    /// 卡片 2: 未授权访问拦截卡片
    pub fn unauthorized_card(chat_id: i64) -> String {
        format!(
            "⛔ *未授权的访问请求*\n\
            > ⚠️ *该 Mac 工作台受本地安全策略保护*\n\
            > ├─ 来源类别: `Telegram 私聊`\n\
            > └─ 拦截状态: `已自动阻断`\n\n\
            🔑 *您的 Chat ID*\n\
            `{}`\n\
            _（点击上方数字可一键复制）_\n\n\
            👉 *授权开通指引*：\n\
            请在 Mac 打开 *OneSpace* → *设置* → *服务* → *Telegram 远程工作台*，将该 ID 添加至管理员白名单。",
            chat_id
        )
    }

    /// 卡片 3: 帮助与功能概览卡片
    pub fn help_card() -> String {
        "🤖 *OneSpace 移动智能工作台*\n\
        > 💡 *已通过授权认证，随时可以下发指令或直接交流*\n\n\
        🛠️ *工作台运维*\n\
        • `/status` ── Mac 本地状态与 Git 提交\n\
        • `/sessions` ── 统一会话概览与进度详情 (`/sessions <id>`)\n\
        • `/host [id]` ── 完全托管会话 (交互提问/主动通知/智能决策)\n\
        • `/unhost` ── 退出当前会话托管守护\n\
        • `/prompt <内容>` ── 向运行中的 OpenCode 会话发送提示词\n\
        • `/projects` ── 切换活跃工作区项目\n\n\
        🚀 *AI 研发流水线*\n\
        • `/planning <需求>` ── 唤起需求调研与方案规划\n\
        • `/plans` ── 查看所有规划方案与状态\n\
        • `/coding <id>` ── 直接实施指定的已规划方案\n\
        • `/diff [id]` ── 查看最新代码改动对比\n\
        • `/cancel` ── 随时紧急中断当前任务\n\n\
        _💬 自然对话：直接发送任何技术问题，OpenCode 自动结合项目代码进行解答、小改动实施或方案规划！_".to_string()
    }

    /// `/running` 与无参数 `/host` 的重定向提示。
    pub fn sessions_redirect_text() -> String {
        "👉 会话列表与进度查看已统一到 `/sessions`。\n发送 `/sessions` 查看当前活跃会话。"
            .to_string()
    }

    /// `/progress` 的重定向提示：有方案 ID 时指向 `/sessions <planId>`。
    pub fn progress_redirect_text(plan_id: Option<&str>) -> String {
        match plan_id {
            Some(plan_id) => format!(
                "👉 方案进度已合并到 `/sessions <planId>`。\n发送 `/sessions {}` 查看该方案进度。",
                plan_id
            ),
            None => "👉 方案进度已合并到 `/sessions <planId>`。\n发送 `/sessions` 查看活跃会话，或使用 `/plans` 浏览方案后以 `/sessions <planId>` 查看指定方案进度。".to_string(),
        }
    }

    /// 多目标 `/prompt` 的重定向提示。
    pub fn prompt_sessions_redirect_text() -> String {
        "👉 检测到多个运行中的会话。\n请先发送 `/sessions` 查看并选择目标会话，再发送 `/prompt <内容>`。".to_string()
    }

    // =========================================================================
    // 2. 工作台状态类卡片 (3 个)
    // =========================================================================

    /// 卡片 4: Mac 工作台运行状态卡片
    pub fn status_card(
        hostname: &str,
        current_time: &str,
        project: &str,
        git_branch: &str,
        git_status: &str,
        latest_commit: &str,
        auto_approve: bool,
    ) -> (String, Option<InlineKeyboardMarkup>) {
        Self::status_card_with_sessions(
            hostname,
            current_time,
            project,
            git_branch,
            git_status,
            latest_commit,
            auto_approve,
            0,
        )
    }

    /// 卡片 4 扩展: Mac 工作台运行状态卡片（带外部会话探测）
    pub fn status_card_with_sessions(
        hostname: &str,
        current_time: &str,
        project: &str,
        git_branch: &str,
        git_status: &str,
        latest_commit: &str,
        auto_approve: bool,
        ext_sessions_count: usize,
    ) -> (String, Option<InlineKeyboardMarkup>) {
        let ext_session_line = if ext_sessions_count > 0 {
            format!("\n> └─ 终端 OpenCode: `🚀 {} 个会话运行中`", ext_sessions_count)
        } else {
            String::new()
        };

        let text = format!(
            "🖥️ *Mac 本地工作台状态*\n\
            > 🟢 *系统运行平稳 ╎ 实时长轮询在线*\n\
            > ├─ 设备主机: `{}`\n\
            > ├─ 系统时间: `{}`\n\
            > └─ 权限审批: `{}`\n\n\
            📁 *当前活跃工作区*\n\
            > ├─ 项目名称: `{}`\n\
            > ├─ Git 分支: `{}` ({})\n\
            > └─ 最新提交: `{}`{}\n\n\
            _👉 输入 `/running` 查看终端会话，`/diff` 查看代码变更。_",
            hostname,
            current_time,
            if auto_approve { "⚡ 自动确认 (Auto-Approve)" } else { "🔒 手动审批" },
            project,
            git_branch,
            git_status,
            latest_commit,
            ext_session_line
        );

        let mut row = vec![
            InlineKeyboardButton::callback("📁 切换项目", "open_projects"),
            InlineKeyboardButton::callback("📋 查看方案", "open_plans"),
        ];

        if ext_sessions_count > 0 {
            row.push(InlineKeyboardButton::callback("🚀 运行中会话", "view_ext_session"));
        }

        let markup = InlineKeyboardMarkup::new(vec![row]);

        (text, Some(markup))
    }

    /// 卡片 5: 工作区项目管理卡片
    pub fn projects_card(active_workspace: &str, physical_path: &str, projects: &[&str]) -> (String, InlineKeyboardMarkup) {
        let text = format!(
            "📂 *工作区项目管理*\n\
            > ├─ 当前活跃: `{}`\n\
            > └─ 本地路径: `{}`\n\n\
            点击下方按钮快速切换指令生效的目标项目：\n\n\
            💡 *小提示：输入 `/projects <名称>` 可快速搜索全量系统工作空间。*",
            active_workspace,
            physical_path
        );

        let mut rows: Vec<Vec<InlineKeyboardButton>> = Vec::new();
        let mut pending_pair: Vec<InlineKeyboardButton> = Vec::new();

        for p in projects {
            let is_active = *p == active_workspace;
            if is_active {
                // 当前选中项独占一行突出展示
                if !pending_pair.is_empty() {
                    rows.push(std::mem::take(&mut pending_pair));
                }
                let label = if p.chars().count() > 14 {
                    format!("📁 {}… (当前选中)", p.chars().take(12).collect::<String>())
                } else {
                    format!("📁 {} (当前选中)", p)
                };
                rows.push(vec![InlineKeyboardButton::callback(label, format!("set_project:{}", p))]);
            } else {
                let label = if p.chars().count() > 10 {
                    format!("📁 {}…", p.chars().take(9).collect::<String>())
                } else {
                    format!("📁 {}", p)
                };
                pending_pair.push(InlineKeyboardButton::callback(label, format!("set_project:{}", p)));
                if pending_pair.len() == 2 {
                    rows.push(std::mem::take(&mut pending_pair));
                }
            }
        }
        if !pending_pair.is_empty() {
            rows.push(pending_pair);
        }

        (text, InlineKeyboardMarkup::new(rows))
    }

    /// 卡片 5-B: 搜索多项结果候选卡片
    pub fn projects_search_card(keyword: &str, candidates: &[(&str, &str)]) -> (String, InlineKeyboardMarkup) {
        let text = format!(
            "🔍 *工作空间搜索结果*\n\
            > ├─ 搜索关键词: `{}`\n\
            > └─ 匹配到 `{}` 个候选项目\n\n\
            请点击下方按钮选择要切换的项目：",
            keyword,
            candidates.len()
        );

        let mut rows = Vec::new();
        for (name, path) in candidates {
            let label = if name.chars().count() > 14 {
                format!("📁 {}…", name.chars().take(13).collect::<String>())
            } else {
                format!("📁 {}", name)
            };
            rows.push(vec![InlineKeyboardButton::callback(label, format!("set_project:{}", path))]);
        }

        (text, InlineKeyboardMarkup::new(rows))
    }

    /// 卡片 5-C: 未找到匹配工作空间
    pub fn project_not_found_card(keyword: &str) -> String {
        format!(
            "⚠️ *未找到匹配的工作空间*\n\
            > 搜索关键词: `{}`\n\n\
            系统中未找到包含该名称的工作空间。可发送 `/projects` 查看精选常用项目，或在桌面端「设置 -> Telegram」中添加常用工作区。",
            keyword
        )
    }

    /// 卡片 6: 项目切换成功确认态
    pub fn project_switch_confirmed(project: &str, timestamp: &str) -> String {
        format!(
            "✅ *工作区已成功切换*\n\
            > ├─ 当前项目: `{}`\n\
            > └─ 生效时间: `{}`\n\n\
            后续下发的所有规划流水线与 Coding 任务将全部作用于该项目。",
            project,
            timestamp
        )
    }

    // =========================================================================
    // 3. 规划 (Planning) 流水线卡片 (7 个)
    // =========================================================================

    /// 卡片 7: 需求/对话接收确认卡片
    pub fn new_feature_ack_card(project_name: &str, prompt: &str, is_explicit_planning: bool) -> (String, InlineKeyboardMarkup) {
        let (header, mode, label, hint) = if is_explicit_planning {
            (
                "🚀 *已开启需求规划流程 (Planning)*",
                "方案调研与规划 (Planning 技能)",
                "📝 *规划目标*：",
                "⏳ _正在拉起 OpenCode 执行深入分析与规划，请稍候..._",
            )
        } else {
            (
                "💬 *已接收对话消息*",
                "自然对话 (遵循 OpenCode 流程)",
                "📝 *消息内容*：",
                "⏳ _OpenCode 正在分析处理中，请稍候..._",
            )
        };

        let text = format!(
            "{}\n\
            > ├─ 目标项目: `{}`\n\
            > └─ 任务模式: `{}`\n\n\
            {}\n\
            > {}\n\n\
            {}",
            header,
            project_name,
            mode,
            label,
            prompt.trim(),
            hint
        );

        let markup = InlineKeyboardMarkup::new(vec![vec![
            InlineKeyboardButton::callback("🛑 中断任务", "cancel_task"),
        ]]);

        (text, markup)
    }

    /// 卡片 8: OpenCode 思考与执行状态卡片
    pub fn planning_thinking_card(is_initial: bool, session_id: Option<&str>) -> String {
        if is_initial {
            "🔍 *OpenCode 正在深入分析*\n\
            > ▫️ 分析阶段: `读取工程规范与代码库依赖`\n\
            > ▫️ 进程状态: `运行中`\n\n\
            ⏳ _正在结合 MEMORY.md 与现有交互代码推导设计方案中..._".to_string()
        } else {
            let sid = session_id.unwrap_or("活跃交互");
            format!(
                "🔍 *OpenCode 正在根据答复深入分析*\n\
                > ▫️ 活跃会话: `{}`\n\
                > ▫️ 处理状态: `推进下一阶段决策`\n\n\
                ⏳ _正在结合您的指导意见继续推进方案完善..._",
                sid
            )
        }
    }

    /// 卡片 9: 规划澄清提问决策卡片
    pub fn question_card(
        question_text: &str,
        options: &[(usize, String, Option<String>)],
        round: Option<usize>,
    ) -> (String, InlineKeyboardMarkup) {
        let round_str = round.map(|r| format!("第 {} 轮澄清决策", r)).unwrap_or_else(|| "架构决策澄清".to_string());
        let mut lines = Vec::new();
        lines.push("🤔 *方案澄清与架构决策*".to_string());
        lines.push(format!("> ▫️ 提问进度: `{}`", round_str));
        lines.push("".to_string());
        lines.push("❓ *决策议题*：".to_string());
        lines.push(question_text.trim().to_string());
        lines.push("".to_string());

        let num_emojis = ["0️⃣", "1️⃣", "2️⃣", "3️⃣", "4️⃣", "5️⃣", "6️⃣", "7️⃣", "8️⃣", "9️⃣", "🔟"];

        if !options.is_empty() {
            lines.push("📋 *备选方案*：".to_string());
            for (num, title, desc) in options {
                let badge = if *num <= 10 { num_emojis[*num] } else { "🔘" };
                lines.push(format!("{} {}", badge, title.trim()));
                if let Some(d) = desc {
                    if !d.trim().is_empty() {
                        lines.push(format!("   {}", d.trim()));
                    }
                }
                lines.push("".to_string());
            }
        }

        lines.push("_👉 点击下方按钮一键提交决策，或直接回复数字序号：_".to_string());

        let mut keyboard_rows = Vec::new();
        for (num, title, _) in options {
            let btn_label = Self::format_button_label(Some(*num), title, 14);
            keyboard_rows.push(vec![InlineKeyboardButton::callback(
                btn_label,
                format!("ans_choice:{}", num),
            )]);
        }

        (lines.join("\n"), InlineKeyboardMarkup::new(keyboard_rows))
    }

    /// 卡片 10: 提问决策已提交状态
    pub fn question_answered_card(choice_label: &str) -> String {
        format!(
            "✅ *决策已提交确认*\n\
            > ├─ 已选方案: `{}`\n\
            > └─ 处理状态: `已同步至 OpenCode 执行上下文`\n\n\
            ⏳ _OpenCode 正在根据您的决策继续推进方案生成..._",
            choice_label.trim()
        )
    }

    /// 卡片 11: OpenCode 分析与自然对话回复卡片
    pub fn analysis_reply_card(workspace: &str, content: &str) -> String {
        format!(
            "💬 *OpenCode 回复与分析*\n\
            > ▫️ 当前工作区: `{}`\n\n\
            {}\n\n\
            _👉 您可直接打字继续讨论，或输入 `/cancel` 结束本轮对话。_",
            workspace,
            content.trim()
        )
    }

    /// 卡片 12: 方案规划完成与审批卡片
    pub fn plan_review_card(plan_id: &str, summary: &str, steps_count: Option<usize>) -> (String, InlineKeyboardMarkup) {
        let steps_desc = steps_count
            .map(|c| format!("共 {} 个实施步骤 (TDD)", c))
            .unwrap_or_else(|| "TDD 测试驱动实施".to_string());

        let text = format!(
            "📋 *方案规划完成 ╎ 等待确认*\n\
            > ├─ 方案标识: `{}`\n\
            > ├─ 规划文件: `spec.md` + `plan.md`\n\
            > └─ 预计步骤: `{}`\n\n\
            📄 *方案核心摘要*：\n\
            {}\n\n\
            _👉 点击下方按钮进行选择（确认即可进入编码实施，补充可打字输入意见）：_",
            plan_id,
            steps_desc,
            Self::quote(summary.trim())
        );

        let safe_tag = if plan_id.len() > 36 { &plan_id[..36] } else { plan_id };
        let markup = InlineKeyboardMarkup::new(vec![vec![
            InlineKeyboardButton::callback("✅ 确认启动实施", format!("approve_plan:{}", safe_tag)),
            InlineKeyboardButton::callback("✏️ 补充修改意见", format!("revise_plan:{}", safe_tag)),
        ]]);

        (text, markup)
    }

    /// 卡片 13: 方案补充指引提示
    pub fn revise_prompt_card(plan_id: &str) -> String {
        format!(
            "✏️ *方案补充建议*\n\
            > ▫️ 针对方案: `{}`\n\n\
            请直接在聊天框打字发送您希望补充或微调的内容。\n\
            OpenCode 将基于您的意见重新润色方案。\n\n\
            _（输入 `/cancel` 可放弃本次补充并保持原方案）_",
            plan_id
        )
    }

    // =========================================================================
    // 4. 实施 (Coding) 与交付卡片 (4 个)
    // =========================================================================

    /// 卡片 14: Coding 实施启动与监控态
    pub fn coding_running_card(plan_id: &str, is_repair: bool) -> (String, InlineKeyboardMarkup) {
        let (title, status_text) = if is_repair {
            (
                "🔄 *自愈修复实施已启动*",
                "针对上一轮测试或规范未通过项调整代码中...",
            )
        } else {
            (
                "🚀 *Coding 实施已启动*",
                "基于当前分支创建本地临时 worktree 编写测试并实施中...",
            )
        };

        let text = format!(
            "{}\n\
            > ├─ 实施方案: `{}`\n\
            > ├─ 隔离构建: `.worktrees/{}`\n\
            > ├─ 独立分支: `ai-workflow/{}`\n\
            > └─ 当前状态: `🟡 {}`\n\n\
            ⏳ _OpenCode 正在严格遵循工程规范与 TDD 流程推进实施..._",
            title,
            plan_id,
            plan_id,
            plan_id,
            status_text
        );

        let markup = InlineKeyboardMarkup::new(vec![vec![
            InlineKeyboardButton::callback("📊 刷新进度", format!("check_progress:{}", plan_id)),
            InlineKeyboardButton::callback("🛑 中断任务", "cancel_task"),
        ]]);

        (text, markup)
    }

    /// 卡片 15: Coding 圆满交付与四重审核卡片
    pub fn coding_success_card(
        plan_id: &str,
        elapsed: Option<&str>,
        commit_summary: &str,
        stat_text: &str,
    ) -> (String, InlineKeyboardMarkup) {
        let elapsed_line = elapsed
            .map(|e| format!("> ├─ 交付耗时: `{}`\n", e))
            .unwrap_or_default();

        let clean_stat = if stat_text.len() > 320 {
            format!("{}...", &stat_text[..320])
        } else {
            stat_text.to_string()
        };

        let text = format!(
            "🎉 *Coding 实施圆满完成*\n\
            > ├─ 交付方案: `{}`\n\
            {}\
            > └─ 提交哈希: `{}`\n\n\
            ✅ *四重证据链严格核验全部达标*：\n\
            • 进程退出 ╎ `Exit 0 (正常退出)`\n\
            • 契约审核 ╎ `### Status: done 达标`\n\
            • 规范审查 ╎ `Spec Review & Standards Review 通过`\n\
            • Git 合入 ╎ `已安全合入工作区主分支`\n\n\
            📊 *改动文件统计*：\n\
            ```text\n\
            {}\n\
            ```\n\n\
            _🌿 临时 Worktree 已自动安全清理，新功能现已就绪！_",
            plan_id,
            elapsed_line,
            commit_summary,
            clean_stat
        );

        let safe_tag = if plan_id.len() > 36 { &plan_id[..36] } else { plan_id };
        let markup = InlineKeyboardMarkup::new(vec![vec![
            InlineKeyboardButton::callback("🔍 查看改动 Diff", format!("view_diff:{}", safe_tag)),
            InlineKeyboardButton::callback("📊 完整实施报告", format!("check_progress:{}", safe_tag)),
        ]]);

        (text, markup)
    }

    /// 卡片 16: 实施中断/审核失败及自愈卡片
    pub fn coding_failed_card(plan_id: &str, stage: &str, reason: &str) -> (String, InlineKeyboardMarkup) {
        let text = format!(
            "❌ *实施中断 ╎ 审核未通过*\n\
            > ├─ 异常方案: `{}`\n\
            > └─ 阻碍阶段: `{}`\n\n\
            🚨 *错误详情*：\n\
            {}\n\n\
            _👉 您可以点击下方按钮自动启动自愈修复，或查看诊断日志：_",
            plan_id,
            stage,
            Self::quote(reason.trim())
        );

        let safe_tag = if plan_id.len() > 36 { &plan_id[..36] } else { plan_id };
        let markup = InlineKeyboardMarkup::new(vec![vec![
            InlineKeyboardButton::callback("🔁 携带报错继续修复", format!("retry_repair:{}", safe_tag)),
            InlineKeyboardButton::callback("📄 查看诊断日志", format!("view_failure_log:{}", safe_tag)),
        ]]);

        (text, markup)
    }

    /// 卡片 17: 实施自愈修复启动卡片
    pub fn self_repair_running_card(plan_id: &str) -> String {
        format!(
            "🔄 *自愈修复已启动*\n\
            > ├─ 修复方案: `{}`\n\
            > └─ 修复模式: `已注入上一轮测试错误与失败上下文`\n\n\
            ⏳ _OpenCode 正在针对失败项调整代码实现与测试断言..._",
            plan_id
        )
    }

    // =========================================================================
    // 5. 审查与诊断类卡片 (4 个)
    // =========================================================================

    /// 卡片 18: 工作区方案列表卡片
    pub fn plans_list_card(
        project: &str,
        plans: &[(String, String, bool)], // (plan_id, status_text, can_run)
    ) -> (String, InlineKeyboardMarkup) {
        let mut lines = Vec::new();
        lines.push("📋 *工作区方案列表*".to_string());
        lines.push(format!("> ▫️ 目标项目: `{}`", project));
        lines.push(format!("> ▫️ 方案总计: `{} 个方案`", plans.len()));
        lines.push("".to_string());

        let mut keyboard_rows = Vec::new();

        for (idx, (plan_id, badge, can_run)) in plans.iter().enumerate() {
            let num = idx + 1;
            lines.push(format!("{}. `{}` ╎ {}", num, plan_id, badge));

            let short_id = if plan_id.len() > 14 {
                format!("{}…", &plan_id[..13])
            } else {
                plan_id.clone()
            };

            let mut row = Vec::new();
            if *can_run {
                row.push(InlineKeyboardButton::callback(
                    format!("🚀 实施: {}", short_id),
                    format!("approve_plan:{}", plan_id),
                ));
            }
            row.push(InlineKeyboardButton::callback(
                format!("📊 进度: {}", short_id),
                format!("check_progress:{}", plan_id),
            ));
            keyboard_rows.push(row);
        }

        lines.push("".to_string());
        lines.push("_👉 点击下方按钮可直接查看对应方案进度或启动实施：_".to_string());

        (lines.join("\n"), InlineKeyboardMarkup::new(keyboard_rows))
    }

    /// 卡片 19: 方案详细实施进度报告卡片
    pub fn plan_progress_card(
        plan_id: &str,
        status_text: &str,
        elapsed_desc: Option<&str>,
        commit_hash: Option<&str>,
        worktree_branch: Option<&str>,
        steps: &[String],
        report_files: &[(String, String)],
        can_run: bool,
        can_refresh: bool,
        is_in_progress: bool,
    ) -> (String, Option<InlineKeyboardMarkup>) {
        let mut lines = Vec::new();
        lines.push("📊 *方案实施进度报告*".to_string());
        lines.push(format!("> ├─ 方案标识: `{}`", plan_id));
        lines.push(format!("> ├─ 当前状态: {}", status_text));

        if let Some(el) = elapsed_desc {
            lines.push(format!("> ├─ 执行耗时: `{}`", el));
        }
        if let Some(c) = commit_hash {
            let short = if c.len() > 8 { &c[..8] } else { c };
            lines.push(format!("> ├─ 交付提交: `{}`", short));
        }
        if let Some(br) = worktree_branch {
            lines.push(format!("> ├─ 隔离构建: `.worktrees/{}`", plan_id));
            lines.push(format!("> └─ 隔离分支: `{}`", br));
        } else {
            lines.push("> └─ 隔离构建: `暂无活跃 worktree`".to_string());
        }

        if !steps.is_empty() {
            lines.push("".to_string());
            lines.push(format!("📝 *规划实施步骤 (共 {} 步)*：", steps.len()));
            for (idx, step) in steps.iter().enumerate() {
                let prefix = if is_in_progress {
                    if idx == 0 { "⏳" } else { "•" }
                } else if status_text.contains("已交付") {
                    "✅"
                } else {
                    "•"
                };
                lines.push(format!("{} {}", prefix, step));
            }
        }

        if !report_files.is_empty() {
            lines.push("".to_string());
            lines.push(format!("📁 *测试与审查证据 (最新 {} 份)*：", report_files.len().min(4)));
            for (name, time) in report_files.iter().take(4) {
                lines.push(format!("• `{}` ({})", name, time));
            }
            if report_files.len() > 4 {
                lines.push(format!("• ...以及其余 {} 份日志", report_files.len() - 4));
            }
        }

        let mut row = Vec::new();
        if can_refresh {
            row.push(InlineKeyboardButton::callback(
                "🔄 刷新进度",
                format!("check_progress:{}", plan_id),
            ));
        }
        if can_run {
            row.push(InlineKeyboardButton::callback(
                "🚀 启动实施",
                format!("approve_plan:{}", plan_id),
            ));
        }
        if is_in_progress {
            row.push(InlineKeyboardButton::callback(
                "🛑 中断任务",
                "cancel_task",
            ));
        }

        let markup = if !row.is_empty() {
            Some(InlineKeyboardMarkup::new(vec![row]))
        } else {
            None
        };

        (lines.join("\n"), markup)
    }

    /// 卡片 20: 代码改动 Diff 预览卡片
    pub fn diff_preview_card(
        plan_title: &str,
        target_scope: &str,
        stat_text: &str,
        diff_snippet: &str,
    ) -> String {
        format!(
            "🔍 *代码改动 Diff 预览*\n\
            > ├─ 方案标识: `{}`\n\
            > └─ 比较基准: `{}`\n\n\
            📊 *改动概览*：\n\
            ```text\n\
            {}\n\
            ```\n\n\
            📝 *核心代码片段*：\n\
            ```diff\n\
            {}\n\
            ```\n\n\
            _👉 输入 `/plans` 可继续查看其他方案。_",
            plan_title,
            target_scope,
            stat_text.trim(),
            diff_snippet.trim()
        )
    }

    /// 卡片 21: 方案诊断日志卡片
    pub fn diagnostic_log_card(plan_id: &str, source_file: Option<&str>, log_content: &str) -> (String, InlineKeyboardMarkup) {
        let src = source_file.unwrap_or("reports/diagnostic.log");
        let text = format!(
            "📄 *方案诊断日志*\n\
            > ├─ 方案标识: `{}`\n\
            > └─ 日志来源: `{}`\n\n\
            📝 *执行日志末尾截取*：\n\
            ```text\n\
            {}\n\
            ```\n\n\
            _👉 点击下方按钮，AI 将携带此日志启动自愈修复：_",
            plan_id,
            src,
            log_content.trim()
        );

        let safe_tag = if plan_id.len() > 36 { &plan_id[..36] } else { plan_id };
        let markup = InlineKeyboardMarkup::new(vec![vec![
            InlineKeyboardButton::callback("🔁 携带报错继续修复", format!("retry_repair:{}", safe_tag)),
        ]]);

        (text, markup)
    }

    // =========================================================================
    // 6. 控制与异常类卡片 (2 个)
    // =========================================================================

    /// 卡片 22: 任务中断与取消确认卡片
    pub fn task_cancelled_card() -> String {
        "🛑 *任务已终止取消*\n\
        > ▫️ 当前状态: `进行中的任务与规划会话已安全退出`\n\
        > ▫️ 工作区: `已复位至空闲状态，无悬挂进程`\n\n\
        您现在可以随时发起新任务（如 `/planning <需求>`）或输入 `/help` 查看说明。".to_string()
    }

    /// 卡片 23: 通用校验与异常报错卡片
    pub fn argument_error_card(command: &str, reason: &str, example: &str) -> String {
        format!(
            "⚠️ *指令格式提示*\n\
            > ├─ 触发指令: `{}`\n\
            > └─ 提示原因: `{}`\n\n\
            💡 *正确使用示例*：\n\
            `{}`\n\n\
            _（输入 `/help` 可查看全部可用指令用法）_",
            command,
            reason,
            example
        )
    }

    // =========================================================================
    // 7. 外部 OpenCode 运行中会话卡片 (4 个)
    // =========================================================================

    /// 辅助方法：生成数字图标
    pub fn number_badge(num: usize) -> &'static str {
        match num {
            1 => "1️⃣",
            2 => "2️⃣",
            3 => "3️⃣",
            4 => "4️⃣",
            5 => "5️⃣",
            6 => "6️⃣",
            7 => "7️⃣",
            8 => "8️⃣",
            9 => "9️⃣",
            10 => "🔟",
            _ => "🔘",
        }
    }

    /// 卡片 24: 外部 OpenCode 会话实时进度卡片
    pub fn external_session_card(session: &ExternalOpenCodeSessionInfo) -> (String, Option<InlineKeyboardMarkup>) {
        let status_emoji = if session.is_busy { "🟢" } else { "🟡" };
        let status_desc = if session.is_busy { "正在处理 (Busy)" } else { "空闲等待 (Idle)" };
        let step_badge = session.current_step.map(|s| format!(" · Step {}", s)).unwrap_or_default();

        let title_line = session.title.as_deref().unwrap_or("未命名会话");
        let model_line = session.model.as_deref().unwrap_or("默认模型");
        let agent_line = session.agent.as_deref().unwrap_or("default");

        let mut text = format!(
            "🚀 *外部 OpenCode 运行中会话*\n\
            > ├─ 目标项目: `{}`\n\
            > ├─ 进程信息: `PID {}` · `{}` (已运行 {})\n\
            > ├─ 会话标题: {}\n\
            > ├─ 当前状态: {} `{}`{}\n\
            > └─ 采用模型: `{}` (Agent: {})\n\n",
            session.project_name,
            session.pid,
            session.command,
            session.elapsed,
            title_line,
            status_emoji,
            status_desc,
            step_badge,
            model_line,
            agent_line
        );

        // 最新动态 (工具与思考)
        text.push_str("▫️ *最新动态 (Latest Action)*\n");
        if let Some(ref tool) = session.last_tool {
            let st = session.last_tool_status.as_deref().unwrap_or("active");
            text.push_str(&format!("> 🛠 工具调用: `{}` (状态: `{}`)\n", tool, st));
            if let Some(ref prev) = session.last_tool_input_preview {
                text.push_str(&format!("> ↳ `{}`\n", prev));
            }
        }
        if let Some(ref reasoning) = session.last_reasoning {
            text.push_str(&format!("> 💭 推理思考: {}\n", reasoning));
        }
        if session.last_tool.is_none() && session.last_reasoning.is_none() {
            text.push_str("> ⏳ 等待执行下一轮操作...\n");
        }
        text.push('\n');

        // 待办任务进展 (若有)
        if !session.todos.is_empty() {
            let completed = session.todos.iter().filter(|t| t.status == "completed").count();
            let total = session.todos.len();
            text.push_str(&format!("▫️ *任务清单 (Todos)*: {}/{} 完成\n", completed, total));
            for todo in session.todos.iter().take(5) {
                let icon = match todo.status.as_str() {
                    "completed" => "✅",
                    "in_progress" => "▶️",
                    _ => "▫️",
                };
                text.push_str(&format!("> {} {}\n", icon, todo.content));
            }
            if session.todos.len() > 5 {
                text.push_str(&format!("> _（还有 {} 项待办）_\n", session.todos.len() - 5));
            }
            text.push('\n');
        }

        // 资源消耗
        let tokens_str = format!(
            "输入 {} · 输出 {} · 思考 {} (缓存读 {})",
            opencode_inspector::format_token_count(session.tokens_input),
            opencode_inspector::format_token_count(session.tokens_output),
            opencode_inspector::format_token_count(session.tokens_reasoning),
            opencode_inspector::format_token_count(session.tokens_cache_read),
        );
        text.push_str(&format!("▫️ *资源用量*: {}\n\n", tokens_str));
        text.push_str(&format!("_👉 工作目录：`{}`_", session.cwd));

        // 操作按钮
        let buttons = vec![
            vec![
                InlineKeyboardButton::callback("🔄 刷新进度", format!("refresh_ext_session:{}", session.pid)),
                InlineKeyboardButton::callback("💬 发送提示词", format!("prompt_ext_session:{}", session.pid)),
            ],
            vec![
                InlineKeyboardButton::callback("📝 详细动态", format!("logs_ext_session:{}", session.pid)),
                InlineKeyboardButton::callback("📊 差异对比 /diff", format!("diff_ext_session:{}", session.pid)),
            ],
            vec![
                InlineKeyboardButton::callback("🤖 完全托管", format!("start_host:{}", session.pid)),
                InlineKeyboardButton::callback("📂 设为当前工作区", format!("switch_ext_ws:{}", session.pid)),
            ],
        ];

        (text, Some(InlineKeyboardMarkup::new(buttons)))
    }

    /// 卡片 25: 多个外部会话列表卡片
    pub fn multiple_external_sessions_card(sessions: &[ExternalOpenCodeSessionInfo]) -> (String, InlineKeyboardMarkup) {
        let mut text = "🚀 *检测到多个终端正在运行 OpenCode 会话*\n\
        > 💡 点击下方对应按钮查看该会话的详细进度：\n\n".to_string();

        let mut buttons = Vec::new();

        for (idx, sess) in sessions.iter().enumerate() {
            let status_emoji = if sess.is_busy { "🟢" } else { "🟡" };
            let title = sess.title.as_deref().unwrap_or("未命名会话");
            let step = sess.current_step.map(|s| format!("Step {}", s)).unwrap_or_else(|| "启动中".to_string());

            text.push_str(&format!(
                "{} *{}* · `{}`\n> ├─ 项目: `{}` (PID: {})\n> └─ 状态: {} `{}` · {}\n\n",
                Self::number_badge(idx + 1),
                sess.project_name,
                title,
                sess.cwd,
                sess.pid,
                status_emoji,
                if sess.is_busy { "Busy" } else { "Idle" },
                step
            ));

            let btn_label = format!("{} {} ({})", Self::number_badge(idx + 1), sess.project_name, sess.pid);
            buttons.push(vec![
                InlineKeyboardButton::callback(btn_label, format!("select_ext_session:{}", sess.pid)),
            ]);
        }

        buttons.push(vec![
            InlineKeyboardButton::callback("🔄 刷新会话列表", "refresh_ext_list".to_string()),
        ]);

        (text, InlineKeyboardMarkup::new(buttons))
    }

    /// 卡片 26: 无外部会话卡片
    pub fn no_external_session_card(hostname: &str) -> (String, Option<InlineKeyboardMarkup>) {
        let text = format!(
            "💡 *当前未检测到正在运行的外部 OpenCode 会话*\n\
            > ▫️ 宿主主机: `{}`\n\
            > ▫️ 探测范围: 终端 (Terminal / tmux) 及系统后台进程\n\n\
            若您在终端启动了 `opencode` 或 `opencode --yolo`，在此发送 `/running` 即可实时查看会话进度、工具调用与任务清单。",
            hostname
        );
        let markup = InlineKeyboardMarkup::new(vec![vec![
            InlineKeyboardButton::callback("🔄 重新探测", "refresh_ext_list".to_string()),
            InlineKeyboardButton::callback("🖥️ 主机状态", "open_status".to_string()),
        ]]);
        (text, Some(markup))
    }

    /// 卡片 27: 外部会话详细日志与思考卡片
    pub fn external_session_logs_card(session: &ExternalOpenCodeSessionInfo) -> (String, InlineKeyboardMarkup) {
        let title_line = session.title.as_deref().unwrap_or("未命名会话");
        let tool_line = session.last_tool.as_deref().unwrap_or("暂无工具调用");
        let status_line = session.last_tool_status.as_deref().unwrap_or("completed");
        let cmd_preview = session.last_tool_input_preview.as_deref().unwrap_or("(无输入参数)");
        let reasoning = session.last_reasoning.as_deref().unwrap_or("暂无思考内容");

        let text = format!(
            "📝 *外部会话详细动态与思考*\n\
            > ├─ 项目: `{}` (PID: {})\n\
            > └─ 会话: {}\n\n\
            🛠️ *最新工具执行*：\n\
            ```text\n\
            工具: {}\n\
            状态: {}\n\
            入参: {}\n\
            ```\n\n\
            💭 *最新思考推理片段*：\n\
            ```text\n\
            {}\n\
            ```",
            session.project_name,
            session.pid,
            title_line,
            tool_line,
            status_line,
            cmd_preview,
            reasoning
        );

        let markup = InlineKeyboardMarkup::new(vec![vec![
            InlineKeyboardButton::callback("⬅️ 返回会话进度", format!("select_ext_session:{}", session.pid)),
            InlineKeyboardButton::callback("🔄 刷新动态", format!("logs_ext_session:{}", session.pid)),
        ]]);

        (text, markup)
    }

    /// 卡片 28: 提示词输入请求引导卡片
    pub fn prompt_input_request_card(session: &ExternalOpenCodeSessionInfo) -> (String, InlineKeyboardMarkup) {
        let title_line = session.title.as_deref().unwrap_or("未命名会话");
        let sid_line = session.session_id.as_deref().unwrap_or("未知");

        let text = format!(
            "💬 *向正在运行的 OpenCode 会话发送提示词*\n\
            > ├─ 目标项目: `{}` (PID: {})\n\
            > └─ 活跃会话: {} (`{}`)\n\n\
            👉 *请直接在此回复您想要发送的提示词内容*：\n\
            例如：`请帮我补充缺少的单元测试，并执行测试对账`\n\n\
            _（发送普通文字即可自动投递执行；若想放弃请点击下方取消按钮）_",
            session.project_name,
            session.pid,
            title_line,
            sid_line
        );

        let markup = InlineKeyboardMarkup::new(vec![vec![
            InlineKeyboardButton::callback("❌ 取消发送", "cancel_prompt_target".to_string()),
            InlineKeyboardButton::callback("⬅️ 返回进度卡片", format!("select_ext_session:{}", session.pid)),
        ]]);

        (text, markup)
    }

    /// 卡片 29: 提示词已投递确认卡片
    pub fn prompt_dispatched_ack_card(session: &super::types::PromptTargetSession, prompt: &str) -> String {
        format!(
            "🚀 *提示词已投递给 OpenCode 会话*\n\
            > ├─ 目标项目: `{}` (PID: {})\n\
            > ├─ 会话标识: `{}`\n\
            > └─ 提示词: {}\n\n\
            ⏳ *OpenCode 正在推进执行，期间保持 typing 状态，执行完毕后将推送结果...*",
            session.project_name,
            session.pid,
            session.session_id,
            prompt
        )
    }

    /// 卡片 30: 启动完全托管卡片
    pub fn hosted_session_started_card(
        project_name: &str,
        pid: u32,
        session_id: &str,
        mode: &super::types::SessionHostingDecisionMode,
    ) -> (String, InlineKeyboardMarkup) {
        let (mode_title, mode_desc) = match mode {
            super::types::SessionHostingDecisionMode::Manual => (
                "🎯 人工决断优先 (Manual)",
                "遇有提问时在 Telegram 弹出交互按钮，由您点击确定",
            ),
            super::types::SessionHostingDecisionMode::AutoRecommended => (
                "⚡ 自动推荐模式 (Auto-Recommended)",
                "对带有推荐标识的方案自动代选并通知，其余由您决断",
            ),
        };

        let text = format!(
            "🤖 *会话已成功交由 OneSpace + Telegram 完全托管*\n\
            > ├─ 目标项目: `{}` (PID: {})\n\
            > ├─ 会话标识: `{}`\n\
            > ├─ 决策模式: `{}`\n\
            > └─ 托管状态: `🟢 守护运行中 (Supervisor Active)`\n\n\
            💡 *全托管守护策略说明*：\n\
            • 📢 *关键节点主动通知*：步骤推进与任务待办自动实时汇报；\n\
            • ❓ *交互提问自动接管*：{}\n\
            • 💬 *双向动态干预*：在此直接回复文字或输入 `/prompt` 即可向会话注入指令；\n\
            • 🎉 *圆满交付报告*：任务执行完后自动推送耗时、Token统计及 Diff。",
            project_name,
            pid,
            session_id,
            mode_title,
            mode_desc
        );

        let markup = InlineKeyboardMarkup::new(vec![
            vec![
                InlineKeyboardButton::callback("⚙️ 切换决策模式", format!("toggle_host_mode:{}", session_id)),
                InlineKeyboardButton::callback("🛑 退出托管", format!("stop_host:{}", session_id)),
            ],
            vec![
                InlineKeyboardButton::callback("🔄 刷新会话状态", format!("refresh_ext_session:{}", pid)),
            ],
        ]);

        (text, markup)
    }

    /// 卡片 31: 待办项里程碑完成主动通知
    pub fn hosted_todo_completed_notice(
        project_name: &str,
        todo_content: &str,
        completed_cnt: usize,
        total_cnt: usize,
    ) -> String {
        format!(
            "✅ *任务待办已达成* · `{}`\n\
            > ├─ 达成项: {}\n\
            > └─ 当前进度: `{}/{}` 项已完成\n\n\
            _OpenCode 正在继续推进剩余步骤..._",
            project_name,
            todo_content,
            completed_cnt,
            total_cnt
        )
    }

    /// 卡片 32: 托管提问与决策卡片
    pub fn hosted_question_card(
        project_name: &str,
        session_id: &str,
        question_text: &str,
        options: &[super::types::HostedOptionItem],
    ) -> (String, InlineKeyboardMarkup) {
        let mut text = format!(
            "❓ *OpenCode 提问与决策确认*\n\
            > ├─ 目标项目: `{}`\n\
            > └─ 托管会话: `{}`\n\n\
            📋 *决策问题*：\n\
            {}\n\n\
            👇 *请直接点击下方按钮做出决策*：\n",
            project_name,
            session_id,
            question_text
        );

        let mut buttons = Vec::new();
        for opt in options {
            let prefix = if opt.is_recommended { "⭐️ " } else { "" };
            let desc_line = opt.description.as_deref().unwrap_or("");
            text.push_str(&format!("• *{}* {}\n", opt.label, if desc_line.is_empty() { String::new() } else { format!("— _{}_", desc_line) }));

            let btn_label = format!("{}{}. {}", prefix, opt.index, opt.label);
            let safe_label = if btn_label.chars().count() > 30 {
                let s: String = btn_label.chars().take(28).collect();
                format!("{}…", s)
            } else {
                btn_label
            };

            buttons.push(vec![InlineKeyboardButton::callback(
                safe_label,
                format!("host_ans:{}:{}", session_id, opt.index),
            )]);
        }

        buttons.push(vec![
            InlineKeyboardButton::callback("🛑 退出托管", format!("stop_host:{}", session_id)),
        ]);

        (text, InlineKeyboardMarkup::new(buttons))
    }

    /// 卡片 33: 托管自主决策代执行通知
    pub fn hosted_auto_decision_card(
        project_name: &str,
        question_text: &str,
        chosen_option: &str,
    ) -> String {
        format!(
            "⚡ *托管决策代执行 (Auto-Approve)*\n\
            > ├─ 目标项目: `{}`\n\
            > ├─ 提问事项: {}\n\
            > └─ 采纳推荐: `⭐️ {}`\n\n\
            _已自动注入该选项应答，外部会话已恢复并继续全速执行。_",
            project_name,
            question_text,
            chosen_option
        )
    }

    /// 卡片 34: 托管会话圆满交付卡片
    pub fn hosted_session_completed_card(
        project_name: &str,
        pid: u32,
        session_id: &str,
        elapsed_desc: &str,
        total_steps: u32,
        tokens_in: i64,
        tokens_out: i64,
    ) -> (String, InlineKeyboardMarkup) {
        let token_summary = format!(
            "输入 {} · 输出 {}",
            super::opencode_inspector::format_token_count(tokens_in),
            super::opencode_inspector::format_token_count(tokens_out)
        );

        let text = format!(
            "🎉 *外部 OpenCode 会话圆满交付*\n\
            > ├─ 目标项目: `{}` (PID: {})\n\
            > ├─ 会话标识: `{}`\n\
            > ├─ 交付耗时: `{}`\n\
            > ├─ 总执行步数: `Step {}`\n\
            > ├─ 资源消耗: {}\n\
            > └─ 托管状态: `🏁 任务已完成，自动解除托管`\n\n\
            ✅ *所有待办步骤已全部完成，终端进程已恢复空闲！*",
            project_name,
            pid,
            session_id,
            elapsed_desc,
            total_steps,
            token_summary
        );

        let markup = InlineKeyboardMarkup::new(vec![vec![
            InlineKeyboardButton::callback("📊 差异对比 /diff", format!("diff_ext_session:{}", pid)),
            InlineKeyboardButton::callback("📂 设为当前工作区", format!("switch_ext_ws:{}", pid)),
        ]]);

        (text, markup)
    }

    // =========================================================================
    // 8. 统一会话详情卡片
    // =========================================================================

    /// 单个会话的真实进度详情，缺失字段显式标记为 `unavailable`。
    pub fn session_detail_card(
        entry: &SessionEntry,
        progress: Option<&SessionProgressDetail>,
    ) -> (String, Option<InlineKeyboardMarkup>) {
        let mut lines = vec![
            "🔎 *会话详情*".to_string(),
            format!("> ├─ 项目: `{}`", entry.project_name),
            format!("> ├─ 会话标识: `{}`", entry.session_id_marker),
        ];
        if let Some(pid) = entry.pid {
            lines.push(format!("> ├─ PID: `{}`", pid));
        }
        if let Some(plan_id) = &entry.plan_id {
            lines.push(format!("> ├─ 方案标识: `{}`", plan_id));
        }
        lines.push(format!("> └─ 状态: `{}`", entry.status_label));

        lines.push(String::new());
        lines.push("▫️ *执行进度*".to_string());
        match progress {
            Some(detail) => {
                match detail.current_step {
                    Some(step) => lines.push(format!("> ├─ 当前步骤: `{}`", step)),
                    None => lines.push("> ├─ 当前步骤: `unavailable`".to_string()),
                }
                match detail.last_tool.as_deref() {
                    Some(tool) => lines.push(format!(
                        "> ├─ 最近工具: `{}` (状态: `{}`)",
                        tool,
                        detail.last_tool_status.as_deref().unwrap_or("unavailable")
                    )),
                    None => lines.push("> ├─ 最近工具: `unavailable`".to_string()),
                }
                match detail.last_tool_input_preview.as_deref() {
                    Some(preview) => lines.push(format!("> ├─ 工具入参: `{}`", preview)),
                    None => lines.push("> ├─ 工具入参: `unavailable`".to_string()),
                }

                if detail.todos.is_empty() {
                    lines.push("> └─ 待办清单: `unavailable`".to_string());
                } else {
                    let completed = detail
                        .todos
                        .iter()
                        .filter(|todo| todo.status == "completed")
                        .count();
                    lines.push(format!(
                        "> └─ 待办清单: {}/{} 完成",
                        completed,
                        detail.todos.len()
                    ));
                    for todo in detail.todos.iter().take(5) {
                        let icon = match todo.status.as_str() {
                            "completed" => "✅",
                            "in_progress" => "▶️",
                            _ => "▫️",
                        };
                        lines.push(format!(">    {} {}", icon, todo.content));
                    }
                }
            }
            None => {
                lines.push("> ├─ 当前步骤: `unavailable`".to_string());
                lines.push("> ├─ 最近工具: `unavailable`".to_string());
                lines.push("> ├─ 工具入参: `unavailable`".to_string());
                lines.push("> └─ 待办清单: `unavailable`".to_string());
            }
        }

        let markup = match (entry.pid, entry.session_id.as_deref()) {
            (Some(pid), Some(_)) => Some(InlineKeyboardMarkup::new(vec![
                vec![
                    InlineKeyboardButton::callback(
                        "🔄 刷新进度",
                        format!("refresh_ext_session:{}", pid),
                    ),
                    InlineKeyboardButton::callback(
                        "💬 发送提示词",
                        format!("prompt_ext_session:{}", pid),
                    ),
                ],
                vec![
                    InlineKeyboardButton::callback("🤖 完全托管", format!("start_host:{}", pid)),
                    InlineKeyboardButton::callback(
                        "📊 差异对比 /diff",
                        format!("diff_ext_session:{}", pid),
                    ),
                ],
            ])),
            _ => None,
        };

        (lines.join("\n"), markup)
    }

    /// 方案进度详情，并附带当前方案执行的会话标识与执行状态。
    pub fn plan_session_detail_card(
        info: &PlanProgressInfo,
        execution: Option<&PlanExecutionState>,
    ) -> (String, Option<InlineKeyboardMarkup>) {
        let (status_text, can_run, can_refresh, is_in_progress) = match info.status {
            PlanExecutionStatus::Completed => ("🟢 `已圆满交付 (Completed)`", false, true, false),
            PlanExecutionStatus::InProgress => ("🟡 `正在实施中 (In-Progress)`", false, true, true),
            PlanExecutionStatus::NotStarted => ("⚪ `尚未实施 (Not Started)`", true, false, false),
            PlanExecutionStatus::Failed => ("🔴 `实施中断/失败 (Failed)`", true, true, false),
        };

        let (base, mut markup) = Self::plan_progress_card(
            &info.plan_id,
            status_text,
            info.elapsed_desc.as_deref(),
            info.commit_hash.as_deref(),
            info.worktree_branch.as_deref(),
            &info.steps,
            &info.report_files,
            can_run,
            can_refresh,
            is_in_progress,
        );

        let summary = super::session_inventory::plan_execution_summary(&info.plan_id, execution);
        let text = format!(
            "{}\n\n▫️ *执行会话 (Session)*\n> ├─ 会话标识: `{}`\n> └─ 执行状态: `{}`",
            base, summary.session_id_marker, summary.execution_status
        );

        // 刷新按钮始终保留，便于从详情重新读取方案进度。
        let refresh_callback = format!("check_progress:{}", info.plan_id);
        let has_refresh = markup.as_ref().map_or(false, |m| {
            m.inline_keyboard
                .iter()
                .flatten()
                .any(|button| button.callback_data.as_deref() == Some(refresh_callback.as_str()))
        });
        if !has_refresh {
            if let Some(ref mut m) = markup {
                m.inline_keyboard.push(vec![InlineKeyboardButton::callback(
                    "🔄 刷新进度",
                    refresh_callback,
                )]);
            } else {
                markup = Some(InlineKeyboardMarkup::new(vec![vec![
                    InlineKeyboardButton::callback("🔄 刷新进度", refresh_callback),
                ]]));
            }
        }

        (text, markup)
    }

    /// 未知或歧义标识符的非操作提示，明确命名原始参数与全部候选项。
    pub fn identifier_error_text(raw: &str, resolution: &IdentifierResolution) -> String {
        match resolution {
            IdentifierResolution::Unknown => format!(
                "⚠️ *未找到匹配的会话或方案*\n\
                > ▫️ 输入标识符: `{}`\n\n\
                未执行任何操作。发送 `/sessions` 查看活跃会话，或使用 `/plans` 浏览方案。",
                raw
            ),
            IdentifierResolution::Ambiguous(candidates) => format!(
                "⚠️ *标识符存在歧义*\n\
                > ├─ 输入标识符: `{}`\n\
                > └─ 匹配到多个会话: `{}`\n\n\
                未执行任何操作。请使用精确的会话 ID 或 PID 后重试。",
                raw,
                candidates.join("`、`")
            ),
            IdentifierResolution::Session(entry) => format!(
                "ℹ️ 标识符 `{}` 已唯一解析为会话 `{}`。",
                raw, entry.session_id_marker
            ),
            IdentifierResolution::Plan { plan_id, .. } => {
                format!("ℹ️ 标识符 `{}` 已唯一解析为方案 `{}`。", raw, plan_id)
            }
        }
    }
}
