import { LauncherToolbar } from "./launcher/components/LauncherToolbar";
import { LauncherEditor } from "./launcher/components/LauncherEditor";
import { LauncherGrid } from "./launcher/components/LauncherGrid";
import { LauncherScriptDialog } from "./launcher/components/LauncherScriptDialog";
import { useLauncherController } from "./launcher/hooks/useLauncherController";

export function Launcher({ isVisible = true }: { isVisible?: boolean }) {
  const controller = useLauncherController(isVisible);

  return (
    <div className="flex flex-col h-full space-y-5">
      <LauncherToolbar
        t={controller.t}
        searchTerm={controller.searchTerm}
        setSearchTerm={controller.setSearchTerm}
        onImport={controller.handleImport}
        onExport={controller.handleExport}
        onCreate={controller.startCreate}
      />

      {controller.isEditing ? (
        <LauncherEditor
          t={controller.t}
          isEditing={controller.isEditing}
          editingId={controller.editingId}
          nameInput={controller.nameInput}
          setNameInput={controller.setNameInput}
          typeInput={controller.typeInput}
          setTypeInput={controller.setTypeInput}
          targetInput={controller.targetInput}
          setTargetInput={controller.setTargetInput}
          pinnedInput={controller.pinnedInput}
          setPinnedInput={controller.setPinnedInput}
          onReset={controller.resetEditor}
          onSave={controller.handleSave}
          onSelectApplication={controller.handleSelectApplication}
          onSelectFolder={controller.handleSelectFolder}
        />
      ) : null}

      <LauncherGrid
        t={controller.t}
        loading={controller.loading}
        searchTerm={controller.searchTerm}
        groupedItems={controller.groupedItems}
        quickInternalTools={controller.quickInternalTools}
        drag={controller.drag}
        typeLabelMap={controller.typeLabelMap}
        pinnedOrderIds={controller.pinnedOrderIds}
        appIconCache={controller.appIconCache}
        appIconCacheKey={controller.appIconCacheKey}
        formatRelativeTime={controller.formatRelativeTime}
        onOpenInternalTarget={controller.openInternalTarget}
        onLaunch={controller.handleLaunch}
        onTogglePin={controller.handleTogglePin}
        onMovePinned={controller.handleMovePinned}
        onEdit={controller.startEdit}
        onDelete={controller.handleDelete}
      />

      {controller.pendingScriptItem ? (
        <LauncherScriptDialog
          t={controller.t}
          pendingScriptItem={controller.pendingScriptItem}
          trustOnConfirm={controller.trustOnConfirm}
          setTrustOnConfirm={controller.setTrustOnConfirm}
          onCancel={controller.cancelScriptLaunch}
          onConfirm={controller.confirmScriptLaunch}
        />
      ) : null}
    </div>
  );
}
