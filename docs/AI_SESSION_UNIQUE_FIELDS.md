# AI 终端会话独特字段分析

本文档分析 Codex、Claude、Gemini、Opencode 四款工具会话数据中的**独特字段**，这些字段可用于增强"AI 终端会话"列表的展示信息。

---

## 字段总览

| 工具 | 独特字段数量 | 最有价值字段 | 数据来源 |
|------|-------------|-------------|---------|
| **Opencode** | 8 个 | `summary_*`, `slug`, `parent_id` | SQLite `session` 表 |
| **Claude** | 5 个 | `usage.*`, `gitBranch`, `slug` | JSONL 消息 |
| **Codex** | 4 个 | `cost`, `tokens.*`, `reasoning_tokens` | JSONL 消息 |
| **Gemini** | 3 个 | `tokenCount`, `cost` | JSON 消息 |

---

## Opencode (最丰富)

### 存储位置
- **SQLite (1.2+)**: `~/.local/share/opencode/opencode.db` - `session` 表
- **文件系统 (1.1.x)**: `~/.local/share/opencode/storage/session_diff/*.json`

### 独有字段

| 字段名 | 类型 | 示例值 | 说明 | 展示用途 |
|--------|------|--------|------|---------|
| **`slug`** | TEXT | `"glowing-moon"` | 会话短名称 (形容词 + 名词) | ✨ 生成趣味会话标签 |
| **`version`** | TEXT | `"1.2.27"` | 创建时的工具版本 | 📌 版本兼容性提示 |
| **`summary_additions`** | INTEGER | `788` | 代码新增行数 | 📊 代码产出量统计 |
| **`summary_deletions`** | INTEGER | `38` | 代码删除行数 | 📊 代码重构程度 |
| **`summary_files`** | INTEGER | `5` | 修改文件数 | 📊 影响范围评估 |
| **`parent_id`** | TEXT | `ses_xxx` | 父会话 ID (fork 关系) | 🔗 会话继承关系 |
| **`share_url`** | TEXT | `https://...` | 分享链接 (如果已分享) | 🔗 快速分享入口 |
| **`time_compacting`** | INTEGER | `1773646415560` | 压缩时间戳 | 🕐 最后优化时间 |
| **`time_archived`** | INTEGER | `NULL` | 归档时间戳 (NULL=活跃) | 🗄️ 归档状态标识 |
| **`revert`** | TEXT | JSON | 回退信息 | ↩️ 回退操作记录 |

### 示例数据

```sql
SELECT id, title, slug, version, 
       summary_additions, summary_deletions, summary_files,
       parent_id, share_url
FROM session 
ORDER BY time_updated DESC 
LIMIT 3;
```

结果:
```
ses_30a81ae6effeba1wqGmqbWZBbW | Opencode 会话同步状态显示异常及机制对齐 | glowing-moon | 1.2.27 | 788 | 38 | 5 | NULL | NULL
ses_30a6feca7ffekucAfkRQ3Jqku7 | 下一个节假日查询 | silent-circuit | 1.2.27 | 0 | 0 | 0 | NULL | NULL
ses_30a793667ffeaS8BpBXStVvG1t | 问候 | misty-pixel | 1.2.27 | 0 | 0 | 0 | NULL | NULL
```

### 展示建议

```tsx
// 会话列表卡片
<div className="session-card">
  <h3>{session.title}</h3>
  
  {/* 趣味标签 */}
  {session.slug && (
    <Badge variant="secondary">#{session.slug}</Badge>
  )}
  
  {/* 代码统计 */}
  {session.summary_additions > 0 && (
    <div className="code-stats">
      <span className="text-green-600">+{session.summary_additions}</span>
      <span className="text-red-600">-{session.summary_deletions}</span>
      <span className="text-muted-foreground">
        {session.summary_files} files
      </span>
    </div>
  )}
  
  {/* 版本标识 */}
  {session.version && (
    <Tooltip content={`Created with Opencode v${session.version}`}>
      <Badge variant="outline">v{session.version}</Badge>
    </Tooltip>
  )}
</div>
```

