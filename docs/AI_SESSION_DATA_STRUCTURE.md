# AI 终端会话数据结构分析

## 概述

本文档分析了 Codex、Claude、Gemini、Opencode 四款 AI 终端工具的会话数据存储结构，整理出共有字段清单，用于 OneSpace 的跨工具会话管理。

---

## 统一数据结构 (HistorySessionEntry)

```rust
pub struct HistorySessionEntry {
    pub tool: String,              // 工具标识
    pub tool_session_id: String,   // 原生会话 ID
    pub title: String,             // 会话标题
    pub working_dir: String,       // 工作目录
    pub model_name: Option<String>, // 使用的模型名称
    pub created_at_ms: i64,        // 创建时间戳 (毫秒)
    pub updated_at_ms: i64,        // 更新时间戳 (毫秒)
}
```

---

## 各工具数据结构详解

### 1. Codex (OpenAI Codex CLI)

**存储位置**: `~/.codex/sessions/` + `~/.codex/session_index.jsonl`

**文件格式**: JSONL (每行一个 JSON 对象)

**关键消息类型**:

| 类型 | 字段路径 | 说明 |
|------|---------|------|
| `session_meta` | `payload.id` | 会话 ID |
| `session_meta` | `payload.cwd` | 工作目录 |
| `session_meta` | `payload.timestamp` | 创建时间 (RFC3339) |
| `turn_context` | `payload.model` | 模型名称 |
| `event_msg` | `payload.message` | 用户消息 (可作为标题) |

**索引文件** (`session_index.jsonl`):
```json
{
  "id": "session-123",
  "thread_name": "会话标题",
  "updated_at": "2026-03-16T15:30:00.000Z"
}
```

**示例**:
```json
{"type":"session_meta","payload":{"id":"session-1","timestamp":"2026-03-03T01:19:17.343Z","cwd":"/project"}}
{"type":"turn_context","payload":{"model":"gpt-5.4"}}
{"type":"event_msg","payload":{"type":"user_message","message":"Name this project better"}}
```

---

### 2. Claude (Anthropic Claude Code)

**存储位置**: `~/.claude/projects/` + `~/.claude/history.jsonl`

**文件格式**: JSONL

**关键消息类型**:

| 类型 | 字段路径 | 说明 |
|------|---------|------|
| - | `sessionId` | 会话 ID |
| - | `cwd` | 工作目录 |
| - | `timestamp` | 时间戳 (RFC3339) |
| - | `display` | 显示标题 |
| `user` | `message.content` | 用户消息 (可作为标题) |
| `assistant` | `model` | 模型名称 |

**历史索引文件** (`history.jsonl`):
```json
{
  "sessionId": "abc-123",
  "project": "/project/path",
  "display": "会话标题",
  "timestamp": "2026-03-16T15:30:00.000Z"
}
```

**项目文件** (`projects/*.jsonl`):
```json
{"sessionId":"abc-123","cwd":"/project","timestamp":"2026-03-16T15:30:00.000Z","type":"user","message":{"content":"Help me"}}
{"sessionId":"abc-123","cwd":"/project","timestamp":"2026-03-16T15:31:00.000Z","type":"assistant","model":"claude-sonnet-4-5"}
```

---

### 3. Gemini (Google Gemini CLI)

**存储位置**: `~/.gemini/tmp/` + `~/.gemini/projects.json`

**文件格式**: JSON (单文件)

**会话文件结构**:
```json
{
  "sessionId": "abc123...",
  "projectHash": "sha256_hash",
  "startTime": "2026-03-16T15:30:00.000Z",
  "lastUpdated": "2026-03-16T15:35:00.000Z",
  "messages": [
    {
      "type": "user",
      "content": "用户消息"
    },
    {
      "type": "assistant", 
      "model": "gemini-2.5-pro"
    }
  ]
}
```

**项目映射** (`projects.json`):
```json
{
  "projects": {
    "/absolute/path/to/project": "identifier_string",
    "sha256_hash": "identifier_string"
  }
}
```

**工作目录解析**:
1. 从 `projectHash` 查找 `projects.json` 映射
2. 从文件路径父目录名称推断
3. SHA256 hash 后备

---

### 4. Opencode (OpenCode CLI)

#### 4.1 版本 1.2+ (SQLite 数据库)

**存储位置**: `~/.local/share/opencode/opencode.db`

