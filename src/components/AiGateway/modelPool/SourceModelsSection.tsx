import {
  CheckSquare,
  ChevronDown,
  Layers,
  Search,
  Server,
  Sparkles,
  X,
} from "lucide-react";
import type { TFunction } from "i18next";
import {
  formatContextWindowDisplay,
  MODEL_POOL_AVAILABILITY_LABEL_KEYS,
  resolveMappingContextWindow,
  resolveModelPoolCandidateState,
  type GatewayModelPoolCandidate,
  type GatewayUpstreamProvider,
} from "@/lib/aiGateway";
import { protocolBadgeClass, protocolShortLabel } from "../gatewayShared";
import type { ModelPoolCandidateStatus, SourceGroup } from "./types";

type SourceModelsSectionProps = {
  t: TFunction;
  sourceGroups: SourceGroup[];
  filteredCandidates: GatewayModelPoolCandidate[];
  allFilteredSelected: boolean;
  normalizedQuery: string;
  query: string;
  onQueryChange: (value: string) => void;
  collapsed: Record<string, boolean>;
  onToggleCollapsed: (providerId: string) => void;
  selectedKeys: Set<string>;
  onToggleCandidate: (candidate: GatewayModelPoolCandidate) => void;
  onToggleSelectAllFiltered: () => void;
  providers: GatewayUpstreamProvider[];
  statusByKey: Map<string, ModelPoolCandidateStatus>;
  busy: boolean;
  saving: boolean;
};

