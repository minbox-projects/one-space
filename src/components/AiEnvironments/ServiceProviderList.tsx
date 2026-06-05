import { useMemo } from 'react';
import {
  Copy,
  Edit3,
  FolderOpen,
  Loader2,
  Play,
  Plus,
  Trash2,
  Zap,
} from 'lucide-react';
import { ServiceProviderAvatar } from './ServiceProviderAvatar';

export interface ServiceProviderListItem {
  id: string;
  name: string;
  tool: string;
  icon?: string;
  description?: string;
  authLabel?: string;
  modelTags?: string[];
  claudeUpstreamModelTags?: string[];
  apiFormatTag?: string | null;
  isGlobal: boolean;
  canLaunch?: boolean;
  canDelete?: boolean;
  launchBusy?: boolean;
  applyBusy?: boolean;
  deleteBusy?: boolean;
  copiedCommand?: boolean;
}

interface ServiceProviderListProps {
  providers: ServiceProviderListItem[];
  onProviderClick: (id: string) => void;
  onEdit: (id: string) => void;
  onApplyGlobal: (id: string) => void;
  onDelete: (id: string) => void;
  onLaunch?: (id: string) => void;
  onCopyLaunchCommand?: (id: string) => void;
  onOpenDirectory?: (id: string) => void;
  onAdd: () => void;
  tool: string;
  t?: (key: string, fallback: string, options?: Record<string, any>) => string;
  searchTerm: string;
  loading?: boolean;
}

