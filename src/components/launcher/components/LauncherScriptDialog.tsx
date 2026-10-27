import { ShieldAlert } from "lucide-react";
import type { TFunction } from "i18next";
import type { LauncherItem } from "../types";

type LauncherScriptDialogProps = {
  t: TFunction;
  pendingScriptItem: LauncherItem;
  trustOnConfirm: boolean;
  setTrustOnConfirm: (value: boolean) => void;
  onCancel: () => void;
  onConfirm: () => void;
};

export function LauncherScriptDialog({
  t,
  pendingScriptItem,
  trustOnConfirm,
  setTrustOnConfirm,
  onCancel,
  onConfirm,
}: LauncherScriptDialogProps) {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-background/80 backdrop-blur-sm p-4">
      <div className="bg-card border rounded-xl shadow-lg w-full max-w-sm overflow-hidden animate-in fade-in zoom-in-95 duration-200">
        <div className="p-5 space-y-3">
          <div className="flex items-center gap-3 text-amber-600 dark:text-amber-500">
            <div className="bg-amber-500/10 p-2 rounded-full">
              <ShieldAlert className="w-5 h-5" />
            </div>
            <h3 className="font-semibold">
              {t("launcherScriptConfirmTitle", "Run untrusted command?")}
            </h3>
          </div>
          <p className="text-sm text-muted-foreground break-all">
            {pendingScriptItem.target}
          </p>
          <label className="inline-flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              checked={trustOnConfirm}
              onChange={(e) => setTrustOnConfirm(e.target.checked)}
              className="w-4 h-4"
            />
            {t(
              "launcherTrustThisItem",
              "Trust this launcher item for future runs",
            )}
          </label>
        </div>
        <div className="p-4 bg-muted/30 border-t flex justify-end gap-3">
          <button
            onClick={onCancel}
            className="px-4 py-2 rounded-md text-sm font-medium hover:bg-muted transition-colors"
          >
            {t("cancel")}
          </button>
          <button
            onClick={onConfirm}
            className="px-4 py-2 rounded-md text-sm font-medium bg-primary text-primary-foreground hover:bg-primary/90 transition-colors"
          >
            {t("launch", "Launch")}
          </button>
        </div>
      </div>
    </div>
  );
}
