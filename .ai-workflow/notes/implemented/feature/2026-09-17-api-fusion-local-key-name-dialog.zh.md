# Agent Note: API Fusion Local Key Creation Uses a Name Dialog

Status: implemented

[English](2026-09-17-api-fusion-local-key-name-dialog.md) | 中文

## Problem

`Api Keys` 面板此前把创建表单渲染在页头里：新建 Key 的名称内联输入框紧挨 `Add key` 按钮，按钮在该输入框有内容前一直禁用，回车即提交，而输入框的 `Label` / `标签` 措辞描述的是一个列表属性，而不是所请求的名称。由于名称状态位于 `LocalKeyList.tsx`，打开创建交互、校验它以及提交后清空它都属于渲染 Key 列表的一部分，页头也长期为一个仅在创建 Key 时才有意义的控件保留空间。

## Decision

新建本地 Key 现在在对话框中进行。`LocalKeyList.tsx` 只保留 `Add key` 按钮，该按钮在面板不忙时始终可用并调用新增的 `onAdd` prop；原来的 `onSave` prop 与 `labelInput` 状态已删除。`src/components/ApiFusion/index.tsx` 持有新增的 `isKeyDialogOpen` 状态，由 `onAdd` 打开，并以 `open`、`onOpenChange`、`busy` 与 `onSave` 渲染 `LocalKeyDialog.tsx`（`src/components/ApiFusion/LocalKeyDialog.tsx`）。对话框在自己的状态中保存名称，每次打开时重置，名称去空白后为空时 `Save` 保持禁用，并以 `{ id: "", label: name.trim(), value: "", enabled: true, created_at: 0 }` 提交，因此创建契约不变：只收集名称，密钥仍由后端生成。其标题复用 `apiFusionAddKey`，描述为新增的 `apiFusionKeyDialogDesc` 键，字段标签复用 `apiFusionKeyLabel`，占位符为 `apiFusionKeyLabelPlaceholder`。

## Alternatives considered

- 保留页头内联输入框、只改写其文案：未采纳，因为创建表单会继续留在列表面板页头，名称状态、其校验与重置也会留在 `LocalKeyList.tsx`，使创建交互继续与列表渲染耦合。
- 连 Key 值也放进对话框收集：未采纳，因为创建流程的既有契约是新建本地 Key 只需名称、密钥由后端生成（[New Local API Keys Never Persist the Mask Placeholder](../bug-fix/2026-09-17-api-key-mask-sentinel-never-persisted.md)），值输入框会被忽略。
- 字段继续沿用 `Label` / `标签` 文案：未采纳，因为该字段请求的是 Key 的名称而创建流程除此之外不需要别的，新的 `Name` / `名称` 标签与 `Key name` / `密钥名称` 占位符才准确描述该字段。

## Consequences

- `Api Keys` 页头不再有名称输入框，`Add key` 在面板不忙时始终可用；点击只打开对话框，因此此前「无名称即禁用」的按钮状态、回车提交路径与提交后内联清空的行为都不再存在。
- `LocalKeyDialog.tsx` 持有名称状态，每次打开时清空，名称去空白后为空时禁用 `Save`，面板忙时两个按钮都禁用；不保存直接关闭对话框不改变任何内容。
- `LocalKeyList` 的契约由 `onSave` 改为 `onAdd`，`api-fusion` 的入口 `src/components/ApiFusion/index.tsx` 持有 `isKeyDialogOpen`，并把对话框的 `onSave` 转交给既有 `handleSaveKey`，后者仍调用 `api_fusion_upsert_key`。
- 提交载荷不变（`id: ""`、去空白的名称、`value: ""`、`enabled: true`、`created_at: 0`），因此后端继续返回生成的 `sk-fusion-` 密钥。
- `src/i18n.ts` 现在为两种语言提供 `apiFusionKeyLabel` = `Name` / `名称`（原 `Label` / `标签`）、`apiFusionKeyLabelPlaceholder` = `Key name` / `密钥名称`（原 `Label` / `标签`），以及新增的 `apiFusionKeyDialogDesc`。
- `src/components/ApiFusion/ApiFusion.test.tsx` 中的行为测试断言页头没有名称输入框、`Add key` 在无名称时可用、点击后对话框打开、对话框没有 Key 值字段、`Save` 在输入名称前禁用，并断言发送的 `api_fusion_upsert_key` 载荷精确匹配。
- `LocalKeyDialog.tsx` 已登记进 `api-fusion` 的 `related_files` 与 `read_scope`，`navigation.md` 由 `navigation.json` 重新生成，且 `ai-workflow context validate` 通过；由于 `context validate` 会拒绝在被校验根中不存在的索引路径，仅存在于 worktree 的文件不能从单源索引引用。
- 没有任何决策被取代：后端掩码防护与终端同步契约不受影响。[New Local API Keys Never Persist the Mask Placeholder](../bug-fix/2026-09-17-api-key-mask-sentinel-never-persisted.md) 的创建流程句子现在同样把名称收集归到 `src/components/ApiFusion/LocalKeyDialog.tsx`，因此两条记录在创建流程上存在重叠，并陈述相同的归属。
- `MEMORY.md` 的本地 Key 标准（新建本地 Key 只需名称、值由后端生成）与本行为一致，因此保持不变。
