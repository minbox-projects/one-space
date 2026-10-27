import { FolderOpen, Rocket } from "lucide-react";
import type { TFunction } from "i18next";
import { CustomSelect } from "@/components/common/CustomSelect";
import { INTERNAL_TARGETS } from "../constants";
import type { LauncherItem } from "../types";

type LauncherEditorProps = {
  t: TFunction;
  isEditing: boolean;
  editingId: string | null;
  nameInput: string;
  setNameInput: (value: string) => void;
  typeInput: LauncherItem["type"];
  setTypeInput: (value: LauncherItem["type"]) => void;
  targetInput: string;
  setTargetInput: (value: string) => void;
  pinnedInput: boolean;
  setPinnedInput: (value: boolean) => void;
  onReset: () => void;
  onSave: () => void;
  onSelectApplication: () => void;
  onSelectFolder: () => void;
};

export function LauncherEditor({
  t,
  isEditing,
  editingId,
  nameInput,
  setNameInput,
  typeInput,
  setTypeInput,
  targetInput,
  setTargetInput,
  pinnedInput,
  setPinnedInput,
  onReset,
  onSave,
  onSelectApplication,
  onSelectFolder,
}: LauncherEditorProps) {
  if (!isEditing) return null;

  return (
    <div className="bg-card border rounded-xl p-5 shadow-sm space-y-4">
      <h3 className="font-semibold flex items-center gap-2">
        <Rocket className="w-4 h-4 text-primary" />
        {editingId
          ? t("editShortcut", "Edit Shortcut")
          : t("newShortcut", "New Shortcut")}
      </h3>
      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        <div className="space-y-2">
          <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
            {t("name")}
          </label>
          <input
            type="text"
            placeholder={t("appNamePlaceholder", "e.g. My App")}
            value={nameInput}
            onChange={(e) => setNameInput(e.target.value)}
            className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
          />
        </div>
        <div className="space-y-2">
          <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
            {t("type", "Type")}
          </label>
          <CustomSelect<LauncherItem["type"]>
            value={typeInput}
            onChange={(val) => setTypeInput(val)}
            ariaLabel={t("type", "Type")}
            options={[
              {
                value: "app",
                label: t("macApp", "Mac Application (open -a)"),
              },
              {
                value: "script",
                label: t("shellCommand", "Shell Command"),
              },
              {
                value: "url",
                label: t("websiteUrl", "Website URL"),
              },
              {
                value: "folder",
                label: t("localFolder", "Local Folder"),
              },
              {
                value: "internal",
                label: t("internalAction", "Internal Action"),
              },
            ]}
          />
        </div>

        {typeInput === "internal" ? (
          <div className="space-y-2 md:col-span-2">
            <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
              {t("targetModule", "Target Module")}
            </label>
            <CustomSelect
              value={targetInput}
              onChange={(val) => setTargetInput(val)}
              ariaLabel={t("targetModule", "Target Module")}
              placeholder={t("selectModule", "Select module...")}
              options={[
                {
                  value: "",
                  label: t("selectModule", "Select module..."),
                },
                ...INTERNAL_TARGETS.map((target) => ({
                  value: target.id,
                  label: t(target.labelKey, target.fallback),
                })),
              ]}
            />
          </div>
        ) : typeInput === "app" ? (
          <div className="space-y-2 md:col-span-2">
            <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
              {t("launchTarget", "Command / Path / URL")}
            </label>
            <div className="flex gap-2">
              <input
                type="text"
                placeholder={t(
                  "selectAppFromApplications",
                  "Choose app from Applications",
                )}
                value={targetInput}
                readOnly
                className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
              />
              <button
                onClick={onSelectApplication}
                className="bg-secondary text-secondary-foreground hover:bg-secondary/80 px-4 py-2 rounded-md flex items-center gap-2 text-sm font-medium transition-colors shrink-0"
              >
                <FolderOpen className="w-4 h-4" />
                {t("browse", "Browse")}
              </button>
            </div>
          </div>
        ) : typeInput === "folder" ? (
          <div className="space-y-2 md:col-span-2">
            <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
              {t("launchTarget", "Command / Path / URL")}
            </label>
            <div className="flex gap-2">
              <input
                type="text"
                placeholder={t("selectFolderPath", "Choose folder")}
                value={targetInput}
                readOnly
                className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
              />
              <button
                onClick={onSelectFolder}
                className="bg-secondary text-secondary-foreground hover:bg-secondary/80 px-4 py-2 rounded-md flex items-center gap-2 text-sm font-medium transition-colors shrink-0"
              >
                <FolderOpen className="w-4 h-4" />
                {t("browse", "Browse")}
              </button>
            </div>
          </div>
        ) : (
          <div className="space-y-2 md:col-span-2">
            <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
              {t("launchTarget", "Command / Path / URL")}
            </label>
            <input
              type="text"
              placeholder={t("pathOrUrlPlaceholder", "Path or URL...")}
              value={targetInput}
              onChange={(e) => setTargetInput(e.target.value)}
              className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm font-mono"
            />
          </div>
        )}

        <div className="md:col-span-2">
          <label className="inline-flex items-center gap-2 text-sm text-muted-foreground">
            <input
              type="checkbox"
              checked={pinnedInput}
              onChange={(e) => setPinnedInput(e.target.checked)}
              className="w-4 h-4"
            />
            {t("pinShortcut", "Pin this shortcut")}
          </label>
        </div>
      </div>

      <div className="flex justify-end gap-3 pt-2">
        <button
          onClick={onReset}
          className="px-4 py-2 rounded-md text-sm font-medium hover:bg-muted transition-colors"
        >
          {t("cancel")}
        </button>
        <button
          onClick={onSave}
          disabled={!nameInput.trim() || !targetInput.trim()}
          className="bg-primary text-primary-foreground hover:bg-primary/90 px-4 py-2 rounded-md text-sm font-medium transition-colors disabled:opacity-50"
        >
          {t("save")}
        </button>
      </div>
    </div>
  );
}
