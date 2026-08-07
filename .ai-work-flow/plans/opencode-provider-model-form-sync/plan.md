# OpenCode 服务商复制与模型快捷表单实施计划

## 计划元数据

- plan-id: `opencode-provider-model-form-sync`
- status: `ready-for-implementation`
- source_spec: `.ai-work-flow/plans/opencode-provider-model-form-sync/spec.md`
- source_spec_digest: `51a2c32cfa0bb618b896de4cdd44bd84c11bf85fd5535587c002700f5f260b28`
- task_mode: `split`

## 技术与代码上下文

- `src/components/AiEnvironments/index.tsx` 拥有服务商列表、详情草稿、`rawJson`、`jsonError`、保存与 OpenCode projection 调用。当前 `buildProviderForSave` 从 `rawJson` 解析 OpenCode 配置，并仍回写已废弃的 OpenCode 专属字段。
- `src/components/AiEnvironments/ServiceProviderList.tsx` 已使用 `Copy` 图标，但尚无复制已保存服务商的回调和入口。
- `src/components/AiEnvironments/ServiceProviderDetail.tsx` 同时渲染 Basic Info、工具专属字段及 `OpenCodeJsonPanel`；OpenCode 当前仍显示 Primary Model 与六个旧字段。
- `src/components/AiEnvironments/providerPresets.ts` 已定义实例字段和按敏感字段名过滤的原则，可扩展为递归、可复用的已保存服务商复制清洗器。
- 现有保存路径必须继续调用 `service_providers_upsert`，成功后维持历史刷新和仅对已激活 OpenCode 服务商调用 `projection_apply` 的行为。不得修改 `src-tauri` 命令、schema 或其输入输出契约。

## 实施方案

建立前端内的 OpenCode 配置适配层，作为 JSON 字符串、最后有效 JSON 快照和模型快捷表单之间的单一转换边界。该适配层从有效 provider JSON 提取表单拥有的 `models` 字段，表单回写时以最后有效快照为基底，仅覆盖模型 ID、`name`、`cost`、`limit`、`options` 和 `variants` 这些已拥有字段；顶层、模型项及上述结构内不能表达的合法字段均原样合并保留。

`AiEnvironments` 继续拥有 JSON 原文、解析错误、最后有效快照及详情草稿。JSON 编辑产生的合法值更新快照和表单；快捷表单产生的合法值由适配层序列化成格式化 JSON 后更新原文。解析或结构校验失败时，不触碰快照和表单数据，并将快捷表单及保存动作置为禁用；原文修复后立即重新建立同步状态。

复制流程只能从 `state.providers` 中命中的 canonical 记录生成未保存草稿。清洗器递归删除敏感字段和实例状态，随后生成全新 `id`、`provider_key`、`code` 及“副本”名称；复制入口不调用 `service_provider_read_opencode_config`，也不调用 upsert，直至用户在详情页保存。

## 顺序执行步骤

1. 在 `providerPresets.ts` 或相邻的专用前端模块中定义复制清洗与 OpenCode 模型转换的纯函数、类型和校验结果；先用单元测试固定敏感字段递归清洗、身份再生成、未知字段保留及序列化确定性。
   验证：`npx vitest run src/components/AiEnvironments/providerPresets.test.ts`。
2. 在 `index.tsx` 接入纯函数，增加复制草稿创建、最后有效 OpenCode 快照和 JSON/表单的事件协调；更新保存构造逻辑以移除旧 OpenCode UI 字段的回写，同时保留现有 upsert、历史和 projection 分支。
   验证：`npx vitest run src/components/AiEnvironments/AiEnvironments.test.tsx`。
3. 在 `ServiceProviderList.tsx` 暴露并渲染复制动作，在 `ServiceProviderDetail.tsx` 移除旧 OpenCode 字段，接入模型快捷表单和冻结状态；表单控件只通过父层回调写入统一 JSON 状态。
   验证：`npx vitest run src/components/AiEnvironments/ServiceProviderDetail.test.tsx`。
4. 扩展三个现有前端测试文件并更新 `docs/USAGE.md`；执行定向测试、完整前端测试与 lint，确认文档描述的是 canonical 复制、凭据清洗、表单限制与非法 JSON 恢复行为。
   验证：`npm test -- --run`（或项目等价的 `npm test`）以及 `npm run lint`。

## 任务边界与依赖

