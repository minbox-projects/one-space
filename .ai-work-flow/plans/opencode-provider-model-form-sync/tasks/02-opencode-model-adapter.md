# 02 - 实现 OpenCode 模型无损转换与校验

- task_id: `opencode-model-adapter`
- order: `02`
- blocked_by: `none`
- source_plan: `../plan.md`
- source_plan_digest: `2614cd332f50cef408924acbcb05910d7b1cd1ffdefba3c2dca7361357f6b584`
- write_scope_mode: `exhaustive`
- write_scope:
  - `src/components/AiEnvironments/opencodeModelConfig.ts`
  - `src/components/AiEnvironments/opencodeModelConfig.test.ts`

## 预期结果

建立 OpenCode JSON、最后有效快照与模型表单之间的纯函数适配层及单元测试：支持 models、name、cost、limit、options、variants 的解析、字段级校验、确定性序列化和深度合并；保证未知合法 provider、模型及嵌套字段不丢失，并覆盖模型 ID、数值边界、自定义 JSON 值和空可选字段规则。依赖：无，可与任务 1 并行。

## 实施清单

- [x] 新建 `opencodeModelConfig.ts`，定义可编辑模型、cost、limit、动态 option 行、variant、字段错误及解析/校验结果类型，并保持模块为无 UI、无 I/O 的纯函数边界。
- [x] 实现 OpenCode JSON 字符串解析和结构校验：根值必须为对象，`models` 及其模型条目必须满足可转换结构；成功时返回可作为最后有效快照的深拷贝和表单值，失败时返回可定位的错误且不产生新快照。
- [x] 实现模型表单字段级校验：模型 ID 必填且唯一；启用 cost 后 `input`、`output` 必填且为非负数，空 `cache_read`/`cache_write` 不输出；启用 limit 后 `context`、`output` 必填且为正数。
- [x] 支持模型 `name`、动态 `options` 和 `variants` 的解析与回写；常见 option 可携带 string、number、boolean 类型信息，自定义 option 值必须能解析为合法 JSON 值，键冲突或无效值返回行级错误。
- [x] 以最后有效快照为合并基底，仅覆盖表单拥有的 `models`、`name`、`cost`、`limit`、`options`、`variants` 内容；深度保留 provider 顶层、模型条目及嵌套对象中表单无法表达的合法未知字段。
- [x] 实现稳定、格式化且确定性的 JSON 序列化，确保相同快照与表单输入得到相同输出，且不得增加 cost 币种或推断货币。
- [x] 新建纯函数测试，覆盖空/重复模型 ID、数值边界、cost/limit 省略、空 cache 字段、自定义 JSON 值、variants 覆盖、确定性输出、非法根结构及多层未知字段往返保留。

## 验收标准

- [x] 有效 OpenCode JSON 可转换为表单并无损合并回 JSON，表单不拥有的 provider、模型和嵌套字段逐层保持不变。
- [x] 所有模型 ID、cost、limit、option 和 variant 错误均可定位到对应字段或动态行，校验失败时不生成替代 JSON。
- [x] cost 和 limit 可完全省略；启用后的边界符合规格，空 cache 值不会写入序列化结果。
- [x] 自定义 option 支持任意合法 JSON 值，序列化结果确定且不写入任何推断币种。

## 验证步骤

- [x] 运行 `npx vitest run src/components/AiEnvironments/opencodeModelConfig.test.ts`，预期适配器解析、校验、合并和序列化测试全部通过。
- [x] 运行 `npx tsc -b --pretty false`，预期新增类型及调用边界通过 TypeScript 检查。
- [x] 运行 `npm run lint -- --quiet src/components/AiEnvironments/opencodeModelConfig.ts src/components/AiEnvironments/opencodeModelConfig.test.ts`；若项目 ESLint 脚本不接受文件参数，则运行 `npm run lint`，预期无新增 lint 错误。

## 范围外事项

- React 模型表单渲染、JSON 编辑器状态协调、复制入口和持久化流程接入。
- 穷尽定义所有 OpenCode provider/model options 或引入 cost 币种模型。

## 禁止事项

- 不得用空对象或表单默认值覆盖最后有效快照中的未知合法字段。
- 不得在 JSON 或表单无效时更新最后有效快照或生成可保存配置。
- 不得调用后端命令、访问组件状态，或修改 Rust、schema 与持久化契约。
