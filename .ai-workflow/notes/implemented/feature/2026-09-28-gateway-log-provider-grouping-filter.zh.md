# Agent Note: Gateway Request Logs Group and Filter by Provider Name

Status: implemented

[English](2026-09-28-gateway-log-provider-grouping-filter.md) | 中文

## Problem

请求日志面板此前只能按模型或 UTC+8 自然日分组、按状态与模型筛选，无法回答流量实际由哪个上游服务商服务，也无法单独隔离某个服务商的请求。加入服务商分组与筛选带来既有契约没有回答的问题：每条日志行同时携带服务商 id 与记录时的服务商显示名，而显示名既不唯一也不稳定；筛选必须与既有的单选状态、模型筛选共存；驱动筛选的选项列表必须在筛选生效时可继续使用。变更还必须保持增量：不改存储 schema、不改记录形状、不改既有分组模式、筛选、分页或合计。

## Decision

服务商分组与筛选以每条日志行记录的服务商显示名（`provider_name`）为键，它是操作者视角下的上游身份。`group_by = "provider"` 按该记录名聚合终止行，因此共享同一显示名的两个服务商条目会合并为一个分组，而之后改名的服务商会拆分历史：新行形成新分组，旧行仍留在旧名的分组。每个分组给出请求数、错误数（只统计 `result = 'failure'` 的行）与最后请求时间，按最近请求倒序、再按名称升序排列；没有记录服务商的终止行形成一个空名分组，界面显示为 `—`。`group_by` 的可用词表变为 `none`、`model`、`provider` 与 `day`；其他取值继续返回可操作错误，错误文本现在列出全部四个值。

`ai_gateway_request_logs` 接受可选 `provider` 参数，按记录的 `provider_name` 精确匹配。空值与全空白值视为未设置；该选择器与生效的状态、模型筛选以 AND 组合；无匹配行时返回空页与零合计，而不是错误。不分组响应返回 `providers: string[]` 面：范围内去重、非空、按字母升序的服务商显示名。与既有 `models` 面一样，它覆盖整个已解析范围、独立于当前页，并忽略自身筛选以便在不先清除筛选的情况下改选，同时尊重生效的状态与模型筛选。`models` 面现在也尊重生效的服务商筛选；没有服务商筛选时其行为不变。两个面在分组响应中都保持为空，与既有模型列表契约一致。空名（无服务商的合成终止行）被排除在 `providers` 面之外，但在按服务商分组时仍作为 `—` 分组可见。

面板在分组选择器中加入 `服务商`，并在筛选浮层中加入 `服务商` 区，提供「全部服务商」选项与范围内每个服务商名一个选项，选项只来自响应的 `providers` 面。选择为单选；应用后同时限制不分组列表与分组聚合结果，并把该名称加入筛选触发标签、把分页重置到第 1 页；清除时一并重置状态、模型、服务商与分页。切换分组模式时服务商筛选保持生效。响应不带服务商列表时，该区只显示「全部服务商」，绝不从可见记录推导选项。双语键为 `aiGatewayGroupProvider`、`aiGatewayFilterProvider` 与 `aiGatewayFilterAnyProvider`。

## Alternatives considered

- 用 `provider_id` 而非记录的服务商显示名作为分组与筛选键：未采纳，因为显示名是操作者视角的身份、日志行已固化其被服务时的名称，而按 id 分组会把共享同一可见名的两个服务商条目拆开，并让服务商条目后来被删除的行失去可读标签；显示名的合并/改名取舍被接受并由交付行为固定。
- 让服务商筛选支持多选：未采纳，因为同一浮层中的状态与模型条件是单选，集合筛选需要 `IN` 子句以及自己的触发标签、「全部」与清除语义，且规格有意把服务商筛选限定为与其他筛选相同的单选。
- 从可见记录或当前页推导服务商选项：未采纳，因为行全部位于当前页之外的服务商也必须可选；选项来自响应的 `providers` 面，面板绝不从可见记录推导——面缺失时只显示「全部服务商」。
- 让服务商面尊重生效的服务商筛选：未采纳，因为当前选择会从选项中消失，必须先清除筛选才能改选；该面像模型面一样忽略自身筛选。
- 把空名作为可选项提供：未采纳，因为无服务商的合成终止行不是上游服务商、也永远无法匹配服务商筛选；它只在按服务商分组时作为 `—` 分组可见。

## Consequences

- 命令契约以增量方式扩展：可选 `provider` 参数、`"provider"` 分组值、不分组 `providers` 面、模型面尊重服务商筛选、分组响应返回空面。省略 `provider` 并使用既有分组值的调用方观察到不变行为；不改变任何已存行、列、schema 版本或迁移，回滚即还原。
- 实现：`LogFilter` 新增 `provider`，`bind` 在共享谓词中加入精确的 `provider_name = ?` 子句；`group_logs` 新增服务商分支；`query_logs` 计算 `models` 面时保留服务商筛选、计算 `providers` 面时清除服务商筛选；`ai_gateway_request_logs` 接受并 trim 该参数。前端 `UsageGroupBy` 新增 `"provider"`，`UsageLogsQuery` 新增可选 `provider`，`UsageLogsPage` 新增可选 `providers`，`aiGatewayRequestLogs` 转发该键，`UsageLogsPanel` 新增选项、单选选项片、触发标签与清除/重置行为。
- 同名服务商条目合并为一个分组，改名会拆分历史；这是被记录的语义，由 `MEMORY.md`、`docs/USAGE.md` 与 `ai-gateway` / `ai-gateway-backend` 导航条目记录，`navigation.md` 按权威 JSON 重新生成。
- 其他都不变：用量统计面板、请求日志记录形状与字段、SQLite schema 与保留、分页、合计、状态与模型筛选本身，以及 `none` / `model` / `day` 分组输出都保持当前语义。
- Supersession: partial. [API Gateway Usage Stats and Request Logs](../architecture/2026-09-17-ai-gateway-usage-logs.md) 曾把 `group_by` 词表记录为 `"model"` 与 `"day"`、把模型面记录为只尊重已解析范围与状态筛选；这两处表述由本记录取代，而它的 SQLite 存储、按尝试记录、记录时价格冻结、保留与范围选择器决策仍然有效。本记录还为 [API Gateway Usage Queries Replace Day Counts with a Named Range Selector](2026-09-22-gateway-usage-yesterday-range.md) 记录的 `ai_gateway_request_logs` 签名加上可选 `provider` 参数；其范围选择器决策仍然有效。
