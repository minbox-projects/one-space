interface ModelMappingRow {
  family: string;
  display_name: string;
  upstream_model: string;
  supports_1m?: boolean;
}

interface ModelMappingTableProps {
  mappings: ModelMappingRow[];
  onChange: (mappings: ModelMappingRow[]) => void;
  fetchedModels?: string[];
  t?: (key: string, fallback: string) => string;
}

const FAMILIES = ['haiku', 'sonnet', 'opus'] as const;

export function ModelMappingTable({ mappings, onChange, fetchedModels, t }: ModelMappingTableProps) {
  const normalizedMappings = FAMILIES.map((family) => {
    return mappings.find((m) => m.family === family) || {
      family,
      display_name: family.charAt(0).toUpperCase() + family.slice(1),
      upstream_model: '',
      supports_1m: false,
    };
  });

  const handleRowChange = (family: string, field: keyof ModelMappingRow, value: any) => {
    const next = normalizedMappings.map((m) => (m.family === family ? { ...m, [field]: value } : m));
    onChange(next);
  };

  return (
    <div className="full-span">
      <table className="w-full border-collapse text-sm">
        <thead>
          <tr className="border-b">
            <th className="text-left py-2 px-2 text-muted-foreground font-medium">
              {t ? t('family', 'Family') : 'Family'}
            </th>
            <th className="text-left py-2 px-2 text-muted-foreground font-medium">
              {t ? t('displayModel', 'Display Name') : 'Display Name'}
            </th>
            <th className="text-left py-2 px-2 text-muted-foreground font-medium">
              {t ? t('upstreamModel', 'Upstream Model') : 'Upstream Model'}
            </th>
            <th className="text-center py-2 px-2 text-muted-foreground font-medium">
              1M
            </th>
          </tr>
        </thead>
        <tbody>
          {normalizedMappings.map((row) => {
            const family = row.family;
            const isHaiku = family === 'haiku';

            return (
              <tr key={family} className="border-b border-border/60 last:border-0">
                <td className="py-2 px-2 font-medium text-muted-foreground capitalize">
                  {family}
                </td>
                <td className="py-2 px-2 field">
                  <input
                    type="text"
                    value={row.display_name}
                    onChange={(e) => handleRowChange(family, 'display_name', e.target.value)}
                  />
                </td>
                <td className="py-2 px-2">
                  <div className="field flex gap-2">
                    <input
                      type="text"
                      value={row.upstream_model}
                      onChange={(e) => handleRowChange(family, 'upstream_model', e.target.value)}
                      className="min-w-0 flex-1"
                    />
                    {fetchedModels && fetchedModels.length > 0 && (
                      <select
                        value=""
                        onChange={(e) => {
                          if (e.target.value) {
                            handleRowChange(family, 'upstream_model', e.target.value);
                          }
                        }}
                        className="w-44 shrink-0"
                        aria-label={t ? t('selectFetchedModel', 'Select fetched model') : 'Select fetched model'}
                      >
                        <option value="">{t ? t('selectModel', 'Select model') : 'Select model'}</option>
                        {fetchedModels.map((m) => (
                          <option key={m} value={m}>{m}</option>
                        ))}
                      </select>
                    )}
                  </div>
                </td>
                <td className="py-2 px-2 text-center">
                  <input
                    type="checkbox"
                    checked={row.supports_1m || false}
                    disabled={isHaiku}
                    onChange={(e) => handleRowChange(family, 'supports_1m', e.target.checked)}
                    title={isHaiku ? (t ? t('haikuNo1m', 'Claude Code only supports 1M on Sonnet/Opus') : 'Claude Code only supports 1M on Sonnet/Opus') : ''}
                  />
                  {isHaiku && (
                    <span style={{ fontSize: 10, color: '#9ca3af', marginLeft: 4 }} title={t ? t('haikuNo1m', 'Claude Code only supports 1M on Sonnet/Opus') : 'Claude Code only supports 1M on Sonnet/Opus'}>
                      ✕
                    </span>
                  )}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