**数据库表结构**:

**`session` 表**:
| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | TEXT | 会话 ID (ses_xxx) |
| `project_id` | TEXT | 项目 ID |
| `slug` | TEXT | 短名称 |
| `directory` | TEXT | 工作目录 |
| `title` | TEXT | 会话标题 |
| `version` | TEXT | 版本号 |
| `time_created` | INTEGER | 创建时间戳 (ms) |
| `time_updated` | INTEGER | 更新时间戳 (ms) |
| `time_archived` | INTEGER | 归档时间 (NULL=活跃) |

**`message` 表** (用于提取 model):
| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | TEXT | 消息 ID |
| `session_id` | TEXT | 关联会话 |
| `time_created` | INTEGER | 时间戳 |
| `data` | TEXT | JSON 消息内容 |

**查询示例**:
```sql
SELECT s.id, s.title, s.directory, s.time_created, s.time_updated,
       (SELECT json_extract(m.data, '$.modelID')
        FROM message m WHERE m.session_id = s.id
        ORDER BY m.time_created DESC LIMIT 1) as model_id
FROM session s
WHERE s.time_archived IS NULL
```

#### 4.2 版本 1.1.x (文件系统)

**存储位置**: `~/.local/share/opencode/storage/`

**会话元数据**: `storage/session/<project_id>/*.json` 或 `storage/session_diff/*.json`

**会话元数据文件结构**:
```json
{
  "id": "ses_xxx",
  "slug": "short-name",
  "projectID": "project_hash",
  "directory": "/working/dir",
  "title": "会话标题",
  "version": "1.1.56",
  "time": {
    "created": 1773646386008,
    "updated": 1773646415560
  }
}
```

**消息文件**: `storage/message/<session_id>/msg_*.json`

**消息文件结构**:
```json
{
  "id": "msg_xxx",
  "sessionID": "ses_xxx",
  "role": "user|assistant",
  "time": {
    "created": 1773646386020
  },
  "path": {
    "cwd": "/working/dir",
    "root": "/working/dir"
  },
  "modelID": "qwen3.5-plus",
  "providerID": "bailian"
}
```

---

## 共有字段清单

### 核心字段 (所有工具共有)

| 字段 | Codex | Claude | Gemini | Opencode 1.2+ | Opencode 1.1.x | 说明 |
|------|-------|--------|--------|---------------|----------------|------|
| **tool_session_id** | ✓ `payload.id` | ✓ `sessionId` | ✓ `sessionId` | ✓ `s.id` | ✓ `id` | 原生会话 ID |
| **title** | ✓ `thread_name` 或首条用户消息 | ✓ `display` 或首条用户消息 | ✓ 首条用户消息 | ✓ `s.title` | ✓ `title` | 会话标题 |
| **working_dir** | ✓ `payload.cwd` | ✓ `cwd` | ✓ 通过 projectHash 映射 | ✓ `s.directory` | ✓ `directory` 或 `path.cwd` | 工作目录 |
| **created_at** | ✓ `payload.timestamp` | ✓ `timestamp` (最早) | ✓ `startTime` | ✓ `s.time_created` | ✓ `time.created` | 创建时间 |
| **updated_at** | ✓ index file | ✓ `timestamp` (最晚) | ✓ `lastUpdated` | ✓ `s.time_updated` | ✓ `time.updated` | 更新时间 |

### 可选字段 (部分工具支持)

| 字段 | Codex | Claude | Gemini | Opencode 1.2+ | Opencode 1.1.x | 说明 |
|------|-------|--------|--------|---------------|----------------|------|
| **model_name** | ✓ `payload.model` | ✓ `model` | ✓ `model` | ✓ message.data | ✓ `modelID` | 使用的模型 |
| **project_id** | - | - | ✓ `projectHash` | ✓ `s.project_id` | ✓ `projectID` | 项目标识 |

---

## 字段提取策略对比

### 会话 ID 提取

```rust
// Codex
session_id = line.payload.id

// Claude  
session_id = line.sessionId 或 文件名 (不含.jsonl)

// Gemini
session_id = value.sessionId

// Opencode
session_id = value.id 或 s.id (数据库)
```

### 标题提取优先级

