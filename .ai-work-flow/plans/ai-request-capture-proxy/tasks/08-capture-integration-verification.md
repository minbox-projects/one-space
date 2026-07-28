# 08 - 集成验收与稳定评审提交

## Goal

在完整产品链路上执行聚焦测试、全量质量门禁和本地 mock 代理验收，确认抓包数据只进入本机目录，并形成供最终双轴审查使用的稳定 review commit。

## Dependencies

03 - 流式转发与抓取保真

04 - AI 元数据、HAR 与 cURL

06 - 敏感操作闭环

07 - 产品导航、Launcher、i18n 与代码索引

## Status

ready-for-agent

## Acceptance Criteria

- [ ] 本地 mock upstream 验收覆盖普通 JSON、chunked request、大正文、SSE、上游错误、敏感 header、HAR 和 cURL，不使用真实生产凭证。
- [ ] 同步屏障证明 SSE 首块不等待上游结束；大于 2 MiB 的双向流量完整转发而持久化样本受限并标记截断。
- [ ] 应用启动恢复、运行错误隔离、筛选分页、详情事件、清空/导出确认和两个产品入口形成完整闭环。
- [ ] 抓包配置和数据库位于本机应用目录，Git/iCloud 共享根目录没有新增抓包文件。
- [ ] 计划规定的 npm、Cargo 和 diff 命令按顺序全部通过，结果被保存到执行记录。
- [ ] 实现差异形成稳定 review commit；工作树在交给 Code Reviewer 前干净。

## Verification

```bash
cargo test --manifest-path src-tauri/Cargo.toml ai_request_capture::tests::integration_acceptance -- --nocapture
git diff --check
npm test
npm run build
npm run lint
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
git status --short
```
