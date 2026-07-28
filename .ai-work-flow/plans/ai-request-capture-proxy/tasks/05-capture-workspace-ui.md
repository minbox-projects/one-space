# 05 - 抓包工作区与 IPC

## Goal

交付可操作的 AI 请求抓包工具页：用户可编辑并应用配置、查看运行状态、筛选分页记录、检查请求/响应明文详情和 AI 元数据，并通过事件保持当前列表与详情同步。

## Dependencies

02 - 基础代理纵向闭环

04 - AI 元数据、HAR 与 cURL（完成详情中的 provider/model/token 与正文表示验收所需）

## Status

ready-for-agent

## Acceptance Criteria

- [ ] TypeScript 包装器覆盖十个命令、统一错误格式和两个事件订阅 helper，命令名、参数和事件名与 Rust 契约一致。
- [ ] 页面持续显示明文风险警告，并提供 Enabled、端口、上游、保存/应用、运行状态、监听地址和最后错误。
- [ ] 左侧列表使用服务端搜索、method/status/provider/model 筛选和稳定分页；刷新与增量事件不会让过期详情响应覆盖新选择。
- [ ] 右侧展示概览及 Request/Response、Headers/Body 视图，完整显示敏感 header、文本/Base64 正文、token、截断和传输错误。
- [ ] 组件隐藏时不轮询，重新可见时主动校准；记录由 `in_progress` 更新到终态时当前列表和详情同步刷新。
- [ ] 左右工作区具有稳定响应式尺寸和 overflow 约束，长 header、URL、Base64 与正文不撑破工具区域。

## Verification

```bash
npm test -- src/lib/aiRequestCapture.test.ts src/components/AiRequestCaptureTool.test.tsx
npm run build
git diff --check
```
