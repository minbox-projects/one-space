# Agent Note: Gateway Model Prices Move Into Provider Mappings

Status: implemented

[English](2026-09-19-gateway-model-prices-in-provider-mappings.md) | 中文

## Problem

模型价格此前存于单张 `model_prices` 表，该表已带可选的 `provider_id`，维护入口位于「用量统计」页签内的 `ModelPriceDialog`。两个服务商提供同一个上游模型时只能共用一条价格；`provider_id` 缺省的全局行会为任何转发该模型名的服务商计价；当本次转发的服务商没有自己的价格行时还会回退到任意服务商的价格行，因此为某一个服务商配置的价格可能悄悄为另一个服务商的流量计价，入口也远离它所计价的映射。运营方要求价格只属于某一个服务商加某一个上游模型、紧邻其映射行编辑，并随服务商一起保存；同时既有全局行对新请求仍须保有含义，已记录金额必须保持冻结。

## Decision

价格改为在新增/编辑上游服务商对话框内按每条映射行维护。`MappingPriceEditor` 在映射行的详情区内渲染四档单价（输入 / 缓存读 / 缓存写 / 输出，美元/百万 tokens）与可选的峰谷配置，该详情区默认收起，由该行展开箭头（`data-testid="api-gateway-mapping-expand-{index}"`，`aria-expanded` 同步，无障碍名称 `apiGatewayMappingDetails`——「映射详情」）与推理档位一同展开；行收起时，编辑器与推理档位均不渲染，且编辑器仅在上游模型非空时渲染；共享同一上游模型的多行编辑同一份草稿，四档全空表示该模型未定价、不产生价格行，任一档有值即为已定价且空档按 0 计（四档都显式填 0 时价格为 `0.0000`），峰谷时段保持既有的 UTC+8 语义（可选星期集合，`0` 为周日、`6` 为周六，缺省或空表示每天，首个命中时段生效）。保存服务商时把服务商与其完整价格行集合在一次 `api_gateway_upsert_provider` 调用中提交，后端在一次原子 `write_config` 中替换恰好该服务商的价格行；不带价格列表的保存保持该服务商的价格行不变，`api_gateway_delete_provider` 同时删除其价格行，删除映射时其价格行也随之移除。

金额在记录时只按本次实际转发的服务商自身的价格行计算：`match_price_for_provider` 要求服务商 id 与上游模型名都精确且大小写敏感地匹配，全局与跨服务商回退均已移除，因此属于其他服务商或不带 `provider_id` 的行绝不为请求计价。版本门控迁移在读取早于当前 schema 版本的配置时一次性归一化价格表，并在同一次原子改写中持久化（[Gateway Migration Is Permanent and Version-Gated](2026-09-24-version-gated-gateway-migration.md)）：模型等于某服务商非空映射 `upstream_model` 或去空白后的 `default_model` 的全局价格行，在该服务商尚无该模型价格行时迁移为该服务商的专属价格行；其余全局行一律删除；服务商已不存在、或模型对该服务商不可达（既不是映射上游模型也不是默认模型）的行一律丢弃，手动服务商同样适用。每次写入都会经 `write_config` 应用同一归一化并盖上当前 schema 版本。已记录金额保持冻结：迁移与改价只影响新请求，用量 SQLite schema 与历史不受影响。

默认模型改为在该服务商去重后的非空映射上游模型加一个空选项之间选择。打开对话框时，若已加载的 `default_model` 未被任何映射覆盖，则物化为一条可见且标记为自动新增的映射行（`local_model` = `upstream_model`、启用、协议继承），并在该服务商已有该模型的专属价格行时携带该价格行；默认选择与转发目标保持不变，不删除该行直接保存即会持久化该映射与其价格。

## Alternatives considered