---

## Claude

### 存储位置
- `~/.claude/projects/*/*.jsonl`
- `~/.claude/history.jsonl` (索引)

### 独有字段

| 字段名 | 类型 | 示例值 | 说明 | 展示用途 |
|--------|------|--------|------|---------|
| **`version`** | TEXT | `"2.1.76"` | 消息的工具版本 | 📌 版本追踪 |
| **`gitBranch`** | TEXT | `"HEAD"` / `"feature/login"` | Git 分支名 | 🌿 分支上下文 |
| **`slug`** | TEXT | `"cosmic-crunching-cake"` | 会话短名称 (同 Opencode) | ✨ 趣味标签 |
| **`usage.input_tokens`** | INTEGER | `26477` | 输入 token 数 | 💰 成本估算 |
| **`usage.output_tokens`** | INTEGER | `42` | 输出 token 数 | 💰 成本估算 |
| **`usage.cache_read_input_tokens`** | INTEGER | `28442` | 缓存读取 token | ⚡ 缓存命中统计 |
| **`usage.cache_creation_input_tokens`** | INTEGER | `0` | 缓存创建 token | ⚡ 缓存写入统计 |
| **`service_tier`** | TEXT | `"standard"` | 服务等级 | 🎯 服务质量标识 |
| **`stop_reason`** | TEXT | `"end_turn"` / `"tool_use"` | 停止原因 | 🛑 会话终止分析 |
| **`parentUuid`** | TEXT | `uuid` | 父消息 UUID | 🔗 消息链关系 |

### 示例数据

```jsonl
{
  "sessionId": "0d04516e-4ce7-4cd3-827f-c2e9d8d8f9c5",
  "version": "2.1.76",
  "gitBranch": "HEAD",
  "slug": "cosmic-crunching-cake",
  "message": {
    "model": "qwen3.5-plus",
    "usage": {
      "input_tokens": 6,
      "output_tokens": 5227,
      "cache_creation_input_tokens": 197,
      "cache_read_input_tokens": 38067
    }
  }
}
```

### 展示建议

```tsx
// Claude 会话详情
<div className="claude-session-detail">
  {/* Git 分支标识 */}
  {session.gitBranch && session.gitBranch !== 'HEAD' && (
    <Badge variant="git">
      <GitBranchIcon className="w-3 h-3" />
      {session.gitBranch}
    </Badge>
  )}
  
  {/* Token 统计 */}
  {session.totalInputTokens > 0 && (
    <div className="token-stats">
      <Tooltip content="Input tokens">
        <span>⬆️ {formatNumber(session.totalInputTokens)}</span>
      </Tooltip>
      <Tooltip content="Output tokens">
        <span>⬇️ {formatNumber(session.totalOutputTokens)}</span>
      </Tooltip>
      {session.totalCacheRead > 0 && (
        <Tooltip content="Cache hit tokens">
          <span className="text-green-600">
            ⚡ {formatNumber(session.totalCacheRead)}
          </span>
        </Tooltip>
      )}
    </div>
  )}
  
  {/* 服务等级 */}
  {session.serviceTier && (
    <Badge variant={session.serviceTier === 'premium' ? 'default' : 'secondary'}>
      {session.serviceTier}
    </Badge>
  )}
</div>
```

---

## Codex

### 存储位置
- `~/.codex/sessions/*/session.jsonl`
- `~/.codex/session_index.jsonl` (索引)

### 独有字段

