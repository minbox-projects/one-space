import { useState, useCallback } from 'react';

interface ConfigJsonEditorProps {
  value: string;
  onChange: (value: string) => void;
  onError: (error: string | null) => void;
  t?: (key: string, fallback: string) => string;
}

export function ConfigJsonEditor({ value, onChange, onError, t }: ConfigJsonEditorProps) {
  const [jsonError, setJsonError] = useState<string | null>(null);

  const handleFormat = useCallback(() => {
    try {
      const parsed = JSON.parse(value);
      const formatted = JSON.stringify(parsed, null, 2);
      onChange(formatted);
      setJsonError(null);
      onError(null);
      } catch (e: any) {
      const msg = e?.message || (t ? t('invalidJson', 'Invalid JSON') : 'Invalid JSON');
      setJsonError(msg);
      onError(msg);
    }
  }, [value, onChange, onError, t]);

  return (
    <div style={{ marginTop: 8 }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 4 }}>
        <span style={{ fontSize: 13, fontWeight: 600 }}>
          {t ? t('configurationJson', 'Configuration JSON') : 'Configuration JSON'}
        </span>
        <button
          type="button"
          onClick={handleFormat}
          style={{
            fontSize: 12,
            padding: 0,
            border: 'none',
            background: 'transparent',
            color: '#2563eb',
            textDecoration: 'underline',
            cursor: 'pointer',
          }}
        >
          {t ? t('formatJson', 'Format') : 'Format'}
        </button>
      </div>
      <textarea
        value={value}
        onChange={(e) => onChange(e.target.value)}
        rows={12}
        style={{
          width: '100%',
          fontFamily: 'ui-monospace, SFMono-Regular, Menlo, monospace',
          fontSize: 12,
          padding: 8,
          border: jsonError ? '1px solid #ef4444' : '1px solid #d1d5db',
          borderRadius: 6,
          resize: 'vertical',
          boxSizing: 'border-box',
        }}
      />
      {jsonError && (
        <div style={{ fontSize: 12, color: '#ef4444', marginTop: 4 }}>
          {t ? t('invalidJson', jsonError) : jsonError}
        </div>
      )}
    </div>
  );
}
