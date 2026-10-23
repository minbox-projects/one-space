import { useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  AlertTriangle,
  ChevronDown,
  ChevronUp,
  Loader2,
  RotateCcw,
  Search,
  Server,
} from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import {
  formatGatewayTimestamp,
  type GatewayModelMapping,
  type GatewayUpstreamProtocol,
  type GatewayUpstreamProvider,
} from "@/lib/aiGateway";
import { protocolBadgeClass, protocolShortLabel } from "./gatewayShared";

type RecoveryResult = Promise<boolean> | boolean | void;

type AutoDisabledModelsDialogProps = {
  providers: GatewayUpstreamProvider[];
  busy: boolean;
  /**
   * Clear one auto-disabled mapping through the existing single-mapping
   * command. The parent keeps the returned configuration authoritative.
   */
  onReenableModel?: (
    providerId: string,
    localModel: string,
    upstreamModel: string,
  ) => RecoveryResult;
};

type AutoDisabledSource = {
  key: string;
  providerId: string;
  providerName: string;
  providerEnabled: boolean;
  localModel: string;
  upstreamModel: string;
  protocols: GatewayUpstreamProtocol[];
  mappingManuallyDisabled: boolean;
  reason: string | null;
  disabledAt: number | null;
};

type AutoDisabledProviderGroup = {
  providerId: string;
  providerName: string;
  providerEnabled: boolean;
  sources: AutoDisabledSource[];
};

type AutoDisabledModelGroup = {
  localModel: string;
  providers: AutoDisabledProviderGroup[];
  providerCount: number;
  sourceCount: number;
};

type FilteredModelGroup = {
  group: AutoDisabledModelGroup;
  providers: AutoDisabledProviderGroup[];
};

const PROTOCOL_ORDER: GatewayUpstreamProtocol[] = [
  "chat_completions",
  "responses",
  "anthropic_messages",
];

const LONG_REASON_LENGTH = 140;

function compareText(left: string, right: string): number {
  if (left === right) return 0;
  return left < right ? -1 : 1;
}

function effectiveProtocol(
  provider: GatewayUpstreamProvider,
  mapping: GatewayModelMapping,
): GatewayUpstreamProtocol {
  return mapping.protocol ?? provider.protocol ?? "chat_completions";
}

function sortProtocols(
  protocols: GatewayUpstreamProtocol[],
): GatewayUpstreamProtocol[] {
  return [...new Set(protocols)].sort(
    (left, right) =>
      PROTOCOL_ORDER.indexOf(left) - PROTOCOL_ORDER.indexOf(right),
  );
}

/**
 * Build the model/provider/source hierarchy from the full provider snapshot.
 * Automatic-disable membership is a row runtime flag, so it must ignore the
 * provider-list filters and the dismissible card pills.
 */
