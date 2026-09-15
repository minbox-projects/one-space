import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Check, Copy, Eye, EyeOff, KeyRound, Plus, Trash2 } from "lucide-react";
import { Switch } from "@/components/ui/switch";
import { maskSecret, type FusionKey } from "@/lib/apiFusion";

type LocalKeyListProps = {
  keys: FusionKey[];
  defaultKeyId: string | null;
  busy: boolean;
  copiedKeyId: string | null;
  onSave: (key: FusionKey) => void;
  onDelete: (keyId: string) => void;
  onSetDefault: (keyId: string) => void;
  onToggleEnabled: (key: FusionKey, enabled: boolean) => void;
  onCopy: (key: FusionKey) => void;
};

export function LocalKeyList({
  keys,
  defaultKeyId,
  busy,
  copiedKeyId,
  onSave,
  onDelete,
  onSetDefault,
  onToggleEnabled,
  onCopy,
}: LocalKeyListProps) {
  const { t } = useTranslation();
  const [labelInput, setLabelInput] = useState("");
  const [valueInput, setValueInput] = useState("");
  const [revealValue, setRevealValue] = useState(false);

  const handleAdd = () => {
    const label = labelInput.trim();
    const value = valueInput.trim();
    if (!label || !value) return;
    onSave({ id: "", label, value, enabled: true, created_at: 0 });
    setLabelInput("");
    setValueInput("");
    setRevealValue(false);
  };

  return (
    <section className="rounded-[24px] border bg-card p-5" data-testid="api-fusion-keys">
      <div className="flex items-center gap-2">
        <KeyRound className="h-4 w-4 text-muted-foreground" />
        <h3 className="text-sm font-semibold">{t("apiFusionKeys", "Local keys")}</h3>
      </div>

      <div className="mt-3 grid gap-2 sm:grid-cols-[minmax(0,1fr)_minmax(0,1.4fr)_auto]">
        <input
          type="text"
          value={labelInput}
          onChange={(event) => setLabelInput(event.target.value)}
          placeholder={t("apiFusionKeyLabelPlaceholder", "Label")}
          aria-label={t("apiFusionKeyLabel", "Label")}
          className="h-10 rounded-md border border-input bg-background px-3 text-sm"
        />
        <div className="relative">
          <input
            type={revealValue ? "text" : "password"}
            value={valueInput}
            onChange={(event) => setValueInput(event.target.value)}
            placeholder={t("apiFusionKeyValuePlaceholder", "sk-...")}
            aria-label={t("apiFusionKeyValue", "Key")}
            className="h-10 w-full rounded-md border border-input bg-background px-3 pr-10 font-mono text-sm"
          />
          <button
            type="button"
            onClick={() => setRevealValue((prev) => !prev)}
            aria-label={
              revealValue
                ? t("apiFusionHideSecret", "Hide secret")
                : t("apiFusionShowSecret", "Show secret")
            }
            title={
              revealValue
                ? t("apiFusionHideSecret", "Hide secret")
                : t("apiFusionShowSecret", "Show secret")
            }
            className="absolute right-1 top-1 inline-flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground hover:bg-muted"
          >
            {revealValue ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
          </button>
        </div>
        <button
          type="button"
          onClick={handleAdd}
          disabled={busy || !labelInput.trim() || !valueInput.trim()}
          className="inline-flex h-10 items-center justify-center gap-2 rounded-md bg-primary px-4 text-sm font-medium text-primary-foreground transition hover:bg-primary/90 disabled:opacity-50"
        >
          <Plus className="h-4 w-4" />
          {t("apiFusionAddKey", "Add key")}
        </button>
      </div>

      {keys.length === 0 ? (
        <p className="mt-4 rounded-2xl border border-dashed px-4 py-5 text-sm text-muted-foreground">
          {t("apiFusionNoKeys", "No local keys yet.")}
        </p>
      ) : (
        <ul className="mt-4 space-y-2">
          {keys.map((key) => {
            const isDefault = key.id === defaultKeyId;
            return (
              <li
                key={key.id}
                data-testid={`api-fusion-key-${key.id}`}
                className="flex flex-wrap items-center gap-3 rounded-2xl border bg-muted/10 px-4 py-3"
              >
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className="truncate text-sm font-medium">{key.label}</span>
                    {isDefault ? (
                      <span className="rounded-full bg-primary/10 px-2 py-0.5 text-[11px] font-medium text-primary">
                        {t("apiFusionDefaultKey", "Default key")}
                      </span>
                    ) : null}
                  </div>
                  <div className="mt-1 flex items-center gap-2">
                    <code
                      className="truncate font-mono text-xs text-muted-foreground"
                      data-testid={`api-fusion-key-value-${key.id}`}
                    >
                      {maskSecret(key.value)}
                    </code>
                    <button
                      type="button"
                      onClick={() => onCopy(key)}
                      aria-label={t("apiFusionCopyKeyAria", {
                        label: key.label,
                        defaultValue: `Copy key ${key.label}`,
                      })}
                      title={t("apiFusionCopyKey", "Copy key")}
                      className="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-md border bg-card text-muted-foreground hover:bg-muted"
                    >
                      {copiedKeyId === key.id ? (
                        <Check className="h-3.5 w-3.5 text-emerald-600" />
                      ) : (
                        <Copy className="h-3.5 w-3.5" />
                      )}
                    </button>
                  </div>
                </div>
                <div className="flex items-center gap-2">
                  <span className="text-xs text-muted-foreground">
                    {key.enabled ? t("apiFusionEnabled", "Enabled") : t("apiFusionDisabled", "Disabled")}
                  </span>
                  <Switch
                    aria-label={t("apiFusionToggleKeyAria", {
                      label: key.label,
                      defaultValue: `Enable key ${key.label}`,
                    })}
                    checked={key.enabled}
                    disabled={busy}
                    onCheckedChange={(checked) => onToggleEnabled(key, checked)}
                  />
                  {!isDefault && key.enabled ? (
                    <button
                      type="button"
                      onClick={() => onSetDefault(key.id)}
                      disabled={busy}
                      className="rounded-full border px-3 py-1 text-xs text-muted-foreground transition hover:bg-muted disabled:opacity-50"
                    >
                      {t("apiFusionSetDefault", "Set as default")}
                    </button>
                  ) : null}
                  <button
                    type="button"
                    onClick={() => onDelete(key.id)}
                    aria-label={t("apiFusionDeleteKeyAria", {
                      label: key.label,
                      defaultValue: `Delete key ${key.label}`,
                    })}
                    title={t("apiFusionDelete", "Delete")}
                    className="inline-flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground transition hover:bg-muted hover:text-destructive"
                  >
                    <Trash2 className="h-4 w-4" />
                  </button>
                </div>
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}
