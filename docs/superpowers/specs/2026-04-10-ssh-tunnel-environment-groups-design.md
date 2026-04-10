# SSH Tunnel Environment Groups Design

## Goal

为 `SSH Tunnels` 新增可维护的“环境分组”能力，让每条 SSH 隧道归属一个环境分组，并在隧道列表页通过与 `Skills` 顶部一致风格的 Tabs 快速切换分组、过滤数据。

## Approved Direction

- `SSH Tunnels` 引入专属的环境分组实体，不与 `SSH Servers` 或其他模块共用。
- 每条 SSH 隧道都必须归属一个分组。
- 系统始终存在一个内置的 `默认分组`。
- 新增或编辑 SSH 隧道时，环境分组字段为非必填；用户不主动选择时，自动归入 `默认分组`。
- 用户可以新增、编辑、删除自定义环境分组。
- 删除自定义环境分组后，该分组下的所有 SSH 隧道自动迁移到 `默认分组`。
- `默认分组` 不允许删除，也不允许重命名。
- 在 SSH 隧道页面模式介绍卡片下方新增分组 Tabs，用当前选中的环境分组过滤列表。
- 分组 Tabs 右侧新增 `管理分组` 按钮，并在 Tabs 与按钮之间增加 `|` 分隔。

## Scope

本次设计只覆盖 `SSH Tunnels` 模块中的分组能力。

包含：

- SSH 隧道持久化结构升级
- 旧数据迁移到默认分组
- 环境分组的新增、编辑、删除
- SSH 隧道新增/编辑弹窗中的分组选择
- SSH 隧道列表页顶部的分组 Tabs 过滤
- 分组相关中英文文案补充

不包含：

- `SSH Servers` 页面改版
- 把环境分组扩展到工作空间、启动台或其他模块
- 权限、共享、排序拖拽等更复杂的分组管理能力
- 新增“全部分组”聚合视图

## Current Context

当前 `SSH Tunnels` 已具备以下基础：

- 后端使用 `src-tauri/src/ssh_tunnels.rs` 中的加密状态文件持久化隧道记录
- 前端使用 `src/components/SshTunnels.tsx` 渲染列表、编辑弹窗、连接状态与探测操作
- 页面顶部已存在一套模式介绍卡片，适合在其下方直接插入分组过滤区
- `Skills` 页面已有一套用户已接受的 Tabs 视觉样式，可作为 SSH 分组筛选的参考实现

当前仓库没有成体系的前端或 Rust 单元测试覆盖该模块，因此本次设计的验证需要以构建检查和手工验收为主。

## Data Model

### High-Level Storage Strategy

环境分组与 SSH 隧道属于同一业务域，因此继续沿用同一个 SSH 隧道加密状态文件，但把顶层结构从“隧道数组”升级为“分组 + 隧道”的状态对象。

推荐结构：

- `groups`: 环境分组数组
- `tunnels`: SSH 隧道数组

这样可以让分组删除和隧道自动迁移在一次持久化写入中完成，避免跨文件同步风险。

### Environment Group Record

每个环境分组至少包含：

- `id`
- `name`
- `created_at`
- `updated_at`
- `is_default`

约束：

- 系统内固定存在 `id = "default"` 的默认分组
- `默认分组` 的 `is_default = true`
- 自定义分组的 `is_default = false`
- 分组名去掉首尾空格后不能为空
- 分组名大小写不敏感去重，避免出现视觉上等价的重复分组

### Tunnel Record Changes

现有 SSH 隧道记录新增：

- `group_id`

约束：

- 每条隧道保存时必须写入有效 `group_id`
- 若前端未传或传入无效分组，后端统一回退为 `default`
- 读取已有隧道时，如果其 `group_id` 指向的分组不存在，也要自动回退为 `default`

## Migration Strategy

### Legacy Data Compatibility

当前线上和本地旧数据仍是纯隧道数组，因此读取逻辑需要兼容两种格式：

1. 旧格式：`Vec<SshTunnelRecord>`
2. 新格式：`SshTunnelState { groups, tunnels }`

### Migration Rules

当读取到旧格式时：

- 自动创建 `默认分组`
- 旧记录中的所有 SSH 隧道统一补上 `group_id = "default"`
- 在首次写回时落成新格式

该迁移必须是透明的，不要求用户手工触发或手工确认。

## Backend Responsibilities

### `src-tauri/src/ssh_tunnels.rs`

后端需承担以下职责：

- 定义环境分组结构与新的 SSH 隧道状态结构
- 保持旧格式兼容读取
- 在读写状态时保证 `默认分组` 永远存在
- 在隧道保存时校验和归一化 `group_id`
- 在分组删除时自动把关联隧道迁移到 `默认分组`
- 继续沿用当前的加密读写与运行态刷新机制，不改变连接与探测逻辑

### New Commands

为前端新增分组相关 Tauri 命令：

- 分组列表
- 分组新增/编辑
- 分组删除

这些命令的职责分别是：

- 返回完整分组列表，供 Tabs 和分组管理弹窗使用
- 校验分组名、创建或更新自定义分组
- 删除自定义分组，并把其名下 SSH 隧道迁回 `默认分组`

