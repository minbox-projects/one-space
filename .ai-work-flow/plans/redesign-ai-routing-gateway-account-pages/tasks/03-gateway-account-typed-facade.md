# 03 - 扩展账号 typed facade

- task_id: `gateway-account-typed-facade`
- order: `03`
- blocked_by: `gateway-account-detail-contract, atomic-api-key-account-save`
- source_plan: `../plan.md`
- source_plan_digest: `6f7f72cf85831fcd249370dc3a8fdcf7ecc5d3981bcc238427b969a3642dbbe1`
- write_scope: `src/lib/aiRoutingGateway.ts、src/lib/aiRoutingGateway.test.ts`

## 预期结果
前端唯一 typed facade 暴露账号详情读取和原子保存接口，并准确映射最终 Rust DTO、命令参数及领域错误，组件无需直接 `invoke` 或编排独立映射、价格保存调用。

## 实施清单
- [ ] 定义无密钥列表类型、专用详情类型、共享保存草稿、模型映射和四字段价格覆盖类型。
- [ ] 添加详情读取和原子保存 facade 方法，沿用统一 `call` 错误转换。
- [ ] 保持启停、排序、删除等既有窄命令接口不变。
- [ ] 增加命令名称、参数封装、camelCase/snake_case 边界、响应和错误包装测试。
- [ ] 增加契约测试，证明列表/bootstrap 类型未获得 API Key 字段，保存只发出一次原子命令调用。

## 验收标准
- [ ] 详情和保存接口的 TypeScript 类型与最终 serde DTO 一致。
- [ ] facade 使用已注册的新命令名和正确参数外壳，错误继续转换为统一前端错误。
- [ ] 单次保存不会调用旧账号更新、映射保存或价格保存接口。
- [ ] 账号列表与 Bootstrap 类型保持无 API Key 明文。
- [ ] 组件实现新流程所需的数据和命令均可经 facade 获得，无需直接使用 Tauri `invoke`。

## 验证步骤
- [ ] 运行 `src/lib/aiRoutingGateway.test.ts` 并确认新增与既有 facade 测试通过。
- [ ] 运行 TypeScript 类型检查或 `npm run build`，确认 DTO 和调用参数可编译。

## 范围外事项
不实现页面组件，不移除后端旧兼容命令，也不在 facade 中缓存或记录 API Key 明文。
