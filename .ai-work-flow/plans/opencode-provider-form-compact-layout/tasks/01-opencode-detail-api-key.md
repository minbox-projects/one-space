# OpenCode 详情密钥数据与复制交互

## 预期结果

在共用的 OpenCode 新增与编辑详情中完成运行时 provider 配置读取、目标 provider 精确匹配和完整 API Key 安全回填，并处理读取失败、字段缺失、用户输入保护及请求乱序；同时提供明文输入、复制成功与失败状态和准确的无障碍标签。配套定向测试覆盖新增与编辑路径、完整值来源、剪贴板行为和列表脱敏安全边界，不改变列表接口、持久化结构或既有保存语义。

## 实施清单

- [x] 沿用 `serviceProviderReadOpenCodeConfig` 读取编辑目标的运行时 OpenCode 配置，按当前 `provider_key` 精确绑定响应，并将 `options.apiKey` 作为详情表单和原始 JSON 的完整密钥来源；列表中的 `********` 不得作为失败兜底。
- [x] 为详情切换、读取失败、provider 或 API Key 缺失、用户已修改或清空输入以及异步响应乱序增加保护，确保迟到响应不能覆盖当前 provider 或用户输入，同时保留现有可编辑状态和错误反馈。
- [x] 在 OpenCode API Key 明文输入框右侧提供复用现有 Lucide 图标体系的复制按钮，复制当前表单值；成功时显示局部成功状态并更新无障碍名称，失败时不误报成功且可重试。
- [x] 扩展详情组件和 AI 环境集成测试，覆盖新增与编辑路径、运行时完整值、目标 provider 匹配、缺失与失败、快速切换、用户输入保护、复制成功与失败、清空值、键盘触发及默认/成功无障碍标签。
- [x] 保留并加强服务商列表及列表接口的递归脱敏回归断言，确认顶层、嵌套配置和历史快照中的 `api_key`/`apiKey` 均不会暴露完整值，且保存顺序、脱敏占位符保留和 `rawJson.options.apiKey` 回填语义不变。

## 验收标准

- [x] 编辑已有 OpenCode provider 时，详情 API Key 与运行时配置中当前 `provider_key` 对应的 `options.apiKey` 完全一致，且不是列表脱敏值。
- [x] 读取失败、目标 provider 或 API Key 缺失、快速切换和请求乱序均不会把脱敏值、旧 provider 密钥或迟到值写入当前表单，也不会覆盖用户已输入或清空的值。
- [x] 新增与编辑详情均以明文显示当前 API Key；复制成功时剪贴板内容完全一致并有准确的成功状态，失败时不显示成功状态且按钮可再次触发。
- [x] 复制按钮可由键盘操作，默认与成功状态均具有准确的 `aria-label` 或等价可访问名称。
- [x] 服务商列表界面和 `service_providers_list` 继续只提供脱敏值，其他 provider、持久化结构和既有保存语义没有变化。

## 验证步骤

- [x] 运行 `npm run test -- src/components/AiEnvironments/ServiceProviderDetail.test.tsx src/components/AiEnvironments/AiEnvironments.test.tsx`，预期完整密钥加载、竞态保护、复制交互和保存回归用例全部通过。
- [x] 运行 `cargo test --manifest-path src-tauri/Cargo.toml provider_list_redaction_covers_top_level_nested_and_history_secrets`，预期列表接口脱敏边界测试通过且输出不包含明文 fixture。
- [x] 运行 `npm run build`，预期 TypeScript 检查与前端构建成功。

## 范围外事项

- 不修改服务商列表接口、OpenCode 配置持久化结构或其他 provider 的密钥展示策略。
- 不新增全局复制组件、剪贴板服务或新的运行时依赖。