export function SourceModelsSection({
  t,
  sourceGroups,
  filteredCandidates,
  allFilteredSelected,
  normalizedQuery,
  query,
  onQueryChange,
  collapsed,
  onToggleCollapsed,
  selectedKeys,
  onToggleCandidate,
  onToggleSelectAllFiltered,
  providers,
  statusByKey,
  busy,
  saving,
}: SourceModelsSectionProps) {
  return (
    <div className="lg:col-span-5 flex flex-col rounded-2xl border border-border/70 bg-card/40 overflow-hidden shadow-2xs">
      {/* 头部：标题与搜索 */}
      <div className="p-3 border-b border-border/60 bg-muted/20 space-y-2">
        <div className="flex items-center justify-between gap-2">
          <div className="flex items-center gap-2">
            <Server className="h-3.5 w-3.5 text-muted-foreground" />
            <span className="text-xs font-semibold text-foreground">
              {t("aiGatewayModelPoolSourcesTitle", "Source models")}
            </span>
            <span className="rounded-full bg-muted px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground">
              {filteredCandidates.length}
            </span>
          </div>
          {filteredCandidates.length > 0 ? (
            <button
              type="button"
              data-testid="model-pool-select-all-filtered"
              onClick={onToggleSelectAllFiltered}
              className="inline-flex items-center gap-1 rounded-md px-2 py-0.5 text-xs font-medium text-primary hover:bg-primary/10 transition-colors"
            >
              <CheckSquare className="h-3.5 w-3.5" />
              <span>
                {allFilteredSelected
                  ? t(
                      "aiGatewayModelPoolDeselectAllFiltered",
                      "Deselect all ({{count}})",
                      { count: filteredCandidates.length },
                    )
                  : t(
                      "aiGatewayModelPoolSelectAllFiltered",
                      "Select all ({{count}})",
                      { count: filteredCandidates.length },
                    )}
              </span>
            </button>
          ) : null}
        </div>

        {/* 搜索框 */}
        <div className="relative">
          <Search className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground/70" />
          <input
            type="search"
            data-testid="model-pool-source-search"
            aria-label={t("aiGatewayModelPoolSourceSearch", "Search source models")}
            value={query}
            onChange={(event) => onQueryChange(event.target.value)}
            placeholder={t(
              "aiGatewayModelPoolSourceSearchPlaceholder",
              "Search by provider or model",
            )}
            className="h-8 w-full rounded-lg border border-border/80 bg-background pl-8 pr-8 text-xs text-foreground shadow-xs outline-none transition focus:border-primary/60 focus-visible:ring-2 focus-visible:ring-primary/20"
          />
          {query ? (
            <button
              type="button"
              onClick={() => onQueryChange("")}
              aria-label={t("aiGatewayModelListClearSearch", "Clear search")}
              className="absolute right-1.5 top-1/2 -translate-y-1/2 rounded-full p-1 text-muted-foreground transition hover:bg-muted hover:text-foreground"
            >
              <X className="h-3 w-3" />
            </button>
          ) : null}
        </div>
      </div>

      {/* 来源列表 */}
      <div className="max-h-[460px] overflow-y-auto p-2 space-y-2">
        {sourceGroups.length === 0 ? (
          <p className="rounded-lg border border-dashed bg-muted/20 px-3 py-6 text-center text-xs text-muted-foreground">
            {t("aiGatewayModelPoolNoSources", "No source models available.")}
          </p>
        ) : (
          sourceGroups.map((group) => {
            const visibleRows = group.rows.filter((row) =>
              normalizedQuery
                ? row.candidate.local_model
                    .toLowerCase()
                    .includes(normalizedQuery) ||
                  row.candidate.upstream_model
                    .toLowerCase()
                    .includes(normalizedQuery) ||
                  group.providerName.toLowerCase().includes(normalizedQuery)
                : true,
            );
            const isCollapsed = Boolean(collapsed[group.providerId]);
            return (
              <div
                key={group.providerId}
                className="rounded-xl border border-border/70 bg-card/60 overflow-hidden"
              >
                <button
                  type="button"
                  data-testid={`model-pool-source-group-${group.providerId}`}
                  aria-expanded={!isCollapsed}
                  onClick={() => onToggleCollapsed(group.providerId)}
                  className="flex w-full items-center justify-between gap-2 px-3 py-2 text-left bg-muted/20 hover:bg-muted/40 transition-colors"
                >
                  <span className="inline-flex min-w-0 flex-1 items-center gap-1.5 text-xs font-semibold text-foreground">
                    <Server className="h-3 w-3 shrink-0 text-muted-foreground" />
                    <span className="min-w-0 truncate">{group.providerName}</span>
                    {group.templateName ? (
                      <span
                        data-testid={`model-pool-source-template-badge-${group.providerId}`}
                        className="inline-flex items-center gap-0.5 rounded-md border border-indigo-500/25 bg-indigo-500/10 px-1.5 py-0.2 text-[9px] font-medium text-indigo-700 dark:text-indigo-300 shrink-0"
                        title={group.templateName}
                      >
                        <Sparkles className="h-2.5 w-2.5" />
                        <span>{group.templateName}</span>
                      </span>
                    ) : null}
                    <span className="shrink-0 rounded-full bg-muted px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground">
                      {group.rows.length}
                    </span>
                  </span>
                  <ChevronDown
                    className={`h-3.5 w-3.5 shrink-0 text-muted-foreground transition-transform ${
                      isCollapsed ? "-rotate-90" : ""
                    }`}
                  />
                </button>
                {!isCollapsed ? (
                  visibleRows.length === 0 ? (
                    <p className="px-3 py-2 text-[11px] text-muted-foreground">
                      {t(
                        "aiGatewayModelListNoMatch",
                        "No models match the current search.",
                      )}
                    </p>
                  ) : (
                    <div className="space-y-1 p-1.5">
                      {visibleRows.map(({ key, candidate }) => {
                        const checked = selectedKeys.has(key);
                        const state = resolveModelPoolCandidateState(
                          candidate,
                          providers,
                          statusByKey.get(key) ?? null,
                        );
                        const labelKey =
                          MODEL_POOL_AVAILABILITY_LABEL_KEYS[
                            state.availability
                          ];
                        const label = labelKey
                          ? t(labelKey)
                          : state.availability;
                        return (
                          <label
                            key={key}
                            className={`flex cursor-pointer items-start gap-2 rounded-lg p-2 transition ${
                              checked
                                ? "bg-primary/5 border border-primary/20"
                                : "hover:bg-muted/50 border border-transparent"
                            }`}
                          >
                            <input
                              type="checkbox"
                              checked={checked}
                              onChange={() => onToggleCandidate(candidate)}
                              aria-label={`${group.providerName} ${candidate.local_model} ${candidate.upstream_model}`}
                              disabled={busy || saving}
                              className="mt-0.5 h-3.5 w-3.5 shrink-0 rounded border-border accent-primary"
                            />
                            <span className="flex min-w-0 flex-1 flex-col gap-1 text-xs">
                              <div className="flex flex-wrap items-center gap-1.5 min-w-0">
                                <span className="break-all font-medium text-foreground">
                                  {candidate.local_model}
                                </span>
                                <span className="text-muted-foreground/60 select-none text-[10px]">
                                  →
                                </span>
                                <code className="break-all rounded border border-border/40 bg-muted/60 px-1 py-0.2 font-mono text-[10px] text-foreground">
                                  {candidate.upstream_model}
                                </code>
                              </div>
                              <div className="flex flex-wrap items-center gap-1 text-[10px]">
                                <span
                                  className={`inline-flex items-center rounded px-1 py-0.2 font-medium ${protocolBadgeClass(
                                    candidate.protocol,
                                  )}`}
                                >
                                  {protocolShortLabel(candidate.protocol)}
                                </span>
                                {(() => {
                                  const contextWindow = resolveMappingContextWindow(
                                    providers,
                                    candidate.provider_id,
                                    candidate.local_model,
                                    candidate.upstream_model,
                                  );
                                  if (!contextWindow) return null;
                                  return (
                                    <span
                                      data-testid={`model-pool-source-context-${key}`}
                                      className="inline-flex items-center gap-0.5 rounded border border-cyan-500/25 bg-cyan-500/10 px-1 py-0.2 font-mono text-[10px] font-medium text-cyan-700 dark:text-cyan-300"
                                      title={t(
                                        "aiGatewayModelListEffectiveContext",
                                        "Conservative effective context",
                                      )}
                                    >
                                      <Layers className="h-2.5 w-2.5 text-cyan-600/80 dark:text-cyan-400/80 shrink-0" />
                                      <span>{formatContextWindowDisplay(contextWindow)}</span>
                                    </span>
                                  );
                                })()}
                                <span
                                  className={`inline-flex items-center rounded px-1 py-0.2 font-medium ${
                                    state.ready
                                      ? "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400"
                                      : "bg-amber-500/15 text-amber-700 dark:text-amber-400"
                                  }`}
                                >
                                  {label}
                                </span>
                                {state.reason ? (
                                  <span className="break-words text-[10px] text-amber-600 dark:text-amber-400 truncate max-w-full">
                                    {state.reason}
                                  </span>
                                ) : null}
                              </div>
                            </span>
                          </label>
                        );
                      })}
                    </div>
                  )
                ) : null}
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}
