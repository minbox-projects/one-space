use super::json::json_nonempty_string;
use super::types::{AntigravityQuotaBucket, AntigravityQuotaGroup, AntigravityQuotaSnapshot};
use serde_json::Value;
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration as StdDuration, Instant};

// ---------------------------------------------------------------------------
// Antigravity quota (`agy -p /usage --output-format json`)
// ---------------------------------------------------------------------------

const ANTIGRAVITY_QUOTA_CACHE_TTL: StdDuration = StdDuration::from_secs(300);
/// Slightly above the CLI's own `--print-timeout 30s` so a well-behaved CLI
/// trips its own deadline first; this is only a hard stop for a hung process.
const ANTIGRAVITY_QUOTA_EXEC_TIMEOUT: StdDuration = StdDuration::from_secs(35);
const ANTIGRAVITY_QUOTA_TEXT_LIMIT: usize = 400;

#[derive(Debug)]
pub(super) struct CachedAntigravityQuota {
    collected_at: Instant,
    snapshot: Arc<AntigravityQuotaSnapshot>,
}

pub(super) fn antigravity_quota_cache() -> &'static Mutex<Option<CachedAntigravityQuota>> {
    static CACHE: OnceLock<Mutex<Option<CachedAntigravityQuota>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

/// Whether a cached quota snapshot collected at `cached_at` is still within the
/// 5 minute TTL relative to `now`.
pub fn antigravity_quota_cache_fresh(cached_at: Instant, now: Instant) -> bool {
    now.saturating_duration_since(cached_at) < ANTIGRAVITY_QUOTA_CACHE_TTL
}

/// Runs the Antigravity usage command and returns the parsed snapshot. Only a
/// successful result is cached (5 minute TTL); failures are retried on demand.
/// When `force_refresh` is true, the 5-minute cache is bypassed and refreshed.
#[tauri::command(async)]
pub fn sessions_antigravity_quota(
    force_refresh: Option<bool>,
) -> Result<AntigravityQuotaSnapshot, String> {
    let cache = antigravity_quota_cache();
    if !force_refresh.unwrap_or(false) {
        let guard = cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(cached) = guard.as_ref() {
            if antigravity_quota_cache_fresh(cached.collected_at, Instant::now()) {
                return Ok((*cached.snapshot).clone());
            }
        }
    }

    let snapshot = fetch_antigravity_quota()?;

    let mut guard = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = Some(CachedAntigravityQuota {
        collected_at: Instant::now(),
        snapshot: Arc::new(snapshot.clone()),
    });
    Ok(snapshot)
}

fn fetch_antigravity_quota() -> Result<AntigravityQuotaSnapshot, String> {
    let stdout = run_antigravity_quota_command()?;
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Err(
            "Antigravity 用量查询无输出 (empty output)，可能未登录或 CLI 未正确安装，请重新登录后重试"
                .to_string(),
        );
    }
    let value: Value = serde_json::from_str(trimmed).map_err(|error| {
        format!(
            "Antigravity 用量输出无法解析 (invalid JSON)，可能未登录或 CLI 输出异常：{error}；stdout: {}",
            truncate_for_error(trimmed)
        )
    })?;
    parse_antigravity_quota_envelope(&value)
}

