import { useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import type { ClaudeModelSelection } from "@/lib/aiGateway";
import {
  CLAUDE_COMPACTION_CAP,
  CLAUDE_COMPACTION_MIN,
  formatContextWindowCompact,
  knownContextWindow,
} from "@/lib/aiGateway";

type ClaudeModelSelectionDialogProps = {
  open: boolean;
  /** Candidate local mapping ids that a native Messages upstream can serve. */
  models: string[];
  /** Declared context per candidate; absent/`null` means unknown. */
  contextByModel?: Record<string, number | null>;
  /** Existing valid selection to reuse; absent/`null` starts empty. */
  initialSelection?: ClaudeModelSelection | null;
  busy?: boolean;
  onSubmit: (selection: ClaudeModelSelection) => void;
  onCancel: () => void;
};

type SlotKey = keyof ClaudeModelSelection;

const EMPTY_SELECTION: ClaudeModelSelection = {
  default_model: "",
  haiku: "",
  sonnet: "",
  opus: "",
};

const SLOTS: ReadonlyArray<{
  field: SlotKey;
  labelKey: string;
  labelFallback: string;
  /**
   * Alias slots (haiku/sonnet/opus) repeat the same candidate list, so their
   * options get a slot-qualified accessible name; the default slot keeps the
   * plain model id so a candidate can always be addressed by its exact id.
   */
  scopeOptionName: boolean;
}> = [
  {
    field: "default_model",
    labelKey: "aiGatewayClaudeDefaultModel",
    labelFallback: "Default model",
    scopeOptionName: false,
  },
  {
    field: "haiku",
    labelKey: "aiGatewayClaudeHaikuModel",
    labelFallback: "Haiku model",
    scopeOptionName: true,
  },
  {
    field: "sonnet",
    labelKey: "aiGatewayClaudeSonnetModel",
    labelFallback: "Sonnet model",
    scopeOptionName: true,
  },
  {
    field: "opus",
    labelKey: "aiGatewayClaudeOpusModel",
    labelFallback: "Opus model",
    scopeOptionName: true,
  },
];

/** A selection is submittable only when every slot holds a non-blank id. */
function isComplete(selection: ClaudeModelSelection): boolean {
  return SLOTS.every(({ field }) => selection[field].trim() !== "");
}

export function ClaudeModelSelectionDialog({
  open,
  models,
  contextByModel,
  initialSelection,
  busy,
  onSubmit,
  onCancel,
}: ClaudeModelSelectionDialogProps) {
  const { t } = useTranslation();
  const selectableModels = Array.from(
    new Set([...models, ...Object.keys(contextByModel ?? {})]),
  );
  const [selection, setSelection] = useState<ClaudeModelSelection>(
    () => initialSelection ?? EMPTY_SELECTION,
  );
  // Refill from the saved selection (or blank) each time the dialog opens,
  // without a state-synchronizing effect: adjusting state during render when a
  // prop changes is React's documented reset pattern.
  const [wasOpen, setWasOpen] = useState(open);
  if (open !== wasOpen) {
    setWasOpen(open);
    if (open) setSelection(initialSelection ?? EMPTY_SELECTION);
  }

  const handleChange = (field: SlotKey, value: string) => {
    setSelection((prev) => ({ ...prev, [field]: value }));
  };

  const handleSubmit = () => {
    if (busy || !isComplete(selection)) return;
    onSubmit({ ...selection });
  };

  // The four slots may repeat one model id, so coverage and the shared minimum
  // are computed over the deduplicated choices, exactly like the backend.
  const selectedModels = Array.from(
    new Set(
      SLOTS.map(({ field }) => selection[field].trim()).filter(
        (value) => value !== "",
      ),
    ),
  );
  const complete = isComplete(selection);
  const declaredValues = selectedModels.map((model) =>
    knownContextWindow(contextByModel?.[model]),
  );
  const allKnown =
    complete && declaredValues.length > 0 && declaredValues.every((value) => value !== null);
  const minimum = allKnown
    ? Math.min(...(declaredValues as number[]))
    : null;
  const applied =
    minimum !== null && minimum >= CLAUDE_COMPACTION_MIN
      ? Math.min(minimum, CLAUDE_COMPACTION_CAP)
      : null;
  const capped = minimum !== null && minimum > CLAUDE_COMPACTION_CAP;
  const coverageKnown = declaredValues.filter((value) => value !== null).length;

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) onCancel();
      }}
    >
      <DialogContent
        className="w-full p-5 sm:max-w-md sm:rounded-xl"
        data-testid="claude-model-selection-dialog"
      >
        <DialogHeader className="space-y-1">
          <DialogTitle className="text-base font-semibold">
            {t("aiGatewayClaudeModelDialogTitle", "Select Claude models")}
          </DialogTitle>
          <DialogDescription className="text-xs text-muted-foreground">
            {t(
              "aiGatewayClaudeModelDialogDesc",
              "Choose the local models Claude Code uses for the default slot and the haiku/sonnet/opus aliases.",
            )}
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-3 py-2">
          {SLOTS.map(({ field, labelKey, labelFallback, scopeOptionName }) => (
            <div className="field full-span" key={field}>
              <label htmlFor={`claude-model-${field}`}>
                {t(labelKey, labelFallback)}
              </label>
              <select
                id={`claude-model-${field}`}
                data-testid={`claude-model-select-${field === "default_model" ? "default" : field}`}
                value={selection[field]}
                onChange={(event) => handleChange(field, event.target.value)}
                disabled={busy}
                aria-label={t(labelKey, labelFallback)}
              >
                <option value="">
                  {t("aiGatewayClaudeModelPlaceholder", "Select a local model")}
                </option>
                {selectableModels.map((model) => (
                  <option
                    key={model}
                    value={model}
                    aria-label={
                      scopeOptionName
                        ? `${t(labelKey, labelFallback)}: ${model}`
                        : undefined
                    }
                  >
                    {model}
                  </option>
                ))}
              </select>
            </div>
          ))}
        </div>

        <div
          role="status"
          aria-live="polite"
          data-testid="claude-context-summary"
          className="space-y-1 rounded-lg border border-border/60 bg-muted/20 px-3 py-2 text-[11px] leading-relaxed text-muted-foreground"
        >
          {!complete ? (
            <div>
              {t(
                "aiGatewayClaudeContextIncomplete",
                "Unavailable: select a model for every slot",
              )}
            </div>
          ) : minimum === null ? (
            <div className="font-medium text-amber-600 dark:text-amber-400">
              {t(
                "aiGatewayClaudeContextUnknown",
                "Unavailable: unknown context for the selected models",
              )}
            </div>
          ) : applied === null ? (
            <div className="font-medium text-amber-600 dark:text-amber-400">
              {t(
                "aiGatewayClaudeContextBelowMinimum",
                "Unavailable: below the 100K compaction minimum",
              )}
            </div>
          ) : (
            <div className="font-medium text-foreground/85">
              {capped
                ? t("aiGatewayClaudeContextCapped", {
                    value: formatContextWindowCompact(applied),
                    raw: formatContextWindowCompact(minimum),
                    defaultValue:
                      "Shared compaction threshold: {{value}} (capped from {{raw}})",
                  })
                : t("aiGatewayClaudeContextShared", {
                    value: formatContextWindowCompact(applied),
                    defaultValue:
                      "Shared compaction threshold: {{value}}",
                  })}
            </div>
          )}
          {complete && selectedModels.length > 0 ? (
            <div>
              {t("aiGatewayClaudeContextCoverage", {
                known: coverageKnown,
                total: selectedModels.length,
                defaultValue: "Coverage: {{known}} of {{total}} known",
              })}
            </div>
          ) : null}
        </div>

        <DialogFooter className="flex flex-row items-center justify-end gap-2 pt-2">
          <button
            type="button"
            onClick={onCancel}
            disabled={busy}
            data-testid="claude-model-selection-cancel"
            className="acc-panel-btn"
          >
            {t("cancel", "Cancel")}
          </button>
          <button
            type="button"
            onClick={handleSubmit}
            disabled={busy || !isComplete(selection)}
            data-testid="claude-model-selection-submit"
            className="acc-panel-btn primary"
          >
            {t("aiGatewaySave", "Save")}
          </button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
