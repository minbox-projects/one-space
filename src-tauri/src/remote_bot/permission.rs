//! Deterministic permission risk classification and pending-request tracking.
//!
//! The classifier is deliberately conservative: only an explicit allowlist of
//! read-only, build/test and project-local commands is treated as routine, and
//! anything ambiguous (unknown commands, command or process substitution,
//! redirection, empty input) fails closed as high risk. It performs no I/O and
//! never consults settings, so the same input always produces the same risk.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Risk level assigned to a permission request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionRisk {
    Routine,
    HighRisk,
}

impl PermissionRisk {
    /// User-facing risk label shown on approval cards.
    pub fn label(self) -> &'static str {
        match self {
            PermissionRisk::Routine => "常规",
            PermissionRisk::HighRisk => "高危",
        }
    }
}

/// Classify an OpenCode permission request by type and, for shell requests, by
/// the command that would run.
pub fn classify_permission(permission_type: &str, command: Option<&str>) -> PermissionRisk {
    let kind = permission_type.trim().to_ascii_lowercase();
    match kind.as_str() {
        "edit" | "write" | "read" | "patch" | "apply_patch" | "list" | "glob" | "grep" => {
            PermissionRisk::Routine
        }
        // Only plain POSIX-style shells are classified as shell requests; any
        // other type, including empty or unknown tools, is high risk.
        "bash" | "sh" => classify_shell(command),
        _ => PermissionRisk::HighRisk,
    }
}

fn classify_shell(command: Option<&str>) -> PermissionRisk {
    let Some(raw) = command else {
        return PermissionRisk::HighRisk;
    };
    if raw.trim().is_empty() {
        return PermissionRisk::HighRisk;
    }
    // Command and process substitution can run arbitrary nested commands, so
    // reject the whole command before looking at individual segments.
    if raw.contains("$(")
        || raw.contains('`')
        || raw.contains("<(")
        || raw.contains(">(")
    {
        return PermissionRisk::HighRisk;
    }

    let mut saw_segment = false;
    for segment in split_segments(raw) {
        if segment.trim().is_empty() {
            continue;
        }
        saw_segment = true;
        if has_unsafe_unquoted_operator(segment) {
            return PermissionRisk::HighRisk;
        }
        let tokens = tokenize(segment);
        if classify_segment(&tokens) != PermissionRisk::Routine {
            return PermissionRisk::HighRisk;
        }
    }

    if saw_segment {
        PermissionRisk::Routine
    } else {
        PermissionRisk::HighRisk
    }
}

/// Split on `;`, `&&`, `||`, `|` and newlines without touching quoted text.
fn split_segments(input: &str) -> Vec<&str> {
    let bytes = input.as_bytes();
    let mut segments = Vec::new();
    let mut start = 0;
    let mut index = 0;
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;

    while index < bytes.len() {
        let byte = bytes[index];
        if escaped {
            escaped = false;
            index += 1;
            continue;
        }
        match byte {
            b'\\' if !in_single => escaped = true,
            b'\'' if !in_double => in_single = !in_single,
            b'"' if !in_single => in_double = !in_double,
            b'\n' | b';' if !in_single && !in_double => {
                segments.push(&input[start..index]);
                start = index + 1;
            }
            b'|' if !in_single && !in_double => {
                segments.push(&input[start..index]);
                index += if index + 1 < bytes.len() && bytes[index + 1] == b'|' {
                    2
                } else {
                    1
                };
                start = index;
                continue;
            }
            b'&' if !in_single
                && !in_double
                && index + 1 < bytes.len()
                && bytes[index + 1] == b'&' =>
            {
                segments.push(&input[start..index]);
                index += 2;
                start = index;
                continue;
            }
            _ => {}
        }
        index += 1;
    }
    segments.push(&input[start..]);
    segments
}

