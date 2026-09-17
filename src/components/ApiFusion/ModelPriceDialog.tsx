import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Plus, Trash2 } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  apiFusionModelPricesGet,
  apiFusionModelPricesSave,
  type ModelPrice,
} from "@/lib/apiFusion";
import { errorToMessage } from "@/lib/messages";

type DraftPrice = {
  upstream_model: string;
  input: string;
  cache_read: string;
  cache_write: string;
  output: string;
};

type ModelPriceDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSaved?: (prices: ModelPrice[]) => void;
};

const priceInputClass =
  "w-full rounded-lg border border-border bg-background px-3 py-2 text-sm text-foreground outline-none transition-all focus:border-primary focus:ring-2 focus:ring-primary/40";

function toDraft(price: ModelPrice): DraftPrice {
  return {
    upstream_model: price.upstream_model,
    input: String(price.input),
    cache_read: String(price.cache_read),
    cache_write: String(price.cache_write),
    output: String(price.output),
  };
}

function parsePriceNumber(value: string): number {
  const parsed = Number(value.trim());
  return Number.isFinite(parsed) ? parsed : 0;
}

export function ModelPriceDialog({
  open,
  onOpenChange,
  onSaved,
}: ModelPriceDialogProps) {
  const { t } = useTranslation();
  const [drafts, setDrafts] = useState<DraftPrice[]>([]);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    setLoading(true);
    setError("");
    apiFusionModelPricesGet()
      .then((prices) => {
        if (cancelled) return;
        setDrafts((prices ?? []).map(toDraft));
      })
      .catch((err) => {
        if (cancelled) return;
        setError(errorToMessage(err));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [open]);

  const updateDraft = (index: number, patch: Partial<DraftPrice>) => {
    setDrafts((prev) =>
      prev.map((draft, draftIndex) =>
        draftIndex === index ? { ...draft, ...patch } : draft,
      ),
    );
  };

  const addDraft = () => {
    setDrafts((prev) => [
      ...prev,
      {
        upstream_model: "",
        input: "0",
        cache_read: "0",
        cache_write: "0",
        output: "0",
      },
    ]);
  };

  const removeDraft = (index: number) => {
    setDrafts((prev) => prev.filter((_draft, draftIndex) => draftIndex !== index));
  };

  const handleSave = async () => {
    setSaving(true);
    setError("");
    try {
      const prices: ModelPrice[] = drafts
        .map((draft) => ({
          upstream_model: draft.upstream_model.trim(),
          input: parsePriceNumber(draft.input),
          cache_read: parsePriceNumber(draft.cache_read),
          cache_write: parsePriceNumber(draft.cache_write),
          output: parsePriceNumber(draft.output),
        }))
        .filter((price) => price.upstream_model !== "");
      const saved = await apiFusionModelPricesSave(prices);
      onSaved?.(saved ?? prices);
      onOpenChange(false);
    } catch (err) {
      setError(errorToMessage(err));
    } finally {
      setSaving(false);
    }
  };

  const columns: Array<{
    key: keyof Omit<DraftPrice, "upstream_model">;
    label: string;
  }> = [
    { key: "input", label: t("apiFusionPriceInput", "Input") },
    { key: "cache_read", label: t("apiFusionPriceCacheRead", "Cache read") },
    { key: "cache_write", label: t("apiFusionPriceCacheWrite", "Cache write") },
    { key: "output", label: t("apiFusionPriceOutput", "Output") },
  ];

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="max-h-[90vh] w-full overflow-y-auto p-5 sm:max-w-3xl sm:rounded-xl"
        data-testid="api-fusion-model-price-dialog"
      >
        <DialogHeader className="space-y-1">
          <DialogTitle className="text-base font-semibold">
            {t("apiFusionModelPriceDialogTitle", "Model prices")}
          </DialogTitle>
          <DialogDescription className="text-xs text-muted-foreground">
            {t(
              "apiFusionModelPriceDialogDesc",
              "Maintain four price tiers per upstream model name. Exact model name matching.",
            )}
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-3 py-2">
          <div className="flex items-center justify-between gap-3">
            <span className="text-xs text-muted-foreground">
              {t("apiFusionPricePerMillion", "USD / million tokens")}
            </span>
            <button
              type="button"
              onClick={addDraft}
              className="inline-flex h-8 items-center gap-1.5 rounded-md border bg-background px-2.5 text-xs font-medium shadow-sm transition hover:bg-muted"
            >
              <Plus className="h-3.5 w-3.5" />
              {t("apiFusionAddPrice", "Add price")}
            </button>
          </div>

          {error ? (
            <div
              role="alert"
              className="rounded-lg border border-destructive/20 bg-destructive/10 px-3 py-2 text-xs text-destructive"
            >
              {error}
            </div>
          ) : null}

          {loading ? (
            <p className="rounded-lg border border-dashed bg-muted/20 px-3 py-4 text-center text-xs text-muted-foreground">
              {t("loading", "Loading...")}
            </p>
          ) : drafts.length === 0 ? (
            <p className="rounded-lg border border-dashed bg-muted/20 px-3 py-4 text-center text-xs text-muted-foreground">
              {t("apiFusionModelPricesEmpty", "No model prices configured yet.")}
            </p>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-left text-xs">
                <thead className="text-muted-foreground">
                  <tr>
                    <th className="px-2 py-1.5 font-medium">
                      {t("apiFusionPriceModel", "Upstream model")}
                    </th>
                    {columns.map((column) => (
                      <th key={column.key} className="px-2 py-1.5 font-medium">
                        {column.label}
                      </th>
                    ))}
                    <th className="w-8 px-2 py-1.5" />
                  </tr>
                </thead>
                <tbody>
                  {drafts.map((draft, index) => (
                    <tr key={index} className="border-t">
                      <td className="px-2 py-1.5">
                        <input
                          type="text"
                          value={draft.upstream_model}
                          onChange={(event) =>
                            updateDraft(index, {
                              upstream_model: event.target.value,
                            })
                          }
                          placeholder={t(
                            "apiFusionPriceModelPlaceholder",
                            "e.g. gpt-4o",
                          )}
                          aria-label={t("apiFusionPriceModel", "Upstream model")}
                          className={`${priceInputClass} min-w-[10rem] font-mono`}
                        />
                      </td>
                      {columns.map((column) => (
                        <td key={column.key} className="px-2 py-1.5">
                          <input
                            type="number"
                            step="any"
                            value={draft[column.key]}
                            onChange={(event) =>
                              updateDraft(index, {
                                [column.key]: event.target.value,
                              })
                            }
                            aria-label={column.label}
                            className={`${priceInputClass} min-w-[5.5rem]`}
                          />
                        </td>
                      ))}
                      <td className="px-2 py-1.5">
                        <button
                          type="button"
                          onClick={() => removeDraft(index)}
                          aria-label={t("apiFusionDeletePriceAria", {
                            model: draft.upstream_model || "—",
                            defaultValue: `Delete price for ${draft.upstream_model || "—"}`,
                          })}
                          className="inline-flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground transition hover:bg-destructive/10 hover:text-destructive"
                        >
                          <Trash2 className="h-3.5 w-3.5" />
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>

        <DialogFooter className="flex flex-row items-center justify-end gap-2 pt-2">
          <button
            type="button"
            onClick={() => onOpenChange(false)}
            disabled={saving}
            className="rounded-lg border bg-background px-4 py-2 text-sm font-medium transition hover:bg-muted disabled:opacity-50"
          >
            {t("cancel", "Cancel")}
          </button>
          <button
            type="button"
            onClick={() => void handleSave()}
            disabled={saving || loading}
            className="rounded-lg bg-primary px-4 py-2 text-sm font-medium text-primary-foreground shadow-sm transition hover:bg-primary/90 disabled:opacity-50"
          >
            {t("apiFusionSave", "Save")}
          </button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
