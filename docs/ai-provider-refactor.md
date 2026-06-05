# AI 终端服务商配置重构计划（含中英文国际化）

## Summary

- 将 AI 终端环境页及相关全栈 domain 从“环境 / Profile / Provider”统一为“服务商 / Service Provider”，并保留旧 API/数据兼容迁移层。
- 添加/编辑服务商改为页面内独立详情页：点击列表项或新增后，AI 终端环境菜单右侧区域整页进入编辑界面，左上角提供返回按钮。
- Claude 服务商新增 API 格式、认证字段、模型映射、模型列表获取、配置 JSON、快捷配置和自定义文字/Emoji 图标。
- 所有新增和改名文案必须补全中文、英文国际化，不允许在 UI 中硬编码中文或英文产品文案。

## Key Changes

- 全栈命名与迁移：
    - 新增 ServiceProviderRecord、ServiceProvidersState、ServiceProviderInput，替代后端 Provider* domain 命名；前端 AiProvider 改为 AiServiceProvider。
    - 新增 service_providers_* Tauri commands；旧 providers_* 和 claude_profile_* 命令保留为 deprecated 兼容壳，内部转调新实现。
    - 持久化迁移到 service_providers schema；读取旧 providers state 自动迁移并写新 state。导入/export 同时接受旧 providers 与新 service_providers payload。
    - 同步事件、Dashboard count、App/Settings/Workflow/AiSessions/ConfigConflict/内部 CLI 等现有调用点统一改用服务商命名；旧 provider_id 字段保留兼容，新增展示/接口字段使用
      service_provider_id。

    - 只改 AI 终端服务商相关概念；SSH 隧道“环境分组”、运行时 profile、Claude CLAUDE_CONFIG_DIR 等非本页产品概念不做误改。

- 国际化：
    - src/i18n.ts 中所有新增 key 必须同时补全 en 和 zh。
    - AI 终端环境页现有可见文案统一改名：
        - 中文：AI 终端环境 -> AI 终端服务商，环境 -> 服务商，环境名称 -> 服务商名称。
        - 英文：AI Environments -> AI Terminal Service Providers，Environment / Provider / Profile 作为本页产品概念时统一为 Service Provider。

    - 需要覆盖列表、搜索、空状态、新增、编辑、删除、保存、激活、导入导出、确认弹窗、错误提示、Claude 配置分组、模型映射、快捷配置、JSON 编辑、模型列表获取等所有新旧文案。
    - UI 组件不得直接写死“服务商”“环境”“Profile”“Provider”等产品文案；全部通过 t(key, fallback) 读取，fallback 也必须与当前英文产品命名一致。
    - 中文文案使用“服务商”，英文文案使用“Service Provider”；技术字段名、env var、API enum 不翻译。

- 页面结构：
    - AiEnvironments 改为 list / detail 模式，不再把编辑表单塞进 AccordionItem。
    - 列表页保留 CLI 卡片、工具切换、搜索、过滤、导入/导出、新增；列表项只做摘要和动作入口。
    - 详情页铺满右侧内容区，左上角返回；顶部固定显示服务商名称、工具、状态、保存/激活/删除等主操作。
    - 服务商头像使用 icon 字段；支持 1-2 个 Unicode 字符或最多 4 个 ASCII 字符。为空时取服务商名称首个非空字母/汉字，颜色按服务商 id 稳定生成。

- 通用配置 JSON：
    - 详情页直接显示“配置 JSON / Configuration JSON”，作为可编辑高级入口；格式化是 link-styled 按钮。
    - JSON 与表单双向同步：表单改动刷新 JSON；JSON 保存前解析并覆盖草稿字段。
    - API Key 能解密时明文显示；若后端只返回 ********，保存时该占位符表示保留现有 secret，不覆盖为空。
    - 无效 JSON 阻止保存并显示本地化错误；权限字段即使表单隐藏，仍保留在 JSON 中供兼容/高级编辑。

- Claude 字段与 UI：
    - claude_api_format: anthropic_messages（默认，原生）、open_ai_chat、open_ai_responses。
    - claude_auth_env_key: 新建默认 ANTHROPIC_AUTH_TOKEN；迁移旧 Claude 服务商时若原来使用 API Key，则保留为 ANTHROPIC_API_KEY。
    - claude_model_mappings: 固定三行 haiku、sonnet、opus，家族列只读；每行包含 display_name、upstream_model、supports_1m。
    - 1M 默认不勾选；Sonnet/Opus 可勾选，保存时给对应上游模型追加 [1m]；Haiku 行展示禁用勾选框并提示 Claude Code 仅文档化 Opus/Sonnet 1M。
    - 移除旧“模型路由分发（仅限 Claude）”表单；旧 claude_haiku_model、claude_sonnet_model、claude_opus_model 迁移到三行模型映射。
    - 隐藏 Claude 详情页整个“权限”分组；旧权限字段不显示、不清空、继续透传。

