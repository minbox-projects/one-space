# 04 - 实现短链接工具页面

- task_id: `short-link-tool-ui`
- order: `04`
- blocked_by: `rust-short-link-backend, short-link-client-history, short-link-navigation-contracts`
- source_plan: `../plan.md`
- source_plan_digest: `bd1294e58912b10cedf6e2f834807758b2ebaf29b9f2f71d80a8caaa70a145e4`
- write_scope: `src/components/ShortLinkTool.tsx；src/components/ShortLinkTool.test.tsx；src/test/mocks/tauri.ts（仅新增四个短链接命令 mock 能力）`

## Outcome

用户可在独立 Short Link 页面安全配置 Token、生成和复制短链接，并管理最近 50 条本地历史；全部失败状态均提供可区分且不泄密的反馈。

## Implementation Checklist

- [ ] 使用现有工具页、Toast、确认对话框和 Lucide 控件约定实现 `ShortLinkTool`，不新增前端依赖。
- [ ] 页面加载时并行读取 Token 配置状态和本地历史，一项失败不阻止另一项显示。
- [ ] 未配置时展示页内 Token 配置；已配置时只展示状态、替换和删除操作，不回填旧 Token。
- [ ] Token 输入默认使用 `password` 类型，通过眼睛图标切换显示；保存成功后立即清空组件内明文。
- [ ] 空白 Token 在前端阻止保存；删除 Token 前使用现有确认交互，删除后保留历史。
- [ ] 使用结构化 URL 解析，仅允许具有有效主机的 HTTP(S) URL；无效输入不调用创建命令。
- [ ] 提交期间锁定生成操作以防重复请求，失败或成功后可靠释放进行中状态。
- [ ] 失败保留输入；输入变化不清除最近一次成功结果。
- [ ] 成功时先展示 `{ longUrl, shortUrl }`，再写入历史；历史保存失败不得移除当前结果，并提示未持久保存。
- [ ] 当前结果和历史项均提供复制操作，并分别反馈复制成功或失败；复制失败不删除结果或历史。
- [ ] 按时间倒序展示最多 50 条历史，支持删除单条及确认后清空全部。
- [ ] 历史损坏时展示一次恢复提示；历史读写失败时展示准确状态，不伪报操作成功。
- [ ] `not_configured` 自动展开配置区；其余稳定错误代码通过 i18n 映射到可区分反馈，不依赖后端 `message` 分支。
- [ ] 删除和清空交互明确为本地记录操作，不调用 TinyURL 删除接口。
- [ ] 扩展 Tauri 测试 mock，并覆盖计划规定的组件与历史核心交互。

## Acceptance Criteria

- [ ] `[SC-2]` Token 可保存、替换和删除，默认遮罩；旧明文从不回填，保存后组件状态不保留明文。
- [ ] `[SC-3]` 合法 HTTP(S) URL 可生成并展示短链接；空白、非 HTTP(S)、相对或无主机输入不触发 IPC。
- [ ] `[SC-4]` 请求进行中不能重复提交；成功结果保留原始链接和短链接；当前结果复制有明确成功/失败反馈。
- [ ] `[SC-5]` 历史重载恢复、最新在前、最多 50 条，并支持复制、删除单条和确认清空。
- [ ] `[SC-6]` 删除历史不调用远端命令，界面不暗示远端链接失效；删除 Token不删除历史。
- [ ] `[SC-7]` 九个后端错误、剪贴板失败、历史读取/损坏/写入失败均产生可区分反馈，反馈不包含 Token。
- [ ] 配置状态和历史并行加载，任一失败时另一部分仍可使用。
- [ ] 历史持久化失败后，本次成功结果仍可查看和复制。
- [ ] 前端测试明确断言配置状态响应不含 Token，且不使用真实 TinyURL 服务。

## Verification Steps

- [ ] 运行 `npm run test -- src/components/ShortLinkTool.test.tsx`，全部组件测试通过。
- [ ] 运行 `npm run build`，确认组件、IPC 类型和 i18n key 无类型错误。
- [ ] 检查 Tauri mock 调用断言，确认无效 URL、历史删除和历史清空均未调用 `short_link_create` 或任何远端删除命令。

## Out of Scope

不接入 More Tools Hub、Launcher 或 App 外壳；不实现 alias、统计、二维码、远端撤销、历史加密或同步。