### Existing Command Adjustments

以下现有命令需要同步扩展：

- `ssh_tunnels_list`
- `ssh_tunnel_upsert`

要求：

- `ssh_tunnels_list` 返回时带上分组信息字段，供前端筛选和展示
- `ssh_tunnel_upsert` 接收可选 `group_id`，在保存时归一化为有效分组

## Frontend Layout And Interaction

### List Page

`src/components/SshTunnels.tsx` 的顶部结构调整为：

1. 页面标题与操作按钮
2. 错误提示
3. 三张模式介绍卡片
4. 环境分组 Tabs + `|` + `管理分组` 按钮
5. SSH 隧道列表

### Group Tabs

分组 Tabs 的交互规则：

- 样式与 `Skills` 页面顶部的推荐/仓库/已安装 Tabs 保持一致的黑白胶囊切换感
- 只展示真实存在的分组，不新增 `全部`
- 首次进入页面时默认选中 `默认分组`
- 点击某个分组后，只渲染该分组下的 SSH 隧道
- 当前选中的分组被删除后，界面自动切回 `默认分组`

### Group Management Entry

在分组 Tabs 右侧放置：

- 文本分隔符 `|`
- `管理分组` 按钮

该按钮不混入 Tabs 本身，避免把筛选项和管理动作混为同一层语义。

### Group Management Dialog

点击 `管理分组` 打开专门的分组管理弹窗。

建议弹窗内容：

- 顶部：说明当前分组只影响 SSH 隧道列表过滤与归属
- 中部：按列表展示现有分组
- 每个自定义分组右侧提供编辑与删除入口
- 底部或列表上方提供新增分组输入区

默认分组在弹窗中：

- 可见
- 有默认标识
- 不显示删除入口
- 不显示编辑入口

### Tunnel Editor

在新建/编辑 SSH 隧道弹窗中，名称字段下方新增 `环境分组` 选择器。

交互规则：

- 字段标记为可选
- UI 上允许展示“未选择”的占位状态
- 保存时如果未选，自然归入 `默认分组`
- 编辑已有隧道时回显其当前所属分组

### Tunnel Card Display

在 SSH 隧道卡片中补充分组信息，让用户在过滤后的列表中依旧能直接识别当前隧道所属环境。

建议位置：

- 与 `Source / Authentication Method / Launch at login` 同一信息行中展示
- 使用轻量文本，不额外做高强调徽标

## Error Handling

### Invalid Group Id

若前端传入的 `group_id` 不存在：

- 后端不要直接报错中断
- 统一回退到 `默认分组`

这样可以兼容分组刚被删掉、前端还没刷新的短暂窗口。

### Delete Group

删除自定义分组时：

- 后端先重写受影响隧道的 `group_id`
- 再删除该分组
- 最后统一发出更新事件

前端不自行迁移数据，只负责刷新和切回安全分组。

### Duplicate Or Empty Names

新增和编辑分组时：

- 空白名称禁止提交
- 与现有分组重名禁止提交
- 错误提示复用当前 SSH 隧道页面已有的 message/dialog 反馈模式

## Implementation Touchpoints

预期主要涉及：

- `src/components/SshTunnels.tsx`
- `src/i18n.ts`
- `src-tauri/src/ssh_tunnels.rs`
- `src-tauri/src/lib.rs`

如需拆分前端分组管理弹窗或分组类型定义，可以新增小型辅助组件或类型文件，但应保持 `SSH Tunnels` 相关职责聚拢，不扩散为新的全局模块。

## Non-Goals

- 不重做 SSH 隧道连接、断开、探测的底层逻辑
- 不引入分组排序、拖拽、颜色标签、图标等附加属性
- 不在这次改造中支持跨模块复用环境分组
- 不增加“全部分组”或跨分组批量操作

## Testing And Verification

### Automated Verification

至少执行：

- `npm run build`
- `npm run lint`
- `cargo check`

### Manual Acceptance

至少覆盖以下场景：

1. 老数据启动后自动迁移，现有隧道都出现在 `默认分组`
2. 成功新增自定义分组，并在 Tabs 中立刻可见
3. 成功编辑自定义分组名称，Tabs 和下拉选择器同步更新
4. 删除自定义分组后，该分组下隧道自动回流 `默认分组`
5. 新建 SSH 隧道时不选分组，保存后进入 `默认分组`
6. 新建 SSH 隧道时选择自定义分组，保存后只出现在对应 Tabs 下
7. 编辑已有 SSH 隧道时切换分组，列表过滤结果正确变化
8. 当前选中分组被删除后，页面自动切回 `默认分组`
9. 连接、断开、探测、删除隧道等既有行为在分组改造后不回归

## Success Criteria

- 用户可以在 `SSH Tunnels` 中维护自己的环境分组
- 每条 SSH 隧道都有稳定、可见、可迁移的分组归属
- 默认不做额外操作时，隧道自动进入 `默认分组`
- 分组 Tabs 能快速过滤 SSH 隧道，并与 `Skills` 顶部切换样式保持一致
- 删除分组不会造成隧道丢失，而是安全回流到 `默认分组`
- 旧用户数据可以无感迁移，不需要手工修复
