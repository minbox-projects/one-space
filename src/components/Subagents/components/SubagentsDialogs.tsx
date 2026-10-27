import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { Download, FolderOpen, RefreshCw, Trash2 } from 'lucide-react';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from '../../ui/dialog';
import { subagentModelOptions } from '../../subagentsModelOptions';
import { renderDiffDocument } from '../helpers';
import type { SubagentsView } from '../view';

export function SubagentsCatalogDetailDialog({ view }: { view: SubagentsView }) {
  const {
    t,
    catalogDetailOpen,
    setCatalogDetailOpen,
    catalogDetailData,
    setCatalogDetailData,
    setCatalogDetailInstallTarget,
    handleOpenCatalogFolder,
    loading,
    handleOpenReloadPreview,
    hasInstallableRepoModels,
    catalogDetailInstallTarget,
    handleInstallFromCatalogDetail,
  } = view;
  return (
    <Dialog
      open={catalogDetailOpen}
      onOpenChange={(open) => {
        setCatalogDetailOpen(open);
        if (!open) {
          setCatalogDetailData(null);
          setCatalogDetailInstallTarget(null);
        }
      }}
    >
      {catalogDetailOpen && (
        <DialogContent className="max-w-4xl h-[85vh] max-h-[85vh] p-0 gap-0 overflow-hidden grid-rows-[auto,minmax(0,1fr),auto]">
          <DialogHeader className="px-6 pt-6 pb-4 border-b">
            <DialogTitle>{catalogDetailData?.subagent.name}</DialogTitle>
            <DialogDescription>{catalogDetailData?.subagent.description}</DialogDescription>
          </DialogHeader>
          <div className="px-6 py-4 min-h-0 overflow-auto">
            <div className="border rounded-md p-4 prose prose-sm dark:prose-invert max-w-none">
              <ReactMarkdown remarkPlugins={[remarkGfm]}>{catalogDetailData?.markdown || ''}</ReactMarkdown>
            </div>
          </div>
          <DialogFooter className="border-t px-6 py-4 flex items-center gap-2">
            <button
              className="px-4 py-2 border rounded-md text-sm font-medium inline-flex items-center gap-2 hover:bg-muted disabled:opacity-50"
              onClick={handleOpenCatalogFolder}
              disabled={loading || !catalogDetailData}
            >
              <FolderOpen className="w-4 h-4" />
              {t('openFolder', 'Open Folder')}
            </button>
            {catalogDetailInstallTarget?.repo_key && (
              <button
                className="px-4 py-2 border rounded-md text-sm font-medium inline-flex items-center gap-2 hover:bg-muted disabled:opacity-50"
                onClick={handleOpenReloadPreview}
                disabled={loading}
              >
                {loading ? <RefreshCw className="w-4 h-4 animate-spin" /> : <RefreshCw className="w-4 h-4" />}
                {t('subagentsReload', 'Compare & Apply')}
              </button>
            )}
            {hasInstallableRepoModels(catalogDetailInstallTarget) && (
              <button
                className="px-4 py-2 bg-primary text-primary-foreground rounded-md text-sm font-medium inline-flex items-center gap-2 disabled:opacity-50"
                onClick={handleInstallFromCatalogDetail}
                disabled={loading}
              >
                {loading && <RefreshCw className="w-4 h-4 animate-spin" />}
                <Download className="w-4 h-4" />
                {t('install', 'Install')}
              </button>
            )}
          </DialogFooter>
        </DialogContent>
      )}
    </Dialog>
  );
}

