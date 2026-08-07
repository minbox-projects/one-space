# OpenCode 服务商复制与模型快捷表单同步

## 规格元数据

- plan-id: `opencode-provider-model-form-sync`
- status: `approved`
- source_context_id: `opencode-provider-model-form-sync-context`
- source_context_digest: `7af3d9e16c4b28f085eab1239e8152ae7f67d564f253b588263cb72e89ea14d0`

## 问题陈述

当前 AI 终端服务商缺少基于已保存配置的安全复制入口。OpenCode 配置同时存在传统表单和 JSON 编辑器，模型相关字段分散且同步不完整，编辑时可能无法安全保留未被表单维护的合法配置。

## 目标与成功标准

提供从应用内 canonical 服务商配置复制创建新服务商的交互，并重构 OpenCode 的模型配置为可动态编辑的快捷表单。快捷表单和有效 OpenCode JSON 必须实时双向同步、保留未知合法字段，并在 JSON 无效时阻止覆盖和保存。保存仍走现有持久化、历史记录和 OpenCode projection 流程，后端契约不变。

## 用户与用户故事

- 桌面端服务商管理员可从列表选择已保存的服务商复制创建，以非敏感配置作为起点后修改并确认保存。
- 用户可在 OpenCode 详情中增删改模型、变体和动态参数，而无需手写常用模型结构。
- 高级用户可直接编辑 JSON；当 JSON 合法时，快捷表单立即反映其内容且不丢失未知字段。
- 前端 `ServiceProviderList`、`AiEnvironments` 状态层、`ServiceProviderDetail` 和 `OpenCodeJsonPanel` 消费本规格；后端 `service_providers_upsert` 与 `projection_apply` 保持既有契约。

## 功能需求

### 服务商复制创建

1. 服务商列表提供“复制创建”入口，用户可选择一个现有服务商作为复制来源。
2. 复制只能读取应用内已保存的 canonical 服务商记录，不读取本机 OpenCode 运行时配置。
3. 草稿必须生成新的 `id`、`provider_key` 和 `code`；名称默认在来源名称后追加“副本”，且可编辑。
4. 复制时必须移除 API Key、`options.apiKey`，以及键名语义属于 `key`、`token`、`secret`、`password` 或 `auth` 的敏感凭据；可复用 `providerPresets.ts` 既有敏感字段清洗原则。
5. 不继承激活状态、收藏状态或变更历史。仅在用户确认保存后创建新记录。

### OpenCode 字段整理与模型快捷表单

1. OpenCode 的 Basic Info 不再展示或维护 Primary Model。
2. OpenCode 的 Tool Specific Config 不再展示或维护 Default Model、Default Agent、Sessions Directory、Small Model、Request Timeout 和 Share Mode。
3. 新增模型列表快捷表单，支持动态新增、删除及编辑模型。
4. 每个模型包含自定义名称、模型 ID、可选 `cost`、可选 `limit`、`variants` 和 `options`。
5. 模型 ID 是 OpenCode `models` 对象的 key；自定义名称写入该模型对象的 `name`。
6. `options` 使用动态行。字段下拉提供经 OpenCode 官方验证的常见字段和匹配的类型控件，并允许自定义键；内置列表不得声称穷尽所有 provider/model 参数。
7. `variants` 可动态新增。每个变体包含独立的 options 覆盖对象，并复用 options 动态行交互。

### 实时双向同步

1. 快捷表单的有效改动必须实时更新 OpenCode JSON。
2. JSON 的有效改动必须实时解析并更新快捷表单。
3. 同步转换必须保留模型条目及 provider JSON 中未由快捷表单维护的合法字段。
4. JSON 语法错误或结构无效时，显示错误；快捷表单保留最后一次有效快照、禁用编辑，并同时禁用保存。
5. JSON 恢复为有效结构后，必须立即重新解析、同步并恢复编辑和保存能力。

## 非功能需求

- 不新增后端 schema 专用字段，使用既有通用 `tool_config`/`extra` JSON 承载扩展配置。
- 双向转换应确定性地合并受表单维护的字段与未维护字段，避免一次编辑造成无关数据丢失。
- 所有新增交互应符合现有桌面端服务商管理界面和既有持久化调用方式。

## 范围

包含前端复制交互、OpenCode 字段清理、模型快捷表单、双向转换与校验、相关前端测试以及 `docs/USAGE.md` 的必要说明。

## 接口与数据

### 数据映射