/// Reject redirection and background operators that are not quoted. Quoted
/// `<`/`>` are plain text, so they stay allowed.
fn has_unsafe_unquoted_operator(segment: &str) -> bool {
    let chars: Vec<char> = segment.chars().collect();
    let mut index = 0;
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;

    while index < chars.len() {
        let current = chars[index];
        if escaped {
            escaped = false;
            index += 1;
            continue;
        }
        match current {
            '\\' if !in_single => escaped = true,
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            '<' | '>' if !in_single && !in_double => return true,
            '&' if !in_single && !in_double => {
                if index + 1 < chars.len() && chars[index + 1] == '&' {
                    index += 2;
                    continue;
                }
                return true;
            }
            _ => {}
        }
        index += 1;
    }
    false
}

struct Token {
    text: String,
}

/// Quote-aware tokenizer that removes surrounding quotes and resolves
/// backslash escapes.
fn tokenize(segment: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut has_token = false;
    let mut chars = segment.chars().peekable();
    let mut in_single = false;
    let mut in_double = false;

    while let Some(current_char) = chars.next() {
        if in_single {
            if current_char == '\'' {
                in_single = false;
            } else {
                current.push(current_char);
            }
            has_token = true;
            continue;
        }
        if in_double {
            match current_char {
                '"' => in_double = false,
                '\\' => match chars.peek().copied() {
                    Some(next) if matches!(next, '"' | '\\' | '$' | '`' | '\n') => {
                        current.push(chars.next().expect("peeked character exists"));
                    }
                    _ => current.push('\\'),
                },
                _ => current.push(current_char),
            }
            has_token = true;
            continue;
        }
        match current_char {
            '\'' => {
                in_single = true;
                has_token = true;
            }
            '"' => {
                in_double = true;
                has_token = true;
            }
            '\\' => {
                if let Some(next) = chars.next() {
                    current.push(next);
                    has_token = true;
                }
            }
            c if c.is_whitespace() => {
                if has_token {
                    tokens.push(Token {
                        text: std::mem::take(&mut current),
                    });
                    has_token = false;
                }
            }
            c => {
                current.push(c);
                has_token = true;
            }
        }
    }

    if has_token {
        tokens.push(Token { text: current });
    }
    tokens
}

/// Environment keys that alter how later commands resolve or execute, so a
/// leading assignment for one can inject code even without shell metacharacters.
fn is_dangerous_env_key(key: &str) -> bool {
    let upper = key.to_ascii_uppercase();
    matches!(
        upper.as_str(),
        "PATH" | "LD_PRELOAD" | "BASH_ENV" | "ENV" | "NODE_OPTIONS"
    ) || upper.starts_with("DYLD_")
        || upper.starts_with("GIT_CONFIG_")
}

