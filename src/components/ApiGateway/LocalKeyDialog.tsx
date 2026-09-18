import { useState, type KeyboardEvent } from "react";
import { useTranslation } from "react-i18next";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import type { GatewayKey } from "@/lib/apiGateway";

type LocalKeyDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  busy: boolean;
  onSave: (key: GatewayKey) => Promise<boolean>;
};

export function LocalKeyDialog({
  open,
  onOpenChange,
  busy,
  onSave,
}: LocalKeyDialogProps) {
  const { t } = useTranslation();
  const [name, setName] = useState("");
  const [saving, setSaving] = useState(false);

  const handleOpenChange = (next: boolean) => {
    if (!next) setName("");
    onOpenChange(next);
  };

  const handleSave = async () => {
    if (busy || saving || !name.trim()) return;
    setSaving(true);
    try {
      const saved = await onSave({
        id: "",
        label: name.trim(),
        value: "",
        enabled: true,
        created_at: 0,
      });
      if (saved) handleOpenChange(false);
    } finally {
      setSaving(false);
    }
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter" && !busy && !saving && name.trim()) {
      event.preventDefault();
      void handleSave();
    }
  };

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent
        className="w-full p-5 sm:max-w-md sm:rounded-xl"
        data-testid="api-gateway-key-dialog"
      >
        <DialogHeader className="space-y-1">
          <DialogTitle className="text-base font-semibold">
            {t("apiGatewayAddKey", "Add key")}
          </DialogTitle>
          <DialogDescription className="text-xs text-muted-foreground">
            {t(
              "apiGatewayKeyDialogDesc",
              "Only the name is required; the key value is generated automatically.",
            )}
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4 py-2">
          <div className="field full-span">
            <label className="required">{t("apiGatewayKeyLabel", "Name")}</label>
            <input
              type="text"
              value={name}
              onChange={(event) => setName(event.target.value)}
              onKeyDown={handleKeyDown}
              placeholder={t("apiGatewayKeyLabelPlaceholder", "Key name")}
              aria-label={t("apiGatewayKeyLabel", "Name")}
              disabled={busy || saving}
            />
          </div>
        </div>

        <DialogFooter className="flex flex-row items-center justify-end gap-2 pt-2">
          <button
            type="button"
            onClick={() => handleOpenChange(false)}
            disabled={busy || saving}
            className="acc-panel-btn"
          >
            {t("cancel", "Cancel")}
          </button>
          <button
            type="button"
            onClick={() => void handleSave()}
            disabled={busy || saving || !name.trim()}
            className="acc-panel-btn primary"
          >
            {t("apiGatewaySave", "Save")}
          </button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
