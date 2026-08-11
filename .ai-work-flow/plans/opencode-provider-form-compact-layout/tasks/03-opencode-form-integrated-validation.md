# 综合回归与桌面窄屏验收

## 预期结果

在密钥详情和模型表单两个工作包完成后，统一执行详情与模型定向测试、配置解析/校验/合并/保存回归和列表脱敏复查，并完成新增与编辑路径在桌面和窄屏下的实际界面截图验收。验收覆盖完整密钥与复制状态、默认折叠和展开编辑、卡片紧凑度、响应式重排及控件无重叠，仅修正本需求引入的回归。

## 实施清单

- [x] 确认 `opencode-detail-api-key` 与 `opencode-model-compact-editor` 已完成，再统一执行详情、模型表单、i18n、配置解析/校验/合并、表单/JSON 同步、保存顺序和列表脱敏相关测试。
- [x] 复查编辑详情完整密钥只来自目标 provider 的运行时配置，复制成功与失败状态准确，列表及列表接口仍保持顶层、嵌套与历史数据脱敏。
- [x] 复查模型专用文案、Options/Variants 默认折叠与独立展开、实时计数、折叠态添加、各层动态增删及配置形状和未知字段保留。
- [x] 启动实际界面，分别验收新增与编辑路径在桌面和窄屏视口下的密钥控件、默认折叠模型卡片和至少一个展开编辑状态，并检查紧凑度、响应式重排、键盘可操作性及文字、输入框、计数和按钮无重叠。
- [x] 生成四张固定路径的验收截图：`screenshot/opencode-provider-form-compact-layout-add-desktop.png`、`screenshot/opencode-provider-form-compact-layout-add-narrow.png`、`screenshot/opencode-provider-form-compact-layout-edit-desktop.png` 和 `screenshot/opencode-provider-form-compact-layout-edit-narrow.png`。
- [x] 仅在前两个任务涉及的实现与测试文件内修正本需求引入的回归；若发现范围外缺陷，记录但不顺带重构。

## 验收标准

- [x] 完整密钥、复制、专用文案、折叠、计数、动态增删、配置同步和保存相关自动化测试全部通过，列表脱敏安全边界无回归。
- [x] 新增与编辑路径的桌面和窄屏四种组合均完成实际界面验收，并存在四张可打开、内容非空且与对应路径和视口一致的截图。
- [x] 截图覆盖完整 API Key 详情控件、默认折叠模型卡片及至少一个展开编辑状态；桌面字段形成紧凑并排网格，窄屏字段按语义顺序重排。
- [x] 四种组合中均无文字、输入框、计数、折叠按钮或新增/删除按钮重叠、截断到不可辨识或不可操作，默认模型卡片纵向占用较改动前明显降低。
- [x] 本轮修正只涉及本需求引入的回归，不改变 OpenCode 配置持久化契约、通用翻译文案或其他 provider 行为。

## 验证步骤

- [x] 运行 `npm run test -- src/components/AiEnvironments/ServiceProviderDetail.test.tsx src/components/AiEnvironments/AiEnvironments.test.tsx src/components/AiEnvironments/opencodeModelConfig.test.ts src/i18n.test.ts`，预期所有 OpenCode 定向与回归测试通过。
- [x] 运行 `cargo test --manifest-path src-tauri/Cargo.toml opencode_config_read_tests`，预期运行时配置读取和列表脱敏测试通过。
- [x] 运行 `npm run lint` 与 `npm run build`，预期无新增 lint、类型或构建错误。
- [x] 逐一打开四张截图并核对新增/编辑身份、桌面/窄屏尺寸、完整密钥控件、默认折叠和展开编辑内容，预期图片非空、内容对应且控件无重叠。

## 范围外事项

- 不修复与本需求无关的既有缺陷，不扩大到其他 provider 详情或全局布局。
- 不修改持久化字段、后端接口契约、通用 `models` 翻译键或引入新依赖。
