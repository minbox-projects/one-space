# OpenCode 服务商保存修复 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 OpenCode 服务商保存自定义图标，并在手动编辑 JSON 后保留该 JSON 内容。

**Architecture:** OpenCode 保存时将 `rawJson` 作为 provider 配置的权威来源；仅从它派生 API Key、Base URL 和首个模型索引。OneSpace 元数据（尤其图标）从详情草稿补回服务商载荷，不被序列化到 OpenCode JSON。

**Tech Stack:** React 19、TypeScript、Vitest、Testing Library、Tauri invoke mock。

## Global Constraints

- 仅修改 OpenCode 的前端保存流程，不修改后端投影或其他服务商。
- 无效 JSON 必须继续阻止保存。
- `icon` 必须保存在 OneSpace 服务商记录中，不能写入 OpenCode provider JSON。

---

### Task 1: 锁定 OpenCode 保存载荷行为

**Files:**
- Modify: `src/components/AiEnvironments/AiEnvironments.test.tsx`

**Interfaces:**
- Consumes: `AiEnvironments` 的 `service_providers_upsert` Tauri 调用。
- Produces: 两个验证保存载荷的回归测试。

- [ ] **Step 1: 写入会失败的 JSON 保留测试**

在测试状态中放入一个 `tool: 'opencode'` 的服务商；打开详情，直接修改 JSON 编辑器为：

```json
{
  "name": "Manual OpenCode",
  "options": { "apiKey": "manual-key", "baseURL": "https://manual.example/v1" },
  "models": { "manual-model": { "limit": { "context": 128000 } } },
  "customAdvancedOption": { "preserve": true }
}
```

点击保存后断言 `service_providers_upsert` 的 `provider` 同时具有 `customAdvancedOption`、原样的 `options` 和 `models`，并具有从 JSON 派生的 `api_key`、`base_url`、`model`。

- [ ] **Step 2: 运行测试确认 RED**

Run: `npm test -- src/components/AiEnvironments/AiEnvironments.test.tsx`

Expected: JSON 测试失败，因为当前 `syncOpenCodeProviderWithJson` 会把 `options.apiKey`、`options.baseURL` 和模型替换为表单草稿中的旧值。

- [ ] **Step 3: 写入会失败的图标保存测试**

在同一 OpenCode 详情测试中选择 `builtin:deepseek` 图标并保存；断言 `service_providers_upsert` 的 `provider.icon` 为 `builtin:deepseek`。

- [ ] **Step 4: 运行测试确认 RED**

Run: `npm test -- src/components/AiEnvironments/AiEnvironments.test.tsx`

Expected: 图标断言失败，因为当前保存组装会从过滤掉 `icon` 的 OpenCode JSON 重建载荷。

### Task 2: 以编辑 JSON 组装 OpenCode 保存载荷

**Files:**
- Modify: `src/components/AiEnvironments/index.tsx:1180-1230`
- Test: `src/components/AiEnvironments/AiEnvironments.test.tsx`

**Interfaces:**
- Consumes: `rawJson: string` 和详情草稿 `provider: Partial<AiProvider>`。
- Produces: `buildProviderForSave(provider): AiProvider` 的 OpenCode 分支。

- [ ] **Step 1: 最小化保存组装逻辑**

在 `buildProviderForSave` 的 `provider.tool === 'opencode'` 分支中，保留解析后的 JSON，并只叠加 OneSpace 元数据：

```ts
baseProvider = {
  ...parsed,
  id: provider.id,
  tool: 'opencode',
  icon: provider.icon,
  is_enabled: true,
  provider_key: provider.provider_key,
  opencode_default_model: provider.opencode_default_model,
  opencode_default_agent: provider.opencode_default_agent,
  opencode_sessions_dir: provider.opencode_sessions_dir,
  small_model: provider.small_model,
  timeout: provider.timeout,
  share_mode: provider.share_mode,
  history: provider.history || [],
};
```

再从 `parsed.options` 和 `parsed.models` 设置 `baseProvider.api_key`、`baseProvider.base_url` 和 `baseProvider.model`。不要在保存阶段调用 `syncOpenCodeProviderWithJson`，因为它的职责是表单编辑时的双向同步，不是保留手动 JSON 的保存逻辑。

- [ ] **Step 2: 运行定向测试确认 GREEN**

Run: `npm test -- src/components/AiEnvironments/AiEnvironments.test.tsx`

Expected: 两条 OpenCode 保存回归测试和既有预设测试均通过。

- [ ] **Step 3: 检查最小改动**

Run: `git diff --check && git diff -- src/components/AiEnvironments/index.tsx src/components/AiEnvironments/AiEnvironments.test.tsx`

Expected: 只包含两项保存行为的测试和 OpenCode 分支改动。

### Task 3: 完整验证与提交

**Files:**
- Modify: `src/components/AiEnvironments/index.tsx`
- Modify: `src/components/AiEnvironments/AiEnvironments.test.tsx`

- [ ] **Step 1: 运行完整前端验证**

Run: `npm test -- src/components/AiEnvironments/AiEnvironments.test.tsx && npm run build`

Expected: Vitest 通过，TypeScript 编译与 Vite 构建退出码为 0。

- [ ] **Step 2: 提交实现**

```bash
git add src/components/AiEnvironments/index.tsx src/components/AiEnvironments/AiEnvironments.test.tsx
git commit -m ':bug: 修复 OpenCode 服务商保存'
```