/// Restores a quota snapshot from a raw `agy` JSON envelope.
///
/// A non-`SUCCESS` status or a missing `command.data.groups` is an error, and a
/// bucket without a numeric `remaining_fraction` fails the whole envelope so a
/// partial snapshot can never be presented as complete.
pub fn parse_antigravity_quota_envelope(
    value: &Value,
) -> Result<AntigravityQuotaSnapshot, String> {
    if value.get("status").and_then(Value::as_str) != Some("SUCCESS") {
        return Err(format!(
            "Antigravity 用量命令返回失败状态 (status != SUCCESS)：{}",
            value
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
        ));
    }
    let groups = value
        .get("command")
        .and_then(|command| command.get("data"))
        .and_then(|data| data.get("groups"))
        .and_then(Value::as_array)
        .ok_or_else(|| {
            "Antigravity 用量响应缺少 command.data.groups (missing groups)".to_string()
        })?;

    let mut parsed_groups = Vec::with_capacity(groups.len());
    for group in groups {
        let mut buckets = Vec::new();
        if let Some(raw_buckets) = group.get("buckets").and_then(Value::as_array) {
            buckets.reserve(raw_buckets.len());
            for bucket in raw_buckets {
                let remaining_fraction = bucket
                    .get("remaining_fraction")
                    .and_then(Value::as_f64)
                    .ok_or_else(|| {
                        "Antigravity 用量分桶缺少数字 remaining_fraction (invalid remaining_fraction)"
                            .to_string()
                    })?;
                buckets.push(AntigravityQuotaBucket {
                    id: json_nonempty_string(bucket.get("id")).unwrap_or_default(),
                    name: json_nonempty_string(bucket.get("name")).unwrap_or_default(),
                    window: json_nonempty_string(bucket.get("window")).unwrap_or_default(),
                    remaining_fraction,
                    reset_time: json_nonempty_string(bucket.get("reset_time"))
                        .unwrap_or_default(),
                    description: json_nonempty_string(bucket.get("description")),
                });
            }
        }
        parsed_groups.push(AntigravityQuotaGroup {
            name: json_nonempty_string(group.get("name")).unwrap_or_default(),
            description: json_nonempty_string(group.get("description")),
            buckets,
        });
    }
    Ok(AntigravityQuotaSnapshot {
        groups: parsed_groups,
    })
}

fn run_antigravity_quota_command() -> Result<String, String> {
    let mut command = Command::new("agy");
    command
        .arg("-p")
        .arg("/usage")
        .arg("--output-format")
        .arg("json")
        .arg("--print-timeout")
        .arg("30s")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // GUI launches often miss Homebrew paths; reuse the CLI probe's PATH fixup.
    if let Some(path) = crate::cli_probe::augmented_path() {
        command.env("PATH", path);
    }

    let mut child = command.spawn().map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => {
            "未检测到 Antigravity CLI (agy not found)，请先安装 Antigravity 命令行工具后再查询用量"
                .to_string()
        }
        _ => format!("无法启动 Antigravity CLI (agy spawn failed)：{error}"),
    })?;

    let stdout_handle = child.stdout.take().map(spawn_pipe_reader);
    let stderr_handle = child.stderr.take().map(spawn_pipe_reader);

    let deadline = Instant::now() + ANTIGRAVITY_QUOTA_EXEC_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = join_pipe_reader(stdout_handle);
                    let _ = join_pipe_reader(stderr_handle);
                    return Err(
                        "Antigravity 用量查询超时 (timeout, 35 秒内未返回)，请检查网络后重试"
                            .to_string(),
                    );
                }
                std::thread::sleep(StdDuration::from_millis(50));
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = join_pipe_reader(stdout_handle);
                let _ = join_pipe_reader(stderr_handle);
                return Err(format!("Antigravity CLI (agy) 执行失败 (exec failed)：{error}"));
            }
        }
    };

    let stdout = join_pipe_reader(stdout_handle).unwrap_or_default();
    let stderr = join_pipe_reader(stderr_handle).unwrap_or_default();

    if !status.success() {
        let code = status
            .code()
            .map(|code| code.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        return Err(format!(
            "Antigravity 用量查询失败 (exit code {code}，可能未登录或未安装)：stderr: {}",
            truncate_for_error(stderr.trim())
        ));
    }
    Ok(stdout)
}

fn spawn_pipe_reader<R>(mut pipe: R) -> std::thread::JoinHandle<String>
where
    R: Read + Send + 'static,
{
    std::thread::spawn(move || {
        let mut buffer = String::new();
        let _ = pipe.read_to_string(&mut buffer);
        buffer
    })
}

fn join_pipe_reader(handle: Option<std::thread::JoinHandle<String>>) -> Option<String> {
    handle.and_then(|handle| handle.join().ok())
}

fn truncate_for_error(text: &str) -> String {
    let mut out: String = text.chars().take(ANTIGRAVITY_QUOTA_TEXT_LIMIT).collect();
    if text.chars().count() > ANTIGRAVITY_QUOTA_TEXT_LIMIT {
        out.push('…');
    }
    if out.is_empty() {
        "（空）".to_string()
    } else {
        out
    }
}
