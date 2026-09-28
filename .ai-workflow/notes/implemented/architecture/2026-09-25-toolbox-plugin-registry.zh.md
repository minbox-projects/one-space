# Agent Note: Toolbox Plugin Registry Replaces Hand-Maintained Tool Lists

Status: implemented

[English](2026-09-25-toolbox-plugin-registry.md) | 中文

## Problem

工具箱工具元数据此前分散在多份手工维护的清单中：`src/lib/navigation.ts` 的 `MoreToolsSection` id 与别名、`src/lib/moreToolPresentation.ts` 的卡片展示、`src/lib/launcherToolVisibility.ts` 的可见性默认值、`src/components/MoreToolsHub.tsx` 的 Hub 卡片、`src/components/Launcher.tsx` 的快速工具与内部目标，以及 `src/App.tsx` 与 `src/lib/trayMenu.ts` 中的 Notes/Snippets 侧边栏与托盘入口。新增、改名或隐藏一个工具都要改遍每份清单，漏改一处就会让某个表面不可达或显示不一致。MD5 工具在 kebab-case 导航 id 之外还带着遗留 camelCase id（`md5Encryption`），Hub 向活动工具传递恒为真的可见性，导致隐藏的工具仍在轮询与刷新；每个工具又各自实现了略有差异的复制反馈、安全 localStorage、历史、Tauri 事件、轮询与徽标逻辑。同一时期还积累了一批没有可达行为的兼容代码：实验性 CloudDrive 表面、解析到无渲染路径页签的 `backup` 幽灵路由、`normalizeLegacyTabTarget`、Launcher 的 `ai-flow` 过滤、一次性的 localStorage 到后端启动项迁移、RandomPassword 与 JT/T 输入历史的遗留迁移、没有任何实现支撑的 `notes_search` 助手能力声明，以及仅被自身测试使用的原子 AI Workflow 保存并激活命令。

## Decision

由唯一注册表作为全部工具表面的单一来源，并删除而非保留已无行为的兼容代码。