| 字段名 | 类型 | 示例值 | 说明 | 展示用途 |
|--------|------|--------|------|---------|
| **`payload.cost`** | NUMBER | `0.00045` | 单次请求成本 (USD) | 💰 精确成本追踪 |
| **`payload.tokens.input`** | INTEGER | `1234` | 输入 token 数 | 📊 Token 统计 |
| **`payload.tokens.output`** | INTEGER | `567` | 输出 token 数 | 📊 Token 统计 |
| **`payload.tokens.reasoning`** | INTEGER | `89` | 推理 token 数 | 🧠 推理强度分析 |
| **`payload.tokens.cache.read`** | INTEGER | `0` | 缓存读取 | ⚡ 缓存效率 |
| **`payload.tokens.cache.write`** | INTEGER | `0` | 缓存写入 | ⚡ 缓存效率 |

### 示例数据

```jsonl
{
  "type": "turn_context",
  "payload": {
    "model": "gpt-5.4",
    "cost": 0.00045,
    "tokens": {
      "input": 1234,
      "output": 567,
      "reasoning": 89,
      "cache": {
        "read": 0,
        "write": 0
      }
    }
  }
}
```

### 展示建议

```tsx
// Codex 成本卡片
<div className="codex-cost-card">
  {/* 成本显示 */}
  {session.totalCost > 0 && (
    <div className="cost-display">
      <DollarSign className="w-4 h-4" />
      <span>{session.totalCost.toFixed(4)}</span>
      <span className="text-muted-foreground text-xs">USD</span>
    </div>
  )}
  
  {/* Token 分布 */}
  <div className="token-breakdown">
    <div className="token-bar">
      <div 
        className="token-segment input" 
        style={{ width: `${inputPercent}%` }}
      />
      <div 
        className="token-segment output" 
        style={{ width: `${outputPercent}%` }}
      />
      <div 
        className="token-segment reasoning" 
        style={{ width: `${reasoningPercent}%` }}
      />
    </div>
    {session.totalReasoningTokens > 0 && (
      <Tooltip content="Reasoning tokens indicate complex analysis">
        <Badge variant="outline">🧠 {session.totalReasoningTokens}</Badge>
      </Tooltip>
    )}
  </div>
</div>
```

---

## Gemini

### 存储位置
- `~/.gemini/tmp/*/chats/*.json`

### 独有字段

| 字段名 | 类型 | 示例值 | 说明 | 展示用途 |
|--------|------|--------|------|---------|
| **`tokenCount`** | INTEGER | `1234` | 单次消息 token 数 | 📊 消息长度统计 |
| **`cost`** | NUMBER | `0.00012` | 单次消息成本 | 💰 成本追踪 |
| **`startTime`** | STRING | ISO8601 | 会话开始时间 | 🕐 精确时间戳 |
| **`lastUpdated`** | STRING | ISO8601 | 最后更新时间 | 🕐 精确时间戳 |

### 示例数据

```json
{
  "sessionId": "f132591f-043a-4f4d-9c9a-587926177c5b",
  "startTime": "2026-01-08T06:50:39.802Z",
  "lastUpdated": "2026-01-08T07:20:52.144Z",
  "messages": [
    {
      "type": "user",
      "message": "docker-compose.yml 的配置...",
      "timestamp": "2026-01-08T06:50:39.802Z"
    }
  ]
}
```

### 展示建议

```tsx
// Gemini 时间线
<div className="gemini-timeline">
  {/* 精确时长 */}
  <div className="duration-badge">
    <ClockIcon className="w-4 h-4" />
    <span>{formatDuration(session.startTime, session.lastUpdated)}</span>
  </div>
  
  {/* 消息密度 */}
  <div className="message-density">
    <span>{session.messageCount} messages</span>
    <span className="text-muted-foreground">
      over {formatDurationReadable(session.startTime, session.lastUpdated)}
    </span>
  </div>
</div>
```

---

## 跨工具对比

### 代码相关指标

| 指标 | Opencode | Claude | Codex | Gemini |
|------|----------|--------|-------|--------|
| 新增行数 | ✅ `summary_additions` | ❌ | ❌ | ❌ |
| 删除行数 | ✅ `summary_deletions` | ❌ | ❌ | ❌ |
| 修改文件数 | ✅ `summary_files` | ❌ | ❌ | ❌ |
| Git 分支 | ❌ | ✅ `gitBranch` | ❌ | ❌ |

