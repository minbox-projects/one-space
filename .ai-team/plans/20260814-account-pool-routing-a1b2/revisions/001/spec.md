---
plan_id: 20260814-account-pool-routing-a1b2
revision: "001"
target_branch: main
supersedes: null
---

# 规格说明

## 背景

AI 路由网关的账号池当前从两个添加入口直接进入统一创建详情，并固定执行 API Key 创建。现有账号卡片进入统一编辑详情，页面再按 `account_type` 显示差异字段。前端、Rust 与 SQLite 已将账号类型定义为 `oauth` 或 `api_key`，现有更新接口不包含 `account_type`。OAuth enrollment 当前受 release gate 阻断，相关 Tauri command 未注册；已有 OAuth 账号可以更新通用元数据，但 OAuth 凭据、连接、映射与价格不提供写入能力。

## 目标

- [ ] 点击添加账号时先选择 OAuth 或 API Key，再进入对应新增页。
- [ ] 账号类型创建后不可修改。
- [ ] 点击账号卡片或编辑入口时按持久化类型进入对应编辑页。
- [ ] 保持 API Key 现有新增与编辑能力，并遵守 OAuth release gate。

## 非目标

- 不支持既有账号在 OAuth 与 API Key 之间转换。
- 不实现 OAuth enrollment、重新登录或 OAuth 专属字段写入。
- 不调整网关密钥、请求日志、设置页或路由核心行为。
- 不新增或修改数据库迁移、Rust command 或 typed IPC 契约。

## 用户场景

### 场景 1：选择 API Key 并新增账号

- 前置条件：用户位于 AI 路由网关账号池。
- 操作：点击任一添加账号入口，在弹框选择 API Key，填写并保存账号。
- 预期结果：进入 API Key 新增页；通过现有校验后原子创建账号及相关配置并返回账号池。
- 异常结果：取消选择或取消表单时不写入；校验或保存失败时显示现有错误并保留表单。

### 场景 2：选择 OAuth 新增账号

- 前置条件：用户位于 AI 路由网关账号池。
- 操作：点击添加账号并选择 OAuth。
- 预期结果：进入 OAuth 专用页，明确显示暂不可用，并可返回账号池。
- 异常结果：不得调用 OAuth enrollment IPC，不得创建账号或凭据。

### 场景 3：编辑不同类型的现有账号

- 前置条件：账号池中存在 API Key 或 OAuth 账号。
- 操作：点击账号卡片或卡片编辑入口。
- 预期结果：按持久化 `account_type` 进入对应编辑页；API Key 保留现有编辑能力，OAuth 仅允许修改通用元数据。
- 异常结果：账号不存在时沿用现有加载或错误行为；任何编辑路径均不得改变账号类型。

## 功能需求

### REQ-001：添加账号前选择类型

- 描述：账号池任一添加账号入口先打开类型选择弹框。
- 输入：用户点击添加账号，并选择 OAuth、API Key 或取消。
- 输出：确认后进入对应新增页；取消后关闭弹框并停留账号池。
- 约束：弹框只提供 OAuth 与 API Key；取消不得触发账号写入。

### REQ-002：API Key 新增流程

- 描述：选择 API Key 后进入 API Key 新增页。
- 输入：名称、API Key、Base URL、认证方式及现有可选配置。
- 输出：通过现有 typed IPC 原子创建账号、加密凭据和关联配置。
- 约束：保留现有必填、URL、认证方式校验，取消不写入，失败保留表单。

### REQ-003：OAuth 暂不可用新增页

- 描述：选择 OAuth 后进入 OAuth 专用暂不可用页。
- 输入：OAuth 类型选择。
- 输出：展示明确的暂不可用状态和返回账号池操作。
- 约束：不展示可提交 enrollment 控件，不调用 OAuth IPC，不创建账号或凭据。

### REQ-004：账号类型不可变

- 描述：账号类型一旦创建不可修改。
- 输入：任一现有账号的编辑操作。
- 输出：编辑保存只更新该类型允许的字段。
- 约束：编辑页不提供类型选择控件；更新 payload、后端更新和持久化均不改变 `account_type`。

### REQ-005：按类型进入编辑页

