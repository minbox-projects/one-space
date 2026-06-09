use super::{
    derive_title, interval_minutes_regex, quoted_name_regex, time_of_day_regex, AgentDefinition,
    AssistantScheduleDraft, AssistantState, ScheduleJob, ScheduleTrigger,
};

pub(in crate::ai_assistant) fn format_trigger_label(trigger: &ScheduleTrigger) -> String {
    match trigger.kind.as_str() {
        "interval" => {
            let minutes = trigger.interval_minutes.unwrap_or(30);
            if minutes % 60 == 0 {
                format!("每 {} 小时", minutes / 60)
            } else {
                format!("每 {} 分钟", minutes)
            }
        }
        "weekly" => {
            let weekdays = if trigger.weekdays.is_empty() {
                vec![1]
            } else {
                trigger.weekdays.clone()
            };
            let day_text = weekdays
                .into_iter()
                .map(|day| match day {
                    1 => "周一",
                    2 => "周二",
                    3 => "周三",
                    4 => "周四",
                    5 => "周五",
                    6 => "周六",
                    7 => "周日",
                    _ => "周一",
                })
                .collect::<Vec<_>>()
                .join("、");
            format!(
                "{} {}",
                day_text,
                trigger
                    .time_of_day
                    .clone()
                    .unwrap_or_else(|| "09:00".to_string())
            )
        }
        _ => format!(
            "每天 {}",
            trigger
                .time_of_day
                .clone()
                .unwrap_or_else(|| "09:00".to_string())
        ),
    }
}

pub(in crate::ai_assistant) fn find_schedule_match<'a>(
    state: &'a AssistantState,
    text: &str,
) -> Option<&'a ScheduleJob> {
    let lower = text.to_lowercase();
    state
        .schedules
        .iter()
        .filter(|schedule| !schedule.name.trim().is_empty())
        .max_by_key(|schedule| {
            let name = schedule.name.to_lowercase();
            if lower.contains(&name) {
                name.len()
            } else {
                0
            }
        })
        .filter(|schedule| lower.contains(&schedule.name.to_lowercase()))
}

pub(in crate::ai_assistant) fn find_agent_match<'a>(
    state: &'a AssistantState,
    text: &str,
    web_search: bool,
) -> Option<&'a AgentDefinition> {
    let lower = text.to_lowercase();
    state
        .agents
        .iter()
        .find(|agent| lower.contains(&agent.name.to_lowercase()))
        .or_else(|| {
            state.agents.iter().find(|agent| {
                if web_search {
                    agent.tool_policy.web_search
                } else {
                    true
                }
            })
        })
        .or_else(|| state.agents.first())
}

pub(in crate::ai_assistant) fn parse_schedule_time(text: &str) -> Option<String> {
    let captures = time_of_day_regex().captures(text)?;
    let hour = captures.name("hour")?.as_str().parse::<u32>().ok()?;
    let minute = captures.name("minute")?.as_str().parse::<u32>().ok()?;
    if hour > 23 || minute > 59 {
        return None;
    }
    Some(format!("{hour:02}:{minute:02}"))
}

pub(in crate::ai_assistant) fn parse_schedule_trigger(
    text: &str,
    existing: Option<&ScheduleJob>,
) -> Option<ScheduleTrigger> {
    let normalized = text.replace('：', ":");
    if let Some(captures) = interval_minutes_regex().captures(&normalized) {
        let value = captures.get(1)?.as_str().parse::<u64>().ok()?;
        let unit = captures.get(2)?.as_str();
        let interval_minutes = if unit.contains("小时") {
            value.saturating_mul(60)
        } else {
            value
        };
        return Some(ScheduleTrigger {
            kind: "interval".to_string(),
            interval_minutes: Some(interval_minutes.max(1)),
            time_of_day: None,
            weekdays: Vec::new(),
        });
    }

    let time = parse_schedule_time(&normalized)
        .or_else(|| existing.and_then(|schedule| schedule.trigger.time_of_day.clone()))
        .unwrap_or_else(|| "09:00".to_string());

    if normalized.contains("工作日") {
        return Some(ScheduleTrigger {
            kind: "weekly".to_string(),
            interval_minutes: None,
            time_of_day: Some(time),
            weekdays: vec![1, 2, 3, 4, 5],
        });
    }

    if normalized.contains("每周") {
        let mut weekdays = Vec::new();
        for (needle, value) in [
            ("一", 1_u8),
            ("二", 2_u8),
            ("三", 3_u8),
            ("四", 4_u8),
            ("五", 5_u8),
            ("六", 6_u8),
            ("日", 7_u8),
            ("天", 7_u8),
        ] {
            if normalized.contains(needle) && !weekdays.contains(&value) {
                weekdays.push(value);
            }
        }
        if weekdays.is_empty() {
            weekdays.push(1);
        }
        weekdays.sort_unstable();
        return Some(ScheduleTrigger {
            kind: "weekly".to_string(),
            interval_minutes: None,
            time_of_day: Some(time),
            weekdays,
        });
    }

    if normalized.contains("每天") || normalized.contains("每日") {
        return Some(ScheduleTrigger {
            kind: "daily".to_string(),
            interval_minutes: None,
            time_of_day: Some(time),
            weekdays: Vec::new(),
        });
    }

    existing.map(|schedule| schedule.trigger.clone())
}

