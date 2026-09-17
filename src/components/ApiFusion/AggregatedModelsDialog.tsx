import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  aggregateModels,
  type FusionUpstreamProtocol,
  type FusionUpstreamProvider,
} from "@/lib/apiFusion";

type AggregatedModelsDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  providers: FusionUpstreamProvider[];
};

function endpointPath(protocol: FusionUpstreamProtocol): string {
  return protocol === "responses" ? "/responses" : "/chat/completions";
}

export function AggregatedModelsDialog({
  open,
  onOpenChange,
  providers,
}: AggregatedModelsDialogProps) {
  const { t } = useTranslation();
  const models = useMemo(() => aggregateModels(providers), [providers]);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="max-h-[90vh] w-full sm:max-w-3xl overflow-y-auto sm:rounded-xl p-5"
        data-testid="api-fusion-aggregated-models"
      >
        <DialogHeader className="space-y-1">
          <DialogTitle className="text-base font-semibold">
            {t("apiFusionAggregatedModels", "Aggregated models")}
          </DialogTitle>
          <DialogDescription className="text-xs text-muted-foreground">
            {t(
              "apiFusionAggregatedModelsDialogDesc",
              "Local models served by enabled upstream providers and the upstream models they map to.",
            )}
          </DialogDescription>
        </DialogHeader>

        {models.length === 0 ? (
          <p
            data-testid="api-fusion-aggregated-models-empty"
            className="rounded-lg border border-dashed bg-muted/20 px-3 py-6 text-center text-xs text-muted-foreground"
          >
            {t(
              "apiFusionAggregatedModelsEmpty",
              "No local models are served by enabled upstream providers yet.",
            )}
          </p>
        ) : (
          <ul className="space-y-2">
            {models.map((row) => (
              <li
                key={row.model}
                data-testid="api-fusion-aggregated-model"
                data-model={row.model}
                className="rounded-xl border bg-muted/20 p-3.5"
              >
                <div className="text-sm font-semibold text-foreground">
                  {row.model}
                </div>
                <ul className="mt-2 space-y-1.5">
                  {row.providers.map((entry, index) => (
                    <li
                      key={`${entry.providerId}-${entry.upstreamModel}-${index}`}
                      data-testid="api-fusion-aggregated-model-provider"
                      className="flex flex-wrap items-center gap-2 text-xs"
                    >
                      <span className="font-medium text-foreground">
                        {entry.providerName}
                      </span>
                      <span aria-hidden="true" className="text-muted-foreground">
                        →
                      </span>
                      <code className="font-mono text-foreground">
                        {entry.upstreamModel}
                      </code>
                      {entry.isDefault ? (
                        <span className="rounded bg-primary/10 px-1.5 py-0.5 text-[10px] font-medium text-primary">
                          {t("apiFusionAggregatedModelDefaultBadge", "Default")}
                        </span>
                      ) : null}
                      <span className="rounded bg-muted px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground">
                        {endpointPath(entry.endpoint)}
                      </span>
                    </li>
                  ))}
                </ul>
              </li>
            ))}
          </ul>
        )}
      </DialogContent>
    </Dialog>
  );
}