function buildAutoDisabledGroups(
  providers: GatewayUpstreamProvider[],
): AutoDisabledModelGroup[] {
  const models = new Map<
    string,
    Map<string, Map<string, AutoDisabledSource & { selected: boolean }>>
  >();

  for (const provider of providers) {
    for (const mapping of provider.mappings ?? []) {
      if (mapping.auto_disabled !== true) continue;
      const localModel = mapping.local_model.trim();
      const upstreamModel = mapping.upstream_model.trim();
      if (!localModel || !upstreamModel) continue;

      let providerMap = models.get(localModel);
      if (!providerMap) {
        providerMap = new Map();
        models.set(localModel, providerMap);
      }
      let sourceMap = providerMap.get(provider.id);
      if (!sourceMap) {
        sourceMap = new Map();
        providerMap.set(provider.id, sourceMap);
      }

      let source = sourceMap.get(upstreamModel);
      if (!source) {
        source = {
          key: `${provider.id}\u0000${localModel}\u0000${upstreamModel}`,
          providerId: provider.id,
          providerName: provider.name.trim() || provider.id,
          providerEnabled: provider.enabled,
          localModel,
          upstreamModel,
          protocols: [],
          mappingManuallyDisabled: false,
          reason: null,
          disabledAt: null,
          selected: false,
        };
        sourceMap.set(upstreamModel, source);
      }

      const protocol = effectiveProtocol(provider, mapping);
      if (!source.protocols.includes(protocol)) {
        source.protocols.push(protocol);
      }
      if (mapping.enabled === false) {
        source.mappingManuallyDisabled = true;
      }

      const disabledAt =
        typeof mapping.disabled_at === "number" &&
        Number.isFinite(mapping.disabled_at)
          ? mapping.disabled_at
          : null;
      const reason = mapping.disabled_reason?.trim() || null;
      // Duplicate identities share one recovery action. Metadata follows the
      // latest non-null disable time; equal times keep the earlier source row.
      if (!source.selected) {
        source.selected = true;
        source.disabledAt = disabledAt;
        source.reason = reason;
      } else if (
        disabledAt !== null &&
        (source.disabledAt === null || disabledAt > source.disabledAt)
      ) {
        source.disabledAt = disabledAt;
        source.reason = reason;
      }
    }
  }

  return Array.from(models.entries())
    .map(([localModel, providerMap]) => {
      const providerGroups = Array.from(providerMap.values())
        .map((sourceMap) => {
          const sources = Array.from(sourceMap.values())
            .map(
              (source): AutoDisabledSource => ({
                key: source.key,
                providerId: source.providerId,
                providerName: source.providerName,
                providerEnabled: source.providerEnabled,
                localModel: source.localModel,
                upstreamModel: source.upstreamModel,
                protocols: sortProtocols(source.protocols),
                mappingManuallyDisabled: source.mappingManuallyDisabled,
                reason: source.reason,
                disabledAt: source.disabledAt,
              }),
            )
            .sort((left, right) =>
              compareText(left.upstreamModel, right.upstreamModel),
            );
          const first = sources[0];
          return {
            providerId: first.providerId,
            providerName: first.providerName,
            providerEnabled: first.providerEnabled,
            sources,
          };
        })
        .sort(
          (left, right) =>
            compareText(left.providerName, right.providerName) ||
            compareText(left.providerId, right.providerId),
        );

      return {
        localModel,
        providers: providerGroups,
        providerCount: providerGroups.length,
        sourceCount: providerGroups.reduce(
          (count, provider) => count + provider.sources.length,
          0,
        ),
      };
    })
    .sort((left, right) => compareText(left.localModel, right.localModel));
}

function filterGroups(
  groups: AutoDisabledModelGroup[],
  query: string,
): FilteredModelGroup[] {
  const needle = query.trim().toLowerCase();
  if (!needle) {
    return groups.map((group) => ({ group, providers: group.providers }));
  }

  const includes = (value: string) =>
    value.toLowerCase().includes(needle);

  return groups.flatMap((group) => {
    const modelMatches = includes(group.localModel);
    const providers = group.providers.flatMap((provider) => {
      const sources = modelMatches
        ? provider.sources
        : provider.sources.filter(
            (source) =>
              includes(source.providerName) ||
              includes(source.providerId) ||
              includes(source.upstreamModel),
          );
      return sources.length > 0 ? [{ ...provider, sources }] : [];
    });
    return providers.length > 0 ? [{ group, providers }] : [];
  });
}

function countAutoDisabledModels(providers: GatewayUpstreamProvider[]): number {
  const models = new Set<string>();
  for (const provider of providers) {
    for (const mapping of provider.mappings ?? []) {
      if (mapping.auto_disabled !== true) continue;
      const localModel = mapping.local_model.trim();
      if (localModel && mapping.upstream_model.trim()) {
        models.add(localModel);
      }
    }
  }
  return models.size;
}