- 描述：账号卡片和卡片编辑入口均按账号类型进入对应编辑页。
- 输入：现有账号 ID 与持久化 `account_type`。
- 输出：OAuth 或 API Key 类型化编辑视图。
- 约束：卡片主体与编辑按钮行为一致，不从用户可编辑状态推断类型。

### REQ-006：保留 API Key 编辑能力

- 描述：API Key 编辑页保留现有可编辑字段、保存顺序与校验。
- 输入：通用元数据、连接配置、模型映射和价格配置。
- 输出：通过现有 typed IPC 更新对应信息。
- 约束：类型分流不得改变现有 IPC payload、错误提示或后端原子性。

### REQ-007：限制 OAuth 编辑范围

- 描述：OAuth 编辑页只允许修改现有后端支持的通用元数据。
- 输入：名称、分组、标签、额度阈值、备注和启用状态。
- 输出：保存后刷新账号列表并展示更新值。
- 约束：OAuth 凭据、连接、映射和价格保持只读；不提供重新登录。

### REQ-008：本地化、交互与回归覆盖

- 描述：新增类型选择、类型化页面和暂不可用状态具备完整中英文与可验证交互。
- 输入：中英文语言环境、鼠标和键盘操作。
- 输出：文案正确，弹框可确认、取消、关闭，焦点和按钮语义沿用现有 Dialog 组件能力。
- 约束：补充组件回归测试并保持现有账号池测试通过。

## 验收标准

### AC-001：类型选择弹框

- Given：用户位于账号池，且列表为空或非空。
- When：点击任一添加账号入口。
- Then：显示包含 OAuth 与 API Key 的类型选择弹框；取消、关闭或按 Escape 后仍在账号池，且没有账号创建 IPC 调用。
- 验证命令或证据：`npm run test -- src/components/AiRoutingGateway/AiRoutingGateway.test.tsx`。

### AC-002：API Key 新增兼容

- Given：类型选择弹框已打开。
- When：选择 API Key 并提交有效或无效表单。
- Then：进入 API Key 新增页；有效输入调用现有原子创建 facade，无效输入不提交；取消不写入，保存失败保留输入。
- 验证命令或证据：组件测试断言页面分流、IPC 参数、取消与失败状态；运行 `npm run test -- src/components/AiRoutingGateway/AiRoutingGateway.test.tsx src/lib/aiRoutingGateway.test.ts`。

### AC-003：OAuth 新增受控阻断

- Given：类型选择弹框已打开。
- When：选择 OAuth。
- Then：进入 OAuth 暂不可用页，可返回账号池；页面无 enrollment 提交操作，且没有 OAuth 或账号创建 IPC 调用。
- 验证命令或证据：组件测试断言暂不可用文案、返回行为和 IPC 未调用。

### AC-004：账号类型不可修改

- Given：存在 OAuth 或 API Key 账号。
- When：进入编辑页并保存允许字段。
- Then：页面无可编辑账号类型控件，更新 payload 不包含 `account_type`，保存后原账号类型不变。
- 验证命令或证据：组件与 facade 测试断言无类型输入和无类型更新字段。

### AC-005：卡片按类型分流

- Given：账号池同时存在 OAuth 与 API Key 账号。
- When：分别点击卡片主体和编辑按钮。
- Then：OAuth 打开 OAuth 编辑页，API Key 打开 API Key 编辑页；标题和可见字段与类型一致。
- 验证命令或证据：组件测试覆盖两种类型、两种点击入口。

### AC-006：API Key 编辑无回归

- Given：打开现有 API Key 账号编辑页。
- When：修改并保存现有支持的通用元数据、连接、映射或价格。
- Then：沿用现有 typed IPC、校验、错误处理和刷新行为，所有既有 API Key 编辑测试通过。
- 验证命令或证据：`npm run test -- src/components/AiRoutingGateway/AiRoutingGateway.test.tsx src/lib/aiRoutingGateway.test.ts`。

### AC-007：OAuth 仅编辑通用元数据

- Given：打开现有 OAuth 账号编辑页。
- When：修改名称、分组、标签、额度阈值、备注或启用状态并保存。
- Then：调用现有通用账号更新；凭据、连接、映射和价格无写控件或保持只读，且不调用对应写 IPC。
- 验证命令或证据：组件测试断言允许字段保存和 OAuth 专属字段只读。

### AC-008：本地化与质量门禁

