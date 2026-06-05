import { CheckCircle2, Edit3, Play, Plus, Trash2 } from 'lucide-react';
import { ServiceProviderAvatar } from './ServiceProviderAvatar';

interface ServiceProviderLite {
  id: string;
  name: string;
  tool: string;
  icon?: string;
  is_enabled?: boolean;
  env_managed?: boolean;
  model?: string;
}

interface ServiceProviderListProps {
  providers: ServiceProviderLite[];
  activeProviderId?: string | null;
  onProviderClick: (id: string) => void;
  onEdit: (id: string) => void;
  onActivate: (id: string) => void;
  onDelete: (id: string) => void;
  onAdd: () => void;
  tool: string;
  t?: (key: string, fallback: string) => string;
  searchTerm: string;
  filterMode: string;
}

export function ServiceProviderList({
  providers,
  activeProviderId,
  onProviderClick,
  onEdit,
  onActivate,
  onDelete,
  onAdd,
  tool,
  t,
  searchTerm,
  filterMode,
}: ServiceProviderListProps) {
  const filtered = providers.filter((p) => {
    if (searchTerm && !p.name.toLowerCase().includes(searchTerm.toLowerCase())) return false;
    if (filterMode === 'active' && p.id !== activeProviderId) return false;
    if (filterMode === 'inactive' && p.id === activeProviderId) return false;
    return true;
  });

  return (
    <div className="space-y-2">
      {filtered.length === 0 ? (
        <div className="border rounded-lg bg-card p-8 text-center text-sm text-muted-foreground">
          <div>
            {t ? t('noProvidersGuide', 'No service providers configured for {{tool}}') : `No service providers configured for ${tool}`}
          </div>
          <button
            type="button"
            onClick={onAdd}
            className="acc-panel-btn primary mt-4"
          >
            <Plus className="w-4 h-4" />
            {t ? t('addProvider', 'Add Service Provider') : 'Add Service Provider'}
          </button>
        </div>
      ) : (
        <div className="border rounded-xl overflow-hidden bg-background">
          {filtered.map((p) => (
            <button
              type="button"
              key={p.id}
              onClick={() => onProviderClick(p.id)}
              className={`acc-item acc-row w-full text-left ${p.id === activeProviderId ? 'open' : ''}`}
            >
              <ServiceProviderAvatar icon={p.icon} name={p.name} id={p.id} size={40} />
              <div className="acc-info">
                <div className="acc-name-row">
                  <span className="acc-name truncate">{p.name}</span>
                  {p.id === activeProviderId && (
                    <span className="badge-pill bg-green-500/10 text-green-700">
                      <CheckCircle2 className="w-3 h-3" />
                      {t ? t('active', 'Active') : 'Active'}
                    </span>
                  )}
                  {p.env_managed === false && (
                    <span className="badge-pill bg-muted/50 text-muted-foreground">
                      {t ? t('inactive', 'Inactive') : 'Inactive'}
                    </span>
                  )}
                </div>
                <div className="acc-badges">
                  <span className="badge-pill border capitalize">{p.tool}</span>
                  {p.model && <span className="badge-pill border">{p.model}</span>}
                </div>
              </div>
              <div className="acc-actions" onClick={(e) => e.stopPropagation()}>
                {p.id !== activeProviderId && (
                  <button type="button" className="acc-btn acc-btn-launch" onClick={() => onActivate(p.id)}>
                    <Play className="w-3 h-3" />
                    {t ? t('activateServiceProvider', 'Activate') : 'Activate'}
                  </button>
                )}
                <button type="button" className="acc-btn" onClick={() => onEdit(p.id)}>
                  <Edit3 className="w-3 h-3" />
                  {t ? t('edit', 'Edit') : 'Edit'}
                </button>
                <button type="button" className="acc-btn" onClick={() => onDelete(p.id)}>
                  <Trash2 className="w-3 h-3" />
                  {t ? t('delete', 'Delete') : 'Delete'}
                </button>
              </div>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
