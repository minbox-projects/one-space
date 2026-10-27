import { Plus, Search, Upload, Download } from "lucide-react";
import type { TFunction } from "i18next";

type LauncherToolbarProps = {
  t: TFunction;
  searchTerm: string;
  setSearchTerm: (value: string) => void;
  onImport: () => void;
  onExport: () => void;
  onCreate: () => void;
};

export function LauncherToolbar({
  t,
  searchTerm,
  setSearchTerm,
  onImport,
  onExport,
  onCreate,
}: LauncherToolbarProps) {
  return (
    <>
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-xl font-bold tracking-tight">
            {t("launcher", "Launcher")}
          </h2>
          <p className="text-sm text-muted-foreground mt-1">
            {t(
              "launcherDesc",
              "Quickly launch favorite apps, local directories, and automated workflows",
            )}
          </p>
        </div>
        <div className="flex items-center gap-2">
          <button
            onClick={onImport}
            className="border border-border/80 bg-background hover:bg-muted text-foreground px-3 py-2 rounded-lg flex items-center gap-2 text-sm font-medium transition-all shadow-subtle hover-lift"
          >
            <Upload className="w-4 h-4" />
            {t("import", "Import")}
          </button>
          <button
            onClick={onExport}
            className="border border-border/80 bg-background hover:bg-muted text-foreground px-3 py-2 rounded-lg flex items-center gap-2 text-sm font-medium transition-all shadow-subtle hover-lift"
          >
            <Download className="w-4 h-4" />
            {t("export", "Export")}
          </button>
          <button
            onClick={onCreate}
            className="bg-primary text-primary-foreground hover:bg-primary/90 px-4 py-2 rounded-lg flex items-center gap-2 text-sm font-medium transition-all shadow-sm hover-lift active:scale-[0.98]"
          >
            <Plus className="w-4 h-4" />
            {t("addShortcut", "Add Shortcut")}
          </button>
        </div>
      </div>

      <div className="relative">
        <Search className="w-4 h-4 absolute left-3 top-3 text-muted-foreground/70" />
        <input
          type="text"
          placeholder={t("searchLauncher", "Search launcher items...")}
          value={searchTerm}
          onChange={(e) => setSearchTerm(e.target.value)}
          className="w-full flex h-10 rounded-lg border border-hairline bg-muted/20 pl-9 pr-3 py-2 text-sm transition-all focus:bg-background focus:border-foreground/30 focus:outline-none focus:ring-1 focus:ring-foreground/20 shadow-subtle"
        />
      </div>
    </>
  );
}
