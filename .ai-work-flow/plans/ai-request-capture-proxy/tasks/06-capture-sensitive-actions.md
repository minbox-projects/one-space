# 06 - 敏感操作闭环

## Goal

在工具页完成 HAR 保存、清空记录和复制 cURL 的用户闭环，以明确二次确认保护明文导出与删除操作，并在样本不完整时阻止用户误认为 cURL 可完整重放。

## Dependencies

04 - AI 元数据、HAR 与 cURL

05 - 抓包工作区与 IPC

## Status

ready-for-agent

## Acceptance Criteria

- [ ] HAR 导出先选择保存路径，再显示明确包含“明文鉴权、Cookie、正文”的二次确认；取消任一步均不调用导出命令。
- [ ] 清空使用现有确认对话框，文案说明进行中请求完成后记录可能重新出现；成功后清除选择并刷新第一页。
- [ ] 完整 cURL 可复制并提供反馈；`complete = false` 时复制前额外提示，复制内容首行包含 warning 注释。
- [ ] 导出使用当前筛选条件但忽略分页，后端只写入已结束记录并返回结构化结果。
- [ ] 风险警告在操作前后持续可见，真实敏感值不被遮罩、脱敏或替换。

## Verification

```bash
npm test -- src/components/AiRequestCaptureTool.test.tsx
cargo test --manifest-path src-tauri/Cargo.toml ai_request_capture::tests::export_enrichment -- --nocapture
git diff --check
```
