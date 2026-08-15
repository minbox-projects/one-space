<!-- ai-team:feature-navigation:start -->
<!-- ai-team:context-format {"renderer_version":"context-renderer-v2","schema_version":2} -->
# 功能导航

| 功能 | 关键词 | 入口路径 | 模块边界 |
| --- | --- | --- | --- |
| AI 路由网关账号池 | AI Routing Gateway, 账号池, OAuth, API Key, account_type | `src/components/AiRoutingGateway/index.tsx`<br>`src/lib/aiRoutingGateway.ts`<br>`src/App.tsx` | 账号新增由组件内类型 Dialog 分流；编辑严格读取持久化 account_type；OAuth enrollment release gate 保持不变。 |

<!-- ai-team:feature-navigation-entry {"entry_paths":["src/components/AiRoutingGateway/index.tsx","src/lib/aiRoutingGateway.ts","src/App.tsx"],"feature":"AI 路由网关账号池","keywords":["AI Routing Gateway","账号池","OAuth","API Key","account_type"],"module_boundary":"账号新增由组件内类型 Dialog 分流；编辑严格读取持久化 account_type；OAuth enrollment release gate 保持不变。"} -->
<!-- ai-team:feature-navigation:end -->