### 成本相关指标

| 指标 | Opencode | Claude | Codex | Gemini |
|------|----------|--------|-------|--------|
| 总成本 | ❌ | ⚠️ 需计算 | ✅ `cost` | ✅ `cost` |
| 输入 token | ❌ | ✅ `usage.input_tokens` | ✅ `tokens.input` | ⚠️ `tokenCount` |
| 输出 token | ❌ | ✅ `usage.output_tokens` | ✅ `tokens.output` | ⚠️ `tokenCount` |
| 推理 token | ❌ | ❌ | ✅ `tokens.reasoning` | ❌ |
| 缓存读取 | ❌ | ✅ `usage.cache_read` | ✅ `tokens.cache.read` | ❌ |
| 缓存写入 | ❌ | ✅ `usage.cache_creation` | ✅ `tokens.cache.write` | ❌ |

### 会话关系

| 特性 | Opencode | Claude | Codex | Gemini |
|------|----------|--------|-------|--------|
| 会话 Fork | ✅ `parent_id` | ⚠️ `parentUuid` (消息级) | ❌ | ❌ |
| 分享链接 | ✅ `share_url` | ❌ | ❌ | ❌ |
| 归档状态 | ✅ `time_archived` | ❌ | ❌ | ❌ |
| 趣味名称 | ✅ `slug` | ✅ `slug` | ❌ | ❌ |

---

## 推荐实现方案

### 1. 数据库扩展

在 OneSpace 会话表中添加扩展字段:

```sql
ALTER TABLE ai_sessions 
ADD COLUMN opencode_slug TEXT,
ADD COLUMN opencode_version TEXT,
ADD COLUMN opencode_additions INTEGER,
ADD COLUMN opencode_deletions INTEGER,
ADD COLUMN opencode_files INTEGER,
ADD COLUMN claude_git_branch TEXT,
ADD COLUMN claude_service_tier TEXT,
ADD COLUMN total_input_tokens INTEGER,
ADD COLUMN total_output_tokens INTEGER,
ADD COLUMN total_cost_usd REAL,
ADD COLUMN total_cache_hit_tokens INTEGER;
```

### 2. 前端组件

```tsx
interface SessionCardProps {
  session: SessionRecord;
}

export function SessionCard({ session }: SessionCardProps) {
  return (
    <Card>
      <CardHeader>
        <div className="flex items-center gap-2">
          <ToolIcon tool={session.tool} />
          <CardTitle>{session.title}</CardTitle>
        </div>
        
        {/* 趣味标签 (Opencode/Claude) */}
        {(session.opencodeSlug || session.claudeSlug) && (
          <Badge variant="secondary" className="w-fit">
            #{session.opencodeSlug || session.claudeSlug}
          </Badge>
        )}
      </CardHeader>
      
      <CardContent>
        {/* Git 分支 (Claude) */}
        {session.claudeGitBranch && session.claudeGitBranch !== 'HEAD' && (
          <div className="git-branch">
            <GitBranchIcon />
            <span>{session.claudeGitBranch}</span>
          </div>
        )}
        
        {/* 代码统计 (Opencode) */}
        {session.opencodeAdditions !== null && (
          <div className="code-stats">
            <span className="text-green-600">+{session.opencodeAdditions}</span>
            <span className="text-red-600">-{session.opencodeDeletions}</span>
            <span className="text-muted-foreground">
              · {session.opencodeFiles} files
            </span>
          </div>
        )}
        
        {/* Token 统计 (跨工具) */}
        {session.totalInputTokens > 0 && (
          <div className="token-stats">
            <span>⬆️ {formatNumber(session.totalInputTokens)}</span>
            <span>⬇️ {formatNumber(session.totalOutputTokens)}</span>
          </div>
        )}
        
        {/* 成本 (Codex/Gemini) */}
        {session.totalCostUsd > 0 && (
          <div className="cost-display">
            <DollarSign />
            <span>${session.totalCostUsd.toFixed(4)}</span>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
```