function AutoDisabledModelsDialogContent({
  providers,
  busy,
  onReenableModel,
}: AutoDisabledModelsDialogProps) {
  const { t } = useTranslation();
  const [query, setQuery] = useState("");
  const [pendingKey, setPendingKey] = useState<string | null>(null);
  const [expandedReasons, setExpandedReasons] = useState<Set<string>>(
    () => new Set(),
  );
  const recoveryGuardRef = useRef(false);

  // The full snapshot is only grouped while the dialog is open.
  const groups = useMemo(
    () => buildAutoDisabledGroups(providers),
    [providers],
  );
  const filteredGroups = useMemo(
    () => filterGroups(groups, query),
    [groups, query],
  );

  const totalSources = useMemo(
    () =>
      groups.reduce((count, group) => count + group.sourceCount, 0),
    [groups],
  );
  const shownSources = useMemo(
    () =>
      filteredGroups.reduce(
        (count, entry) =>
          count +
          entry.providers.reduce(
            (providerCount, provider) =>
              providerCount + provider.sources.length,
            0,
          ),
        0,
      ),
    [filteredGroups],
  );

  const hasManualDisable = useMemo(
    () =>
      groups.some(
        (group) =>
          group.providers.some(
            (provider) =>
              !provider.providerEnabled ||
              provider.sources.some(
                (source) => source.mappingManuallyDisabled,
              ),
          ),
      ),
    [groups],
  );

  const allActionsDisabled = busy || pendingKey !== null;

  const recover = async (source: AutoDisabledSource) => {
    if (recoveryGuardRef.current || !onReenableModel) return;
    recoveryGuardRef.current = true;
    setPendingKey(source.key);
    try {
      await onReenableModel(
        source.providerId,
        source.localModel,
        source.upstreamModel,
      );
    } catch {
      // The parent action surface owns the error toast. Keeping the row here
      // lets the operator retry without an optimistic removal.
    } finally {
      recoveryGuardRef.current = false;
      setPendingKey(null);
    }
  };

  const toggleReason = (key: string) => {
    setExpandedReasons((previous) => {
      const next = new Set(previous);
      if (next.has(key)) {
        next.delete(key);
      } else {
        next.add(key);
      }
      return next;
    });
  };

  return (
    <DialogContent
      data-testid="ai-gateway-auto-disabled-models-dialog"
      className="flex max-h-[85vh] w-[calc(100vw-2rem)] max-w-2xl flex-col gap-0 overflow-hidden p-0 sm:max-w-2xl"
    >
      <DialogHeader className="shrink-0 border-b border-border/50 px-5 pb-3 pt-5 pr-12 text-left">
        <DialogTitle className="text-base font-semibold">
          {t(
            "aiGatewayAutoDisabledDialogTitle",
            "Auto-disabled model sources",
          )}
        </DialogTitle>
        <DialogDescription className="text-xs leading-5 text-muted-foreground">
          {t(
            "aiGatewayAutoDisabledDialogDescription",
            "Inspect mappings the gateway disabled after repeated failures. Clearing an automatic disable leaves manual provider and model switches unchanged.",
          )}
        </DialogDescription>
      </DialogHeader>

      <div className="shrink-0 space-y-2 px-5 pt-3">
        <div className="relative">
          <Search className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
          <input
            type="search"
            role="searchbox"
            aria-label={t(
              "aiGatewayAutoDisabledSearchLabel",
              "Search auto-disabled models",
            )}
            placeholder={t(
              "aiGatewayAutoDisabledSearchPlaceholder",
              "Search model, provider, or upstream ID",
            )}
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            className="h-8 w-full rounded-lg border border-border/70 bg-background pl-8 pr-3 text-xs text-foreground outline-none transition focus:border-primary focus:ring-2 focus:ring-primary/40"
          />
        </div>
        <p
          aria-live="polite"
          className="text-[11px] text-muted-foreground"
        >
          {t(
            "aiGatewayAutoDisabledSummary",
            "Showing {{shownModels}} of {{totalModels}} models · {{shownSources}} of {{totalSources}} sources",
            {
              shownModels: filteredGroups.length,
              totalModels: groups.length,
              shownSources,
              totalSources,
            },
          )}
        </p>
        {hasManualDisable ? (
          <p className="rounded-lg border border-amber-500/30 bg-amber-500/5 px-2.5 py-1.5 text-[11px] leading-4 text-amber-700 dark:text-amber-400">
            {t(
              "aiGatewayAutoDisabledManualNote",
              "Clearing an automatic disable keeps manual switches unchanged.",
            )}
          </p>
        ) : null}
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-5 pb-5 pt-3">
        {groups.length === 0 ? (
          <div className="flex flex-col items-center justify-center rounded-lg border border-dashed px-4 py-10 text-center">
            <AlertTriangle className="h-5 w-5 text-muted-foreground/70" />
            <p className="mt-2 text-xs font-medium text-foreground">
              {t(
                "aiGatewayAutoDisabledEmpty",
                "Zero auto-disabled model sources.",
              )}
            </p>
          </div>
        ) : filteredGroups.length === 0 ? (
          <div className="flex flex-col items-center justify-center rounded-lg border border-dashed px-4 py-10 text-center">
            <Search className="h-5 w-5 text-muted-foreground/70" />
            <p className="mt-2 text-xs font-medium text-foreground">
              {t(
                "aiGatewayAutoDisabledNoMatch",
                "No matching auto-disabled sources.",
              )}
            </p>
            <button
              type="button"
              onClick={() => setQuery("")}
              className="mt-3 inline-flex h-7 items-center rounded-md border bg-background px-2.5 text-[11px] font-medium transition hover:bg-muted"
            >
              {t("aiGatewayAutoDisabledClearSearch", "Clear search")}
            </button>
          </div>
        ) : (
          <div className="space-y-4">
            {filteredGroups.map(({ group, providers: visibleProviders }) => (
              <section
                key={group.localModel}
                aria-labelledby={`ai-gateway-auto-disabled-model-${group.localModel}`}
                className="rounded-lg border border-border/60 bg-card/40"
              >
                <div className="flex flex-wrap items-center gap-x-2 gap-y-1 border-b border-border/40 px-3 py-2">
                  <h3
                    id={`ai-gateway-auto-disabled-model-${group.localModel}`}
                    className="min-w-0 break-all font-mono text-xs font-semibold text-foreground"
                    title={group.localModel}
                  >
                    {group.localModel}
                  </h3>
                  <span className="inline-flex shrink-0 items-center rounded-full border border-border/60 bg-background px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground">
                    {t(
                      "aiGatewayAutoDisabledProviderCount",
                      "{{count}} provider(s)",
                      { count: group.providerCount },
                    )}
                  </span>
                  <span className="inline-flex shrink-0 items-center rounded-full border border-border/60 bg-background px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground">
                    {t(
                      "aiGatewayAutoDisabledSourceCount",
                      "{{count}} source(s)",
                      { count: group.sourceCount },
                    )}
                  </span>
                </div>

                <ul className="space-y-2 p-2.5">
                  {visibleProviders.map((provider) => (
                    <li
                      key={provider.providerId}
                      className="rounded-lg border border-border/50 bg-background/70"
                    >
                      <div className="flex flex-wrap items-center gap-x-1.5 gap-y-1 border-b border-border/30 px-2.5 py-1.5">
                        <Server className="h-3.5 w-3.5 shrink-0 text-muted-foreground/70" />
                        <span className="min-w-0 break-words text-[11px] font-semibold text-foreground">
                          {provider.providerName}
                        </span>
                        <span
                          className="min-w-0 break-all font-mono text-[10px] text-muted-foreground"
                          title={provider.providerId}
                        >
                          {provider.providerId}
                        </span>
                        {!provider.providerEnabled ? (
                          <span
                            title={t(
                              "aiGatewayAutoDisabledManualNote",
                              "Clearing an automatic disable keeps manual switches unchanged.",
                            )}
                            className="inline-flex shrink-0 items-center rounded-full border border-amber-500/40 bg-amber-500/10 px-1.5 py-0.5 text-[10px] font-medium text-amber-700 dark:text-amber-400"
                          >
                            {t(
                              "aiGatewayAutoDisabledProviderManual",
                              "Provider switch off",
                            )}
                          </span>
                        ) : null}
                      </div>

                      <ul className="divide-y divide-border/30">
                        {provider.sources.map((source) => {
                          const isPending = pendingKey === source.key;
                          const reasonExpanded = expandedReasons.has(
                            source.key,
                          );
                          const isLongReason =
                            (source.reason?.length ?? 0) > LONG_REASON_LENGTH;
                          const disabledAtLabel =
                            formatGatewayTimestamp(source.disabledAt) ?? "—";
                          const actionLabel = source.mappingManuallyDisabled
                            ? t(
                                "aiGatewayAutoDisabledClearAutoDisable",
                                "Clear auto-disable",
                              )
                            : t(
                                "aiGatewayAutoDisabledReenable",
                                "Re-enable",
                              );
                          const actionAria = t(
                            source.mappingManuallyDisabled
                              ? "aiGatewayAutoDisabledClearAria"
                              : "aiGatewayAutoDisabledReenableAria",
                            source.mappingManuallyDisabled
                              ? "Clear automatic disable for {{upstream}} for {{local}} on {{provider}}"
                              : "Re-enable {{upstream}} for {{local}} on {{provider}}",
                            {
                              upstream: source.upstreamModel,
                              local: source.localModel,
                              provider: source.providerName,
                            },
                          );
                          return (
                            <li
                              key={source.key}
                              aria-label={t(
                                "aiGatewayAutoDisabledSourceAria",
                                "Auto-disabled source {{local}} to {{upstream}} on {{provider}}",
                                {
                                  local: source.localModel,
                                  upstream: source.upstreamModel,
                                  provider: source.providerName,
                                },
                              )}
                              className="flex flex-col gap-2 px-2.5 py-2 sm:flex-row sm:items-start sm:justify-between"
                            >
                              <div className="min-w-0 flex-1 space-y-1">
                                {source.upstreamModel !==
                                source.localModel ? (
                                  <div className="flex min-w-0 flex-wrap items-baseline gap-x-1.5 gap-y-0.5">
                                    <span className="text-[10px] font-medium uppercase tracking-wide text-muted-foreground">
                                      {t(
                                        "aiGatewayAutoDisabledUpstreamLabel",
                                        "Upstream",
                                      )}
                                    </span>
                                    <span
                                      className="min-w-0 break-all font-mono text-[11px] text-foreground"
                                      title={source.upstreamModel}
                                    >
                                      {source.upstreamModel}
                                    </span>
                                  </div>
                                ) : null}

                                <div className="flex flex-wrap items-center gap-1">
                                  {source.protocols.map((protocol) => (
                                    <span
                                      key={protocol}
                                      className={`inline-flex items-center rounded-md px-1.5 py-0.5 text-[10px] font-medium leading-4 ${protocolBadgeClass(
                                        protocol,
                                      )}`}
                                    >
                                      {protocolShortLabel(protocol)}
                                    </span>
                                  ))}
                                  {source.mappingManuallyDisabled ? (
                                    <span
                                      title={t(
                                        "aiGatewayAutoDisabledManualNote",
                                        "Clearing an automatic disable keeps manual switches unchanged.",
                                      )}
                                      className="inline-flex items-center rounded-full border border-amber-500/40 bg-amber-500/10 px-1.5 py-0.5 text-[10px] font-medium text-amber-700 dark:text-amber-400"
                                    >
                                      {t(
                                        "aiGatewayAutoDisabledMappingManual",
                                        "Model switch off",
                                      )}
                                    </span>
                                  ) : null}
                                </div>

                                <div className="space-y-0.5 text-[11px] leading-4 text-muted-foreground">
                                  <p
                                    className={
                                      isLongReason && !reasonExpanded
                                        ? "line-clamp-2 break-words"
                                        : "whitespace-pre-wrap break-words"
                                    }
                                  >
                                    <span className="font-medium text-foreground/80">
                                      {t(
                                        "aiGatewayAutoDisabledReasonLabel",
                                        "Reason",
                                      )}
                                      :{" "}
                                    </span>
                                    {source.reason ? (
                                      source.reason
                                    ) : (
                                      <span className="italic">
                                        {t(
                                          "aiGatewayAutoDisabledReasonUnavailable",
                                          "Reason unavailable",
                                        )}
                                      </span>
                                    )}
                                  </p>
                                  {isLongReason ? (
                                    <button
                                      type="button"
                                      aria-expanded={reasonExpanded}
                                      aria-label={t(
                                        reasonExpanded
                                          ? "aiGatewayAutoDisabledHideReasonAria"
                                          : "aiGatewayAutoDisabledShowReasonAria",
                                        reasonExpanded
                                          ? "Show less of the reason for {{upstream}} for {{local}} on {{provider}}"
                                          : "Show the full reason for {{upstream}} for {{local}} on {{provider}}",
                                        {
                                          upstream: source.upstreamModel,
                                          local: source.localModel,
                                          provider: source.providerName,
                                        },
                                      )}
                                      onClick={() => toggleReason(source.key)}
                                      className="inline-flex items-center gap-0.5 rounded text-[11px] font-medium text-primary transition hover:underline"
                                    >
                                      {reasonExpanded ? (
                                        <ChevronUp className="h-3 w-3" />
                                      ) : (
                                        <ChevronDown className="h-3 w-3" />
                                      )}
                                      {reasonExpanded
                                        ? t(
                                            "aiGatewayAutoDisabledHideReason",
                                            "Show less",
                                          )
                                        : t(
                                            "aiGatewayAutoDisabledShowReason",
                                            "Show full reason",
                                          )}
                                    </button>
                                  ) : null}
                                  <p>
                                    {t(
                                      "aiGatewayAutoDisabledDisabledAt",
                                      "Disabled at {{time}}",
                                      { time: disabledAtLabel },
                                    )}
                                  </p>
                                </div>
                              </div>

                              <button
                                type="button"
                                onClick={() => void recover(source)}
                                disabled={
                                  allActionsDisabled || !onReenableModel
                                }
                                title={
                                  onReenableModel
                                    ? undefined
                                    : t(
                                        "aiGatewayAutoDisabledActionUnavailable",
                                        "Recovery is unavailable",
                                      )
                                }
                                aria-label={actionAria}
                                className="inline-flex h-7 shrink-0 items-center justify-center gap-1 self-start rounded-md border border-border bg-background px-2 text-[11px] font-medium text-foreground shadow-xs transition hover:bg-muted disabled:cursor-not-allowed disabled:opacity-50"
                              >
                                {isPending ? (
                                  <Loader2 className="h-3 w-3 animate-spin" />
                                ) : (
                                  <RotateCcw className="h-3 w-3" />
                                )}
                                {isPending
                                  ? t(
                                      "aiGatewayAutoDisabledReenabling",
                                      "Re-enabling…",
                                    )
                                  : actionLabel}
                              </button>
                            </li>
                          );
                        })}
                      </ul>
                    </li>
                  ))}
                </ul>
              </section>
            ))}
          </div>
        )}
      </div>
    </DialogContent>
  );
}

