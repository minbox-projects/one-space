import { createContext, useCallback, useContext, useMemo, useRef, useState, type ReactNode } from 'react';
import { AlertCircle, Info } from 'lucide-react';

type ConfirmKind = 'info' | 'warning' | 'error';

interface ConfirmOptions {
  title?: string;
  okLabel?: string;
  cancelLabel?: string;
  kind?: ConfirmKind;
}

type ConfirmDialogFn = (message: string, options?: ConfirmOptions) => Promise<boolean>;

interface PendingConfirm {
  message: string;
  options?: ConfirmOptions;
  resolve: (value: boolean) => void;
}

const ConfirmDialogContext = createContext<ConfirmDialogFn | null>(null);

export function ConfirmDialogProvider({ children }: { children: ReactNode }) {
  const queueRef = useRef<PendingConfirm[]>([]);
  const currentRef = useRef<PendingConfirm | null>(null);
  const [current, setCurrent] = useState<PendingConfirm | null>(null);

  const shiftNext = useCallback(() => {
    const next = queueRef.current.shift() || null;
    currentRef.current = next;
    setCurrent(next);
  }, []);

  const confirm = useCallback<ConfirmDialogFn>((message, options) => {
    return new Promise<boolean>((resolve) => {
      queueRef.current.push({ message, options, resolve });
      if (!currentRef.current) {
        shiftNext();
      }
    });
  }, [shiftNext]);

  const resolveCurrent = useCallback((value: boolean) => {
    const active = currentRef.current;
    if (!active) return;
    active.resolve(value);
    shiftNext();
  }, [shiftNext]);

  const contextValue = useMemo(() => confirm, [confirm]);
  const isInfo = current?.options?.kind === 'info';
  const title = current?.options?.title || 'Confirm';
  const okLabel = current?.options?.okLabel || (isInfo ? 'OK' : 'Delete');
  const cancelLabel = current?.options?.cancelLabel || 'Cancel';

  return (
    <ConfirmDialogContext.Provider value={contextValue}>
      {children}
      {current && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-background/80 backdrop-blur-sm p-4">
          <div className="bg-card border rounded-xl shadow-lg w-full max-w-sm overflow-hidden animate-in fade-in zoom-in-95 duration-200">
            <div className="p-5">
              <div className={`flex items-center gap-3 mb-3 ${isInfo ? 'text-primary' : 'text-destructive'}`}>
                <div className={`${isInfo ? 'bg-primary/10' : 'bg-destructive/10'} p-2 rounded-full`}>
                  {isInfo ? <Info className="w-5 h-5" /> : <AlertCircle className="w-5 h-5" />}
                </div>
                <h3 className="font-semibold">{title}</h3>
              </div>
              <p className="text-sm text-muted-foreground whitespace-pre-wrap break-words">
                {current.message}
              </p>
            </div>
            <div className="p-4 bg-muted/30 border-t flex justify-end gap-3">
              <button
                onClick={() => resolveCurrent(false)}
                className="px-4 py-2 rounded-md text-sm font-medium hover:bg-muted transition-colors"
              >
                {cancelLabel}
              </button>
              <button
                onClick={() => resolveCurrent(true)}
                className={`px-4 py-2 rounded-md flex items-center gap-2 text-sm font-medium transition-colors ${
                  isInfo
                    ? 'bg-primary text-primary-foreground hover:bg-primary/90'
                    : 'bg-destructive text-destructive-foreground hover:bg-destructive/90'
                }`}
              >
                {okLabel}
              </button>
            </div>
          </div>
        </div>
      )}
    </ConfirmDialogContext.Provider>
  );
}

export function useConfirmDialog() {
  const context = useContext(ConfirmDialogContext);
  if (!context) {
    throw new Error('useConfirmDialog must be used within ConfirmDialogProvider');
  }
  return context;
}
