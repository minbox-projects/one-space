import type { Dispatch, RefObject, SetStateAction } from "react";
import {
  ArrowDownUp,
  ChevronDown,
  ChevronUp,
  Layers,
  Search,
  ShieldAlert,
  SlidersHorizontal,
  Sparkles,
  Trash2,
  X,
} from "lucide-react";
import type { TFunction } from "i18next";
import {
  CANONICAL_REASONING_EFFORTS,
  expandModelPoolFallbackModel,
  formatContextWindowDisplay,
  resolveMappingContextWindow,
  type GatewayUpstreamProvider,
} from "@/lib/aiGateway";
import { protocolBadgeClass, protocolShortLabel } from "../gatewayShared";

type FallbackModelsSectionProps = {
  t: TFunction;
  providers?: GatewayUpstreamProvider[];
  defaultEffort: string;
  onDefaultEffortChange: (value: string) => void;
  fallbackModels: string[];
  fallbackQuery: string;
  setFallbackQuery: Dispatch<SetStateAction<string>>;
  fallbackDropdownOpen: boolean;
  setFallbackDropdownOpen: Dispatch<SetStateAction<boolean>>;
  fallbackComboboxRef: RefObject<HTMLDivElement | null>;
  fallbackInputRef: RefObject<HTMLInputElement | null>;
  fallbackProviderCounts: Map<string, number>;
  fallbackModelOptions: string[];
  filteredFallbackModelOptions: string[];
  filteredFallbackModels: string[];
  onAddFallbackModel: (fallbackModelId: string) => void;
  onMoveFallbackModel: (index: number, delta: number) => void;
  onRemoveFallbackModel: (fallbackModelId: string) => void;
  onClearFallbackModels: () => void;
  busy: boolean;
  saving: boolean;
};

function getFallbackModelMeta(
  providers: GatewayUpstreamProvider[] | undefined,
  modelId: string,
) {
  if (!providers || providers.length === 0) {
    return { protocols: [], contextWindow: null };
  }
  const candidates = expandModelPoolFallbackModel(providers, modelId);
  const protocols = Array.from(
    new Set(candidates.map((c) => c.protocol).filter(Boolean)),
  );
  const contextWindows = candidates
    .map((c) =>
      resolveMappingContextWindow(
        providers,
        c.provider_id,
        c.local_model,
        c.upstream_model,
      ),
    )
    .filter((cw): cw is number => typeof cw === "number" && cw > 0);
  const contextWindow =
    contextWindows.length > 0 ? Math.min(...contextWindows) : null;
  return { protocols, contextWindow };
}