export function ServiceProviderList({
  providers,
  onProviderClick,
  onEdit,
  onApplyGlobal,
  onDelete,
  onLaunch,
  onCopyLaunchCommand,
  onOpenDirectory,
  onAdd,
  tool,
  t,
  searchTerm,
  loading = false,
}: ServiceProviderListProps) {
  const search = searchTerm.trim().toLowerCase();
  const isClaudeTool = tool === 'claude';
  const primaryActionClass =
    'inline-flex h-8 items-center justify-center gap-1.5 rounded-md px-2.5 text-sm font-medium transition-colors disabled:opacity-60';
  const launchButtonClass = `${primaryActionClass} bg-primary text-primary-foreground hover:bg-primary/90`;
  const activateButtonClass = `${primaryActionClass} border border-input bg-background text-foreground hover:bg-muted`;
  const iconButtonClass =
    'inline-flex h-8 w-8 items-center justify-center rounded-md border border-input bg-background text-muted-foreground transition-colors hover:bg-muted hover:text-foreground disabled:opacity-60';
  const dangerIconButtonClass =
    'inline-flex h-8 w-8 items-center justify-center rounded-md border border-destructive/30 bg-background text-destructive transition-colors hover:bg-destructive/10 disabled:opacity-60';

  const filtered = useMemo(
    () =>
      providers.filter((provider) => {
        if (!search) return true;
        const searchPool = [
          provider.name,
          provider.description,
          provider.authLabel,
          ...(provider.modelTags || []),
          ...(provider.claudeUpstreamModelTags || []),
          provider.apiFormatTag || '',
        ]
          .filter(Boolean)
          .join(' ')
          .toLowerCase();
        return searchPool.includes(search);
      }),
    [providers, search],
  );

  const renderEmptyState = () => {
    const isSearchEmpty = providers.length > 0 && filtered.length === 0;
    return (
      <div className="rounded-xl border border-dashed bg-card p-8 text-center text-sm text-muted-foreground">
        <div>
          {isSearchEmpty
            ? t?.('noProvidersSearchResults', 'No matching service providers') ||
              'No matching service providers'
            : t?.('noProvidersGuide', 'No {{tool}} service providers configured', { tool }) ||
              `No ${tool} service providers configured`}
        </div>
        <button
          type="button"
          onClick={onAdd}
          className="mt-4 inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90"
        >
          <Plus className="h-4 w-4" />
          {t?.('addProvider', 'Add Service Provider') || 'Add Service Provider'}
        </button>
      </div>
    );
  };

  return (
    <div className="space-y-4">
      {filtered.length === 0 ? (
        renderEmptyState()
      ) : (
        filtered.map((provider) => {
          const claudeTags = provider.claudeUpstreamModelTags || [];
          const footerTags = isClaudeTool ? claudeTags : provider.modelTags || [];

          return (
            <div
              key={provider.id}
              className="relative rounded-xl border bg-card p-5 shadow-sm transition-all hover:border-primary/30"
            >
              <div className="flex flex-col gap-4 lg:flex-row lg:items-start lg:justify-between">
                <button
                  type="button"
                  onClick={() => onProviderClick(provider.id)}
                  className="flex flex-1 items-start gap-3 text-left"
                >
                  <ServiceProviderAvatar
                    icon={provider.icon}
                    id={provider.id}
                    name={provider.name}
                    size={42}
                    tool={provider.tool}
                  />
                  <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="text-base font-semibold text-foreground">
                        {provider.name}
                      </span>
                      {provider.apiFormatTag ? (
                        <span className="rounded-full border bg-background px-2 py-0.5 text-[11px] font-medium text-muted-foreground">
                          {provider.apiFormatTag}
                        </span>
                      ) : null}
                      {provider.isGlobal ? (
                        <span className="rounded-full border px-2 py-0.5 text-[11px] font-medium text-foreground">
                          {t?.('globalConfig', 'Global Config') || 'Global Config'}
                        </span>
                      ) : null}
                      {provider.authLabel ? (
                        <span className="rounded-full border bg-muted px-2 py-0.5 text-[11px] font-medium text-muted-foreground">
                          {provider.authLabel}
                        </span>
                      ) : null}
                    </div>
                    {provider.description ? (
                      <div className="mt-2 text-sm text-muted-foreground">
                        {provider.description}
                      </div>
                    ) : null}
                  </div>
                </button>

                <div className="flex w-full shrink-0 flex-wrap items-center justify-start gap-2 lg:w-auto lg:max-w-[360px] lg:justify-end">
                  {isClaudeTool ? (
                    <>
                      <button
                        type="button"
                        onClick={() => onLaunch?.(provider.id)}
                        disabled={loading || provider.launchBusy}
                        className={launchButtonClass}
                        title={t?.('claudeProfileLaunch', '启动') || '启动'}
                      >
                        {provider.launchBusy ? (
                          <Loader2 className="h-4 w-4 animate-spin" />
                        ) : (
                          <Play className="h-4 w-4" />
                        )}
                        {t?.('claudeProfileLaunch', '启动') || '启动'}
                      </button>
                      {!provider.isGlobal ? (
                      <button
                        type="button"
                        onClick={() => onApplyGlobal(provider.id)}
                        disabled={loading || provider.applyBusy}
                        className={activateButtonClass}
                        title={t?.('activate', '激活') || '激活'}
                      >
                        {provider.applyBusy ? (
                          <Loader2 className="h-4 w-4 animate-spin" />
                        ) : (
                            <Zap className="h-4 w-4" />
                          )}
                          {t?.('activate', '激活') || '激活'}
                        </button>
                      ) : null}
                      <button
                        type="button"
                        onClick={() => onCopyLaunchCommand?.(provider.id)}
                        className={iconButtonClass}
                        title={
                          provider.copiedCommand
                            ? t?.('copied', 'Copied') || 'Copied'
                            : t?.('copyLaunchCommand', 'Copy Launch Command') ||
                              'Copy Launch Command'
                        }
                      >
                        <Copy className="h-3.5 w-3.5" />
                      </button>
                      <button
                        type="button"
                        onClick={() => onOpenDirectory?.(provider.id)}
                        className={iconButtonClass}
                        title={t?.('openDirectory', 'Open Directory') || 'Open Directory'}
                      >
                        <FolderOpen className="h-3.5 w-3.5" />
                      </button>
                      <button
                        type="button"
                        onClick={() => onEdit(provider.id)}
                        className={iconButtonClass}
                        title={t?.('edit', 'Edit') || 'Edit'}
                      >
                        <Edit3 className="h-3.5 w-3.5" />
                      </button>
                      <button
                        type="button"
                        onClick={() => onDelete(provider.id)}
                        disabled={provider.deleteBusy}
                        className={dangerIconButtonClass}
                        title={t?.('delete', 'Delete') || 'Delete'}
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                      </button>
                    </>
                  ) : provider.isGlobal ? (
                    <>
                      <button
                        type="button"
                        onClick={() => onEdit(provider.id)}
                        className={iconButtonClass}
                        title={t?.('edit', 'Edit') || 'Edit'}
                      >
                        <Edit3 className="h-3.5 w-3.5" />
                      </button>
                      <button
                        type="button"
                        onClick={() => onDelete(provider.id)}
                        disabled={provider.deleteBusy}
                        className={dangerIconButtonClass}
                        title={t?.('delete', 'Delete') || 'Delete'}
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                      </button>
                    </>
                  ) : (
                    <>
                      <button
                        type="button"
                        onClick={() => onApplyGlobal(provider.id)}
                        disabled={loading || provider.applyBusy}
                        className={activateButtonClass}
                        title={t?.('activate', '激活') || '激活'}
                      >
                        {provider.applyBusy ? (
                          <Loader2 className="h-4 w-4 animate-spin" />
                        ) : (
                          <Zap className="h-4 w-4" />
                        )}
                        {t?.('activate', '激活') || '激活'}
                      </button>
                      <button
                        type="button"
                        onClick={() => onEdit(provider.id)}
                        className={iconButtonClass}
                        title={t?.('edit', 'Edit') || 'Edit'}
                      >
                        <Edit3 className="h-3.5 w-3.5" />
                      </button>
                      <button
                        type="button"
                        onClick={() => onDelete(provider.id)}
                        disabled={provider.deleteBusy}
                        className={dangerIconButtonClass}
                        title={t?.('delete', 'Delete') || 'Delete'}
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                      </button>
                    </>
                  )}
                </div>
              </div>
              {footerTags.length > 0 ? (
                <div className="mt-3 overflow-x-auto">
                  <div className="flex min-w-0 flex-nowrap gap-2">
                    {footerTags.map((tag) => (
                      <span
                        key={`${provider.id}-tag-${tag}`}
                        className="shrink-0 rounded-full border bg-background px-2 py-0.5 text-[11px] font-medium text-muted-foreground"
                      >
                        {tag}
                      </span>
                    ))}
                  </div>
                </div>
              ) : null}
            </div>
          );
        })
      )}
    </div>
  );
}
