# 07 - 产品导航、Launcher、i18n 与代码索引

## Goal

将 AI 请求抓包作为独立工具接入 More Tools、Launcher 和内部导航，提供兼容旧本地设置的默认可见性、完整中英文文案，并更新三份代码导航索引以标明前后端入口和 Protocol Router 边界。

## Dependencies

05 - 抓包工作区与 IPC

## Status

ready-for-agent

## Acceptance Criteria

- [ ] 新增统一 nav/tool ID `ai-request-capture`，More Tools 卡片和 Launcher 均可进入工具，返回目的地保持现有语义。
- [ ] 使用仓库现有 Lucide 网络检查图标，优先 `ScanSearch`，不手绘 SVG。
- [ ] Launcher 默认显示该工具；旧 localStorage 对象缺少新 key 时回落为 `true`，且不改变已有用户选择。
- [ ] 中英文资源覆盖工具名、说明、配置、状态、筛选、详情、风险、确认、HAR、cURL、截断和错误，界面无裸 key。
- [ ] `.ai-work-flow/index/feature-navigation.md`、`frontend-navigation.md`、`backend-navigation.md` 记录真实入口，并明确本工具不属于 Protocol Router。
- [ ] 导航、More Tools、Launcher 可见性/展示和中英文资源测试通过。

## Verification

```bash
npm test -- src/components/MoreToolsHub.test.tsx src/App.moreToolsNavigation.test.tsx
npm test -- src/lib/aiRequestCapture.test.ts src/components/AiRequestCaptureTool.test.tsx
npm run build
git diff --check
```