1. `src/toolbox/types.ts` 定义描述符模型（`ToolboxSurface`、`ToolboxToolDescriptor`、`ToolboxNavAlias`、`ToolboxBilingualText`、`ResolvedToolboxTarget`），`src/toolbox/plugins/` 中每个工具一个描述符，`src/toolbox/registry.ts` 提供 `TOOLBOX_TOOLS`、`listToolboxTools`、`getToolboxTool`、`resolveToolboxNavigationAlias`、`assertToolboxRegistryIntegrity` 与 `resolveToolboxText`。Hub 卡片来自注册表 `hub` surface 且绝不因启动台可见性隐藏；Hub 详情、Launcher 快速工具、Launcher 内部工具箱目标（Notes 与 Snippets，经 `launcher-internal`）、`resolveNavigationTarget` 别名、启动台可见性默认值、`listToolboxTools` 按描述符 `defaultOrder` 的排序、Notes/Snippets 侧边栏条目以及 App 活动工具面包屑均由这些描述符派生，而托盘菜单结构仍为手工维护、仅其 Notes/Snippets 标签经注册表解析（缺失时回退 `tray.notes` / `tray.snippets`）。`assertToolboxRegistryIntegrity` 拒绝重复 id、未知 surface 与非 kebab-case 的 Hub id。Notes 与 Snippets 是没有 Hub 卡片的 plugins；`md5-encryption` 是规范 id，遗留 `md5Encryption` 可见性记录被忽略，工具因此按默认值可见。`src/lib/moreToolPresentation.ts` 在最后一个消费者迁移到注册表后删除。
2. `src/toolbox/invoke.ts`（`invokeToolboxCommand`、`ToolboxInvokeError`、`isToolboxInvokeAvailable`）、`src/toolbox/localStore.ts`（`readLocalJson`/`writeLocalJson`）、`src/toolbox/historyStore.ts`（`createHistoryStore`）、`src/toolbox/useCopyToClipboard.ts`（`COPIED_FEEDBACK_RESET_MS = 1600`）、`src/toolbox/useTauriEvent.ts` 与 `src/toolbox/useVisibleInterval.ts` 取代手写的运行时实现，`src/components/toolbox/` 提供 `ToolShell`、`ToolErrorBanner`、`ToolStatusBadge` 与 `ToolEmptyState`。七个复制入口（Random Password、JSON Parser、MD5、Short Link、File Sharing、Protocol Router 与 JT/T 解析器）使用共享复制反馈，已迁移的事件、轮询与徽标调用点使用共享 helper；共享历史存储与安全 localStorage 写入器当前支撑 `shortLinkHistory`，而 `jttInputHistory` 保留自己的全有或全无无效记录恢复。SshServers 与 ProtocolRouterTool 为其剩余后端命令保留原始 `invoke`，但每个调用点都先检查 `isToolboxInvokeAvailable()`，因此非 Tauri 运行时不存在可达的原始 invoke。`MoreToolsHub` 的卡片来自注册表 `hub` surface，并把宿主页签可见性以 `isVisible && activeTool === id` 传给活动工具，隐藏工具因此不发起轮询或事件驱动刷新；它还订阅 `LAUNCHER_TOOL_VISIBILITY_UPDATED_EVENT`，使详情页 `Show in Launcher` 开关随启动台可见性变化刷新。
3. CloudDrive 端到端移除：组件、插件描述符、Hub 与 Launcher 入口、路由别名、`onespace_aliyun_token` 访问与双语键全部删除。该移除行为中性，因为实验性表面没有消费者，也没有其他代码引用它。
4. `backup` 幽灵 Hub 路由移除：`backup` 既不是 Hub 工具 id 也不是 More Tools 别名，备份操作描述符改为指向真实的 `mcp-servers` 页签，因此备份消息解析到已存在的页面，而不是没有渲染路径的页签。
5. 旧导航兼容移除：`normalizeLegacyTabTarget` 与 Launcher `ai-flow` 过滤删除，`resolveNavigationTarget` 只解析当前 smart-workspace 别名与注册表别名。不再可解析的过期持久化启动项或历史消息目标原样透传并打开空白，这是已批准的代价，且不再有任何生产者发出这些目标。
6. 一次性 localStorage 到后端启动项迁移删除；启动项只从后端存储读取。对当前数据而言该移除行为中性，因为已无生产者写入旧 localStorage 格式，已批准代价是未迁移的遗留条目不会被导入。
7. 遗留历史迁移删除：RandomPassword 的遗留 localStorage 历史改为清除而非迁移，`jttInputHistory` 的字符串数组迁移删除，因此存储的遗留字符串数组会被其全有或全无恢复视为无效历史并清除，当前格式的读写保持不变。遗留格式已不再产生，因此不会丢失任何有效历史。
8. `notes_search` 端到端撤出助手工具策略、能力快照、默认 agents、模型工具定义、会话提示、stub 分派器、AI Workspace 能力徽标与开关、前端类型以及双语标签，包括从两个 i18n bundle 删除的无引用键 `allowNotesSearchLabel`。撤出行为中性，因为该能力从来没有实现支撑，继续声明会让助手提供无法运行的能力；`AgentToolPolicy` 现在只携带已实现的 `web_search` 与 `workspace_read`。
9. AI Workflow 原子链移除：`save_and_activate_profile`、`ai_workflow_save_and_activate_profile`、命令注册与 re-export、前端 `saveAndActivateProfile` 封装以及以该原子语义为主题的 Rust 测试全部删除，`ai_workflow_rename_profile` 仍保留在注册命令中。保存与激活仍以不变行为彼此分离，且该原子命令没有生产调用方，因此移除不改变任何用户可见流程。
10. 无可达路径使用的死代码与重复工具代码删除：JsonParser 不可达回退、FileSharing 未用快照字段与未使用的 `subscribeFileSharingUpdates` 导出、空快照重复与 `window.confirm`、SshServers 重复的 host 类型与仅在控制台输出的密钥文件错误、SshTunnels 重复的批量路径与过期的已保存探测展示（已保存探测现在会过期）、未本地化的草稿探测错误与硬编码中文、ProtocolRouter 重复的后端默认值与归一化器、双重统计控件与未加守卫的路由测试，以及 jttDataParser 重复的 hex/时间/uint32 helper、重复错误类型与死 CRC 条件。JT/T 位置解析器现在是唯一按规范排序的实现（纬度在字节偏移 8、经度在字节偏移 12），同时供给 tree、JSON、0x0801 多媒体与 JT809 路径，存有反向顺序的合成 fixture `JT808_F3_FRAGMENT_1` 通过交换位置 dword 修正，校验和保持不变。这些移除保留全部可达行为，并使标签与规范一致。