- 保留「用量统计」页签内紧邻分析的模型价格入口：未采纳，因为那个视图无法把价格绑定到真正转发该模型的服务商，映射行才是「服务商 + 上游模型」这一对唯一无歧义的位置，且保留两处入口会让同一份价格可以从两个界面编辑。
- 把价格嵌入映射 schema 本身（例如在每个 `ModelMapping` 上放一个 `price` 对象）：未采纳，因为重复映射可能共享同一上游模型并因此持有彼此分叉的副本，删除与恢复将不得不按映射行而非按服务商维护价格行，而现有 `model_prices` 列表已经以「服务商 + 上游模型」为键保存行且无需 schema 迁移。
- 为没有服务商认领的价格行保留隐形的全局回退：未采纳，因为一个会悄悄为其他服务商流量计价的隐藏回退正是本次要移除的缺陷；无法匹配的全局行改为迁移或删除，而新的入口在价格生效的位置可见。

## Consequences

- 每一条生效的价格行都带服务商 id，只有本次实际转发服务商自己的精确价格行为请求计价，因此一个服务商的价格不再能为另一个服务商的流量计价，全局行也永远不能计价。
- 服务商对话框是唯一的价格入口：「用量统计」页签保留用量分析、`—` 显示与未定价提示但不再有任何价格按钮，`ModelPriceDialog`、`apiGatewayModelPricesGet` / `apiGatewayModelPricesSave` 封装以及 `api_gateway_model_prices_get` / `api_gateway_model_prices_save` 命令均已删除。
- 迁移版本门控且只执行一次：读取旧配置时每条全局行为每个尚无该模型价格行的可达服务商生成一条专属行，无法匹配或不可达的行包括手动服务商在内一律删除，归一化结果在同一次改写中持久化；当前版本的读取不归一化也不写入，改写失败时保留此前完整字节供下一次读取重试。服务商默认模型的专属价格行会因携带它的自动新增映射行而挺过物化保存。
- 保存原子且精确：一次 `api_gateway_upsert_provider` 调用替换恰好该服务商的价格行，新服务商提交的行绑定到生成的 id，空模型与重复上游模型被丢弃，删除映射或服务商时其价格行移除而其他服务商不受影响，不带价格列表时价格行保持不变。
- 模板绑定服务商保留其绑定与忽略集合生命周期，但任何模板相关流程都不再触碰价格：[Provider Templates Drop Built-in Model Catalogs and Prices](2026-09-19-provider-template-manual-model-sync.md) 移除了模板价格数据与同步价格传播，因此同步、创建与恢复绝不创建或修改任何价格行，新建或恢复的模板模型在操作者输入价格前为未定价，所有服务商专属价格行都在映射对话框中手动录入；删除映射仍会移除其价格行。
- 已记录历史保持冻结：迁移与改价绝不重算过往金额，用量 SQLite schema、记录字段与保留行为均不变。
- 回滚：代码回退无需数据迁移，因为旧构建仍优先匹配服务商专属价格行，新构建写入的行继续为新请求计价；已被迁移或删除的全局行不会自动恢复，运营方如需未绑定行仍可用旧弹窗重建；用量历史与 SQLite schema 不受影响。
- 部分取代：[API Gateway Usage Stats and Request Logs](2026-09-17-ai-gateway-usage-logs.md) 被保留并交叉链接；本记录只取代其价格入口与全局匹配决策，其 SQLite 日志、记录时价格冻结与保留决策仍然有效，其 `unpriced_count` 资格由 [Gateway Unpriced Hint Counts Only Billable Usage](../bug-fix/2026-09-21-gateway-unpriced-billable-usage.md) 部分取代。部分取代：[Gateway Migration Is Permanent and Version-Gated](2026-09-24-version-gated-gateway-migration.md) 只取代本记录读取时归一化并由下一次写入持久化的设计；按服务商精确匹配、映射对话框入口与金额冻结规则仍然成立。
- `MEMORY.md`、`docs/USAGE.md` 与 `navigation.json` 中的 `api-gateway` / `api-gateway-backend` 条目描述同一套入口、匹配、迁移、原子保存与默认模型行为，`navigation.md` 已按权威 JSON 重新生成。
