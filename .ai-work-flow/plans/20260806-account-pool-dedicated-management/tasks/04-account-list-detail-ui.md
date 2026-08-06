# 04 - 账号卡片列表与 Create/Edit 详情

- task_id: `account-list-detail-ui`
- order: `04`
- blocked_by: `typescript-typed-ipc`
- source_plan: `../plan.md`
- source_plan_digest: `e8b89e919845f40f6d6d49ba0f3d16866c91d9e0cba261cc610a4b01a5976187`
- write_scope: `src/components/AiRoutingGateway/index.tsx 及同模块内必要的账号池专属局部组件（非穷举，不含测试文件）`

## 预期结果

账号池在模块内部以卡片列表和独立详情两种状态工作，API Key 新增一次原子提交完整配置，API Key 编辑保持旧写路径，OAuth 映射和价格只读。

## 实施清单

- [ ] 用 `viewMode: "list" | "detail"`、`detailMode: "create" | "edit"` 和 `selectedAccountId` 替换列表内 `expanded`、`showCreate` 和内嵌详情状态，不引入 URL 路由。
- [ ] 将账号列表改为卡片呈现名称、API Key/OAuth 标签、API 地址和 `public -> upstream` 映射列表，并保留标签筛选、分组创建、上移、下移、启停和删除动作。
- [ ] 让卡片主体、编辑动作和新增动作进入独立详情；提供返回动作，返回或保存成功后切回列表并执行 `reload`。
- [ ] 创建详情从 `data.models` 初始化所有官方模型的同名、启用映射，并为每个模型维护 input、output、cache read、cache write 四类每百万 token USD 字段。
- [ ] 允许创建时修改上游模型名与启用状态；将连接信息、最终显式映射和非空/空价格语义一次传给新 typed facade。
- [ ] 新增提交不得串联旧创建、映射保存或价格保存；失败时显示归一化错误并保留所有表单状态，成功时清除本地敏感 API Key 后刷新返回列表。
- [ ] 编辑 API Key 时继续使用旧 `account_update`、`mapping_save` 和 `price_save` 交互，不改变既有编辑能力。
- [ ] OAuth 详情加载并显示连接、映射和价格，但以只读内容替换映射开关、添加/保存映射、价格输入和保存动作，且不触发任何映射或价格写 facade。

## 验收标准

- [ ] 列表不再内嵌展开详情，卡片完整显示冻结 plan 要求的字段和既有管理动作，无 URL 或深链接变化。
- [ ] 创建页对全部官方模型预填同名启用映射和四类空价格，并允许按模型编辑这些值。
- [ ] 创建保存只发起一次新原子 facade；失败后字段、映射、价格和 API Key 均保留，成功后敏感值被清除且列表刷新。
- [ ] 已有 API Key 编辑仍通过旧独立 facade 保存连接、映射和价格。
- [ ] OAuth 详情可查看连接、映射和价格，但不存在可写控件或映射/价格写调用路径。

## 验证步骤

- [ ] 运行 `npm run build`，预期 list/detail/create/edit 状态与 typed facade 集成通过类型检查和构建。
- [ ] 在本地账号池人工检查卡片字段、筛选及管理动作、进入和返回详情、API Key 创建默认值与四类价格、API Key 编辑，以及 OAuth 只读状态，预期行为与验收标准一致。

## 范围外事项

不修改 `AiEnvironments` 服务商组件，不新增路由、深链接、OAuth 登录、官方模型管理或官方价格管理；不直接调用 `invoke`；不修改后端或测试文件。
