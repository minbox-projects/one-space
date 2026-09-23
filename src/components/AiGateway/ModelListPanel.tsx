import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Boxes,
  Check,
  Copy,
  Layers,
  Search,
  Server,
  X,
  ArrowRight,
} from "lucide-react";
import { useToast } from "@/components/ToastProvider";
import { SelectDropdown } from "./SelectDropdown";
import {
  aggregateModels,
  AI_GATEWAY_DEFAULT_PORT,
  localBaseUrl,
  resolveAggregatedModelName,
  resolveAggregatedReasoningEfforts,
  type GatewayUpstreamProtocol,
  type GatewayUpstreamProvider,
} from "@/lib/aiGateway";

function endpointPath(protocol: GatewayUpstreamProtocol): string {
  return protocol === "responses" ? "/responses" : "/chat/completions";
}

function ProtocolBadge({ protocol }: { protocol: GatewayUpstreamProtocol }) {
  const isResponses = protocol === "responses";
  const path = endpointPath(protocol);

  return (
    <span
      className={`inline-flex items-center gap-1 rounded-md border px-1.5 py-0.5 font-mono text-[10px] font-medium leading-none shadow-2xs ${
        isResponses
          ? "border-purple-500/25 bg-purple-500/10 text-purple-700 dark:text-purple-300"
          : "border-sky-500/25 bg-sky-500/10 text-sky-700 dark:text-sky-300"
      }`}
    >
      <span
        aria-hidden="true"
        className={`h-1.5 w-1.5 rounded-full shrink-0 ${
          isResponses ? "bg-purple-500" : "bg-sky-500"
        }`}
      />
      <span>{path}</span>
    </span>
  );
}

type ModelListPanelProps = {
  providers: GatewayUpstreamProvider[];
  port?: number;
  onNavigateProviders?: () => void;
};

