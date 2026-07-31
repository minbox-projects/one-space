# 02 - 实现前端调用与历史存储

- task_id: `short-link-client-history`
- order: `02`
- blocked_by: `none`
- source_plan: `../plan.md`
- source_plan_digest: `bd1294e58912b10cedf6e2f834807758b2ebaf29b9f2f71d80a8caaa70a145e4`
- write_scope: `src/lib/shortLink.ts；src/lib/shortLink.test.ts；src/lib/shortLinkHistory.ts；src/lib/shortLinkHistory.test.ts`

## Outcome

前端获得类型明确的四个 Tauri 调用函数和可独立测试的短链接历史存储能力，并能可靠区分成功、后端错误、历史损坏及 localStorage 读写失败。

## Implementation Checklist

- [x] 在 `shortLink.ts` 封装四个固定命令及计划规定的响应类型，不提供读取 Token 明文的函数。
- [x] 以稳定 `code` 识别后端结构化错误，安全 `message` 仅作为可选诊断信息，不用于业务分支。
- [x] 在 `shortLinkHistory.ts` 固定使用 `onespace:short-link-history`。
- [x] 定义严格 schema `{ id, longUrl, shortUrl, createdAt }`，要求四项均为字符串且 `createdAt` 是有效 ISO 8601 时间。
- [x] 使用 `crypto.randomUUID()` 生成记录 ID，使用 `new Date().toISOString()` 记录创建时间；每次成功创建都新增记录，不去重。
- [x] 加载时严格验证整个数组，按 `createdAt` 倒序排列并截断为最多 50 条。
- [x] JSON 损坏或任一记录不符合 schema 时删除该 key，返回一次可展示的恢复状态并从空历史继续。
- [x] localStorage 访问被拒绝、解析失败后的 key 清理失败、配额不足等情况返回明确失败状态，不伪报成功。
- [x] 新增、删除单条和清空操作只修改本地 key，不调用任何 Tauri 或 TinyURL 删除命令。
- [x] 为 IPC 命令名、参数、响应、错误传递及全部历史边界补齐单元测试。

## Acceptance Criteria

- [x] IPC 包装器严格调用 `short_link_config_status`、`short_link_save_token`、`short_link_delete_token`、`short_link_create`。
- [x] 配置状态和保存/删除响应只包含配置布尔值，前端 API 中不存在读取旧 Token 的能力。
- [x] `[SC-5/历史]` 成功记录最新在前、重载可恢复、超过 50 条时淘汰最旧记录，且不自动去重。
- [x] 损坏或不符合 schema 的历史被丢弃并返回恢复提示状态，不进行猜测性迁移。
- [x] 读写失败可与空历史、成功写入区分；生成结果可由调用方独立保留。
- [x] `[SC-6]` 删除单条或清空只操作 `onespace:short-link-history`，不会调用远端撤销能力。
- [x] 库测试不包含真实 Token，不访问生产 TinyURL。

## Verification Steps

- [x] 运行 `npm run test -- src/lib/shortLink.test.ts src/lib/shortLinkHistory.test.ts`，全部测试通过。
- [x] 运行 `npm run build`，确认公开类型与调用契约可被 TypeScript 正确消费。
- [x] 检查测试中的 Tauri mock 调用，确认历史删除和清空没有产生任何远端命令。

## Out of Scope

不实现 React 页面、Toast、确认对话框、导航接入、历史加密、同步、迁移或远端短链接管理。