fn is_env_assignment(token: &str) -> bool {
    let Some((name, _)) = token.split_once('=') else {
        return false;
    };
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() || first == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn classify_segment(tokens: &[Token]) -> PermissionRisk {
    let mut index = 0;
    loop {
        while index < tokens.len() && is_env_assignment(&tokens[index].text) {
            if let Some((key, _)) = tokens[index].text.split_once('=') {
                if is_dangerous_env_key(key) {
                    return PermissionRisk::HighRisk;
                }
            }
            index += 1;
        }
        if index >= tokens.len() {
            return PermissionRisk::HighRisk;
        }
        match tokens[index].text.as_str() {
            "timeout" => {
                index += 1;
                if index >= tokens.len() {
                    return PermissionRisk::HighRisk;
                }
                index += 1; // duration
            }
            "nice" | "command" | "builtin" => index += 1,
            _ => break,
        }
    }

    let command = tokens[index].text.as_str();
    let args: Vec<&str> = tokens[index + 1..]
        .iter()
        .map(|token| token.text.as_str())
        .collect();
    classify_command(command, &args)
}

fn classify_command(command: &str, args: &[&str]) -> PermissionRisk {
    match command {
        "pwd" | "ls" | "cat" | "head" | "tail" | "wc" | "stat" | "file" | "du" | "df"
        | "tree" | "rg" | "grep" | "sort" | "uniq" | "cut" | "tr" | "diff" | "cmp" | "which"
        | "type" | "echo" | "printf" | "basename" | "dirname" | "realpath" | "readlink"
        | "date" | "uname" | "sw_vers" | "whoami" | "id" | "ps" | "pgrep" | "true" | "false" => {
            PermissionRisk::Routine
        }
        "sed" => classify_sed(args),
        "find" => classify_find(args),
        "cargo" => subcommand_in(
            args,
            &[
                "test", "build", "check", "fmt", "clippy", "metadata", "tree", "bench", "doc",
            ],
        ),
        "npm" => classify_npm(args),
        "npx" => subcommand_in(args, &["tsc", "vitest", "jest", "eslint", "prettier"]),
        "pnpm" | "yarn" => subcommand_in(args, &["test", "build", "lint"]),
        "vitest" | "jest" | "pytest" | "mypy" | "eslint" => PermissionRisk::Routine,
        "python" => {
            if args.len() >= 2 && args[0] == "-m" && args[1] == "pytest" {
                PermissionRisk::Routine
            } else {
                PermissionRisk::HighRisk
            }
        }
        "go" => subcommand_in(args, &["test", "build", "vet"]),
        "make" => subcommand_in(args, &["test", "build", "lint", "check"]),
        "ruff" => {
            if args.first() == Some(&"check") {
                PermissionRisk::Routine
            } else {
                PermissionRisk::HighRisk
            }
        }
        "prettier" => {
            if args.first() == Some(&"--check") {
                PermissionRisk::Routine
            } else {
                PermissionRisk::HighRisk
            }
        }
        "git" => classify_git(args),
        "mkdir" | "touch" | "cd" | "sleep" => PermissionRisk::Routine,
        _ => PermissionRisk::HighRisk,
    }
}

fn subcommand_in(args: &[&str], allowed: &[&str]) -> PermissionRisk {
    match args.first() {
        Some(sub) if allowed.contains(sub) => PermissionRisk::Routine,
        _ => PermissionRisk::HighRisk,
    }
}

fn classify_npm(args: &[&str]) -> PermissionRisk {
    match args.first().copied() {
        Some("test") => PermissionRisk::Routine,
        Some("run") => match args.get(1).copied() {
            Some("test" | "build" | "lint" | "check" | "typecheck") => PermissionRisk::Routine,
            _ => PermissionRisk::HighRisk,
        },
        _ => PermissionRisk::HighRisk,
    }
}

fn classify_sed(args: &[&str]) -> PermissionRisk {
    let mut index = 0;
    let mut quiet = false;
    while index < args.len() {
        let arg = args[index];
        if arg == "-i" || arg.starts_with("-i") || arg == "--in-place" || arg.starts_with("--in-place=") {
            return PermissionRisk::HighRisk;
        }
        if arg == "-n" || arg == "--quiet" || arg == "--silent" {
            quiet = true;
            index += 1;
            continue;
        }
        if arg.starts_with('-') {
            // Unknown options have unknown effects; fail closed.
            return PermissionRisk::HighRisk;
        }
        break;
    }

    if !quiet {
        return PermissionRisk::HighRisk;
    }
    let Some(script) = args.get(index) else {
        return PermissionRisk::HighRisk;
    };
    if script.contains(['w', 'W', 'e']) {
        PermissionRisk::HighRisk
    } else {
        PermissionRisk::Routine
    }
}

fn classify_find(args: &[&str]) -> PermissionRisk {
    const DESTRUCTIVE: [&str; 5] = ["-delete", "-exec", "-execdir", "-ok", "-okdir"];
    if args
        .iter()
        .any(|arg| DESTRUCTIVE.iter().any(|flag| arg.starts_with(flag)))
    {
        PermissionRisk::HighRisk
    } else {
        PermissionRisk::Routine
    }
}

fn classify_git(args: &[&str]) -> PermissionRisk {
    let Some(subcommand) = args.first().copied() else {
        return PermissionRisk::HighRisk;
    };
    let rest = &args[1..];

    match subcommand {
        "status" | "diff" | "log" | "show" | "rev-parse" | "blame" | "describe" | "ls-files"
        | "grep" | "add" => PermissionRisk::Routine,
        "commit" => {
            if rest.iter().any(|arg| *arg == "--amend") {
                PermissionRisk::HighRisk
            } else {
                PermissionRisk::Routine
            }
        }
        "branch" => {
            const FORBIDDEN: [&str; 8] =
                ["-d", "-D", "--delete", "-m", "-M", "--move", "-c", "-C"];
            if rest.iter().any(|arg| FORBIDDEN.contains(arg)) {
                PermissionRisk::HighRisk
            } else {
                PermissionRisk::Routine
            }
        }
        "tag" => {
            if rest.iter().any(|arg| *arg == "-d" || *arg == "--delete") {
                PermissionRisk::HighRisk
            } else {
                PermissionRisk::Routine
            }
        }
        "remote" => {
            if rest.len() == 1 && rest[0] == "-v" {
                PermissionRisk::Routine
            } else {
                PermissionRisk::HighRisk
            }
        }
        "config" => {
            if rest.first() == Some(&"--get") {
                PermissionRisk::Routine
            } else {
                PermissionRisk::HighRisk
            }
        }
        "checkout" => {
            if rest.first() == Some(&"-b") {
                PermissionRisk::Routine
            } else {
                PermissionRisk::HighRisk
            }
        }
        "switch" => {
            if rest.first() == Some(&"-c") {
                PermissionRisk::Routine
            } else {
                PermissionRisk::HighRisk
            }
        }
        "stash" => match rest.first().copied() {
            Some("push" | "pop") => PermissionRisk::Routine,
            _ => PermissionRisk::HighRisk,
        },
        _ => PermissionRisk::HighRisk,
    }
}

/// How long a pending permission request may wait for a decision.
///
/// The bound is absolute rather than extended by activity so a request that is
/// never answered stops blocking the agent instead of lingering indefinitely.
pub const PERMISSION_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// Whether a request sent at `sent_at` has reached the decision deadline.
///
/// `saturating_duration_since` keeps a `now` earlier than `sent_at` from
/// panicking or reporting expiry.
pub fn permission_timed_out(sent_at: Instant, now: Instant) -> bool {
    now.saturating_duration_since(sent_at) >= PERMISSION_TIMEOUT
}

/// Reply value returned to the agent for a resolved permission request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionDecision {
    Once,
    Always,
    Deny,
}

