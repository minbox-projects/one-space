# 01 - 实现服务商安全复制创建

- task_id: `provider-copy-sanitization`
- order: `01`
- blocked_by: `none`
- source_plan: `../plan.md`
- source_plan_digest: `2614cd332f50cef408924acbcb05910d7b1cd1ffdefba3c2dca7361357f6b584`
- write_scope_mode: `exhaustive`
- write_scope:
  - `src/components/AiEnvironments/providerPresets.ts`
  - `src/components/AiEnvironments/providerPresets.test.ts`

## 预期结果

实现仅基于 state.providers 中 canonical 已保存记录的复制草稿流程及纯函数测试：递归清除对象和数组中的敏感凭据，移除激活、收藏、历史等实例状态，生成全新的 id、provider_key、code 和可编辑的“副本”名称；复制时不得读取 OpenCode 运行时配置、调用 upsert 或提前持久化。依赖：无。

## 实施清单

- [x] 在 `providerPresets.ts` 中新增可复用的纯函数，以调用方提供的 canonical 已保存服务商为唯一输入生成深拷贝草稿，不读取运行时配置或执行任何持久化副作用。
- [x] 对对象和数组进行递归清洗；键名按不区分大小写的语义删除包含 `key`、`token`、`secret`、`password`、`auth` 的凭据，明确覆盖 `api_key` 与 `options.apiKey`，同时保留非敏感未知字段。
- [x] 删除来源实例身份和状态，包括旧 `id`、`provider_key`、`code`、`is_enabled`、`env_managed`、`favorite_at`、`history` 及同类仅属于已保存实例的状态。
- [x] 为草稿生成彼此独立且不复用来源值的新 `id`、`provider_key`、`code`，将默认名称设置为来源名称追加“副本”，并保持名称后续可编辑。
- [x] 用纯函数测试覆盖嵌套对象、数组、混合大小写敏感键、`options.apiKey`、身份刷新、实例状态清除、来源对象不被修改及非敏感未知配置保留。

## 验收标准

- [x] 对任意嵌套层级和数组元素，复制结果均不包含命中敏感键规则的值，且来源对象保持原样。
- [x] 复制草稿的 `id`、`provider_key`、`code` 均为新值，名称包含可编辑的“副本”后缀，且不继承激活、收藏、历史或环境托管状态。
- [x] 合法的非敏感通用字段、工具配置及未知嵌套字段在复制后保持结构和值不变。
- [x] 纯函数不依赖 `service_provider_read_opencode_config`、`service_providers_upsert`、`projection_apply` 或其他 I/O；列表和状态层接入留给任务 3。

## 验证步骤

- [x] 运行 `npx vitest run src/components/AiEnvironments/providerPresets.test.ts`，预期复制清洗与现有预设测试全部通过。
- [x] 运行 `npm run lint -- --quiet src/components/AiEnvironments/providerPresets.ts src/components/AiEnvironments/providerPresets.test.ts`；若项目 ESLint 脚本不接受文件参数，则运行 `npm run lint`，预期无新增 lint 错误。

## 范围外事项

- 列表复制按钮、`AiEnvironments` 状态接入和保存流程协调。
- OpenCode 模型表单、JSON 适配、后端命令或数据库 schema 修改。

## 禁止事项

- 不得从本机 OpenCode 配置或其他运行时投影反向读取复制来源。
- 不得在复制函数或复制动作中调用 upsert、projection、历史刷新或提前创建持久化记录。
- 不得只做浅层清洗、通过敏感值替换为空字符串规避删除，或复用来源身份字段。
