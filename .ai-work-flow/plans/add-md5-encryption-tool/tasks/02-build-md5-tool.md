# 02 - 实现 MD5 工具界面与交互

- task_id: `build-md5-tool`
- order: `02`
- blocked_by: `extract-md5-compatibility`
- source_plan: `../plan.md`
- source_plan_digest: `b3abab4c25876cd7f28dc2ccbf97da0bc079bc75b9885acfe3ff0226e35a8b29`
- write_scope: `src/components/Md5EncryptionTool.tsx, src/components/Md5EncryptionTool.test.tsx, src/i18n.ts`

## Outcome

独立 `Md5EncryptionTool` 可按用户显式操作计算并展示四种 MD5 格式，完整支持复制反馈、清空聚焦、中英文文案、可访问性和窄屏稳定布局。

## Implementation Checklist

- [ ] 新建 `Md5EncryptionTool.tsx`，复用现有 More Tools 详情页的 `section` 语义、Tailwind、Lucide、Clipboard、Toast 和 i18n 模式，并调用任务 01 的 `md5Hex`。
- [ ] 使用受控普通文本输入、`null` 未生成状态和包含四项值的完整结果对象；输入变化仅更新输入并保留旧结果。
- [ ] “加密”处理只调用一次 `md5Hex`，由 32 位小写值派生大写值和 `[8, 24)` 的 16 位值，并以一次状态更新整体替换四项结果。
- [ ] 渲染安全说明、输入标签、加密/清空操作、未生成空状态，以及固定顺序的 32 位小写、32 位大写、16 位小写、16 位大写结果行。
- [ ] 为四行分别提供固定尺寸的 Lucide 复制图标按钮和结果类型专属 `aria-label`；成功与失败显示对应 Toast，失败不得修改输入或结果。
- [ ] “清空”同步重置输入和结果，再通过输入引用恢复焦点；未生成时不得执行空值复制。
- [ ] 为结果文本设置可收缩、换行或横向容纳边界，为复制按钮设置不可压缩尺寸，避免窄屏页面级横向溢出、裁切或重叠。
- [ ] 在 `src/i18n.ts` 的英文和中文 `translation` 下以 `md5Encryption` 为功能段补齐标题、描述、安全说明、输入标签、操作按钮、四种结果标签、空状态及复制成功/失败反馈，组件不硬编码可见文案。
- [ ] 新增组件测试，覆盖显式计算、原子重算、空字符串、原样空白/Unicode、四项复制成功与失败、清空聚焦、语义与可访问名称，并验证双语关键 key 可解析。
- [ ] 完成本任务 checklist，并只提交 `write_scope` 内的实现与测试改动。

## Acceptance Criteria

- [ ] 初始状态与已计算空字符串状态可区分；初始无可执行复制，计算空字符串后显示 `d41d8cd98f00b204e9800998ecf8427e` 及其三项派生格式。
- [ ] 对任意一次计算，结果严格满足 `upper32 = lower32.toUpperCase()`、`lower16 = lower32.slice(8, 24)`、`upper16 = lower16.toUpperCase()`，四项同时更新。
- [ ] 输入首尾空格、制表符、LF/CRLF、中文及 NFC/NFD 序列按原始 UTF-8 计算；修改输入不清除旧结果，再次点击才替换结果。
- [ ] 四个复制按钮分别复制本行准确值，具有明确且不同的可访问名称；成功/失败 Toast 可见，失败后输入、四项结果和操作能力保持不变。
- [ ] 清空后输入和全部结果消失且焦点回到输入控件；输入标签正确关联，结果区域具有 `section` 语义，键盘可操作。
- [ ] 英文与中文所需 key 均存在且可解析，界面明确说明 MD5 是不可逆哈希且不适合密码存储或安全加密。
- [ ] 组件静态布局约束可保证摘要文本与固定尺寸复制按钮在窄屏中不相互覆盖。

## Verification Steps

- [ ] 按仓库现有 Vitest 调用方式运行 `src/components/Md5EncryptionTool.test.tsx`，预期全部行为、Clipboard、Toast、i18n 和可访问性测试通过。
- [ ] 运行任务 01 的 MD5 单元测试与本组件测试，预期组件派生值与共享算法结果一致。
- [ ] 对 `Md5EncryptionTool.tsx`、其测试和 `src/i18n.ts` 运行现有 TypeScript/lint 检查，预期无新增错误。

## Out of Scope

不注册 More Tools、Launcher、导航别名、展示元数据或可见性字段，不更新项目索引，不增加文件哈希、历史、持久化或其他摘要算法。
