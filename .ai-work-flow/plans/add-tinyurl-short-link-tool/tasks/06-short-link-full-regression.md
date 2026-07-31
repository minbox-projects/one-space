# 06 - 完成跨层整合与完整回归

- task_id: `short-link-full-regression`
- order: `06`
- blocked_by: `rust-short-link-backend, short-link-client-history, short-link-navigation-contracts, short-link-tool-ui, short-link-shell-integration`
- source_plan: `../plan.md`
- source_plan_digest: `bd1294e58912b10cedf6e2f834807758b2ebaf29b9f2f71d80a8caaa70a145e4`
- write_scope: `仅允许修正任务 01-05 所列文件中的跨层契约或回归问题；不得新增功能、依赖、公共接口或扩大文件范围`

## Outcome

前端、Tauri 命令、secret 存储、TinyURL 客户端、历史和导航形成一致的可发布流程，所有聚焦测试、全量检查及安全回归通过。

## Implementation Checklist

- [x] 核对四个 Tauri 命令的名称、参数、camelCase 成功响应和 `{ code, message? }` 错误契约在 Rust、前端包装器及测试 mock 中完全一致。
- [x] 核对稳定 ID `short-link` 在导航、展示、Launcher、可见性、App 和测试中的一致性。
- [x] 核对历史 key、schema、ISO 时间、50 条限制及本地删除语义在存储模块和组件中一致。
- [x] 核对九个稳定错误代码均有 Rust 映射、前端分支和中英文文案。
- [x] 审查 Token 数据流，确认不存在读取回前端、持久化到 localStorage、日志输出或错误泄漏路径。
- [x] 审查测试网络目标，确认自动化测试只使用本地 mock server，不访问 TinyURL 生产 API。
- [x] 先运行新增聚焦测试，再运行全量前端测试、Lint、构建、Rust 测试和 Rust 检查。
- [x] 仅修复验证发现的跨层契约或回归问题，不重复实现前置任务的核心功能。

## Acceptance Criteria

- [x] `[SC-1]` More Tools 与 Launcher 均可打开工具，返回、标题、面包屑及可见性设置符合现有行为。
- [x] `[SC-2]` Token 可安全保存、替换、删除并默认遮罩；后端不向前端返回已保存明文。
- [x] `[SC-3]` 仅合法 HTTP(S) URL 能触发 TinyURL 请求，成功结果正确展示。
- [x] `[SC-4]` 重复提交被阻止，输入和成功结果按计划保留，复制反馈明确。
- [x] `[SC-5]` 最近 50 条成功记录按时间倒序持久化，重载、复制、删除和确认清空均有效。
- [x] `[SC-6]` 本地历史操作不调用远端删除接口，且文案不暗示远端链接失效。
- [x] `[SC-7]` 凭据、限流、拒绝、服务、网络、响应、剪贴板和历史故障可区分且不泄露 Token。
- [x] `[SC-8]` 前端核心交互与历史、Rust URL/HTTP/错误映射、导航回归、TypeScript 构建、Lint、Rust 测试及检查全部通过。
- [x] 自动化验证未使用真实 TinyURL Token，也未访问 `https://api.tinyurl.com/create`。
- [x] 未增加前端依赖、数据库迁移、CSP 变更、供应商抽象或计划外功能。

## Verification Steps

- [x] 运行 `npm run test -- src/lib/shortLink.test.ts src/lib/shortLinkHistory.test.ts src/components/ShortLinkTool.test.tsx`。
- [x] 运行 `npm run test -- src/components/MoreToolsHub.test.tsx src/components/Launcher.test.tsx src/App.moreToolsNavigation.test.tsx src/lib/launcherToolVisibility.test.ts`。
- [x] 在 `src-tauri` 运行 `cargo test short_link`。
- [x] 在仓库根目录运行 `npm run test`。
- [x] 在仓库根目录运行 `npm run lint`。
- [x] 在仓库根目录运行 `npm run build`。
- [x] 在 `src-tauri` 运行 `cargo test`。
- [x] 在 `src-tauri` 运行 `cargo check`。
- [x] 检查完整测试输出，确认无真实 TinyURL 请求、Token、Authorization header 或完整敏感长链接。

## Out of Scope

不增加新功能、不调整已确认交互、不进行真实 TinyURL 手工调用、不修改计划外文件，也不处理 alias、统计、远端撤销、多供应商、历史加密或同步。