## Alternatives considered

- 保留平行的手工清单、只增加一致性测试：未采纳，因为测试只能在漂移之后发现问题，而每个标签、图标、默认值或别名改动仍会在注册表现已派生的多份清单中重复。
- 用 codegen 从既有清单生成注册表：未采纳，因为事实来源仍会含糊不清，而每个工具一个描述符的数据量足够小，可以直接声明并由完整性断言约束不变量。
- 保留兼容垫片（旧导航归一化、`ai-flow` 过滤、启动项与历史迁移、幽灵路由、`notes_search` 声明与原子命令）：未采纳，因为已无生产者发出这些输入，剩余调用方只是被移除行为的测试，而已批准代价是过期目标打开空白，而不是让不可达代码路径继续存在。
- 把遗留历史迁移进共享存储而不是清除：未采纳，因为遗留记录形态已不再产生，既有无效记录恢复逻辑已经返回空历史，迁移只会为当前版本不写入的数据增加一次性转换代码。
- 在 Hub 保留恒为真的可见性契约：未采纳，因为隐藏工具仍在轮询；转发宿主真实可见性才是如实的契约，并由轮询与事件测试断言。

## Consequences

- 新增或调整工具箱工具只需编辑一个描述符：id、图标与配色、i18n 键或双语文本、别名、surface、默认可见性与顺序、组件。Hub 卡片、Launcher 快速与内部工具箱目标、侧边栏条目与导航别名由它派生，而托盘菜单结构仍为手工维护、仅 Notes/Snippets 标签经注册表解析；注册表完整性测试拒绝重复 id、未知 surface 与非 kebab-case 的 Hub id。
- 共享运行时是已迁移的复制反馈、安全 localStorage 与历史 helper、Tauri 事件、可见性门控轮询以及工具 shell、徽标、空状态与错误横幅表面的唯一实现；共享历史存储当前支撑 `shortLinkHistory`，`jttInputHistory` 保留自己的恢复逻辑，SshServers 与 ProtocolRouterTool 的剩余原始 invoke 全部有调用时可用性检查。复制反馈在 1600 ms 后清除，损坏存储读取返回安全默认值，写入绝不抛出，隐藏工具不发起轮询或事件驱动 invoke。
- 已移除的表面不再存在：CloudDrive、`backup` 路由、旧导航归一化、`ai-flow` 过滤、旧启动项与历史迁移、`moreToolPresentation.ts`、`notes_search` 与原子 AI Workflow 命令。过期持久化启动项与历史消息目标打开空白，这是已批准行为；没有生产者发出它们，也没有任何测试被削弱。
- 助手能力契约如实：工具策略与能力快照只携带已有实现的字段，AI Workspace 徽标、开关、类型与标签与之一致。
- `MEMORY.md` 与 `.ai-workflow/index/navigation.json` 记录注册表、共享模块与移除项，本记录记录注册表决策、移除理由与行为中性论证。[Tray menu ownership and contract](2026-09-22-tray-menu-ownership-and-contract.md)、[AI Workflow Model Switcher Delivers 9-by-3 Matrix with Backend Profile Commands](../feature/2026-09-22-ai-workflow-model-switcher.md) 与 [AI Workflow Profile Save and Activation Are Separate Actions](../feature/2026-09-23-ai-workflow-profile-save-activation.md) 保留各自原有决策（无完全取代），并已就地更新为交付后的注册表布线、命令集与保存/激活分离流程。