export function SubagentsInstallDialog({ view }: { view: SubagentsView }) {
  const {
    t,
    installDialogOpen,
    installSubmitting,
    setInstallDialogOpen,
    setInstallTarget,
    setInstallMode,
    setInstallModels,
    installTarget,
    installAllowedModels,
    installModels,
    toggleInstallModel,
    canSubmitInstall,
    handleInstallConfirm,
  } = view;
  return (
    <Dialog
      open={installDialogOpen}
      onOpenChange={(open) => {
        if (installSubmitting && !open) return;
        setInstallDialogOpen(open);
        if (!open) {
          setInstallTarget(null);
          setInstallMode('catalog');
          setInstallModels([]);
        }
      }}
    >
      {installDialogOpen && (
        <DialogContent className="max-w-xl">
          <DialogHeader>
            <DialogTitle>{t('subagentsInstallSelectModelsTitle', 'Select models to install')}</DialogTitle>
            <DialogDescription>
              {t('subagentsInstallSelectModelsDesc', 'Choose model targets for {{name}}', {
                name: installTarget?.name || '',
              })}
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-2">
            <label className="text-sm font-medium text-muted-foreground">{t('sourceModels', 'Apply Models')}</label>
            <div className="grid grid-cols-2 gap-2">
              {installAllowedModels.map((model) => {
                const option = subagentModelOptions.find((item) => item.id === model);
                if (!option) return null;
                const active = installModels.includes(model);
                return (
                  <button
                    key={`install-model-${model}`}
                    type="button"
                    onClick={() => toggleInstallModel(model)}
                    className={`flex items-center gap-2 rounded-xl border px-3 py-2 text-sm transition-all ${
                      active
                        ? 'bg-primary text-primary-foreground border-primary shadow-sm'
                        : 'bg-background hover:bg-muted/50 text-foreground border-border'
                    }`}
                  >
                    <option.Icon className="w-4 h-4 shrink-0" />
                    <span className="truncate">{option.label}</span>
                  </button>
                );
              })}
            </div>
            {installModels.length === 0 && (
              <p className="text-xs text-destructive">{t('sourceModelsRequired', 'Select at least one model.')}</p>
            )}
          </div>
          <DialogFooter>
            <button
              type="button"
              onClick={() => {
                setInstallDialogOpen(false);
                setInstallTarget(null);
                setInstallMode('catalog');
                setInstallModels([]);
              }}
              className="px-4 py-2 border rounded-md text-sm hover:bg-muted"
              disabled={installSubmitting}
            >
              {t('cancel', 'Cancel')}
            </button>
            <button
              type="button"
              disabled={!canSubmitInstall}
              onClick={handleInstallConfirm}
              className="px-4 py-2 bg-primary text-primary-foreground rounded-md text-sm font-medium disabled:opacity-50 inline-flex items-center gap-2"
            >
              {installSubmitting && <RefreshCw className="w-4 h-4 animate-spin" />}
              {t('install', 'Install')}
            </button>
          </DialogFooter>
        </DialogContent>
      )}
    </Dialog>
  );
}

export function SubagentsInstalledDetailDialog({ view }: { view: SubagentsView }) {
  const { t, detailOpen, setDetailOpen, detailData, handleOpenFolder, handleUninstall } = view;
  return (
    <Dialog open={detailOpen} onOpenChange={setDetailOpen}>
      {detailOpen && (
        <DialogContent className="max-w-4xl h-[85vh] max-h-[85vh] p-0 gap-0 overflow-hidden grid-rows-[auto,minmax(0,1fr),auto]">
          <DialogHeader className="px-6 pt-6 pb-4 border-b">
            <DialogTitle>{detailData?.subagent.name}</DialogTitle>
            <DialogDescription>{detailData?.subagent.description}</DialogDescription>
          </DialogHeader>
          <div className="px-6 py-4 min-h-0 overflow-auto">
            <div className="border rounded-md p-4 prose prose-sm dark:prose-invert max-w-none">
              <ReactMarkdown remarkPlugins={[remarkGfm]}>{detailData?.markdown || ''}</ReactMarkdown>
            </div>
          </div>
          <DialogFooter className="border-t px-6 py-4">
            <button
              className="px-4 py-2 border rounded-md text-sm hover:bg-muted inline-flex items-center gap-2 disabled:opacity-50"
              onClick={() => detailData && handleOpenFolder(detailData.subagent)}
              disabled={!detailData}
            >
              <FolderOpen className="w-4 h-4" />
              {t('openFolder', 'Open Folder')}
            </button>
            <button
              className="px-4 py-2 border rounded-md text-sm text-destructive hover:bg-destructive/10 inline-flex items-center gap-2 disabled:opacity-50"
              onClick={() => detailData && handleUninstall(detailData.subagent)}
              disabled={!detailData}
            >
              <Trash2 className="w-4 h-4" />
              {t('uninstall', 'Uninstall')}
            </button>
          </DialogFooter>
        </DialogContent>
      )}
    </Dialog>
  );
}

