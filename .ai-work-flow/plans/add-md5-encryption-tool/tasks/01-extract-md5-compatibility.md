# 01 - 建立共享 MD5 与设置页兼容基线

- task_id: `extract-md5-compatibility`
- order: `01`
- blocked_by: `none`
- source_plan: `../plan.md`
- source_plan_digest: `b3abab4c25876cd7f28dc2ccbf97da0bc079bc75b9885acfe3ff0226e35a8b29`
- write_scope: `src/lib/md5.ts, src/lib/md5.test.ts, src/components/SettingsView.tsx, src/components/SettingsView.test.tsx`

## Outcome

应用具备经过标准向量验证的共享 `md5Hex(input: string): string`，且设置页随机 MD5 密码生成与字段填充契约保持不变。

## Implementation Checklist

- [x] 新建 `src/lib/md5.ts`，从 `SettingsView` 提取 MD5 常量、位旋转、十六进制编码和摘要逻辑，仅导出 `md5Hex`。
- [x] 使用运行环境标准 UTF-8 编码能力处理 JavaScript 字符串；不得执行裁剪、换行改写或 Unicode 规范化，不得新增依赖或调用 Web Crypto MD5、网络及 Tauri/Rust 能力。
- [x] 如原算法未通过标准向量，在共享模块内修正，确保结果固定为 32 位小写十六进制。
- [x] 删除 `SettingsView` 内重复 MD5 实现并改为导入共享函数；保持 `generateRandomMd5String` 的 `crypto.randomUUID()`、`Date.now()`、两次 `Math.random()`、种子拼接顺序、UUID 分段及 `newPass`/`confirmNewPass` 填充流程不变。
- [x] 新增共享算法测试，覆盖空串、`a`、`abc`、中文、单个空格、制表符、LF、CRLF，以及 NFC/NFD 形式不同但视觉相同的 Unicode 输入。
- [x] 新增设置页回归测试，以确定性桩验证全部随机源及调用次数、生成值的 `8-4-4-4-12` 十六进制格式和两个设置字段填充值一致。
- [x] 完成本任务 checklist，并只提交 `write_scope` 内的实现与测试改动。

## Acceptance Criteria

- [x] `md5Hex` 的固定结果分别为：空串 `d41d8cd98f00b204e9800998ecf8427e`、`a` 为 `0cc175b9c0f1b6a831c399e269772661`、`abc` 为 `900150983cd24fb0d6963f7d28e17f72`、`中文` 为 `a7bac2239fcdcb3a067903d8077c4a07`。
- [x] 单个空格、制表符、LF、CRLF 的结果分别为 `7215ee9c7d9dc229d2921a40e899ec5f`、`5e732a1878be2342dbfeff5fe3ca5aa3`、`68b329da9893e34099c7d8ad5cb9c940`、`81051bcc2cf1bedf378224b0a93e2877`。
- [x] `é` 与 `e\u0301` 不被规范化，结果分别为 `66ddcd97cfdeabb2f6fb8a999b4bc76f` 与 `5526861fbb1e71a1bda6ac364310a807`；所有结果匹配 `/^[0-9a-f]{32}$/`。
- [x] `SettingsView` 不再包含 MD5 算法副本，并继续以原有随机源和调用约束生成 UUID 分段格式值，同时填入 `newPass` 与 `confirmNewPass`。
- [x] 本任务未增加 npm 依赖、后端能力、Tauri command、网络调用或托盘改动。

## Verification Steps

- [x] 按仓库现有 Vitest 调用方式运行 `src/lib/md5.test.ts`，预期全部标准向量与原样 UTF-8 测试通过。
- [x] 按仓库现有 Vitest 调用方式运行 `src/components/SettingsView.test.tsx`，预期随机种子、UUID 格式及字段填充回归测试通过。
- [x] 运行与上述文件对应的 TypeScript/lint 检查，预期无新增类型或 lint 错误。

## Out of Scope

不实现 MD5 工具界面、i18n、More Tools/Launcher/导航注册、可见性配置、索引或视觉验收，也不修改系统托盘代码。