export function AutoDisabledModelsDialog({
  providers,
  busy,
  onReenableModel,
}: AutoDisabledModelsDialogProps) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const modelCount = useMemo(
    () => countAutoDisabledModels(providers),
    [providers],
  );

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <button
          type="button"
          data-testid="ai-gateway-auto-disabled-models"
          aria-label={t(
            "aiGatewayAutoDisabledOpenAria",
            "Open auto-disabled models ({{count}} affected models)",
            { count: modelCount },
          )}
          className="inline-flex h-8 shrink-0 items-center gap-1.5 whitespace-nowrap rounded-lg border border-border/70 bg-background px-3 text-xs font-medium text-foreground shadow-xs transition hover:bg-muted active:bg-muted/80"
        >
          <AlertTriangle className="h-3.5 w-3.5 text-amber-600 dark:text-amber-400" />
          <span>
            {t("aiGatewayAutoDisabledEntryLabel", "Auto-disabled models")}
          </span>
          <span
            data-testid="ai-gateway-auto-disabled-models-count"
            className="rounded-full border border-amber-500/30 bg-amber-500/10 px-1.5 py-0.5 text-[10px] font-semibold leading-none text-amber-700 dark:text-amber-400"
          >
            {modelCount}
          </span>
        </button>
      </DialogTrigger>
      {open ? (
        <AutoDisabledModelsDialogContent
          providers={providers}
          busy={busy}
          onReenableModel={onReenableModel}
        />
      ) : null}
    </Dialog>
  );
}