impl PermissionDecision {
    /// Protocol reply string understood by the agent permission endpoint.
    pub fn as_reply(self) -> &'static str {
        match self {
            PermissionDecision::Once => "once",
            PermissionDecision::Always => "always",
            PermissionDecision::Deny => "deny",
        }
    }
}

/// Decision encoded in an inline-keyboard callback for a permission request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionCallback {
    Once(String),
    Always(String),
    Deny(String),
}

/// Parse permission callback data of the form `perm_once:<id>`,
/// `perm_always:<id>` or `perm_deny:<id>`.
///
/// Only the first colon separates the action from the identifier, because
/// request ids may themselves contain colons; the identifier is preserved
/// exactly as sent. Empty or whitespace-only identifiers are rejected so a
/// blank callback can never resolve a request.
pub fn parse_permission_callback(data: &str) -> Option<PermissionCallback> {
    let (prefix, id) = data.split_once(':')?;
    let build = match prefix {
        "perm_once" => PermissionCallback::Once,
        "perm_always" => PermissionCallback::Always,
        "perm_deny" => PermissionCallback::Deny,
        _ => return None,
    };
    if id.trim().is_empty() {
        return None;
    }
    Some(build(id.to_string()))
}

/// A permission request awaiting a human decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingPermissionRequest {
    pub request_id: String,
    pub chat_id: i64,
    pub permission_type: String,
    pub command: Option<String>,
    pub risk: PermissionRisk,
    pub card_message_id: Option<i64>,
    pub session_id: Option<String>,
    pub sent_at: Instant,
}