- Given：应用分别使用中文和英文。
- When：执行新增类型选择、OAuth 暂不可用和类型化编辑流程。
- Then：新增文案均使用 i18n key 且无缺失；弹框键盘关闭有效；聚焦测试、lint、build 和受管上下文校验通过。
- 验证命令或证据：`npm run test -- src/components/AiRoutingGateway/AiRoutingGateway.test.tsx src/lib/aiRoutingGateway.test.ts`、`npm run lint`、`npm run build`、`ai-team context validate --project /Users/yuqiyu/AiHistorys/one-space/onespace-app`。

## 数据与接口

- 沿用前端 `AccountType = oauth | api_key` 和账号数据中的 `account_type`。
- API Key 新增继续调用现有 `aiRoutingGatewayAccountCreateApiKeyWithConfiguration` facade；不改变参数、返回值或错误语义。
- 编辑继续使用现有通用账号更新及 API Key 专属写接口；更新 DTO 不新增 `account_type`。
- OAuth 暂不可用页不调用后端；不注册现有未注册的 OAuth commands。
- 不修改 SQLite schema、CHECK 约束、凭据格式或错误码。

## 兼容约束

- 现有行为必须保持：API Key 原子新增、取消不写入、失败保留表单、API Key 编辑、OAuth 通用元数据编辑及专属字段只读。
- 迁移兼容窗口：无数据迁移；现有 OAuth 与 API Key 记录直接按 `account_type` 分流。

## 安全约束

- 权限边界：前端只经 `src/lib` typed IPC facade 调用 Tauri command；OAuth 暂不可用页不得绕过 release gate。
- 敏感数据处理：不在页面、日志或测试快照中暴露 API Key 或 OAuth 凭据；沿用后端加密存储。
- 路径和输入校验：API Key 名称、密钥、URL 和认证方式继续使用现有校验；账号类型只接受已定义联合类型。

## 错误与边界

- 非法输入：API Key 表单沿用现有字段错误；未知账号类型不得进入可提交页，应回到账号池或显示受控错误。
- 空数据：空账号池的添加入口与非空列表工具栏入口行为一致。
- 超时或外部依赖失败：API Key 保存失败沿用现有错误展示并保留表单；OAuth 暂不可用页不发起外部请求。
- 重试和幂等：不新增自动重试；API Key 创建继续由现有原子事务防止部分写入。

## 迁移发布回滚

- 发布步骤：完成聚焦测试、全量 lint/build 与手工类型分流检查后，按现有桌面端发布流程交付。
- 迁移步骤：无数据库或配置迁移；实现角色需同步 `MEMORY.md` 与 `.ai-work-flow/index/feature-navigation.md` 的受管项目上下文并通过校验。
- 回滚触发条件和操作：若添加入口无法打开、API Key 创建/编辑回归、OAuth 页面触发写入或卡片分流错误，则回滚本次前端、i18n、测试和上下文变更；无需数据回滚。

## 已确认偏好

- 用户已明确决定：新增选择 OAuth 后进入专用暂不可用页，保留 OAuth release gate。
- 用户已明确决定：现有 OAuth 账号仅允许编辑通用元数据，OAuth 专属字段保持只读。
- 用户已明确决定：确认 REQ-001 至 REQ-008 及非目标作为规格基线。

## 默认取舍

- 使用账号池组件现有视图状态实现类型化页面，不新增应用级 URL 路由；理由是当前详情页已由组件内部状态控制。
- 复用项目现有 Radix Dialog、按钮、图标和样式模式；理由是保持桌面端交互一致。
- 不拆分实现任务；理由是 UI 状态、i18n 与组件测试集中在同一功能边界且写入范围高度重叠。

## 已关闭问题

- 问题：选择 OAuth 后是否实现完整 enrollment？结论：不实现，进入暂不可用页。证据：typed decision `decision_01KZZ9XFHRTG4B8AZ1NNQRGQEW`。
- 问题：OAuth 编辑字段范围？结论：仅通用元数据。证据：typed decision `decision_01KZZA33WBF14GDVP841Y3F48T`。
- 问题：是否确认完整需求清单？结论：确认。证据：typed decision `decision_01KZZA5YGXA075QC29XZYGYH6D`。

## 未决问题

- 无。