export function FallbackModelsSection({
  t,
  providers,
  defaultEffort,
  onDefaultEffortChange,
  fallbackModels,
  fallbackQuery,
  setFallbackQuery,
  fallbackDropdownOpen,
  setFallbackDropdownOpen,
  fallbackComboboxRef,
  fallbackInputRef,
  fallbackProviderCounts,
  fallbackModelOptions,
  filteredFallbackModelOptions,
  filteredFallbackModels,
  onAddFallbackModel,
  onMoveFallbackModel,
  onRemoveFallbackModel,
  onClearFallbackModels,
  busy,
  saving,
}: FallbackModelsSectionProps) {
  return (
    <div className="rounded-2xl border border-border/70 bg-card/60 p-4 sm:p-5 shadow-2xs space-y-4">
      {/* 模块主头部 */}
      <div className="flex items-center justify-between gap-3 border-b border-border/60 pb-3.5">
        <div className="flex items-center gap-2.5 min-w-0">
          <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-xl border border-primary/25 bg-primary/10 text-primary shadow-2xs">
            <SlidersHorizontal className="h-4 w-4" />
          </div>
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <span className="text-xs font-semibold text-foreground">
                高级路由策略与故障降级
              </span>
              <span className="rounded-full bg-primary/10 px-2 py-0.5 text-[10px] font-medium text-primary">
                Advanced
              </span>
            </div>
            <p className="text-[11px] text-muted-foreground mt-0.5 line-clamp-1">
              配置全局默认推理映射偏好，以及所有主候选模型不可用时的有序兜底降级链条
            </p>
          </div>
        </div>
      </div>

      {/* 策略 1：默认推理强度偏好卡片 */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 rounded-xl border border-border/60 bg-muted/20 p-3.5 transition-colors">
        <div className="flex items-start gap-2.5 min-w-0">
          <div className="mt-0.5 flex h-6 w-6 shrink-0 items-center justify-center rounded-lg border border-border/70 bg-background/80 text-primary shadow-2xs">
            <Sparkles className="h-3.5 w-3.5" />
          </div>
          <div className="min-w-0 space-y-0.5">
            <div className="flex items-center gap-2">
              <span className="text-xs font-semibold text-foreground">
                {t("aiGatewayModelPoolReasoningDefaultLabel", "Reasoning default")}
              </span>
              <span className="rounded-md border border-primary/20 bg-primary/10 px-1.5 py-0.2 font-mono text-[10px] font-medium text-primary">
                {defaultEffort}
              </span>
            </div>
            <p className="text-[11px] text-muted-foreground leading-relaxed">
              当上游请求未显式指定 reasoning effort 时模型池采用的基准偏好
            </p>
          </div>
        </div>

        <div className="flex items-center gap-2 shrink-0 self-start sm:self-center">
          <select
            data-testid="model-pool-reasoning-default"
            aria-label={t(
              "aiGatewayModelPoolReasoningDefaultLabel",
              "Reasoning default",
            )}
            value={defaultEffort}
            onChange={(event) => onDefaultEffortChange(event.target.value)}
            disabled={busy || saving}
            className="h-8.5 rounded-lg border border-border/80 bg-background px-3 text-xs font-medium text-foreground shadow-xs outline-none transition hover:border-border focus:border-primary/60 focus-visible:ring-2 focus-visible:ring-primary/20 disabled:opacity-50 cursor-pointer"
          >
            <option value="auto">
              {t(
                "aiGatewayModelPoolReasoningDefaultAuto",
                "Provider default (auto)",
              )}
            </option>
            {CANONICAL_REASONING_EFFORTS.map((effort) => (
              <option key={effort} value={effort}>
                {effort}
              </option>
            ))}
          </select>
        </div>
      </div>

      {/* 策略 2：有序故障兜底降级链条卡片 */}
      <div className="rounded-xl border border-border/60 bg-card/40 p-3.5 sm:p-4 space-y-3.5">
        {/* 标题栏与清空按钮 */}
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div className="flex items-center gap-2">
            <div className="flex h-6 w-6 items-center justify-center rounded-md bg-muted/60 text-muted-foreground border border-border/60">
              <ArrowDownUp className="h-3 w-3" />
            </div>
            <span className="text-xs font-semibold text-foreground">
              {t("aiGatewayModelPoolFallbackLabel", "Fallback models")}
            </span>
            <span className="rounded-full bg-muted px-2 py-0.5 text-[10px] font-medium text-muted-foreground">
              {fallbackModels.length}
            </span>
          </div>

          {fallbackModels.length > 0 ? (
            <button
              type="button"
              onClick={onClearFallbackModels}
              disabled={busy || saving}
              aria-label={t(
                "aiGatewayModelPoolFallbackClear",
                "Clear fallback models",
              )}
              className="inline-flex h-7 shrink-0 items-center gap-1 rounded-lg border border-border/80 bg-background px-2.5 text-[11px] font-medium text-muted-foreground transition hover:border-destructive/30 hover:bg-destructive/10 hover:text-destructive disabled:opacity-50 cursor-pointer"
            >
              <Trash2 className="h-3 w-3" />
              <span>
                {t(
                  "aiGatewayModelPoolFallbackClear",
                  "Clear fallback models",
                )}
              </span>
            </button>
          ) : null}
        </div>

        <p className="text-[11px] leading-relaxed text-muted-foreground">
          {t(
            "aiGatewayModelPoolFallbackHint",
            "Ordered fallback models are used only after every regular candidate is exhausted.",
          )}
        </p>

        {/* 搜索与添加下拉框 */}
        <div className="space-y-1.5">
          <div className="flex items-center justify-between text-xs font-medium text-foreground">
            <span className="text-[11px] text-muted-foreground">
              {t(
                "aiGatewayModelPoolFallbackAddLabel",
                "Add fallback model",
              )}
            </span>
            {fallbackQuery.trim() ? (
              <span
                data-testid="model-pool-fallback-match-count"
                className="text-[10px] text-muted-foreground font-normal"
              >
                {t(
                  "aiGatewayModelPoolFallbackMatchCount",
                  "{{count}} fallback models matching",
                  { count: filteredFallbackModelOptions.length },
                )}
              </span>
            ) : null}
          </div>

          <div ref={fallbackComboboxRef} className="relative w-full">
            <div className="relative flex items-center">
              <Search className="pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground/70" />
              <input
                ref={fallbackInputRef}
                type="search"
                data-testid="model-pool-fallback-search"
                aria-label={t(
                  "aiGatewayModelPoolFallbackComboboxAria",
                  "Search fallback models to add",
                )}
                aria-expanded={fallbackDropdownOpen}
                value={fallbackQuery}
                onFocus={() => setFallbackDropdownOpen(true)}
                onChange={(event) => {
                  setFallbackQuery(event.target.value);
                  if (!fallbackDropdownOpen) setFallbackDropdownOpen(true);
                }}
                onKeyDown={(event) => {
                  if (event.key === "Escape") {
                    setFallbackDropdownOpen(false);
                  } else if (
                    event.key === "Enter" &&
                    filteredFallbackModelOptions.length > 0
                  ) {
                    event.preventDefault();
                    onAddFallbackModel(filteredFallbackModelOptions[0]);
                    setFallbackQuery("");
                    setFallbackDropdownOpen(false);
                  }
                }}
                placeholder={t(
                  "aiGatewayModelPoolFallbackComboboxPlaceholder",
                  "Search or select fallback model to add...",
                )}
                disabled={busy || saving}
                className="h-9 w-full rounded-lg border border-border/80 bg-background pl-9 pr-16 text-xs text-foreground shadow-xs outline-none transition focus:border-primary/60 focus-visible:ring-2 focus-visible:ring-primary/20 disabled:opacity-50"
              />
              <div className="absolute right-1.5 flex items-center gap-0.5">
                {fallbackQuery ? (
                  <button
                    type="button"
                    tabIndex={-1}
                    onClick={() => {
                      setFallbackQuery("");
                      fallbackInputRef.current?.focus();
                    }}
                    aria-label={t("aiGatewayModelListClearSearch", "Clear search")}
                    className="rounded-full p-1 text-muted-foreground hover:bg-muted hover:text-foreground cursor-pointer"
                  >
                    <X className="h-3 w-3" />
                  </button>
                ) : null}
                <button
                  type="button"
                  tabIndex={-1}
                  data-testid="model-pool-fallback-combobox-toggle"
                  onClick={() =>
                    setFallbackDropdownOpen((previous) => !previous)
                  }
                  aria-label={t(
                    "aiGatewayModelPoolFallbackAddAria",
                    "Add fallback model",
                  )}
                  disabled={busy || saving}
                  className="rounded-md p-1 text-muted-foreground hover:bg-muted hover:text-foreground disabled:opacity-50 cursor-pointer"
                >
                  <ChevronDown
                    className={`h-3.5 w-3.5 transition-transform duration-150 ${
                      fallbackDropdownOpen ? "rotate-180" : ""
                    }`}
                  />
                </button>
              </div>
            </div>

            {/* Dropdown menu */}
            {fallbackDropdownOpen && (
              <div
                role="listbox"
                data-testid="model-pool-fallback-combobox-menu"
                className="absolute left-0 top-full z-40 mt-1 max-h-56 w-full overflow-y-auto rounded-xl border border-border/80 bg-popover/95 backdrop-blur-md p-1 shadow-xl animate-in fade-in-0 zoom-in-95 duration-100"
              >
                {filteredFallbackModelOptions.length > 0 ? (
                  filteredFallbackModelOptions.map((candidateModel) => {
                    const providerCount =
                      fallbackProviderCounts.get(candidateModel) ?? 0;
                    const meta = getFallbackModelMeta(providers, candidateModel);
                    return (
                      <div
                        key={candidateModel}
                        role="option"
                        aria-selected={false}
                        data-testid={`model-pool-fallback-option-${candidateModel}`}
                        onClick={() => {
                          onAddFallbackModel(candidateModel);
                          setFallbackQuery("");
                          setFallbackDropdownOpen(false);
                        }}
                        className="flex cursor-pointer items-center justify-between gap-2 rounded-lg px-2.5 py-1.5 text-xs text-foreground hover:bg-muted/80 transition-colors"
                      >
                        <span className="truncate font-mono font-medium">
                          {candidateModel}
                        </span>
                        <div className="flex shrink-0 items-center gap-1.5">
                          {meta.protocols.map((protocol) => (
                            <span
                              key={protocol}
                              className={`inline-flex items-center rounded px-1 py-0.2 text-[9px] font-medium ${protocolBadgeClass(
                                protocol,
                              )}`}
                            >
                              {protocolShortLabel(protocol)}
                            </span>
                          ))}
                          {meta.contextWindow ? (
                            <span className="inline-flex items-center gap-0.5 rounded border border-cyan-500/25 bg-cyan-500/10 px-1 py-0.2 font-mono text-[9px] font-medium text-cyan-700 dark:text-cyan-300">
                              <Layers className="h-2 w-2 text-cyan-600/80 dark:text-cyan-400/80 shrink-0" />
                              <span>{formatContextWindowDisplay(meta.contextWindow)}</span>
                            </span>
                          ) : null}
                          <span className="shrink-0 rounded-md bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground">
                            {t(
                              "aiGatewayModelPoolFallbackProviderCount",
                              "{{count}} providers",
                              { count: providerCount },
                            )}
                          </span>
                        </div>
                      </div>
                    );
                  })
                ) : (
                  <div
                    data-testid="model-pool-fallback-combobox-empty"
                    className="px-3 py-3 text-center text-xs text-muted-foreground"
                  >
                    {t(
                      "aiGatewayModelPoolFallbackNoMatch",
                      "No matching fallback models",
                    )}
                  </div>
                )}
              </div>
            )}

            {/* 兼容 select */}
            <select
              data-testid="model-pool-fallback-add"
              aria-label={t(
                "aiGatewayModelPoolFallbackAddAria",
                "Add fallback model",
              )}
              value=""
              onChange={(event) => {
                onAddFallbackModel(event.target.value);
              }}
              disabled={busy || saving}
              className="sr-only"
            >
              <option value="">
                {t(
                  "aiGatewayModelPoolFallbackAddPlaceholder",
                  "Add fallback model",
                )}
              </option>
              {fallbackModelOptions.map((m) => (
                <option key={m} value={m}>
                  {m}
                </option>
              ))}
            </select>
          </div>
        </div>

        {/* 降级链路列表呈现 */}
        {fallbackModels.length === 0 ? (
          <div className="flex flex-col items-center justify-center rounded-xl border border-dashed border-border/80 bg-muted/10 py-6 px-4 text-center">
            <div className="flex h-9 w-9 items-center justify-center rounded-full bg-muted/60 text-muted-foreground mb-2">
              <ShieldAlert className="h-4.5 w-4.5 opacity-60" />
            </div>
            <p className="text-xs font-medium text-foreground/85">
              {t(
                "aiGatewayModelPoolFallbackEmpty",
                "No fallback models configured.",
              )}
            </p>
            <p className="text-[11px] text-muted-foreground mt-1 max-w-sm">
              可从上方搜索添加模型，作为所有主候选模型发生故障或限流耗尽时的有序兜底保障
            </p>
          </div>
        ) : filteredFallbackModels.length === 0 ? (
          <div
            data-testid="model-pool-fallback-empty-filter"
            className="rounded-xl border border-dashed border-border/80 bg-muted/10 px-3 py-4 text-center text-xs text-muted-foreground"
          >
            {t(
              "aiGatewayModelPoolFallbackNoConfiguredMatch",
              "No configured fallback models match the filter.",
            )}
          </div>
        ) : (
          <ul
            data-testid="model-pool-fallback-list"
            className="space-y-2"
          >
            {filteredFallbackModels.map((m) => {
              const rawIndex = fallbackModels.indexOf(m);
              const providerCount =
                fallbackProviderCounts.get(m) ?? 0;
              const meta = getFallbackModelMeta(providers, m);
              return (
                <li
                  key={m}
                  role="listitem"
                  className="group flex items-center justify-between gap-3 rounded-xl border border-border/70 bg-card/80 p-2.5 shadow-2xs transition hover:border-border hover:shadow-xs"
                >
                  <div className="flex items-center gap-2.5 min-w-0">
                    <div className="flex h-6 w-6 shrink-0 items-center justify-center rounded-lg border border-primary/25 bg-primary/10 font-mono text-[11px] font-bold text-primary">
                      {rawIndex + 1}
                    </div>
                    <div className="min-w-0">
                      <div className="break-all font-mono text-xs font-semibold text-foreground">
                        {m}
                      </div>
                      <div className="flex flex-wrap items-center gap-1.5 mt-1 text-[10px]">
                        {providerCount > 0 ? (
                          <span className="inline-flex items-center gap-1 text-muted-foreground">
                            <span className="h-1.5 w-1.5 rounded-full bg-emerald-500" />
                            <span>
                              {t(
                                "aiGatewayModelPoolFallbackProviderCount",
                                "{{count}} providers",
                                { count: providerCount },
                              )}
                            </span>
                          </span>
                        ) : (
                          <span className="inline-flex items-center gap-1 font-medium text-amber-600 dark:text-amber-400">
                            <span className="h-1.5 w-1.5 rounded-full bg-amber-500" />
                            <span>
                              {t(
                                "aiGatewayModelPoolFallbackUnavailable",
                                "Unavailable — no provider mapping",
                              )}
                            </span>
                          </span>
                        )}

                        {/* 模型协议 tags */}
                        {meta.protocols.map((protocol) => (
                          <span
                            key={protocol}
                            className={`inline-flex items-center rounded px-1.5 py-0.2 text-[10px] font-medium ${protocolBadgeClass(
                              protocol,
                            )}`}
                          >
                            {protocolShortLabel(protocol)}
                          </span>
                        ))}

                        {/* 上下文长度限制 tag */}
                        {meta.contextWindow ? (
                          <span
                            data-testid={`model-pool-fallback-context-${m}`}
                            className="inline-flex items-center gap-0.5 rounded border border-cyan-500/25 bg-cyan-500/10 px-1.5 py-0.2 font-mono text-[10px] font-medium text-cyan-700 dark:text-cyan-300"
                            title={t(
                              "aiGatewayModelListEffectiveContext",
                              "Conservative effective context",
                            )}
                          >
                            <Layers className="h-2.5 w-2.5 text-cyan-600/80 dark:text-cyan-400/80 shrink-0" />
                            <span>{formatContextWindowDisplay(meta.contextWindow)}</span>
                          </span>
                        ) : null}
                      </div>
                    </div>
                  </div>

                  <div className="flex shrink-0 items-center gap-0.5 rounded-lg border border-border/50 bg-background/60 p-0.5">
                    <button
                      type="button"
                      onClick={() => onMoveFallbackModel(rawIndex, -1)}
                      disabled={busy || saving || rawIndex === 0}
                      aria-label={t(
                        "aiGatewayModelPoolFallbackMoveUpAria",
                        {
                          model: m,
                          defaultValue: "Move fallback {{model}} up",
                        },
                      )}
                      title="上移此降级模型"
                      className="inline-flex h-6.5 w-6.5 items-center justify-center rounded-md text-muted-foreground transition hover:bg-muted hover:text-foreground disabled:opacity-25 disabled:hover:bg-transparent cursor-pointer"
                    >
                      <ChevronUp className="h-3.5 w-3.5" />
                    </button>
                    <button
                      type="button"
                      onClick={() => onMoveFallbackModel(rawIndex, 1)}
                      disabled={
                        busy ||
                        saving ||
                        rawIndex === fallbackModels.length - 1
                      }
                      aria-label={t(
                        "aiGatewayModelPoolFallbackMoveDownAria",
                        {
                          model: m,
                          defaultValue: "Move fallback {{model}} down",
                        },
                      )}
                      title="下移此降级模型"
                      className="inline-flex h-6.5 w-6.5 items-center justify-center rounded-md text-muted-foreground transition hover:bg-muted hover:text-foreground disabled:opacity-25 disabled:hover:bg-transparent cursor-pointer"
                    >
                      <ChevronDown className="h-3.5 w-3.5" />
                    </button>
                    <div className="h-3.5 w-px bg-border/60 mx-0.5" />
                    <button
                      type="button"
                      onClick={() => onRemoveFallbackModel(m)}
                      disabled={busy || saving}
                      aria-label={t(
                        "aiGatewayModelPoolFallbackRemoveAria",
                        {
                          model: m,
                          defaultValue: "Remove fallback {{model}}",
                        },
                      )}
                      title="移除此降级模型"
                      className="inline-flex h-6.5 w-6.5 items-center justify-center rounded-md text-muted-foreground transition hover:bg-destructive/10 hover:text-destructive disabled:opacity-40 cursor-pointer"
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                    </button>
                  </div>
                </li>
              );
            })}
          </ul>
        )}
      </div>
    </div>
  );
}