export function SubagentsReloadDialog({ view }: { view: SubagentsView }) {
  const {
    t,
    reloadOpen,
    setReloadOpen,
    reloadPreview,
    setReloadPreview,
    setReloadTargetRepoKey,
    reloadSelectedPath,
    setReloadSelectedPath,
    reloadSelectedFile,
    reloadSelectedDiff,
    getReloadStatusMeta,
    reloadSubmitting,
    handleApplyReload,
  } = view;
  return (
    <Dialog
      open={reloadOpen}
      onOpenChange={(open) => {
        setReloadOpen(open);
        if (!open) {
          setReloadPreview(null);
          setReloadSelectedPath('');
          setReloadTargetRepoKey(null);
        }
      }}
    >
      {reloadOpen && (
        <DialogContent className="w-[calc(100vw-2rem)] max-w-7xl h-[90vh] max-h-[90vh] p-0 gap-0 overflow-hidden grid-rows-[auto,minmax(0,1fr),auto]">
          <DialogHeader className="px-6 pt-6 pb-4 border-b">
            <DialogTitle>{t('subagentsReloadPreviewTitle', 'Compare & Apply Preview')}</DialogTitle>
            <DialogDescription>
              {t(
                'subagentsReloadPreviewDesc',
                'Compare indexed baseline and current repository snapshot before applying changes.'
              )}
            </DialogDescription>
          </DialogHeader>

          <div className="px-6 py-4 min-h-0 overflow-auto">
            <div className="space-y-3">
              <div className="text-xs text-muted-foreground">
                {reloadPreview?.before_label || t('localVersion', 'Local')}
                {' -> '}
                {reloadPreview?.after_label || t('remoteVersion', 'Remote')}
              </div>
              {(reloadPreview?.installed_models || []).length > 0 ? (
                <div className="text-xs text-muted-foreground">
                  {t('subagentsReloadInstalledModels', 'Installed models')}: {(reloadPreview?.installed_models || []).join(', ')}
                </div>
              ) : (
                <div className="text-xs text-muted-foreground">
                  {t('subagentsReloadNoInstalledModels', 'This subagent is not installed to any model.')}
                </div>
              )}

              {!reloadPreview?.has_changes ? (
                <div className="rounded-md border border-dashed px-4 py-5 text-sm text-muted-foreground">
                  {t('subagentsReloadNoChanges', 'No differences found between baseline and current repository snapshot.')}
                </div>
              ) : (
                <div className="grid grid-cols-1 lg:grid-cols-[280px,1fr] gap-3">
                  <div className="border rounded-md max-h-[58vh] overflow-auto divide-y">
                    {(reloadPreview?.changed_files || []).map((file) => {
                      const active = reloadSelectedPath === file.path;
                      const statusMeta = getReloadStatusMeta(file.status);
                      return (
                        <button
                          key={`reload-file-${file.path}`}
                          type="button"
                          onClick={() => setReloadSelectedPath(file.path)}
                          className={`w-full text-left px-3 py-2 transition-colors ${
                            active ? 'bg-muted' : 'hover:bg-muted/40'
                          }`}
                        >
                          <div className="flex items-center justify-between gap-2">
                            <span className="text-xs font-mono break-all">{file.path}</span>
                            <span className={`text-[10px] px-1.5 py-0.5 rounded border shrink-0 ${statusMeta.className}`}>
                              {statusMeta.label}
                            </span>
                          </div>
                          {file.is_binary && (
                            <div className="mt-1 text-[10px] text-muted-foreground">
                              {t('subagentsReloadBinaryFile', 'Binary file')}
                            </div>
                          )}
                        </button>
                      );
                    })}
                  </div>

                  <div className="border rounded-md p-3">
                    {!reloadSelectedFile ? (
                      <div className="text-sm text-muted-foreground">
                        {t('subagentsReloadSelectFile', 'Select a changed file to inspect details.')}
                      </div>
                    ) : reloadSelectedFile.is_binary ? (
                      <div className="text-sm text-muted-foreground">
                        {t('subagentsReloadBinaryChanged', 'Binary file changed. Line-level diff is unavailable.')}
                      </div>
                    ) : (
                      <div className="grid grid-cols-1 xl:grid-cols-2 gap-3">
                        <div className="border rounded-md p-3 max-h-[52vh] overflow-auto">
                          <div className="text-xs font-semibold mb-2">
                            {reloadPreview?.before_label || t('localVersion', 'Local')}
                          </div>
                          <div className="mb-2 text-[11px] text-amber-700 bg-amber-50 border border-amber-200 rounded px-2 py-1">
                            {t('changedLines', 'Changed lines')}: {reloadSelectedDiff?.before_changed_lines.join(', ') || '--'}
                          </div>
                          {renderDiffDocument(
                            reloadSelectedDiff?.before_content || '',
                            reloadSelectedDiff?.before_changed_lines || []
                          )}
                        </div>
                        <div className="border rounded-md p-3 max-h-[52vh] overflow-auto">
                          <div className="text-xs font-semibold mb-2">
                            {reloadPreview?.after_label || t('remoteVersion', 'Remote')}
                          </div>
                          <div className="mb-2 text-[11px] text-amber-700 bg-amber-50 border border-amber-200 rounded px-2 py-1">
                            {t('changedLines', 'Changed lines')}: {reloadSelectedDiff?.after_changed_lines.join(', ') || '--'}
                          </div>
                          {renderDiffDocument(
                            reloadSelectedDiff?.after_content || '',
                            reloadSelectedDiff?.after_changed_lines || []
                          )}
                        </div>
                      </div>
                    )}
                  </div>
                </div>
              )}
            </div>
          </div>

          <DialogFooter className="border-t px-6 py-4">
            <button
              type="button"
              className="px-4 py-2 border rounded-md text-sm hover:bg-muted"
              onClick={() => setReloadOpen(false)}
              disabled={reloadSubmitting}
            >
              {t('cancel', 'Cancel')}
            </button>
            <button
              type="button"
              className="px-4 py-2 bg-primary text-primary-foreground rounded-md text-sm font-medium inline-flex items-center gap-2 disabled:opacity-50"
              onClick={handleApplyReload}
              disabled={!reloadPreview || reloadSubmitting}
            >
              {reloadSubmitting && <RefreshCw className="w-4 h-4 animate-spin" />}
              {(reloadPreview?.installed_models || []).length > 0
                ? t('subagentsReloadApplyAndSync', 'Sync to installed models')
                : t('subagentsReloadApplyIndexOnly', 'Refresh index only')}
            </button>
          </DialogFooter>
        </DialogContent>
      )}
    </Dialog>
  );
}

