import { useEffect, useRef, useState } from 'react';
import { Check, Copy, History, RotateCcw, X } from 'lucide-react';
import { cn } from '@/lib/utils';
import {
  buildHistoryDiff,
  copyableDiffValue,
  diffValueIsSet,
  historyTimestamp,
  type HistoryEntry,
} from './historyDiff';

function HistoryDiffValue({
  value,
  rawValue,
  copyKey,
  tone,
  t,
}: {
  value: string;
  rawValue: any;
  copyKey: string;
  tone: 'before' | 'after';
  t?: (key: string, fallback: string, options?: Record<string, any>) => string;
}) {
  const [copiedKey, setCopiedKey] = useState<string | null>(null);
  const canCopy = diffValueIsSet(rawValue);
  const bgClass = tone === 'before' ? 'bg-red-500/10' : 'bg-green-500/10';

  const handleCopy = async (event: React.MouseEvent<HTMLButtonElement>) => {
    event.stopPropagation();
    if (!canCopy) return;
    await navigator.clipboard.writeText(copyableDiffValue(rawValue));
    setCopiedKey(copyKey);
  };

  return (
    <div
      className={cn('group relative whitespace-pre-wrap break-words rounded px-1.5 py-1 pr-7', bgClass)}
      title={value}
      onMouseLeave={() => {
        if (copiedKey === copyKey) setCopiedKey(null);
      }}
    >
      {value}
      {canCopy ? (
        <button
          type="button"
          onClick={(event) => void handleCopy(event)}
          className="absolute right-1 top-1 inline-flex h-4 w-4 items-center justify-center rounded text-muted-foreground opacity-70 transition-colors hover:bg-background/80 hover:text-foreground group-hover:opacity-100"
          title={copiedKey === copyKey ? t?.('copied', 'Copied') || 'Copied' : t?.('copy', 'Copy') || 'Copy'}
          aria-label={copiedKey === copyKey ? t?.('copied', 'Copied') || 'Copied' : t?.('copy', 'Copy') || 'Copy'}
        >
          {copiedKey === copyKey ? (
            <Check className="h-3 w-3 text-green-600" />
          ) : (
            <Copy className="h-3 w-3" />
          )}
        </button>
      ) : null}
    </div>
  );
}

export function HistoryPanel({
  currentProvider,
  history,
  onRollback,
  t,
}: {
  currentProvider: any;
  history: HistoryEntry[];
  onRollback?: (entry: HistoryEntry) => void;
  t?: (key: string, fallback: string, options?: Record<string, any>) => string;
}) {
  const historyRef = useRef<HTMLDivElement>(null);
  const [showHistory, setShowHistory] = useState(false);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const selectedEntry = history[selectedIndex];
  const diffRows = selectedEntry ? buildHistoryDiff(currentProvider, selectedEntry) : [];

  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (historyRef.current && !historyRef.current.contains(event.target as Node)) {
        setShowHistory(false);
      }
    }
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, []);

  return (
    <div className="relative" ref={historyRef}>
      <button type="button" onClick={() => setShowHistory((prev) => !prev)} className="acc-panel-btn">
        <History className="h-4 w-4" />
        {t ? t('aiHistory', 'History') : 'History'}
      </button>
      {showHistory && (
        <div className="absolute right-0 top-full z-50 mt-2 grid h-[420px] max-h-[calc(100vh-160px)] w-[560px] grid-cols-[190px_1fr] overflow-hidden rounded-lg border bg-popover shadow-xl">
          <div className="flex min-h-0 flex-col border-r">
            <div className="flex shrink-0 items-center justify-between border-b bg-muted/30 p-3">
              <span className="text-xs font-bold uppercase">
                {t ? t('aiHistory', 'History') : 'History'}
              </span>
              <button type="button" onClick={() => setShowHistory(false)}>
                <X className="h-4 w-4" />
              </button>
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto p-1">
              {history.length === 0 ? (
                <div className="p-6 text-center text-xs text-muted-foreground">
                  {t ? t('noHistory', 'No history') : 'No history'}
                </div>
              ) : (
                history.map((entry, index) => (
                  <button
                    key={`${historyTimestamp(entry)}-${index}`}
                    type="button"
                    onClick={() => setSelectedIndex(index)}
                    className={cn(
                      'mb-1 w-full rounded-md border p-2 text-left text-xs transition-colors',
                      selectedIndex === index ? 'border-primary bg-primary/5' : 'border-transparent hover:border-border hover:bg-muted/50',
                    )}
                  >
                    <div className="font-mono text-[10px] text-muted-foreground">
                      {new Date(historyTimestamp(entry)).toLocaleString()}
                    </div>
                    <div className="mt-1 truncate font-medium">
                      {entry.action || t?.('providerHistoryUpdate', 'Update') || 'Update'}
                    </div>
                  </button>
                ))
              )}
            </div>
          </div>
          <div className="min-h-0 min-w-0 p-3">
            {!selectedEntry ? (
              <div className="flex h-full items-center justify-center text-xs text-muted-foreground">
                {t ? t('noHistory', 'No history') : 'No history'}
              </div>
            ) : (
              <div className="flex h-full min-h-0 flex-col gap-3">
                <div className="flex items-center justify-between gap-2">
                  <div className="min-w-0">
                    <div className="truncate text-xs font-semibold">
                      {new Date(historyTimestamp(selectedEntry)).toLocaleString()}
                    </div>
                    <div className="text-[10px] text-muted-foreground">
                      {selectedEntry.summary || selectedEntry.action || ''}
                    </div>
                  </div>
                  <button
                    type="button"
                    onClick={() => {
                      onRollback?.(selectedEntry);
                      setShowHistory(false);
                    }}
                    className="acc-btn"
                  >
                    <RotateCcw className="h-3 w-3" />
                    {t ? t('rollback', 'Rollback') : 'Rollback'}
                  </button>
                </div>
                <div className="min-h-0 flex-1 overflow-y-auto rounded-md border">
                  {diffRows.length === 0 ? (
                    <div className="whitespace-pre-wrap break-words p-4 font-mono text-xs text-muted-foreground">
                      {selectedEntry.content || t?.('providerHistoryNoDiff', 'No field differences to show.') || 'No field differences to show.'}
                    </div>
                  ) : (
                    <div className="divide-y text-xs">
                      {diffRows.map((row) => (
                        <div key={row.key} className="grid grid-cols-[110px_1fr] gap-2 p-2">
                          <div className="min-w-0">
                            <div className="truncate font-medium" title={row.key}>{row.key}</div>
                            <div className="text-[10px] uppercase text-muted-foreground">{row.kind}</div>
                          </div>
                          <div className="min-w-0 space-y-1 font-mono text-[10px]">
                            <HistoryDiffValue
                              value={row.before}
                              rawValue={row.beforeValue}
                              copyKey={`${row.key}:before`}
                              tone="before"
                              t={t}
                            />
                            <HistoryDiffValue
                              value={row.after}
                              rawValue={row.afterValue}
                              copyKey={`${row.key}:after`}
                              tone="after"
                              t={t}
                            />
                          </div>
                        </div>
                      ))}
                    </div>
                  )}
                </div>
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