```rust
// Codex
1. session_index.jsonl 中的 thread_name
2. 第一条 user_message 的 message 字段
3. 会话 ID 后备

// Claude
1. history.jsonl 中的 display
2. 第一条 user 消息的 content
3. 会话 ID 后备

// Gemini
1. 第一条 type="user" 消息的 content
2. 会话 ID 后备

// Opencode
1. session 文件的 title 字段
2. session_diff 文件的 title 字段
3. 会话 ID 后备
```

### 工作目录提取

```rust
// Codex
working_dir = line.payload.cwd

// Claude
working_dir = line.cwd 或 history.jsonl.project

// Gemini
1. 从 projectHash 查 projects.json 映射
2. 从文件路径推断
3. SHA256 hash 后备

// Opencode 1.2+
working_dir = s.directory (数据库)

// Opencode 1.1.x
1. session 文件的 directory 字段
2. message 文件的 path.cwd 字段
3. projectID 映射后备
```

### 模型名称提取

```rust
// Codex
model_name = turn_context.payload.model (最后一个)

// Claude
model_name = assistant 消息的 model 字段 (最后一个)

// Gemini
model_name = 非 user 消息的 model 字段

// Opencode 1.2+
model_name = SELECT json_extract(data, '$.modelID') FROM message

// Opencode 1.1.x
model_name = message 文件的 modelID 字段
```

---

## 时间戳格式

| 工具 | 格式 | 解析方式 |
|------|------|---------|
| Codex | RFC3339 字符串 | `parse_rfc3339_millis` |
| Claude | RFC3339 字符串 | `parse_rfc3339_millis` |
| Gemini | RFC3339 字符串 | `parse_rfc3339_millis` |
| Opencode 1.2+ | Unix 毫秒 (INTEGER) | 直接使用 |
| Opencode 1.1.x | Unix 毫秒 (INTEGER) | 直接使用 |

---

## 存储架构对比

| 特性 | Codex | Claude | Gemini | Opencode 1.2+ | Opencode 1.1.x |
|------|-------|--------|--------|---------------|----------------|
| 存储类型 | 文件 (JSONL) | 文件 (JSONL) | 文件 (JSON) | SQLite | 文件 (JSON) |
| 索引文件 | ✓ session_index.jsonl | ✓ history.jsonl | - | - | - |
| 会话元数据 | ✓ session_meta 消息 | ✓ 内联 | ✓ 文件头部 | ✓ session 表 | ✓ session/*.json |
| 消息存储 | ✓ 同一 JSONL | ✓ 同一 JSONL | ✓ 同一 JSON | ✓ message 表 | ✓ message/*.json |
| 项目映射 | - | ✓ history.jsonl | ✓ projects.json | ✓ session 表 | ✓ project/*.json |
| 删除标记 | - | - | - | ✓ time_archived | - |

---

## OneSpace 实现建议

### 1. 统一查询接口

```rust
pub fn collect_history_sessions_for_tool(
    tool: &str,
    min_updated_at_ms: Option<i64>,
) -> Result<Vec<HistorySessionEntry>, String>
```

### 2. 增量同步策略

- 记录每个工具的 `last_seen_updated_at_ms`
- 仅扫描更新后的文件/记录
- 保留 15 秒缓冲窗口避免遗漏

### 3. 会话绑定逻辑

```rust
fn should_bind_history_entry_to_placeholder(
    session: &SessionRecord,
    entry: &HistorySessionEntry,
) -> bool {
    // 1. 工具类型匹配
    // 2. 占位符无 tool_session_id
    // 3. 状态为 pending_bind 或 unbound
    // 4. 工作目录匹配
    // 5. 时间戳在 15 分钟窗口内
}
```

### 4. 数据去重

按 `(tool, tool_session_id)` 去重，保留最新 `updated_at_ms` 的记录。

---

## 版本兼容性

| 工具 | 支持版本 | 备注 |
|------|---------|------|
| Codex | 最新 | 结构稳定 |
| Claude | 最新 | 结构稳定 |
| Gemini | 最新 | 结构稳定 |
| Opencode | 1.1.x, 1.2+ | 1.2+ 使用 SQLite，1.1.x 使用文件系统 |

---

## 测试数据示例

详见 `src-tauri/src/ai_sessions.rs` 中的单元测试:
- `opencode_history_parser_reads_title_and_message_model`
- `codex_history_parser_reads_title_model_and_working_dir`
- `claude_history_parser_prefers_last_prompt_and_reads_model`
- `gemini_history_parser_reads_first_user_title_and_model`