1. `provider-copy-sanitization`：实现服务商安全复制创建。实现仅基于 `state.providers` 中 canonical 已保存记录的复制草稿流程及纯函数测试：递归清除对象和数组中的敏感凭据，移除激活、收藏、历史等实例状态，生成全新的 `id`、`provider_key`、`code` 和可编辑的“副本”名称；复制时不得读取 OpenCode 运行时配置、调用 upsert 或提前持久化。依赖：无。
2. `opencode-model-adapter`：实现 OpenCode 模型无损转换与校验。建立 OpenCode JSON、最后有效快照与模型表单之间的纯函数适配层及单元测试：支持 `models`、`name`、`cost`、`limit`、`options`、`variants` 的解析、字段级校验、确定性序列化和深度合并；保证未知合法 provider、模型及嵌套字段不丢失，并覆盖模型 ID、数值边界、自定义 JSON 值和空可选字段规则。依赖：无，可与任务 1 并行。
3. `opencode-form-and-state-sync`：接入动态模型表单与双向同步。接入列表复制入口、OpenCode 动态模型快捷表单和 `AiEnvironments` 状态协调：移除 Primary Model 及六个旧专属字段的展示与保存回写，实现模型、cost、limit、options 和 variants 的动态编辑；合法 JSON 与表单实时双向同步，非法 JSON 或表单校验失败时保留最后有效快照、冻结表单并禁用保存，修复后立即恢复，同时保持既有 upsert、历史刷新和激活后 projection 行为。依赖：任务 1、任务 2。
4. `opencode-regression-tests-and-usage-docs`：完善回归测试与使用文档。扩展现有前端测试，完整验收复制未保存与新身份、递归凭据清洗、旧字段移除、动态模型与参数编辑、未知字段保留、双向同步、非法 JSON 冻结和恢复，以及既有 upsert/projection 契约；更新 `docs/USAGE.md` 说明 canonical 复制、敏感信息处理、模型快捷表单、高级 JSON 同步和限制，并执行定向测试、完整前端测试及 lint。依赖：任务 1、任务 2、任务 3。

## 具体改动

- `src/components/AiEnvironments/providerPresets.ts`：保留预设行为；新增或导出服务商复制清洗函数。递归遍历对象与数组，删除键名小写后包含 `key`、`token`、`secret`、`password`、`auth` 的值，显式确保 `api_key` 和 `options.apiKey` 不可保留；排除 `id`、`provider_key`、`code`、`is_enabled`、`env_managed`、`favorite_at`、`history` 及其他实例状态。
- `src/components/AiEnvironments/opencodeModelConfig.ts`（新增，名称可在实施时按本目录惯例调整）：定义可编辑模型、动态 option 行、variant 和解析结果类型；实现解析、表单校验、快照合并和稳定 JSON 序列化。模型 ID 必填且唯一；启用 cost 时 `input`/`output` 为非负数，空 cache 字段不输出；启用 limit 时 `context`/`output` 为正数；自定义 option 值解析为合法 JSON。
- `src/components/AiEnvironments/OpenCodeModelForm.tsx`（新增，名称可在实施时按本目录惯例调整）：渲染模型增删改、名称、ID、cost、limit、options 和 variants。常见 option 键只作为非穷尽下拉建议，按字段类型提供 string/number/boolean 控件，并保留自定义键与 JSON 值输入。冻结时所有写入控件禁用并显示已有 JSON 错误。
- `src/components/AiEnvironments/ServiceProviderList.tsx`：在每个已保存服务商的操作区增加具备可访问名称和 tooltip 的复制图标按钮，并将 `onDuplicate` 回调上传；复制不复用启动命令的 `Copy` 语义或回调。
- `src/components/AiEnvironments/index.tsx`：从 `state.providers` 查找复制来源，调用清洗器创建仅存在于详情状态的草稿；为 OpenCode 保存/打开/回滚初始化最后有效快照；处理 JSON 有效编辑、表单有效编辑、冻结及恢复；在 OpenCode 保存 payload 中不再写入 `model`、`opencode_default_model`、`opencode_default_agent`、`opencode_sessions_dir`、`small_model`、`timeout`、`share_mode` 等旧 UI 字段。保留 JSON 内未知合法字段和现有 `service_providers_upsert`、刷新、历史、激活后 `projection_apply` 调用路径。
- `src/components/AiEnvironments/ServiceProviderDetail.tsx`：OpenCode Basic Info 不渲染 Primary Model；删除 OpenCode 工具专属旧字段区；在 JSON 面板之前或相邻位置挂载模型快捷表单，接收父层冻结、校验错误和变更回调。保存按钮条件扩展为“正在保存、JSON 无效或模型表单无效”时禁用。
- `src/components/AiEnvironments/providerPresets.test.ts`：覆盖嵌套对象/数组敏感字段、`options.apiKey`、身份和状态清除及非敏感未知字段保留。
- `src/components/AiEnvironments/ServiceProviderDetail.test.tsx`：覆盖 OpenCode 旧字段不渲染、模型动态表单、字段类型控件、表单冻结和保存禁用。
- `src/components/AiEnvironments/AiEnvironments.test.tsx`：覆盖复制未保存、保存后新身份、未读取运行时配置、JSON/表单双向同步、未知字段保留、非法 JSON 冻结与修复恢复，以及保存仍使用既有 upsert/projection 约束。
- `docs/USAGE.md`：在 AI Environments 的 OpenCode 使用段补充复制创建的凭据清洗、模型快捷表单和高级 JSON 编辑的同步/冻结限制。

