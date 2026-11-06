use super::types::{InlineKeyboardButton, InlineKeyboardMarkup};

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
            • `/newfeat <需求>` ── 唤起需求规划与实施\n\
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
        • `/projects` ── 切换活跃工作区项目\n\n\
        🚀 *AI 研发流水线*\n\
        • `/newfeat <需求>` ── 启动需求调研与方案规划\n\
        • `/plans` ── 查看所有规划方案与状态\n\
        • `/progress [id]` ── 查看指定方案实施进度\n\
        • `/coding <id>` ── 直接实施指定的已规划方案\n\
        • `/diff [id]` ── 查看最新代码改动对比\n\
        • `/cancel` ── 随时紧急中断当前任务\n\n\
        _💬 自然对话：直接发送任何技术问题，OpenCode 结合项目代码为您解答！_".to_string()
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
        let text = format!(
            "🖥️ *Mac 本地工作台状态*\n\
            > 🟢 *系统运行平稳 ╎ 实时长轮询在线*\n\
            > ├─ 设备主机: `{}`\n\
            > ├─ 系统时间: `{}`\n\
            > └─ 权限审批: `{}`\n\n\
            📁 *当前活跃工作区*\n\
            > ├─ 项目名称: `{}`\n\
            > ├─ Git 分支: `{}` ({})\n\
            > └─ 最新提交: `{}`\n\n\
            _👉 输入 `/diff` 可查看代码变更，`/plans` 可查看方案。_",
            hostname,
            current_time,
            if auto_approve { "⚡ 自动确认 (Auto-Approve)" } else { "🔒 手动审批" },
            project,
            git_branch,
            git_status,
            latest_commit
        );

        let markup = InlineKeyboardMarkup::new(vec![vec![
            InlineKeyboardButton::callback("📁 切换项目", "open_projects"),
            InlineKeyboardButton::callback("📋 查看方案", "open_plans"),
        ]]);

        (text, Some(markup))
    }

    /// 卡片 5: 工作区项目管理卡片
    pub fn projects_card(active_workspace: &str, physical_path: &str, projects: &[&str]) -> (String, InlineKeyboardMarkup) {
        let text = format!(
            "📂 *工作区项目管理*\n\
            > ▫️ 当前活跃: `{}`\n\
            > ▫️ 本地路径: `{}`\n\n\
            点击下方按钮切换指令生效的目标项目：",
            active_workspace,
            physical_path
        );

        let mut rows = Vec::new();
        for p in projects {
            let is_active = *p == active_workspace;
            let label = if is_active {
                format!("📁 {} (当前选中)", p)
            } else {
                format!("📁 {}", p)
            };
            rows.push(vec![InlineKeyboardButton::callback(label, format!("set_project:{}", p))]);
        }

        (text, InlineKeyboardMarkup::new(rows))
    }

    /// 卡片 6: 项目切换成功确认态
    pub fn project_switch_confirmed(project: &str, timestamp: &str) -> String {
        format!(
            "✅ *工作区已成功切换*\n\
            > ▫️ 当前项目: `{}`\n\
            > ▫️ 生效时间: `{}`\n\n\
            后续下发的需求规划与 Coding 任务将全部作用于该项目。",
            project,
            timestamp
        )
    }

    // =========================================================================
    // 3. 规划 (Planning) 流水线卡片 (7 个)
    // =========================================================================

    /// 卡片 7: 新需求接收确认卡片
    pub fn new_feature_ack_card(project_name: &str, prompt: &str) -> (String, InlineKeyboardMarkup) {
        let text = format!(
            "🚀 *已接收新需求任务*\n\
            > ├─ 目标项目: `{}`\n\
            > └─ 任务模式: `方案调研与规划 (Planning)`\n\n\
            📝 *需求描述*：\n\
            > {}\n\n\
            ⏳ _正在拉起 OpenCode 执行深入分析，请稍候..._",
            project_name,
            prompt.trim()
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
        您现在可以随时发起新任务（如 `/newfeat <需求>`）或输入 `/help` 查看说明。".to_string()
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
}
