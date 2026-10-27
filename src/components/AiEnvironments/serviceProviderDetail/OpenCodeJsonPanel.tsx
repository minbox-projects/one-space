import { WandSparkles } from 'lucide-react';
import Editor from 'react-simple-code-editor';
import { highlight, languages } from 'prismjs';
import 'prismjs/components/prism-json';
import 'prismjs/themes/prism-tomorrow.css';
import { cn } from '@/lib/utils';

export function OpenCodeJsonPanel({
  value,
  jsonError,
  isRollbackMode,
  onChange,
  onFormat,
  t,
}: {
  value: string;
  jsonError?: string | null;
  isRollbackMode?: boolean;
  onChange?: (value: string) => void;
  onFormat?: () => void;
  t?: (key: string, fallback: string, options?: Record<string, any>) => string;
}) {
  return (
    <>
      <div className="mb-3 flex justify-end">
        <button type="button" onClick={onFormat} className="acc-btn">
          <WandSparkles className="w-3 h-3" />
          {t ? t('format', 'Format') : 'Format'}
        </button>
      </div>

      <div className={cn(
        'overflow-hidden rounded-md border bg-white font-mono text-sm shadow-inner transition-colors',
        isRollbackMode ? 'border-amber-500 ring-2 ring-amber-500' : jsonError ? 'border-destructive' : 'border-border',
      )}>
        <Editor
          value={value}
          onValueChange={(code) => onChange?.(code)}
          highlight={(code) => highlight(code, languages.json, 'json')}
          padding={16}
          style={{
            fontFamily: '"Fira Code", "Fira Mono", monospace',
            minHeight: '240px',
            backgroundColor: 'white',
            color: '#1a1a1a',
          }}
          className="focus:outline-none"
        />
      </div>
      {jsonError ? <p className="mt-2 text-xs text-destructive">{jsonError}</p> : null}
    </>
  );
}