## 接口与数据流

1. 复制：`ServiceProviderList` 的复制事件 -> `AiEnvironments` 在 `state.providers` 查找来源 -> 清洗器返回新草稿和新身份 -> 设置 `detailProvider`、`rawJson` 与未保存集合 -> `ServiceProviderDetail` 编辑 -> 既有 `saveDetailProvider` -> `service_providers_upsert`。此链路不读取本机 OpenCode 配置，也不提前持久化。
2. JSON 到表单：`OpenCodeJsonPanel.onChange` -> `AiEnvironments` 保存原文并调用适配器解析/结构校验 -> 成功时更新最后有效快照、表单值及清除错误；失败时仅保存原文和错误，保留表单快照、冻结表单和保存。
3. 表单到 JSON：`OpenCodeModelForm.onChange` -> 适配器先校验表单 -> 以最后有效快照为底合并表单拥有字段与未知字段 -> 格式化 JSON -> `AiEnvironments` 同步 `rawJson`、快照、详情派生字段和错误状态。表单无效时不生成 JSON，并禁用保存。
4. 保存：`buildProviderForSave` 仅从已验证的有效 OpenCode JSON 构造既有通用 payload，保留其中 `options`、`models` 和未知 JSON；不增加后端字段，不改变 `normalizeProviderForSave`、`service_providers_upsert` 或 `projection_apply` 的参数结构。

## 失败处理

- 复制来源在状态刷新后不存在、工具不匹配或不可用时，显示可理解错误并保持列表，不创建草稿。
- 所有敏感键名匹配在任意对象嵌套层级均删除；数组元素递归处理，避免浅拷贝导致凭据泄露。
- JSON 仅在可解析且根值为预期对象结构时更新快照。语法错误、非对象根值或不符合模型结构时，不允许表单序列化覆盖原文本。
- 模型 ID 重复/为空、cost/limit 边界违规和动态自定义 JSON 值错误均定位到字段，阻止 JSON 生成与保存。
- 适配器不得用空对象覆盖合法未知 `provider`、`models[modelId]` 或嵌套结构；每个合并测试应比较深层未知值。
- 任何保存失败继续走现有错误消息和 toast 处理；成功后的 projection 失败仅沿用当前记录/日志行为，不回滚已成功 upsert。

## 测试与验证

- 纯函数：测试复制清洗、身份刷新、未继承激活/收藏/历史、模型往返转换、未知字段深层保留、cost/limit 边界、options 类型和 variants 覆盖。
- 组件：测试 OpenCode 不再显示 Primary Model 与六个旧字段；模型、option 行和 variant 可增删改；冻结时编辑控件和 Save 禁用，修复 JSON 后立即恢复。
- 状态集成：测试复制只使用保存的 canonical provider 且未调用 `service_provider_read_opencode_config`；确认未保存时没有 upsert；测试 JSON 编辑驱动表单、表单编辑驱动 JSON、保存 payload 保留未知字段并走原命令。
- 执行命令：`npx vitest run src/components/AiEnvironments/providerPresets.test.ts src/components/AiEnvironments/ServiceProviderDetail.test.tsx src/components/AiEnvironments/AiEnvironments.test.tsx`；`npm test`；`npm run lint`。

## 验收标准

- 复制入口仅以应用内已保存的 canonical 服务商生成未保存草稿；草稿拥有新 `id`、`provider_key`、`code` 和可编辑的“副本”名称，不带任何凭据、激活、收藏或历史状态。
- OpenCode 页面不显示且保存构造不再维护 Primary Model、Default Model、Default Agent、Sessions Directory、Small Model、Request Timeout、Share Mode。
- 模型快捷表单可动态编辑模型、cost、limit、options 和 variants，遵循模型 ID 与数值校验规则，并不推断 cost 货币。
- 合法 JSON 与表单实时双向同步，所有非拥有的合法 provider/model 字段在编辑后仍存在。
- 非法 JSON 或结构无效时，保留最后有效表单，冻结表单和保存；恢复有效 JSON 后立即解冻并重新同步。
- 后端持久化、历史记录和 projection 的契约及调用顺序不变。

## 兼容、迁移与发布

- 不进行数据迁移；既有记录仍由通用 JSON 字段读取和保存。旧 OpenCode 专属字段停止由 UI 写入，但未知合法 JSON 继续保留。
- 本次仅修改前端与文档；不修改 Rust 命令、数据库 schema、`service_providers_upsert` 或 `projection_apply`。
- 发布前以包含旧字段、嵌套未知字段、多个 variants、空可选 cache 值及非法 JSON 的真实感测试夹具回归；升级后首次编辑并保存应不造成无关配置丢失。