export function SubagentsUpdateDiffDialog({ view }: { view: SubagentsView }) {
  const { t, diffOpen, setDiffOpen, diffData, handleApplyUpdate } = view;
  return (
    <Dialog open={diffOpen} onOpenChange={setDiffOpen}>
      {diffOpen && (
        <DialogContent className="max-w-6xl">
          <DialogHeader>
            <DialogTitle>{t('updateDiff', 'Update Diff')}</DialogTitle>
            <DialogDescription>
              {t('subagentsUpdateDiffDesc', 'Compare local and remote subagent markdown before updating')}
            </DialogDescription>
          </DialogHeader>
          <div className="grid grid-cols-1 lg:grid-cols-2 gap-3">
            <div className="border rounded-md p-3 max-h-[58vh] overflow-auto">
              <div className="text-xs font-semibold mb-2">{t('localVersion', 'Local')}</div>
              <div className="mb-2 text-[11px] text-amber-700 bg-amber-50 border border-amber-200 rounded px-2 py-1">
                {t('changedLines', 'Changed lines')}: {diffData?.local_changed_lines.join(', ') || '--'}
              </div>
              {renderDiffDocument(diffData?.local_markdown || '', diffData?.local_changed_lines || [])}
            </div>
            <div className="border rounded-md p-3 max-h-[58vh] overflow-auto">
              <div className="text-xs font-semibold mb-2">{t('remoteVersion', 'Remote')}</div>
              <div className="mb-2 text-[11px] text-amber-700 bg-amber-50 border border-amber-200 rounded px-2 py-1">
                {t('changedLines', 'Changed lines')}: {diffData?.remote_changed_lines.join(', ') || '--'}
              </div>
              {renderDiffDocument(diffData?.remote_markdown || '', diffData?.remote_changed_lines || [])}
            </div>
          </div>
          <div className="flex justify-end">
            <button
              className="px-4 py-2 bg-primary text-primary-foreground rounded-md text-sm font-medium"
              onClick={handleApplyUpdate}
            >
              {t('update', 'Update')}
            </button>
          </div>
        </DialogContent>
      )}
    </Dialog>
  );
}
