# AI 终端会话独特字段分析

本文档分析 Codex、Claude、Gemini、Opencode 四款工具会话数据中的**独特字段**，这些字段可用于增强"AI 终端会话"列表的展示信息。

---

## 字段总览

| 工具 | 独特字段数量 | 最有价值字段 | 数据来源 |
|------|-------------|-------------|---------|
| **Codex** | 12 个 | `git_*`, `tokens_used`, `agent_*`, `approval_mode` | SQLite `threads` 表 |
| **Opencode** | 8 个 | `summary_*`, `slug`, `parent_id` | SQLite `session` 表 |
| **Claude** | 5 个 | `usage.*`, `gitBranch`, `slug` | JSONL 消息 |
| **Gemini** | 6 个 | `account`, `mcpServers`, `previewFeatures`, `sessionRetention` | JSON 配置 |

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

## Codex (最强大的企业级特性)

### 存储位置
- **SQLite (主要)**: `~/.codex/state_5.sqlite` - `threads` 表
- **JSONL (历史)**: `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`
- **索引文件**: `~/.codex/session_index.jsonl`
- **配置**: `~/.codex/config.toml`

### 独有字段

| 字段名 | 类型 | 示例值 | 说明 | 展示用途 |
|--------|------|--------|------|---------|
| **`git_branch`** | TEXT | `"main"` | Git 分支名 | 🌿 开发上下文 |
| **`git_sha`** | TEXT | `"10b42bad4..."` | Git 提交 SHA | 🔗 代码版本追溯 |
| **`git_origin_url`** | TEXT | `"https://..."` | 远程仓库 URL | 🔗 仓库快捷入口 |
| **`tokens_used`** | INTEGER | `9167995` | 会话总 token 消耗 | 📊 资源使用统计 |
| **`approval_mode`** | TEXT | `"on-request"` | 审批模式 | 🔐 安全策略标识 |
| **`sandbox_policy`** | TEXT | `workspace-write` | 沙箱策略 | 🔒 权限级别展示 |
| **`model_provider`** | TEXT | `"openai"` | 模型提供商 | 🤖 模型来源标识 |
| **`cli_version`** | TEXT | `"0.115.0-alpha.11"` | CLI 版本 | 📌 版本追踪 |
| **`agent_nickname`** | TEXT | `"助手"` | Agent 昵称 | 🤖 个性化标识 |
| **`agent_role`** | TEXT | `"coding"` | Agent 角色 | 🎯 专业领域标签 |
| **`source`** | TEXT | `"vscode"` / `"cli"` | 会话来源 | 💻 使用场景分析 |
| **`memory_mode`** | TEXT | `"enabled"` | 记忆模式状态 | 🧠 智能记忆标识 |
| **`has_user_event`** | INTEGER | `0/1` | 是否有用户事件 | 📈 交互活跃度 |
| **`archived`** | INTEGER | `0/1` | 归档状态 | 🗄️ 归档标识 |

### 示例数据

```sql
SELECT id, title, cwd, git_branch, git_sha, 
       tokens_used, approval_mode, model_provider,
       cli_version, agent_nickname, source
FROM threads 
ORDER BY updated_at DESC 
LIMIT 5;
```

结果:
```
019cf563-7815-7c73-a95c-51cfc0ec956c | opencode 工具新创建的会话没有被加载 | /Users/yuqiyu/AiHistorys/one-space/onespace-app | main | 10b42bad4... | 9167995 | on-request | openai | 0.115.0-alpha.11 | | vscode
019cf56e-8814-7c20-9903-22d9e25db082 | 在选择文件时不应该提示网络异常 | /Users/yuqiyu/AiHistorys/one-space/onespace-app | main | 10b42bad4... | 730850 | on-request | openai | 0.115.0-alpha.11 | | vscode
019cf557-79a7-76f0-9a62-c5ed362d21a7 | hello | /Users/yuqiyu/AiHistorys | (null) | (null) | 14579 | never | openai | 0.114.0 | | cli
```

### 环境统计数据

```sql
SELECT 
  COUNT(*) as total_sessions,
  COUNT(DISTINCT model_provider) as providers,
  COUNT(DISTINCT git_branch) as branches,
  AVG(tokens_used) as avg_tokens
FROM threads;
```

结果:
```
total_sessions: 177
providers: 4 (openai, bailian, anthropic, azure)
branches: 4 (main, feature/*, dev, null)
avg_tokens: 7,355,573
```

### 展示建议

```tsx
// Codex 会话卡片
<div className="codex-session-card">
  {/* Git 信息 */}
  {session.git_branch && (
    <Badge variant="git">
      <GitBranchIcon />
      {session.git_branch}
    </Badge>
  )}
  {session.git_sha && (
    <Tooltip content={`Commit: ${session.git_sha}`}>
      <Badge variant="outline" className="font-mono">
        {session.git_sha.slice(0, 7)}
      </Badge>
    </Tooltip>
  )}
  
  {/* Token 消耗 */}
  <div className="token-consumption">
    <TokenIcon />
    <span>{formatNumber(session.tokens_used)}</span>
    {session.tokens_used > 1000000 && (
      <span className="text-amber-600 text-xs">高消耗</span>
    )}
  </div>
  
  {/* 审批模式 */}
  <Tooltip content={`Approval: ${session.approval_mode}`}>
    <ShieldIcon className={
      session.approval_mode === 'never' ? 'text-red-600' :
      session.approval_mode === 'on-request' ? 'text-amber-600' :
      'text-green-600'
    } />
  </Tooltip>
  
  {/* 来源标识 */}
  {session.source && (
    <Badge variant={session.source === 'vscode' ? 'default' : 'secondary'}>
      {session.source === 'vscode' ? '🖥️ VSCode' : '💻 CLI'}
    </Badge>
  )}
  
  {/* Agent 信息 */}
  {session.agent_nickname && (
    <div className="agent-info">
      <BotIcon />
      <span>{session.agent_nickname}</span>
    </div>
  )}
</div>
```

