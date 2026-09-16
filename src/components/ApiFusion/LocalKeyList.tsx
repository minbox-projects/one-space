import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Check, Copy, KeyRound, Plus, ShieldCheck, Trash2 } from "lucide-react";
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

  const handleAdd = () => {
    const label = labelInput.trim();
    if (!label) return;
    onSave({ id: "", label, value: "", enabled: true, created_at: 0 });
    setLabelInput("");
  };

  return (
    <section className="space-y-4" data-testid="api-fusion-keys">
      {/* 头部说明与添加栏 */}
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="space-y-0.5">
          <div className="flex items-center gap-2">
            <KeyRound className="h-4 w-4 text-indigo-600" />
            <h3 className="text-base font-semibold">
              {t("apiFusionKeys", "Local keys")}
            </h3>
            <span className="rounded-full bg-muted px-2 py-0.5 text-xs font-medium text-muted-foreground">
              {keys.length}
            </span>
          </div>
          <p className="text-xs text-muted-foreground">
            {t(
              "apiFusionKeysDesc",
              "Manage local bearer tokens used by clients to authenticate with this relay.",
            )}
          </p>
        </div>

        {/* 快速添加 Key 输入框与按钮 */}
        <div className="flex items-center gap-2">
          <input
            type="text"
            value={labelInput}
            onChange={(event) => setLabelInput(event.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && labelInput.trim() && !busy) {
                handleAdd();
              }
            }}
            placeholder={t("apiFusionKeyLabelPlaceholder", "Key name / label...")}
            aria-label={t("apiFusionKeyLabel", "Label")}
            className="h-9 w-48 rounded-lg border border-input bg-background px-3 text-xs focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring sm:w-64"
          />
          <button
            type="button"
            onClick={handleAdd}
            disabled={busy || !labelInput.trim()}
            className="inline-flex h-9 items-center justify-center gap-1.5 rounded-lg bg-primary px-3 text-xs font-medium text-primary-foreground shadow-sm transition hover:bg-primary/90 disabled:opacity-50"
          >
            <Plus className="h-3.5 w-3.5" />
            {t("apiFusionAddKey", "Add key")}
          </button>
        </div>
      </div>

      {keys.length === 0 ? (
        <div className="flex flex-col items-center justify-center rounded-2xl border border-dashed bg-card/50 px-6 py-12 text-center">
          <div className="rounded-full bg-muted/60 p-3 text-muted-foreground">
            <KeyRound className="h-6 w-6" />
          </div>
          <h4 className="mt-3 text-sm font-medium">
            {t("apiFusionNoKeys", "No local keys yet.")}
          </h4>
          <p className="mt-1 max-w-sm text-xs text-muted-foreground">
            {t(
              "apiFusionNoKeysGuide",
              "Create a local key to start accessing the proxy service securely from external tools.",
            )}
          </p>
        </div>
      ) : (
        <div className="space-y-2.5">
          {keys.map((key) => {
            const isDefault = key.id === defaultKeyId;
            const isCopied = copiedKeyId === key.id;

            return (
              <div
                key={key.id}
                data-testid={`api-fusion-key-${key.id}`}
                className={`flex flex-wrap items-center justify-between gap-3 rounded-2xl border bg-card p-3.5 shadow-sm transition hover:border-primary/40 ${
                  isDefault ? "border-primary/30 bg-primary/[0.02]" : ""
                }`}
              >
                {/* 密钥信息 */}
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className="truncate text-sm font-semibold">{key.label}</span>
                    {isDefault ? (
                      <span className="inline-flex items-center gap-1 rounded-full bg-primary/10 px-2 py-0.5 text-[11px] font-medium text-primary">
                        <ShieldCheck className="h-3 w-3" />
                        {t("apiFusionDefaultKey", "Default key")}
                      </span>
                    ) : null}
                  </div>

                  <div className="mt-1.5 flex items-center gap-2">
                    <code
                      className="rounded bg-muted/60 px-2 py-0.5 font-mono text-xs text-muted-foreground"
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
                      className="inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-md border bg-background text-muted-foreground transition hover:bg-muted hover:text-foreground"
                    >
                      {isCopied ? (
                        <Check className="h-3 w-3 text-emerald-600 dark:text-emerald-400" />
                      ) : (
                        <Copy className="h-3 w-3" />
                      )}
                    </button>
                  </div>
                </div>

                {/* 状态开关与操作 */}
                <div className="flex items-center gap-2.5">
                  <div className="flex items-center gap-1.5">
                    <span className="text-xs text-muted-foreground">
                      {key.enabled
                        ? t("apiFusionEnabled", "Enabled")
                        : t("apiFusionDisabled", "Disabled")}
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
                  </div>

                  {!isDefault && key.enabled ? (
                    <button
                      type="button"
                      onClick={() => onSetDefault(key.id)}
                      disabled={busy}
                      className="rounded-lg border bg-background px-2.5 py-1 text-xs font-medium text-muted-foreground transition hover:bg-muted hover:text-foreground disabled:opacity-50"
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
                    className="inline-flex h-8 w-8 items-center justify-center rounded-lg text-muted-foreground transition hover:bg-destructive/10 hover:text-destructive disabled:opacity-50"
                  >
                    <Trash2 className="h-4 w-4" />
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      )}

      {/* 使用说明底栏 */}
      <div className="rounded-xl border bg-muted/20 p-3 text-xs text-muted-foreground">
        <span className="font-medium text-foreground">
          {t("apiFusionUsageTipTitle", "How to use:")}
        </span>{" "}
        {t(
          "apiFusionUsageTipDesc",
          "Pass the key as Bearer token in the Authorization header: `Authorization: Bearer <key>`.",
        )}
      </div>
    </section>
  );
}
