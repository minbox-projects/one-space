import { FolderOpen, Undo2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { open } from "@tauri-apps/plugin-dialog";

export interface FolderPickerProps {
  value?: string;
  placeholder?: string;
  onChange: (path: string) => void;
  onReset?: () => void;
  isAppDirectory?: boolean;
  disabled?: boolean;
}

export function FolderPicker({
  value = "",
  placeholder,
  onChange,
  onReset,
  isAppDirectory = false,
  disabled = false,
}: FolderPickerProps) {
  const { t } = useTranslation();

  const handleBrowse = async () => {
    if (disabled) return;
    try {
      if (isAppDirectory) {
        const selected = await open({
          directory: false,
          multiple: false,
          defaultPath: "/Applications",
          filters: [{ name: "Application", extensions: ["app"] }],
        });
        if (selected && typeof selected === "string") {
          const appName = selected.split("/").pop()?.replace(".app", "") || selected;
          onChange(appName);
        }
      } else {
        const selected = await open({
          directory: true,
          multiple: false,
        });
        if (selected && typeof selected === "string") {
          onChange(selected);
        }
      }
    } catch (err: unknown) {
      console.error("Folder picker error:", err);
    }
  };

  const defaultPlaceholder =
    placeholder || t("defaultDownloadsDirPlaceholder", "默认下载至系统 Downloads 目录");

  return (
    <div className="flex gap-2">
      <input
        type="text"
        readOnly
        disabled={disabled}
        className="flex-1 bg-muted/50 border rounded-xl px-4 py-2.5 text-sm text-muted-foreground font-mono truncate cursor-default focus:outline-none"
        value={value || defaultPlaceholder}
      />
      <button
        type="button"
        disabled={disabled}
        onClick={handleBrowse}
        className="px-4 py-2.5 bg-secondary text-secondary-foreground rounded-xl text-sm font-medium hover:bg-secondary/80 transition-all active:scale-95 flex items-center gap-1.5 shrink-0 disabled:opacity-50"
      >
        <FolderOpen className="w-4 h-4" />
        <span>{t("browseFolder", "浏览")}</span>
      </button>
      {value && onReset ? (
        <button
          type="button"
          disabled={disabled}
          onClick={onReset}
          className="px-3 py-2.5 bg-muted text-muted-foreground rounded-xl text-sm font-medium hover:text-foreground hover:bg-muted/80 transition-all active:scale-95 shrink-0"
          title={t("resetDefault", "恢复默认")}
        >
          <Undo2 className="w-4 h-4" />
        </button>
      ) : null}
    </div>
  );
}
