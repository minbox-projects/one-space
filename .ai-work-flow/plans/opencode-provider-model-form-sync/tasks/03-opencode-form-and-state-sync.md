# 03 - 接入动态模型表单与双向同步

- task_id: `opencode-form-and-state-sync`
- order: `03`
- blocked_by: `provider-copy-sanitization, opencode-model-adapter`
- source_plan: `../plan.md`
- source_plan_digest: `2614cd332f50cef408924acbcb05910d7b1cd1ffdefba3c2dca7361357f6b584`
- write_scope_mode: `exhaustive`
- write_scope:
  - `src/components/AiEnvironments/index.tsx`
  - `src/components/AiEnvironments/ServiceProviderList.tsx`
  - `src/components/AiEnvironments/ServiceProviderDetail.tsx`
  - `src/components/AiEnvironments/OpenCodeModelForm.tsx`

## 预期结果

接入列表复制入口、OpenCode 动态模型快捷表单和 AiEnvironments 状态协调：移除 Primary Model 及六个旧专属字段的展示与保存回写，实现模型、cost、limit、options 和 variants 的动态编辑；合法 JSON 与表单实时双向同步，非法 JSON 或表单校验失败时保留最后有效快照、冻结表单并禁用保存，修复后立即恢复，同时保持既有 upsert、历史刷新和激活后 projection 行为。依赖：任务 1、任务 2。

## 实施清单

- [x] 在 `ServiceProviderList.tsx` 为每个已保存服务商增加独立的复制创建图标按钮、可访问名称和 tooltip，并通过明确的 `onDuplicate` 回调上抛 canonical 服务商标识。
- [x] 在 `index.tsx` 仅从当前 `state.providers` 查找复制来源；来源缺失、工具不匹配或不可用时显示可理解错误并保持列表，否则调用任务 1 清洗器创建仅存在于详情状态的未保存草稿。
- [x] 在 OpenCode 详情打开、复制、保存成功刷新和历史回滚路径中初始化并维护 JSON 原文、解析错误、模型表单值与最后有效快照，统一使用任务 2 适配器作为转换边界。
- [x] 实现 JSON 到表单同步：每次保留用户原文；仅在 JSON 语法与结构有效时更新快照和表单并清除错误，无效时保留最后有效表单、冻结快捷表单并禁用保存。
- [x] 实现表单到 JSON 同步：先执行字段级校验；有效时基于最后有效快照深度合并并更新格式化 JSON、快照和详情状态，无效时保留 JSON 和快照并禁用保存；修复后立即恢复同步。
- [x] 新建 `OpenCodeModelForm.tsx`，提供模型增删改、模型名称与 ID、可选 cost/limit、动态 options 和 variants 编辑；常见 option 仅作为非穷尽建议并匹配 string/number/boolean 控件，自定义键使用 JSON 值输入。
- [x] 冻结状态下禁用模型表单的所有写入控件并展示现有 JSON 错误；表单字段错误定位到对应控件，Save 在保存中、JSON 无效或模型表单无效时禁用。
- [x] 在 `ServiceProviderDetail.tsx` 移除 OpenCode Basic Info 中的 Primary Model 与旧工具专属字段区，并在 JSON 面板相邻位置挂载模型快捷表单。
- [x] 调整 `index.tsx` 的 OpenCode 保存构造逻辑，停止从旧 UI 回写 `model`、`opencode_default_model`、`opencode_default_agent`、`opencode_sessions_dir`、`small_model`、`timeout`、`share_mode`，同时保留有效 JSON 内的未知配置。
- [x] 保持现有 `service_providers_upsert` 调用、成功后状态与历史刷新，以及仅对已激活 OpenCode 服务商执行 `projection_apply` 的契约和顺序。

## 验收标准

- [x] 列表复制只使用 `state.providers` 中命中的 canonical 记录；进入详情后草稿未保存且拥有新身份，复制阶段未读取运行时 OpenCode 配置、未 upsert、未 projection。
- [x] OpenCode 页面不再渲染或通过旧表单维护 Primary Model、Default Model、Default Agent、Sessions Directory、Small Model、Request Timeout 和 Share Mode。
- [x] 用户可动态编辑模型、cost、limit、options 和 variants；有效表单改动立即更新 JSON，有效 JSON 改动立即更新表单。
- [x] JSON 或模型表单无效时，最后有效快照和表单不被覆盖，表单与保存均按规则禁用；修复后无需重新打开详情即可恢复。
- [x] 保存 payload 保留未知合法字段，并继续遵循既有 upsert、历史刷新和激活后 projection 行为。

## 验证步骤

- [x] 运行 `npx vitest run src/components/AiEnvironments/providerPresets.test.ts src/components/AiEnvironments/opencodeModelConfig.test.ts`，预期两个前置纯函数契约保持通过。
- [x] 运行 `npx vitest run src/components/AiEnvironments/ServiceProviderDetail.test.tsx src/components/AiEnvironments/AiEnvironments.test.tsx`，预期现有详情与状态集成测试无回归；新增完整场景由任务 4 补齐。
- [x] 运行 `npx tsc -b --pretty false`，预期新增组件、回调和状态类型通过检查。
- [x] 运行 `npm run lint`，预期无新增 lint 错误。

## 范围外事项

- 后端命令、Rust 实现、数据库 schema、数据迁移及 upsert/projection 输入输出契约变更。
- 完整 options 字段目录、cost 币种选择或本机 OpenCode 配置导入。
- 回归测试矩阵和使用文档完善，由任务 4 负责。

## 禁止事项

- 不得绕过适配器分别维护互相漂移的 JSON 与表单配置副本。
- 不得在无效 JSON 或无效表单状态下覆盖最后有效快照或允许保存。
- 不得把复制按钮复用为启动命令复制语义，不得读取 `service_provider_read_opencode_config` 作为复制来源。
- 不得改变 `service_providers_upsert`、历史刷新或 `projection_apply` 的既有参数契约和调用条件。