export function ModelListPanel({
  providers,
  port = AI_GATEWAY_DEFAULT_PORT,
  onNavigateProviders,
}: ModelListPanelProps) {
  const { t } = useTranslation();
  const { pushToast } = useToast();
  const [query, setQuery] = useState("");
  const [providerFilter, setProviderFilter] = useState<string>("all");
  const [protocolFilter, setProtocolFilter] = useState<string>("all");
  const [copiedTarget, setCopiedTarget] = useState<string | null>(null);

  const rows = useMemo(
    () =>
      aggregateModels(providers).map((entry) => ({
        entry,
        name: resolveAggregatedModelName(entry),
        protocols: Array.from(
          new Set(entry.providers.map((upstream) => upstream.endpoint)),
        ),
        reasoningEfforts: resolveAggregatedReasoningEfforts(entry),
      })),
    [providers],
  );

  // 提取可用服务商选项（仅包含有生效模型的服务商）
  const providerOptions = useMemo(() => {
    const providerMap = new Map<string, string>();
    rows.forEach(({ entry }) => {
      entry.providers.forEach((upstream) => {
        providerMap.set(upstream.providerId, upstream.providerName);
      });
    });

    const options = [
      {
        value: "all",
        label: t("aiGatewayModelListAllProviders", "All providers"),
      },
    ];

    Array.from(providerMap.entries())
      .sort((a, b) => a[1].localeCompare(b[1]))
      .forEach(([id, name]) => {
        options.push({ value: id, label: name });
      });

    return options;
  }, [rows, t]);

  // 协议筛选选项
  const protocolOptions = useMemo(
    () => [
      {
        value: "all",
        label: t("aiGatewayModelListAllProtocols", "All protocols"),
      },
      {
        value: "chat_completions",
        label: "Chat Completions",
      },
      {
        value: "responses",
        label: "Responses",
      },
    ],
    [t],
  );

  // 多条件过滤
  const normalizedQuery = query.trim().toLowerCase();
  const visibleRows = useMemo(() => {
    let result = rows;

    if (normalizedQuery) {
      result = result.filter(
        ({ entry, name }) =>
          entry.model.toLowerCase().includes(normalizedQuery) ||
          name.toLowerCase().includes(normalizedQuery),
      );
    }

    if (providerFilter !== "all") {
      result = result.filter(({ entry }) =>
        entry.providers.some((p) => p.providerId === providerFilter),
      );
    }

    if (protocolFilter !== "all") {
      result = result.filter(({ protocols }) =>
        protocols.includes(protocolFilter as GatewayUpstreamProtocol),
      );
    }

    return result;
  }, [normalizedQuery, protocolFilter, providerFilter, rows]);

  const modelsApiUrl = `${localBaseUrl(port)}/models`;

  const handleCopy = async (
    text: string,
    targetKey: string,
    successTitle?: string,
  ) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopiedTarget(targetKey);
      pushToast({
        title:
          successTitle ??
          t(
            "aiGatewayModelListCopySuccess",
            "Model ID copied to clipboard",
          ),
        kind: "success",
      });
      setTimeout(() => {
        setCopiedTarget((prev) => (prev === targetKey ? null : prev));
      }, 1500);
    } catch {
      // 剪贴板异常优雅降级
    }
  };

  const handleClearFilters = () => {
    setQuery("");
    setProviderFilter("all");
    setProtocolFilter("all");
  };

  const hasActiveFilters =
    Boolean(normalizedQuery) ||
    providerFilter !== "all" ||
    protocolFilter !== "all";

  return (
    <div className="space-y-4" data-testid="ai-gateway-model-list">
      {/* 头部区域：对齐模块规范，增强视觉层次 */}
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <div className="space-y-0.5">
          <div className="flex items-center gap-2">
            <Boxes className="h-4 w-4 text-primary" />
            <h3 className="text-sm font-semibold text-foreground">
              {t("aiGatewayModelListTab", "Model list")}
            </h3>
            <span className="rounded-full bg-primary/10 px-2 py-0.5 text-[11px] font-semibold text-primary">
              {rows.length}
            </span>
          </div>
          <p className="text-xs text-muted-foreground">
            {t(
              "aiGatewayModelListDesc",
              "View local models exposed by the gateway, calling protocols, and mapped upstream routing sources.",
            )}
          </p>
        </div>

        {/* 数量与筛选状态概览 */}
        {rows.length > 0 ? (
          <div className="text-xs text-muted-foreground">
            {t("aiGatewayModelListFilteredCount", {
              shown: visibleRows.length,
              total: rows.length,
              defaultValue: `Showing ${visibleRows.length} of ${rows.length}`,
            })}
          </div>
        ) : null}
      </div>

      {/* 模型列表 API 获取地址栏 */}
      <div
        data-testid="ai-gateway-model-list-api-url-bar"
        className="flex flex-wrap items-center justify-between gap-2.5 rounded-lg border border-border/70 bg-muted/30 px-3 py-2 text-xs"
      >
        <div className="flex flex-wrap items-center gap-2 min-w-0">
          <span className="inline-flex items-center rounded-md border border-primary/20 bg-primary/10 px-1.5 py-0.5 font-mono text-[10px] font-semibold text-primary">
            GET
          </span>
          <span className="font-medium text-foreground/90">
            {t("aiGatewayModelListApiUrl", "Models API endpoint")}:
          </span>
          <div className="flex items-center gap-1 min-w-0">
            <code
              data-testid="ai-gateway-model-list-api-url"
              className="rounded border border-border/50 bg-background/80 px-2 py-0.5 font-mono text-xs text-foreground select-all truncate"
              title={modelsApiUrl}
            >
              {modelsApiUrl}
            </code>
            <button
              type="button"
              data-testid="ai-gateway-model-list-copy-api-url-btn"
              aria-label={t(
                "aiGatewayModelListCopyApiUrlAria",
                "Copy models API endpoint",
              )}
              title={t(
                "aiGatewayModelListCopyApiUrlAria",
                "Copy models API endpoint",
              )}
              onClick={() =>
                void handleCopy(
                  modelsApiUrl,
                  "api-url",
                  t(
                    "aiGatewayModelListCopyApiUrlSuccess",
                    "Models API endpoint copied to clipboard",
                  ),
                )
              }
              className="rounded p-1 text-muted-foreground transition hover:bg-muted hover:text-foreground"
            >
              {copiedTarget === "api-url" ? (
                <Check className="h-3.5 w-3.5 text-emerald-500" />
              ) : (
                <Copy className="h-3.5 w-3.5" />
              )}
            </button>
          </div>
        </div>
      </div>

      {/* 控制栏：美化型搜索框与防换行过滤器 */}
      {rows.length > 0 ? (
        <div className="flex flex-wrap items-center gap-2.5">
          {/* 美化搜索框 */}
          <div className="relative w-full sm:w-80 md:w-96">
            <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground/70 transition-colors" />
            <input
              type="search"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              data-testid="ai-gateway-model-list-search"
              aria-label={t("aiGatewayModelListSearch", "Search models")}
              placeholder={t(
                "aiGatewayModelListSearchPlaceholder",
                "Search by model ID or name",
              )}
              className="h-9 w-full rounded-lg border border-border/80 bg-background/80 pl-9 pr-9 text-xs text-foreground shadow-xs outline-none transition-all placeholder:text-muted-foreground/70 hover:border-border hover:bg-background focus:border-primary/60 focus:bg-background focus-visible:ring-2 focus-visible:ring-primary/20"
            />
            {query ? (
              <button
                type="button"
                onClick={() => setQuery("")}
                aria-label={t("aiGatewayModelListClearSearch", "Clear search")}
                className="absolute right-2 top-1/2 -translate-y-1/2 rounded-full p-1 text-muted-foreground transition hover:bg-muted hover:text-foreground"
              >
                <X className="h-3.5 w-3.5" />
              </button>
            ) : null}
          </div>

          {/* 服务商筛选（绝不换行） */}
          {providerOptions.length > 2 ? (
            <SelectDropdown
              value={providerFilter}
              options={providerOptions}
              onChange={setProviderFilter}
              ariaLabel={t(
                "aiGatewayModelListFilterProvider",
                "Filter by provider",
              )}
              testId="ai-gateway-model-list-provider-filter"
              buttonClassName="h-9 text-xs font-normal whitespace-nowrap shadow-xs"
              menuClassName="min-w-[150px] whitespace-nowrap"
              icon={<Server className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />}
            />
          ) : null}

          {/* 协议筛选（绝不换行） */}
          <SelectDropdown
            value={protocolFilter}
            options={protocolOptions}
            onChange={setProtocolFilter}
            ariaLabel={t(
              "aiGatewayModelListFilterProtocol",
              "Filter by protocol",
            )}
            testId="ai-gateway-model-list-protocol-filter"
            buttonClassName="h-9 text-xs font-normal whitespace-nowrap shadow-xs"
            menuClassName="min-w-[150px] whitespace-nowrap"
            icon={<Layers className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />}
          />

          {/* 重置筛选按钮 */}
          {hasActiveFilters ? (
            <button
              type="button"
              onClick={handleClearFilters}
              className="inline-flex h-9 items-center gap-1.5 rounded-lg border border-dashed px-3 text-xs whitespace-nowrap text-muted-foreground transition hover:bg-muted hover:text-foreground shadow-xs"
            >
              <X className="h-3.5 w-3.5" />
              <span>
                {t("aiGatewayModelListClearFilters", "Clear filters")}
              </span>
            </button>
          ) : null}
        </div>
      ) : null}

      {/* 列表渲染与空状态 */}
      {rows.length === 0 ? (
        <div
          data-testid="ai-gateway-model-list-empty"
          className="flex flex-col items-center justify-center rounded-xl border border-dashed bg-muted/20 px-4 py-10 text-center"
        >
          <div className="rounded-full bg-muted/50 p-3 text-muted-foreground">
            <Boxes className="h-6 w-6" />
          </div>
          <p className="mt-3 text-xs font-medium text-foreground">
            {t(
              "aiGatewayModelListEmpty",
              "No local models are served by enabled upstream providers yet.",
            )}
          </p>
          <p className="mt-1 max-w-sm text-[11px] text-muted-foreground">
            {t(
              "aiGatewayAggregatedModelsDialogDesc",
              "Local models served by enabled upstream providers and the upstream models they map to.",
            )}
          </p>
          {onNavigateProviders ? (
            <button
              type="button"
              onClick={onNavigateProviders}
              className="mt-4 inline-flex items-center gap-1.5 rounded-lg bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground shadow-xs transition hover:bg-primary/90"
            >
              <span>
                {t(
                  "aiGatewayModelListGoToProviders",
                  "Configure upstream providers",
                )}
              </span>
              <ArrowRight className="h-3.5 w-3.5" />
            </button>
          ) : null}
        </div>
      ) : visibleRows.length === 0 ? (
        <div
          data-testid="ai-gateway-model-list-no-match"
          className="flex flex-col items-center justify-center rounded-xl border border-dashed bg-muted/20 px-4 py-8 text-center"
        >
          <div className="rounded-full bg-muted/50 p-2.5 text-muted-foreground">
            <Search className="h-5 w-5" />
          </div>
          <p className="mt-2 text-xs font-medium text-muted-foreground">
            {t(
              "aiGatewayModelListNoMatch",
              "No models match the current search.",
            )}
          </p>
          <button
            type="button"
            onClick={handleClearFilters}
            className="mt-3 inline-flex items-center gap-1 rounded-lg border bg-background px-2.5 py-1 text-xs font-medium text-foreground shadow-xs transition hover:bg-muted"
          >
            <X className="h-3 w-3" />
            <span>{t("aiGatewayModelListClearFilters", "Clear filters")}</span>
          </button>
        </div>
      ) : (
        <div className="overflow-x-auto rounded-xl border bg-card shadow-xs">
          <table
            data-testid="ai-gateway-model-list-table"
            className="w-full border-collapse text-xs"
          >
            <thead>
              <tr className="border-b bg-muted/40 text-left text-muted-foreground">
                <th className="px-3.5 py-2.5 font-medium sm:w-1/4">
                  {t("aiGatewayModelListIdColumn", "Model ID")}
                </th>
                <th className="px-3.5 py-2.5 font-medium sm:w-1/4">
                  {t("aiGatewayModelListNameColumn", "Model name")}
                </th>
                <th className="px-3.5 py-2.5 font-medium sm:w-1/2">
                  {t("aiGatewayModelListUpstreamColumn", "Upstream models")}
                </th>
              </tr>
            </thead>
            <tbody className="divide-y divide-border/60">
              {visibleRows.map(({ entry, name, protocols, reasoningEfforts }) => {
                const localTargetKey = `local:${entry.model}`;
                const isCopied = copiedTarget === localTargetKey;
                return (
                  <tr
                    key={entry.model}
                    data-testid="ai-gateway-model-list-row"
                    data-model={entry.model}
                    className="align-top transition-colors hover:bg-muted/30"
                  >
                    {/* 第一列：模型 ID 与快捷复制 */}
                    <td className="px-3.5 py-2.5">
                      <div className="flex items-center justify-between gap-2">
                        <div className="flex flex-wrap items-center gap-1.5">
                          <span
                            data-testid="ai-gateway-model-list-id"
                            className="font-mono font-medium text-foreground"
                          >
                            {entry.model}
                          </span>
                          {entry.providers.length > 1 ? (
                            <span
                              title={t("aiGatewayModelListMultiUpstreamBadge", {
                                count: entry.providers.length,
                                defaultValue: `${entry.providers.length} upstream sources`,
                              })}
                              className="inline-flex items-center rounded-full bg-emerald-500/10 px-1.5 py-0.2 text-[10px] font-semibold text-emerald-600 dark:text-emerald-400"
                            >
                              {entry.providers.length}
                            </span>
                          ) : null}
                        </div>

                        <button
                          type="button"
                          data-testid="ai-gateway-model-list-copy-btn"
                          aria-label={t(
                            "aiGatewayModelListCopyAria",
                            "Copy model ID",
                          )}
                          onClick={() => void handleCopy(entry.model, localTargetKey)}
                          className="rounded p-1 text-muted-foreground transition hover:bg-muted hover:text-foreground"
                        >
                          {isCopied ? (
                            <Check className="h-3.5 w-3.5 text-emerald-500" />
                          ) : (
                            <Copy className="h-3.5 w-3.5" />
                          )}
                        </button>
                      </div>
                    </td>

                    {/* 第二列：显示名称、协议徽标与推理强度 */}
                    <td className="px-3.5 py-2.5">
                      <div className="space-y-1.5">
                        <span
                          data-testid="ai-gateway-model-list-name"
                          className="block text-foreground"
                        >
                          {name}
                        </span>
                        <div className="flex flex-wrap items-center gap-1">
                          {protocols.map((proto) => (
                            <ProtocolBadge key={proto} protocol={proto} />
                          ))}
                        </div>
                        {reasoningEfforts.length > 0 ? (
                          <div
                            data-testid="ai-gateway-model-list-reasoning-efforts"
                            className="flex flex-wrap items-center gap-1 pt-0.5"
                          >
                            <span
                              title={t("aiGatewayReasoningEfforts", "Reasoning efforts")}
                              className="text-[10px] font-medium text-muted-foreground"
                            >
                              {t("aiGatewayReasoningEfforts", "Reasoning efforts")}:
                            </span>
                            {reasoningEfforts.map((effort) => (
                              <span
                                key={effort}
                                data-testid={`ai-gateway-model-list-effort-${effort}`}
                                className="inline-flex items-center rounded border border-border/80 bg-muted/60 px-1.5 py-0.2 font-mono text-[10px] font-medium text-foreground/85 leading-tight shadow-2xs"
                              >
                                {effort}
                              </span>
                            ))}
                          </div>
                        ) : null}
                      </div>
                    </td>

                    {/* 第三列：上游模型映射与来源 */}
                    <td
                      data-testid="ai-gateway-model-list-upstreams"
                      className="px-3.5 py-2.5"
                    >
                      <ul className="space-y-1.5">
                        {entry.providers.map((upstream, index) => {
                          const upstreamKey = `${upstream.providerId}-${upstream.upstreamModel}-${index}`;
                          const upstreamTargetKey = `upstream:${upstreamKey}`;
                          const isUpstreamCopied =
                            copiedTarget === upstreamTargetKey;

                          return (
                            <li
                              key={upstreamKey}
                              data-testid="ai-gateway-model-list-upstream"
                              data-default={upstream.isDefault ? "true" : "false"}
                              className="flex items-center justify-between gap-2.5 rounded-lg border border-border/60 bg-muted/20 px-2.5 py-1.5 text-xs transition-colors hover:bg-muted/40"
                            >
                              <div className="flex flex-wrap items-center gap-1.5 min-w-0">
                                <span
                                  data-testid="ai-gateway-model-list-upstream-provider"
                                  className="font-medium text-foreground"
                                >
                                  {upstream.providerName}
                                </span>
                                <span
                                  aria-hidden="true"
                                  className="text-muted-foreground/60 text-[11px]"
                                >
                                  →
                                </span>
                                <code
                                  data-testid="ai-gateway-model-list-upstream-model"
                                  className="rounded border border-border/40 bg-muted/60 px-1.5 py-0.5 font-mono text-[11px] text-foreground"
                                >
                                  {upstream.upstreamModel}
                                </code>
                                <button
                                  type="button"
                                  data-testid="ai-gateway-model-list-upstream-copy-btn"
                                  aria-label={t(
                                    "aiGatewayModelListCopyUpstreamAria",
                                    "Copy upstream model ID",
                                  )}
                                  onClick={() =>
                                    void handleCopy(
                                      upstream.upstreamModel,
                                      upstreamTargetKey,
                                    )
                                  }
                                  className="rounded p-0.5 text-muted-foreground transition hover:bg-muted hover:text-foreground"
                                >
                                  {isUpstreamCopied ? (
                                    <Check className="h-3.5 w-3.5 text-emerald-500" />
                                  ) : (
                                    <Copy className="h-3.5 w-3.5" />
                                  )}
                                </button>
                                {upstream.isDefault ? (
                                  <span
                                    data-testid="ai-gateway-model-list-upstream-default"
                                    className="rounded bg-primary/10 px-1.5 py-0.5 text-[10px] font-medium text-primary"
                                  >
                                    {t(
                                      "aiGatewayAggregatedModelDefaultBadge",
                                      "Default",
                                    )}
                                  </span>
                                ) : null}
                              </div>

                              <div className="shrink-0">
                                <ProtocolBadge protocol={upstream.endpoint} />
                              </div>
                            </li>
                          );
                        })}
                      </ul>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