### 配置信息 (可扩展字段)

```toml
# ~/.codex/config.toml
model = "gpt-5.4"
model_reasoning_effort = "xhigh"
disable_response_storage = true

[projects."/Users/yuqiyu/project"]
trust_level = "trusted"

[features]
multi_agent = true
```

这些配置可提取:
- `model`: 默认模型偏好
- `model_reasoning_effort`: 推理强度 (`low`/`medium`/`high`/`xhigh`)
- `projects.trust_level`: 项目级信任状态

---

## Gemini (Google AI Studio 集成)

### 存储位置
- `~/.gemini/tmp/*/chats/*.json` - 会话文件
- `~/.gemini/projects.json` - 项目映射
- `~/.gemini/settings.json` - 配置
- `~/.gemini/google_accounts.json` - 账户信息
- `~/.gemini/antigravity/brain/*/` - 知识库文件

### 独有字段

| 字段名 | 类型 | 示例值 | 说明 | 展示用途 |
|--------|------|--------|------|---------|
| **`account.active`** | STRING | `"user@gmail.com"` | 活跃 Google 账户 | 👤 账户切换 |
| **`account.old`** | ARRAY | `["old@gmail.com"]` | 历史账户列表 | 👤 多账户管理 |
| **`settings.previewFeatures`** | BOOLEAN | `true` | 预览功能开关 | 🧪 新功能标识 |
| **`settings.sessionRetention.enabled`** | BOOLEAN | `true` | 会话保留策略 | 🗄️ 数据管理 |
| **`settings.sessionRetention.maxAge`** | STRING | `"120d"` | 会话保留时长 | 🕐 自动清理策略 |
| **`settings.mcpServers`** | OBJECT | `{...}` | MCP 服务器配置 | 🔌 扩展能力展示 |
| **`settings.ide.enabled`** | BOOLEAN | `true` | IDE 集成状态 | 💻 编辑器联动 |
| **`projectHash`** | STRING | `"sha256_hash"` | 项目唯一标识 | 📁 项目分组 |
| **`messages[].model`** | STRING | `"gemini-2.5-pro"` | 使用的模型 | 🤖 模型版本 |

### MCP 服务器配置示例

```json
{
  "mcpServers": {
    "context7": {
      "command": "npx",
      "args": ["-y", "@upstash/context7-mcp"],
      "timeout": 60000
    },
    "filesystem-mcp": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "~/Downloads"],
      "trust": true
    },
    "exa": {
      "type": "http",
      "url": "https://mcp.exa.ai/mcp"
    },
    "jetbrains": {
      "url": "http://localhost:64342/sse"
    }
  }
}
```

### 展示建议

```tsx
// Gemini 会话卡片
<div className="gemini-session-card">
  {/* 账户标识 */}
  {session.account && (
    <Badge variant="account">
      <UserIcon />
      {session.account}
    </Badge>
  )}
  
  {/* 会话保留策略 */}
  {session.retentionEnabled && (
    <Tooltip content={`Session retention: ${session.retentionMaxAge}`}>
      <Badge variant="outline">
        <ClockIcon />
        {session.retentionMaxAge}
      </Badge>
    </Tooltip>
  )}
  
  {/* MCP 服务器标识 */}
  {session.mcpServers?.length > 0 && (
    <div className="mcp-servers">
      <PuzzleIcon />
      <span>{session.mcpServers.length} extensions</span>
      {session.mcpServers.some(s => s.trust) && (
        <span className="text-amber-600 text-xs">• trusted</span>
      )}
    </div>
  )}
  
  {/* IDE 集成状态 */}
  {session.ideEnabled && (
    <Badge variant="secondary">
      <CodeIcon />
      IDE Linked
    </Badge>
  )}
  
  {/* 预览功能标识 */}
  {session.previewFeatures && (
    <Badge variant="experimental">
      🧪 Preview
    </Badge>
  )}
</div>
```

### 知识库文件结构

Gemini 在 `~/.gemini/antigravity/brain/` 中存储知识库:

```
~/.gemini/antigravity/brain/
├── 0a1b2f7d-f9d8-473e-8fd7-66bd5178485c/
│   ├── task.md.metadata.json
│   ├── walkthrough.md.metadata.json
│   └── implementation_plan.md.metadata.json
└── 0ac93f7c-e990-4d91-836f-04e84ee50018/
    ├── code_review_report.md.metadata.json
    └── task.md.metadata.json
```

元数据文件包含:
- 文件类型 (`task`, `walkthrough`, `implementation_plan`)
- 创建时间戳
- 关联会话 ID
- 知识分类标签

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