### 3. 同步逻辑

```rust
// src-tauri/src/ai_sessions.rs

fn extract_opencode_enhanced_fields(value: &Value) -> OpencodeEnhancedFields {
    OpencodeEnhancedFields {
        slug: value.get("slug").and_then(|v| v.as_str()).map(String::from),
        version: value.get("version").and_then(|v| v.as_str()).map(String::from),
        additions: value.get("summary_additions").and_then(|v| v.as_i64()),
        deletions: value.get("summary_deletions").and_then(|v| v.as_i64()),
        files: value.get("summary_files").and_then(|v| v.as_i64()),
        parent_id: value.get("parent_id").and_then(|v| v.as_str()).map(String::from),
        share_url: value.get("share_url").and_then(|v| v.as_str()).map(String::from),
    }
}

fn extract_claude_enhanced_fields(line: &Value) -> ClaudeEnhancedFields {
    ClaudeEnhancedFields {
        version: line.get("version").and_then(|v| v.as_str()).map(String::from),
        git_branch: line.get("gitBranch").and_then(|v| v.as_str()).map(String::from),
        slug: line.get("slug").and_then(|v| v.as_str()).map(String::from),
        input_tokens: line.get("message")
            .and_then(|m| m.get("usage"))
            .and_then(|u| u.get("input_tokens"))
            .and_then(|v| v.as_i64()),
        output_tokens: line.get("message")
            .and_then(|m| m.get("usage"))
            .and_then(|u| u.get("output_tokens"))
            .and_then(|v| v.as_i64()),
        service_tier: line.get("message")
            .and_then(|m| m.get("usage"))
            .and_then(|u| u.get("service_tier"))
            .and_then(|v| v.as_str())
            .map(String::from),
    }
}
```

---

## 优先级建议

### 高优先级 (立即实现)

1. **Opencode 代码统计** (`summary_additions`, `summary_deletions`, `summary_files`)
   - 最直观的生产力量化指标
   - 仅 Opencode 提供，差异化优势

2. **Opencode/Claude 趣味标签** (`slug`)
   - 增加趣味性
   - 易于识别和记忆会话

3. **Claude Git 分支** (`gitBranch`)
   - 开发上下文重要信息
   - 便于切换项目时识别

### 中优先级 (后续迭代)

4. **Token 统计** (所有工具)
   - 需要跨消息聚合计算
   - 成本估算基础

5. **Opencode 版本标识** (`version`)
   - 排查兼容性问题
   - 版本分布分析

### 低优先级 (可选)

6. **成本显示** (Codex/Gemini)
   - 需要 API 定价表支持
   - 汇率转换复杂

7. **缓存统计** (Claude/Codex)
   - 高级用户关注
   - 优化建议参考

---

## 数据来源验证

### 当前环境实测数据

**Opencode**:
```bash
sqlite3 ~/.local/share/opencode/opencode.db \
  "SELECT slug, version, summary_additions, summary_deletions, summary_files \
   FROM session ORDER BY time_updated DESC LIMIT 3;"

# 输出:
# glowing-moon|1.2.27|788|38|5
# silent-circuit|1.2.27|0|0|0
# misty-pixel|1.2.27|0|0|0
```

**Claude**:
```bash
head -5 ~/.claude/projects/*/*.jsonl | grep -E '"slug"|"gitBranch"|"usage"'

# 输出:
# "slug":"cosmic-crunching-cake","version":"2.1.76","gitBranch":"HEAD"
# "usage":{"input_tokens":26477,"output_tokens":42}
```

**Codex**:
```bash
# 当前环境无 Codex 会话数据
```

**Gemini**:
```bash
# Gemini 会话文件格式不包含 token/cost，需从消息中计算
```
