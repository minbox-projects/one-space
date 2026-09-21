# Agent Note: Gateway Synced Model Names Cover the Whole Identifier

Status: implemented

[English](2026-09-20-gateway-synced-model-name-completion.md) | 中文

## Problem

模板同步模型清单时，`parse_model_list_source` 把对象条目的源 `name` 按原样存为模板模型的 `display_name`。上游名称常短于其标识：`poolside/laguna-s-2.1-free` 的源名为 "Laguna S 2.1"，于是网关模型列表与后续终端同步都显示 "Laguna S 2.1"，丢掉了用以区分的 `free` 后缀——以及其他仅存在于标识中的部分，如 `paid`、`:free`、日期后缀与尺寸后缀。存储的名称因此不能代表完整的上游标识，而「仅当未被改动才更新」的传播规则又把这个不完整的名称忠实扩散到派生服务商。

## Decision

模型清单同步现在存储覆盖整个标识的本地名称。`parse_model_list_source` 旁边的 `complete_model_display_name`（`src-tauri/src/api_gateway/templates.rs`）接收基名与上游标识：去空白后的源 `name` 为基，源名缺失或空白时回退到同一标识之前已存的显示名，两者皆无时则直接存储变换后的标识段。覆盖比较把标识最后一个 `/` 段（该段为空时取整个标识）与基名按大小写不敏感的 ASCII 字母数字序列匹配；已消费前缀视为已表达，从最后一个已消费字母数字之后开始的原始剩余部分经变换——丢弃前导分隔符、分隔符连续段变单个空格、每个小写词首大写、`2.1` 等版本号保持完整——后以单个空格追加。已覆盖该段的基名按去空白后原样存储，保留其大小写、标点与 `(latest)` 等额外内容；最后一个 `/` 之前的厂商前缀绝不追加。同一模型清单响应解析两次得到字节一致的名称。该规则除 `display_name` 外不增加任何写入：`local_model`、协议处理、启用标志、价格行、忽略集合与单次加密 `write_config` 原子落盘保持原样；无同步的加载不改写任何已存名称；派生映射仅在既有「仅当未被改动才更新」规则下取补全后的名称。[Provider Templates Drop Built-in Model Catalogs and Prices](../architecture/2026-09-19-provider-template-manual-model-sync.md) 已就其「源 `name` 即显示名」的已交付事实原地修正，其余决策继续有效。

## Alternatives considered

- 无条件变换每个标识并忽略源名：未采纳，因为端点才是官方名称的作者；丢弃源名会丢失标识未携带的大小写、标点与额外内容，而实时目录表明完整的源名必须原样保留。
- 在加载或启动时改写已持久化的名称：未采纳，因为加载不得迁移数据；名称只在所属模板下次同步时变化，写入边界仍为模板状态与派生服务商。
- 保留源名永久优先并按原样存储：未采纳，因为正是该规则丢掉了 `free`、`paid`、`:free`、日期与尺寸部分；端点仍是协议的权威，但显示名必须表达整个标识。

## Consequences

- 报告的用例现已补全：`poolside/laguna-s-2.1-free` 以源名 "Laguna S 2.1" 存为 "Laguna S 2.1 Free"，本地模型 id 仍为 `poolside/laguna-s-2.1-free`。
- 完整的源名除去空白外按字节原样保留，厂商前缀与重复词绝不追加。
- 源名缺失或空白时，用已存本地名对标识补全；两者皆无时则直接存储变换后的标识段。
- 同一响应同步两次得到字节一致的名称，无同步的加载不改变任何已存名称。
- 行为覆盖位于 `src-tauri/src/api_gateway/tests/templates.rs`，包括 2026-09-20 从 `https://api.commandcode.ai/provider/v1/models` 抓取的 71 条实时载荷夹具。
- `MEMORY.md` 与 `navigation.json` 的 `api-gateway` / `api-gateway-backend` 条目在同一变更中陈述补全名称规则，`navigation.md` 已按权威 JSON 重新生成。