| 快捷表单字段 | OpenCode JSON 映射 | 规则 |
| --- | --- | --- |
| 模型 ID | `models[modelId]` | 必填，列表唯一 |
| 自定义名称 | `models[modelId].name` | 写入模型名称 |
| cost input/output | `models[modelId].cost.input/output` | 启用 cost 后必填，非负数 |
| cost cache_read/cache_write | `models[modelId].cost.cache_read/cache_write` | 可选；空值不写入 JSON |
| limit context/output | `models[modelId].limit.context/output` | 启用 limit 后必填，正数 |
| options 动态行 | `models[modelId].options[key]` | 常见字段或自定义键；值为匹配类型或合法 JSON 值 |
| variant options | `models[modelId].variants[variantName]` | 独立 options 覆盖对象 |

`cost` 的数值界面只说明“每 100 万 token 的计费数值”。OpenCode 官方 schema 未定义货币代码，因此不得绑定、推断或写入币种。

### 数据保留策略

解析有效 JSON 时建立最后一次有效快照。表单序列化仅更新其拥有的模型结构字段，并将原模型对象、模型项和 provider JSON 的未知合法字段合并回输出。无法被快捷表单表现但结构合法的数据必须原样保留。无效 JSON 不参与解析或序列化，因而不能被表单改动覆盖。

## 失败模式

- 复制来源不存在或不可用时，不创建草稿并显示可理解的错误。
- 保存前出现模型 ID 缺失、重复、cost 或 limit 不符合规则时，显示字段错误并阻止保存。
- JSON 语法错误或不是预期 OpenCode 配置结构时，显示 JSON/结构错误，冻结快捷表单和保存。
- 动态 options 的自定义值不是合法 JSON 值时，显示该行错误且不生成无效配置。
- 复制清洗遇到嵌套敏感字段时，按敏感字段规则移除，不保留其值。

## 验收标准

1. 列表可选择已保存服务商并进入复制创建流程；新草稿使用新身份、可改的“副本”名称，且未保存前不创建记录。
2. 复制结果不包含 API Key、`options.apiKey` 或其他敏感凭据，也不包含来源的激活、收藏和历史状态。
3. OpenCode 页面不再显示或通过旧表单写入 Primary Model 及六个移除的专属字段。
4. 用户可新增、修改、删除模型；模型 ID 唯一且映射为 `models` 的 key，名称映射为 `name`。
5. cost 和 limit 可完全省略；启用后的必填值和数值边界符合本规格，且可选 cache 字段空值不输出。
6. options 和 variants 支持动态行、常见字段类型控件和自定义键；变体值独立覆盖模型 options。
7. 从任一方向编辑有效配置后，另一视图即时更新，且未知合法 provider/model 字段仍存在。
8. 输入非法 JSON 后出现错误、表单和保存被禁用并保留最后有效快照；修复 JSON 后立即恢复同步和保存。
9. 保存继续使用现有 upsert、历史记录及 OpenCode projection 行为，后端接口契约无变化。

## 兼容性与迁移

既有服务商记录继续使用现有通用 JSON 字段读取和保存，不要求数据迁移。旧表单移除的 OpenCode 专属字段不再由 UI 维护；其余未知合法 JSON 字段按保留策略兼容。后端 `service_providers_upsert` 和 `projection_apply` 不新增字段、不改变输入输出契约。

## 范围外事项

- 修改后端持久化 schema、`service_providers_upsert` 或 `projection_apply` 契约。
- 读取、导入或合并本机 OpenCode 运行时配置。
- 定义完整穷尽的 OpenCode provider/model options 字段目录。
- 为 cost 增加货币选择、汇率处理或货币代码持久化。
- 迁移、恢复或复制服务商历史记录、激活状态或收藏状态。

## 假设

- canonical 服务商记录包含可复制的完整非敏感配置。
- 动态参数值至少支持 OpenCode 常见字段所需的 string、number、boolean，以及自定义合法 JSON 值。
- 现有 `tool_config`/`extra` 可保留本规格涉及的扩展结构。

## 开放问题

N/A

## 测试与验证

扩展 `AiEnvironments.test.tsx`、`ServiceProviderDetail.test.tsx` 和 `providerPresets.test.ts`，覆盖以下场景：复制创建及敏感字段清洗；移除字段不再渲染或写入；模型增删改和模型 ID 唯一校验；cost/limit 边界和可省略行为；options 与 variants 的动态编辑及自定义键；未知字段保留；表单到 JSON 和 JSON 到表单的实时同步；非法 JSON 冻结、禁用保存及修复后的恢复。更新 `docs/USAGE.md`，说明复制创建的敏感信息处理和 OpenCode 模型快捷表单的使用限制。
