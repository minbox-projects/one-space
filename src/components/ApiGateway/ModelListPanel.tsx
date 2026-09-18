import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  aggregateModels,
  resolveAggregatedModelName,
  type GatewayUpstreamProvider,
} from "@/lib/apiGateway";

export function ModelListPanel({
  providers,
}: {
  providers: GatewayUpstreamProvider[];
}) {
  const { t } = useTranslation();
  const [query, setQuery] = useState("");

  const rows = useMemo(
    () =>
      aggregateModels(providers).map((entry) => ({
        entry,
        name: resolveAggregatedModelName(entry),
      })),
    [providers],
  );

  const normalizedQuery = query.trim().toLowerCase();
  const visibleRows = normalizedQuery
    ? rows.filter(
        ({ entry, name }) =>
          entry.model.toLowerCase().includes(normalizedQuery) ||
          name.toLowerCase().includes(normalizedQuery),
      )
    : rows;

  return (
    <div className="space-y-3.5" data-testid="api-gateway-model-list">
      <input
        type="search"
        value={query}
        onChange={(event) => setQuery(event.target.value)}
        data-testid="api-gateway-model-list-search"
        aria-label={t("apiGatewayModelListSearch", "Search models")}
        placeholder={t(
          "apiGatewayModelListSearchPlaceholder",
          "Search by model ID or name",
        )}
        className="h-9 w-full rounded-lg border bg-background px-3 text-xs text-foreground shadow-xs outline-none placeholder:text-muted-foreground focus-visible:ring-2 focus-visible:ring-ring sm:max-w-sm"
      />

      {rows.length === 0 ? (
        <p
          data-testid="api-gateway-model-list-empty"
          className="rounded-lg border border-dashed bg-muted/20 px-3 py-6 text-center text-xs text-muted-foreground"
        >
          {t(
            "apiGatewayModelListEmpty",
            "No local models are served by enabled upstream providers yet.",
          )}
        </p>
      ) : visibleRows.length === 0 ? (
        <p
          data-testid="api-gateway-model-list-no-match"
          className="rounded-lg border border-dashed bg-muted/20 px-3 py-6 text-center text-xs text-muted-foreground"
        >
          {t("apiGatewayModelListNoMatch", "No models match the current search.")}
        </p>
      ) : (
        <div className="overflow-x-auto rounded-xl border bg-card shadow-xs">
          <table
            data-testid="api-gateway-model-list-table"
            className="w-full border-collapse text-xs"
          >
            <thead>
              <tr className="border-b bg-muted/40 text-left text-muted-foreground">
                <th className="px-3 py-2 font-medium">
                  {t("apiGatewayModelListIdColumn", "Model ID")}
                </th>
                <th className="px-3 py-2 font-medium">
                  {t("apiGatewayModelListNameColumn", "Model name")}
                </th>
                <th className="px-3 py-2 font-medium">
                  {t("apiGatewayModelListUpstreamColumn", "Upstream models")}
                </th>
              </tr>
            </thead>
            <tbody>
              {visibleRows.map(({ entry, name }) => (
                <tr
                  key={entry.model}
                  data-testid="api-gateway-model-list-row"
                  data-model={entry.model}
                  className="border-b align-top last:border-0"
                >
                  <td
                    data-testid="api-gateway-model-list-id"
                    className="px-3 py-2 font-mono text-foreground"
                  >
                    {entry.model}
                  </td>
                  <td
                    data-testid="api-gateway-model-list-name"
                    className="px-3 py-2 text-foreground"
                  >
                    {name}
                  </td>
                  <td
                    data-testid="api-gateway-model-list-upstreams"
                    className="px-3 py-2"
                  >
                    <ul className="space-y-1.5">
                      {entry.providers.map((upstream, index) => (
                        <li
                          key={`${upstream.providerId}-${upstream.upstreamModel}-${index}`}
                          data-testid="api-gateway-model-list-upstream"
                          data-default={upstream.isDefault ? "true" : "false"}
                          className="flex flex-wrap items-center gap-2"
                        >
                          <span
                            data-testid="api-gateway-model-list-upstream-provider"
                            className="font-medium text-foreground"
                          >
                            {upstream.providerName}
                          </span>
                          <span aria-hidden="true" className="text-muted-foreground">
                            →
                          </span>
                          <code
                            data-testid="api-gateway-model-list-upstream-model"
                            className="font-mono text-foreground"
                          >
                            {upstream.upstreamModel}
                          </code>
                          {upstream.isDefault ? (
                            <span
                              data-testid="api-gateway-model-list-upstream-default"
                              className="rounded bg-primary/10 px-1.5 py-0.5 text-[10px] font-medium text-primary"
                            >
                              {t("apiGatewayAggregatedModelDefaultBadge", "Default")}
                            </span>
                          ) : null}
                        </li>
                      ))}
                    </ul>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
