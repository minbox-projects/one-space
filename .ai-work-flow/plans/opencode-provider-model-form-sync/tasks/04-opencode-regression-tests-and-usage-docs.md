# 04 - 完善回归测试与使用文档

- task_id: `opencode-regression-tests-and-usage-docs`
- order: `04`
- blocked_by: `provider-copy-sanitization, opencode-model-adapter, opencode-form-and-state-sync`
- source_plan: `../plan.md`
- source_plan_digest: `2614cd332f50cef408924acbcb05910d7b1cd1ffdefba3c2dca7361357f6b584`
- write_scope_mode: `exhaustive`
- write_scope:
  - `src/components/AiEnvironments/providerPresets.test.ts`
  - `src/components/AiEnvironments/ServiceProviderDetail.test.tsx`
  - `src/components/AiEnvironments/AiEnvironments.test.tsx`
  - `docs/USAGE.md`

## 预期结果

扩展现有前端测试，完整验收复制未保存与新身份、递归凭据清洗、旧字段移除、动态模型与参数编辑、未知字段保留、双向同步、非法 JSON 冻结和恢复，以及既有 upsert/projection 契约；更新 docs/USAGE.md 说明 canonical 复制、敏感信息处理、模型快捷表单、高级 JSON 同步和限制，并执行定向测试、完整前端测试及 lint。依赖：任务 1、任务 2、任务 3。

## 实施清单

- [x] 扩展 `providerPresets.test.ts` 的回归矩阵，覆盖对象与数组中的深层敏感键、混合大小写、`api_key`、`options.apiKey`、身份与实例状态清除、来源不可变及非敏感未知字段保留。
- [x] 扩展 `ServiceProviderDetail.test.tsx`，覆盖 Primary Model 和六个旧专属字段不再渲染，模型增删改、模型 ID、cost/limit 开关与边界、不同 option 类型、自定义 JSON 值、variants 动态编辑、字段错误、冻结状态及 Save 禁用。
- [x] 扩展 `AiEnvironments.test.tsx`，确认复制来源来自 canonical `state.providers`，草稿在显式保存前不 upsert，身份全新，且复制时不调用 `service_provider_read_opencode_config`。
- [x] 在状态集成测试中覆盖 JSON 到表单和表单到 JSON 的即时同步、provider/model 多层未知字段保留、非法 JSON 或结构错误冻结、最后有效快照保持，以及修复后的立即恢复。
- [x] 覆盖保存 payload 不再由旧 UI 回写移除字段，仍调用既有 `service_providers_upsert`，成功后刷新状态与历史，并且仅在原有激活条件满足时调用 `projection_apply`；保存或 projection 失败沿用既有处理。
- [x] 使用包含旧字段、嵌套未知字段、多个 variants、空 cache 可选值、自定义 options 和非法 JSON 的真实感 fixture 完成回归，不新增与产品行为无关的 snapshot。
- [x] 更新 `docs/USAGE.md` 的 AI Environments/OpenCode 使用说明，说明 canonical 复制入口、递归凭据清洗与不继承状态、显式保存时机、模型快捷表单字段、高级 JSON 双向同步、未知字段保留和无效状态冻结限制。
- [x] 文档明确常见 option 仅为非穷尽建议、cost 不推断币种，且复制不会读取或合并本机 OpenCode 运行时配置。

## 验收标准

- [x] 自动化测试可判定复制创建的新身份、未保存语义、递归清洗及禁止读取运行时配置，而非仅断言页面文案。
- [x] 自动化测试完整覆盖旧字段移除、动态模型与参数编辑、校验边界、未知字段深层保留、双向同步、冻结与恢复。
- [x] upsert、历史刷新和激活后 projection 的既有契约与调用条件有明确回归断言。
- [x] `docs/USAGE.md` 与实际交互一致，清楚描述敏感信息处理、模型表单能力、高级 JSON 行为和限制，不暗示 options 穷尽或 cost 币种。
- [x] 定向测试、完整前端测试、TypeScript 构建检查和 lint 全部通过，且未修改产品源码或后端文件以规避测试。

## 验证步骤

- [x] 运行 `npx vitest run src/components/AiEnvironments/providerPresets.test.ts src/components/AiEnvironments/opencodeModelConfig.test.ts src/components/AiEnvironments/ServiceProviderDetail.test.tsx src/components/AiEnvironments/AiEnvironments.test.tsx`，预期本功能全部定向测试通过。
- [x] 运行 `npm test`，预期完整前端测试套件通过。
- [x] 运行 `npx tsc -b --pretty false`，预期 TypeScript 项目检查通过。
- [x] 运行 `npm run lint`，预期 lint 通过且无新增警告或错误。

## 范围外事项

- 产品功能实现、后端命令、Rust、schema、数据迁移和运行时配置导入。
- 为 OpenCode options 建立穷尽字段目录，或增加 cost 货币处理。

## 禁止事项

- 不得修改产品源码、后端实现或接口契约来迁就测试断言。
- 不得以大范围 snapshot 替代关键交互、payload、命令调用和未知字段保留的精确断言。
- 不得在文档中声称复制保留凭据、自动持久化、读取本机配置，或声称常见 options 列表完整穷尽。
