# OpenCode 服务商保存修复设计

## 目标

修复 AI 终端服务商中 OpenCode 服务商的两个保存问题：

1. 在详情页更改服务商图标后，重新加载仍显示旧图标。
2. 在详情页手动编辑 Configuration JSON 后点击保存，编辑内容会被表单中的旧值覆盖。

## 方案选择

考虑过三种策略：

1. 每次保存都由表单重建 OpenCode JSON。实现简单，但会继续覆盖 JSON 编辑器中未建模的 OpenCode 配置；不采用。
2. 为每一个表单字段增加脏状态，并只覆盖脏字段。能保留 JSON，但为现有表单引入状态同步复杂度；不采用。
3. 保存时以有效的 JSON 编辑内容为准，只提取 OneSpace 所需的服务商元数据；图标独立保存。该方式最小化改动，也符合编辑器“直接编辑高级 OpenCode 配置”的提示；采用。

## 数据流

`rawJson` 表示 OpenCode provider JSON（将写入 `opencode.json` 的 provider 条目）。保存时：

1. 解析 `rawJson`；无效 JSON 仍阻止保存。
2. 保留解析后的全部 JSON 键和值，不再用详情表单的旧值重建或覆盖它。
3. 从 JSON 的 `options.apiKey`、`options.baseURL` 与 `models` 推导 OneSpace 服务商记录所需的 API Key、Base URL 和模型索引字段。
4. 保留 OneSpace 元数据：稳定的 `id`、`tool`、`provider_key`、启用状态、历史记录和当前表单中的 `icon`。
5. `icon` 只随 OneSpace 服务商记录持久化，不进入 OpenCode JSON 或生成的 `opencode.json` provider 配置。

表单字段仍可在编辑过程中同步其对应的 JSON；本次修复只保证“只编辑 JSON 然后保存”不会被保存流程反向覆盖。

## 影响范围

- 前端仅修改 `src/components/AiEnvironments/index.tsx` 的 OpenCode 保存组装逻辑。
- 不修改 OpenCode 配置投影、后端存储结构或其他 CLI 服务商的保存行为。

## 验证

新增前端回归测试，覆盖：

1. 更换 OpenCode 服务商图标并保存时，提交给 `service_providers_upsert` 的服务商载荷保留该图标。
2. 手动修改 OpenCode JSON 中的高级字段、`options.apiKey`、`options.baseURL` 或模型后保存时，提交载荷保留原始 JSON 内容，并从 JSON 更新索引字段。

运行对应 Vitest 用例以及前端类型/构建检查。
