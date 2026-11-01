# Agent Note: Beta Release Channel with a Stable Fallback

Status: implemented

[English](2026-11-01-beta-release-channel.md) | 中文

## Problem

OneSpace 过去只能发布稳定版本。维护者没有受支持的方式在不推进稳定版本线的前提下，为即将发布的版本交付多个测试构建；用户无法通过常规更新器选择这些构建；Beta 安装与稳定安装也没有区别。Beta 通道还必须使用生产数据路径而不是独立 profile，并且实验性的更新查询绝不能使用户失去稳定通道，也绝不能提供降级。

## Decision

发布助手在四个发布文件 `package.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml` 与 `src-tauri/Cargo.lock` 上新增 `beta <base> [--dry-run]`。基版本是显式的 `X.Y.Z`；当前稳定版低于基版本时从 `<base>-beta.1` 开始，当前版本已是同一基版本的 Beta 时递增其编号；非法或非基版本目标、等于或低于当前稳定版的基版本、低于当前 Beta 基版本的基版本，以及发布文件不一致都被拒绝且不写入。`--dry-run` 只报告下一个版本。既有的 `tag` 操作接受由此得到的预发布版本。

`previous-tag` 与 `is-prerelease` 向发布工作流暴露发布范围。Beta 标签 `v<base>-beta.N` 以 GitHub prerelease 发布；稳定发布以上一个稳定标签为基准生成更新说明并跳过 Beta，Beta 发布则以上一个标签为基准。

Beta 发布构建解析与稳定版相同的 `~/.config/onespace` 目录、配置与数据。不存在 Beta profile 或迁移，开发 profile `~/.config/onespace-dev` 保持不变。`receive_beta_updates` 是设备本地的可选配置字段，默认 false，同一设备上的稳定版与 Beta 版共享，不跨设备同步，旧构建会忽略它。

设置 → 更新提供「接收 Beta 更新」。开启后，更新检查选择版本最高、非草稿且发布 `latest.json` 资源的 GitHub 发布（包含预发布），先经既有的签名校验更新器检查该发布，再依次回退到配置的稳定端点；查找失败、没有可用发布，或所选 Beta 缺少当前平台或检查出错时，稳定检查仍然保留。关闭时只使用稳定端点，任何路径都不降级用户。

`src/lib/appVersion.ts` 暴露 `useAppVersion`（失败安全的运行时版本 hook）与 `isBetaVersion`（只匹配 `X.Y.Z-beta.N`）。主外壳在 Beta 运行时版本的左上品牌区渲染 BETA 徽标，展开与折叠侧边栏均显示；稳定、缺失或格式错误的版本不渲染徽标。前端回退比较把稳定版排在其自身 Beta 之上，并把更高的 Beta 编号排在更低者之上。

## Alternatives considered

- 独立的 Beta 应用目录、配置 profile 或数据迁移：未采纳，因为 Beta 构建必须使用生产数据路径，而第二份 profile 会为没有用户收益的目标引入迁移与双份数据风险。
- 自动加入 Beta 或仅面向 Beta 的偏好：未采纳，因为该偏好保持显式且默认关闭，并且同一设备偏好必须同时约束两种构建。
- 只按版本选择 Beta 发布或忽略 `latest.json`：未采纳，因为草稿、非法标签和没有可用清单的发布会成为更新端点。
- 用一次合并的 Beta 与稳定端点检查代替顺序的第二次检查：未采纳，因为缺少当前平台的 Beta 发布或 Beta 检查错误绝不能阻塞有效的稳定更新。
- 从重复的构建期版本来源推导徽标：未采纳，因为运行时 `getVersion()` 值是唯一来源，且查找失败绝不能抛入外壳。

## Consequences

- Beta 标签是合法的语义化版本预发布并且保持加性：配置的稳定端点、签名校验、产物命名与签名证书均不变，从不开启该偏好的稳定客户端看不到差异。
- 该偏好在一台设备上由稳定版与 Beta 版共享且不同步，因此开启 Beta 提供会同时影响该机器上的两个安装；旧构建接受并忽略该字段。
- 更新可用性取决于 GitHub Releases API 与每个所选发布的 `latest.json`；中断、限流、空结果或平台缺口都会回退到稳定端点且不降级。
- Beta 徽标只依赖运行时版本，因此格式错误或缺失的版本不显示任何徽标，也绝不破坏外壳。
- 行为覆盖位于发布助手的 shell 测试、前端徽标、偏好与比较测试，以及后端的选择、回退与配置测试；文档、导航索引与本 Note 经项目命令校验。