impl PendingPermissionRequest {
    pub fn new(
        request_id: impl Into<String>,
        chat_id: i64,
        permission_type: impl Into<String>,
        command: Option<String>,
        risk: PermissionRisk,
        sent_at: Instant,
    ) -> Self {
        Self {
            request_id: request_id.into(),
            chat_id,
            permission_type: permission_type.into(),
            command,
            risk,
            card_message_id: None,
            session_id: None,
            sent_at,
        }
    }

    /// Attach the OpenCode session that raised the request, when known.
    pub fn with_session_id(mut self, session_id: Option<String>) -> Self {
        self.session_id = session_id;
        self
    }

    /// Attach the Telegram message id of the delivered approval card.
    pub fn with_card_message_id(mut self, card_message_id: Option<i64>) -> Self {
        self.card_message_id = card_message_id;
        self
    }
}

/// A pending request paired with the decision that resolved it.
#[derive(Debug)]
pub struct PermissionResolution {
    pub request: PendingPermissionRequest,
    pub decision: PermissionDecision,
}

/// In-memory store of permission requests that no decision has resolved yet.
///
/// Shared across async tasks, so access is serialized by a `Mutex` and no lock
/// is held across an `await`; the registry is `Send + Sync` because the guard
/// never escapes a method call.
#[derive(Default)]
pub struct PermissionRegistry {
    pending: Mutex<HashMap<String, PendingPermissionRequest>>,
}

impl PermissionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// A panic while another thread held the lock must not disable the
    /// registry; recovered data is still the last consistent map contents.
    fn lock(&self) -> MutexGuard<'_, HashMap<String, PendingPermissionRequest>> {
        self.pending.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Store `request`, returning the request it replaced for the same id.
    pub fn insert(&self, request: PendingPermissionRequest) -> Option<PendingPermissionRequest> {
        self.lock().insert(request.request_id.clone(), request)
    }

    pub fn get(&self, id: &str) -> Option<PendingPermissionRequest> {
        self.lock().get(id).cloned()
    }

    /// Record the delivered approval card's message id on `request_id`.
    ///
    /// Returns `false` when no pending request with that id exists, so a card
    /// delivered after the request resolved is simply ignored.
    pub fn set_card_message_id(&self, request_id: &str, message_id: i64) -> bool {
        match self.lock().get_mut(request_id) {
            Some(request) => {
                request.card_message_id = Some(message_id);
                true
            }
            None => false,
        }
    }

    pub fn len(&self) -> usize {
        self.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.lock().is_empty()
    }

    /// Resolve `id` exactly once: the entry is removed before returning, so a
    /// repeated or concurrent decision cannot apply twice.
    pub fn resolve(&self, id: &str, decision: PermissionDecision) -> Option<PermissionResolution> {
        let request = self.lock().remove(id)?;
        Some(PermissionResolution { request, decision })
    }

    /// Remove `id` only if it has passed the timeout at `now`.
    pub fn expire(&self, id: &str, now: Instant) -> Option<PendingPermissionRequest> {
        let mut guard = self.lock();
        let timed_out = guard
            .get(id)
            .is_some_and(|request| permission_timed_out(request.sent_at, now));
        if timed_out {
            guard.remove(id)
        } else {
            None
        }
    }

    /// Remove every request that has passed the timeout at `now`.
    pub fn take_expired(&self, now: Instant) -> Vec<PendingPermissionRequest> {
        let mut guard = self.lock();
        let expired_ids: Vec<String> = guard
            .values()
            .filter(|request| permission_timed_out(request.sent_at, now))
            .map(|request| request.request_id.clone())
            .collect();
        expired_ids
            .into_iter()
            .filter_map(|id| guard.remove(&id))
            .collect()
    }

    /// Remove and return every pending request.
    pub fn clear(&self) -> Vec<PendingPermissionRequest> {
        self.lock().drain().map(|(_, request)| request).collect()
    }

    pub fn remove(&self, id: &str) -> Option<PendingPermissionRequest> {
        self.lock().remove(id)
    }
}
