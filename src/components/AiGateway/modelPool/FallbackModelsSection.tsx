import type { Dispatch, RefObject, SetStateAction } from "react";
import {
  ChevronDown,
  ChevronUp,
  Search,
  SlidersHorizontal,
  Trash2,
  X,
} from "lucide-react";
import type { TFunction } from "i18next";
import { CANONICAL_REASONING_EFFORTS } from "@/lib/aiGateway";

type FallbackModelsSectionProps = {
  t: TFunction;
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

export function FallbackModelsSection({
  t,
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
    <div className="rounded-2xl border border-border/70 bg-card/60 p-4 shadow-2xs space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-2 border-b border-border/60 pb-3">
        <div className="flex items-center gap-2">
          <SlidersHorizontal className="h-4 w-4 text-primary" />
          <span className="text-xs font-semibold text-foreground">
            高级路由策略与故障降级
          </span>
        </div>
        <div className="flex items-center gap-2">
          <span className="text-[11px] text-muted-foreground">
            {t("aiGatewayModelPoolReasoningDefaultLabel", "Reasoning default")}:
          </span>
          <select
            data-testid="model-pool-reasoning-default"
            aria-label={t(
              "aiGatewayModelPoolReasoningDefaultLabel",
              "Reasoning default",
            )}
            value={defaultEffort}
            onChange={(event) => onDefaultEffortChange(event.target.value)}
            disabled={busy || saving}
            className="h-8 rounded-lg border border-border/80 bg-background px-2 text-xs font-medium text-foreground shadow-xs outline-none transition focus:border-primary/60 focus-visible:ring-2 focus-visible:ring-primary/20 disabled:opacity-50"
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

      {/* Fallback models 列表与添加 */}
      <div className="space-y-3">
        <div className="flex items-start justify-between gap-2">
          <div className="min-w-0 space-y-0.5">
            <div className="text-xs font-semibold text-foreground">
              {t("aiGatewayModelPoolFallbackLabel", "Fallback models")}
            </div>
            <p className="text-[11px] leading-relaxed text-muted-foreground">
              {t(
                "aiGatewayModelPoolFallbackHint",
                "Ordered fallback models are used only after every regular candidate is exhausted.",
              )}
            </p>
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
              className="inline-flex h-7 shrink-0 items-center rounded-md border border-border/80 bg-background px-2 text-[11px] font-medium text-foreground transition hover:bg-muted disabled:opacity-50"
            >
              {t(
                "aiGatewayModelPoolFallbackClear",
                "Clear fallback models",
              )}
            </button>
          ) : null}
        </div>

        {/* 搜索与添加下拉框 */}
        <div className="flex flex-col gap-1.5 text-xs font-medium text-foreground">
          <div className="flex items-center justify-between">
            <span>
              {t(
                "aiGatewayModelPoolFallbackAddLabel",
                "Add fallback model",
              )}
            </span>
            {fallbackQuery.trim() ? (
              <span
                data-testid="model-pool-fallback-match-count"
                className="text-[10px] text-muted-foreground"
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
                className="h-8.5 w-full rounded-lg border border-border/80 bg-background pl-8.5 pr-16 text-xs text-foreground shadow-xs outline-none transition focus:border-primary/60 focus-visible:ring-2 focus-visible:ring-primary/20 disabled:opacity-50"
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
                    className="rounded-full p-1 text-muted-foreground hover:bg-muted hover:text-foreground"
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
                  className="rounded-md p-1 text-muted-foreground hover:bg-muted hover:text-foreground disabled:opacity-50"
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
                className="absolute left-0 top-full z-40 mt-1 max-h-56 w-full overflow-y-auto rounded-lg border border-border/80 bg-popover p-1 shadow-lg"
              >
                {filteredFallbackModelOptions.length > 0 ? (
                  filteredFallbackModelOptions.map((candidateModel) => {
                    const providerCount =
                      fallbackProviderCounts.get(candidateModel) ?? 0;
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
                        className="flex cursor-pointer items-center justify-between gap-2 rounded-md px-2.5 py-1.5 text-xs text-foreground hover:bg-muted transition-colors"
                      >
                        <span className="truncate font-mono font-medium">
                          {candidateModel}
                        </span>
                        <span className="shrink-0 text-[10px] text-muted-foreground">
                          {t(
                            "aiGatewayModelPoolFallbackProviderCount",
                            "{{count}} providers",
                            { count: providerCount },
                          )}
                        </span>
                      </div>
                    );
                  })
                ) : (
                  <div
                    data-testid="model-pool-fallback-combobox-empty"
                    className="px-3 py-2 text-center text-xs text-muted-foreground"
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

        {fallbackModels.length === 0 ? (
          <p className="rounded-lg border border-dashed border-border/70 bg-card/40 px-3 py-3 text-center text-[11px] text-muted-foreground">
            {t(
              "aiGatewayModelPoolFallbackEmpty",
              "No fallback models configured.",
            )}
          </p>
        ) : filteredFallbackModels.length === 0 ? (
          <p
            data-testid="model-pool-fallback-empty-filter"
            className="rounded-lg border border-dashed border-border/70 bg-card/40 px-3 py-3 text-center text-[11px] text-muted-foreground"
          >
            {t(
              "aiGatewayModelPoolFallbackNoConfiguredMatch",
              "No configured fallback models match the filter.",
            )}
          </p>
        ) : (
          <ul
            data-testid="model-pool-fallback-list"
            className="space-y-1.5"
          >
            {filteredFallbackModels.map((m) => {
              const rawIndex = fallbackModels.indexOf(m);
              const providerCount =
                fallbackProviderCounts.get(m) ?? 0;
              return (
                <li
                  key={m}
                  role="listitem"
                  className="flex items-center justify-between gap-2 rounded-lg border border-border/60 bg-card/70 px-2.5 py-1.5"
                >
                  <div className="min-w-0">
                    <div className="break-all font-mono text-xs font-medium text-foreground">
                      {m}
                    </div>
                    <div
                      className={`text-[10px] ${
                        providerCount > 0
                          ? "text-muted-foreground"
                          : "text-amber-700 dark:text-amber-400"
                      }`}
                    >
                      {providerCount > 0
                        ? t(
                            "aiGatewayModelPoolFallbackProviderCount",
                            "{{count}} providers",
                            { count: providerCount },
                          )
                        : t(
                            "aiGatewayModelPoolFallbackUnavailable",
                            "Unavailable — no provider mapping",
                          )}
                    </div>
                  </div>
                  <div className="flex shrink-0 items-center gap-0.5">
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
                      className="inline-flex h-6.5 w-6.5 items-center justify-center rounded-md text-muted-foreground transition hover:bg-muted hover:text-foreground disabled:opacity-40"
                    >
                      <ChevronUp className="h-3 w-3" />
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
                      className="inline-flex h-6.5 w-6.5 items-center justify-center rounded-md text-muted-foreground transition hover:bg-muted hover:text-foreground disabled:opacity-40"
                    >
                      <ChevronDown className="h-3 w-3" />
                    </button>
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
                      className="inline-flex h-6.5 w-6.5 items-center justify-center rounded-md text-muted-foreground transition hover:bg-destructive/10 hover:text-destructive disabled:opacity-50"
                    >
                      <Trash2 className="h-3 w-3" />
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