pub(in crate::ai_assistant) fn derive_schedule_name(
    text: &str,
    existing: Option<&ScheduleJob>,
    agent: Option<&AgentDefinition>,
) -> String {
    if let Some(schedule) = existing {
        return schedule.name.clone();
    }
    if let Some(captures) = quoted_name_regex().captures(text) {
        if let Some(name) = captures.get(1) {
            let trimmed = name.as_str().trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
    }
    if let Some(agent) = agent {
        return format!("{} 定时任务", agent.name);
    }
    derive_title(text)
}

pub(in crate::ai_assistant) fn build_schedule_draft(
    state: &AssistantState,
    text: &str,
) -> Option<AssistantScheduleDraft> {
    let lower = text.to_lowercase();
    let matched_schedule = find_schedule_match(state, text);
    let wants_search = lower.contains("联网")
        || lower.contains("搜索")
        || lower.contains("新闻")
        || lower.contains("research");
    let agent = find_agent_match(state, text, wants_search);

    let action = if matched_schedule.is_some()
        && (text.contains("立即执行") || text.contains("马上执行") || lower.contains("run now"))
    {
        Some("run_now")
    } else if matched_schedule.is_some() && (text.contains("删除") || text.contains("移除")) {
        Some("delete")
    } else if matched_schedule.is_some() && (text.contains("暂停") || text.contains("停用")) {
        Some("toggle_off")
    } else if matched_schedule.is_some() && (text.contains("启用") || text.contains("恢复")) {
        Some("toggle_on")
    } else if matched_schedule.is_some()
        && (text.contains("修改")
            || text.contains("更新")
            || text.contains("调整")
            || text.contains("改成")
            || text.contains("改为")
            || text.contains("变更"))
    {
        Some("update")
    } else if text.contains("定时任务")
        || text.contains("提醒")
        || text.contains("每天")
        || text.contains("每周")
        || text.contains("每隔")
        || text.contains("工作日")
    {
        Some("create")
    } else {
        None
    }?;

    let action = action.to_string();
    if matches!(
        action.as_str(),
        "run_now" | "delete" | "toggle_off" | "toggle_on"
    ) {
        let target = matched_schedule?;
        let (title, summary, desired_enabled) = match action.as_str() {
            "run_now" => (
                "run schedule now".to_string(),
                format!("Assistant 想立即执行定时任务“{}”。", target.name),
                None,
            ),
            "delete" => (
                "delete schedule".to_string(),
                format!("Assistant 想删除定时任务“{}”。", target.name),
                None,
            ),
            "toggle_off" => (
                "pause schedule".to_string(),
                format!("Assistant 想暂停定时任务“{}”。", target.name),
                Some(false),
            ),
            _ => (
                "enable schedule".to_string(),
                format!("Assistant 想启用定时任务“{}”。", target.name),
                Some(true),
            ),
        };
        return Some(AssistantScheduleDraft {
            action,
            title,
            summary,
            schedule: None,
            target_schedule_id: Some(target.id.clone()),
            target_schedule_name: Some(target.name.clone()),
            desired_enabled,
            agent_name: None,
            trigger_label: Some(format_trigger_label(&target.trigger)),
        });
    }

    let base_schedule = matched_schedule.cloned();
    let trigger = parse_schedule_trigger(text, matched_schedule)?;
    let mut schedule = base_schedule.unwrap_or_else(|| ScheduleJob {
        id: String::new(),
        name: String::new(),
        assistant_id: None,
        agent_id: String::new(),
        prompt: String::new(),
        model_profile_id: None,
        model_override_id: None,
        web_search_enabled: false,
        trigger: ScheduleTrigger {
            kind: "daily".to_string(),
            interval_minutes: None,
            time_of_day: Some("09:00".to_string()),
            weekdays: Vec::new(),
        },
        timezone: Some("Asia/Shanghai".to_string()),
        output_target: "assistant_conversation".to_string(),
        conversation_id: None,
        enabled: true,
        next_run_at: None,
        last_run_at: None,
        last_status: None,
        last_error: None,
        misfire_policy: "skip".to_string(),
        max_retries: 0,
        retry_count: 0,
        created_at: 0,
        updated_at: 0,
    });
    if let Some(agent) = agent {
        schedule.assistant_id = Some(agent.id.clone());
        schedule.agent_id = agent.id.clone();
    }
    schedule.name = derive_schedule_name(text, matched_schedule, agent);
    schedule.prompt = text.trim().to_string();
    schedule.web_search_enabled = wants_search || schedule.web_search_enabled;
    schedule.trigger = trigger.clone();
    schedule.timezone = Some(
        schedule
            .timezone
            .clone()
            .unwrap_or_else(|| "Asia/Shanghai".to_string()),
    );
    schedule.enabled = true;

    Some(AssistantScheduleDraft {
        action,
        title: if matched_schedule.is_some() {
            "update schedule".to_string()
        } else {
            "create schedule".to_string()
        },
        summary: if matched_schedule.is_some() {
            format!("Assistant 想更新定时任务“{}”。", schedule.name)
        } else {
            format!("Assistant 想创建定时任务“{}”。", schedule.name)
        },
        schedule: Some(schedule.clone()),
        target_schedule_id: matched_schedule.map(|schedule| schedule.id.clone()),
        target_schedule_name: Some(schedule.name.clone()),
        desired_enabled: Some(true),
        agent_name: agent.map(|item| item.name.clone()),
        trigger_label: Some(format_trigger_label(&trigger)),
    })
}

pub(in crate::ai_assistant) async fn read_sse_response<F>(
    mut response: reqwest::Response,
    mut on_event: F,
) -> Result<(), String>
where
    F: FnMut(Option<&str>, &str) -> Result<(), String>,
{
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(if body.trim().is_empty() {
            format!("Request failed with status {}", status)
        } else {
            body
        });
    }

    let mut buffer = String::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        let normalized = String::from_utf8_lossy(&chunk).replace("\r\n", "\n");
        buffer.push_str(&normalized);
        while let Some(index) = buffer.find("\n\n") {
            let block = buffer[..index].to_string();
            buffer = buffer[index + 2..].to_string();
            let mut event_name: Option<String> = None;
            let mut data_lines = Vec::new();
            for line in block.lines() {
                let line = line.trim_end();
                if line.is_empty() || line.starts_with(':') {
                    continue;
                }
                if let Some(rest) = line.strip_prefix("event:") {
                    event_name = Some(rest.trim().to_string());
                } else if let Some(rest) = line.strip_prefix("data:") {
                    data_lines.push(rest.trim_start().to_string());
                }
            }
            if data_lines.is_empty() {
                continue;
            }
            let data = data_lines.join("\n");
            on_event(event_name.as_deref(), &data)?;
        }
    }

    if !buffer.trim().is_empty() {
        let block = buffer.trim().to_string();
        let mut event_name: Option<String> = None;
        let mut data_lines = Vec::new();
        for line in block.lines() {
            let line = line.trim_end();
            if line.is_empty() || line.starts_with(':') {
                continue;
            }
            if let Some(rest) = line.strip_prefix("event:") {
                event_name = Some(rest.trim().to_string());
            } else if let Some(rest) = line.strip_prefix("data:") {
                data_lines.push(rest.trim_start().to_string());
            }
        }
        if !data_lines.is_empty() {
            let data = data_lines.join("\n");
            on_event(event_name.as_deref(), &data)?;
        }
    }

    Ok(())
}