- Claude 投射规则：
    - 认证：按 claude_auth_env_key 写入 env.ANTHROPIC_AUTH_TOKEN 或 env.ANTHROPIC_API_KEY，并移除另一个认证 env。
    - Anthropic 原生：写真实服务商 key、ANTHROPIC_BASE_URL、模型 env。
    - OpenAI Chat/Responses：自动创建/更新协议代理 route，route id 固定为 service-provider-{id}；保存时启用本地协议代理并启动。Claude settings 写本地代理 token 和
      http://127.0.0.1:{port}/anthropic/{routeId}/v1，上游真实 key 只保存在 route/secret 中。

    - 模型映射 env：
        - haiku -> ANTHROPIC_DEFAULT_HAIKU_MODEL
        - sonnet -> ANTHROPIC_DEFAULT_SONNET_MODEL
        - opus -> ANTHROPIC_DEFAULT_OPUS_MODEL
        - 显示名 -> 对应 ANTHROPIC_DEFAULT_{FAMILY}_MODEL_NAME

    - 隐藏 AI 署名：写 settings.attribution = { commit: "", pr: "" }，并清理旧 includeCoAuthoredBy 冲突。
    - 启用 Tool Search：写 env.ENABLE_TOOL_SEARCH = "true"；取消勾选时移除该 env，不写 false。

- 模型列表获取：
    - 新增 service_provider_fetch_models(service_provider_id | draft)，支持从未保存草稿拉取。
    - Anthropic 原生：规范化 Base URL 后请求 /v1/models，带 anthropic-version: 2023-06-01 和 x-api-key。
    - OpenAI Chat/Responses：规范化 Base URL 后请求 /models，带 Authorization: Bearer <key>。
    - 解析 data[].id，缓存到草稿 fetched_models，不自动覆盖映射。
    - 每个映射行的“上游模型 / Upstream Model”支持手输，也支持从拉取结果下拉选择后写入输入框。

## Test Plan

- Rust:
    - 旧 providers state、旧导入文件、旧 active map 自动迁移到 service providers state。
    - providers_* / claude_profile_* 兼容命令仍可用，新 service_providers_* 返回新结构。
    - Claude 投射覆盖 Anthropic 原生、OpenAI Chat、OpenAI Responses；认证 env、Base URL、本地 proxy token、模型映射、[1m]、_NAME、署名和 Tool Search 均正确写入。
    - service_provider_fetch_models 能解析 Anthropic Models API 和 OpenAI 兼容 /models。
    - Secret 占位符 ******** 保存时保留旧 secret，不覆盖为空。

- Frontend:
    - npm run build 通过。
    - 新增、编辑、返回、保存、激活、删除、导入、导出流程可用。
    - src/i18n.ts 新增/修改 key 同时存在中文和英文；切换语言后 AI 终端服务商页面无缺失 key、无硬编码产品文案。
    - 中文界面不残留 AI 终端服务商相关“环境 / Profile / Provider”；英文界面不残留本页产品概念中的 Environment / Profile / 裸 Provider。
    - Claude 详情页显示配置 JSON，格式化、JSON 编辑同步、快捷配置、API 格式、认证字段、模型映射、模型列表下拉均可用。
    - 权限分组不显示，旧权限字段保存后未丢失。
    - 图标 fallback 和自定义文字/Emoji 展示稳定。

- Manual / visual:
    - 启动本地 dev server，用浏览器分别检查中文和英文界面的列表页、详情页、弹窗、空状态、错误提示。
    - 验证桌面/窄宽度下文本不溢出、不重叠。
    - 验证已有旧服务商数据打开后不丢失 API Key、active selection、Claude 隔离目录和工作流引用。

## Assumptions

- “服务商 / Service Provider”是 AI 终端配置的新产品概念；内部兼容命令和旧数据字段可保留，但新代码路径和新 UI 不再使用旧命名。
- OpenAI Chat/Responses 的“本地协议转换”由 OneSpace 自动管理协议代理 route，用户不需要先去 Protocol Proxy 页面手动建 route。
- 自定义图标只支持文字/Emoji，不做图片上传。
- 配置 JSON 是高级编辑入口，权限字段可在 JSON 中保留。
- 官方参考：
    - Claude Code model configuration (https://code.claude.com/docs/en/model-config)
    - Claude Code environment variables (https://code.claude.com/docs/en/env-vars)
    - Claude Code settings (https://code.claude.com/docs/en/settings)
    - Anthropic Models API (https://platform.claude.com/docs/en/api/models/list)