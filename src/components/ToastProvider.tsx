import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { AlertCircle, CheckCircle2, Info, X } from "lucide-react";
import { useTranslation } from "react-i18next";

type ToastKind = "info" | "success" | "error";

type ToastInput = {
  title: string;
  description?: string;
  kind?: ToastKind;
  durationMs?: number;
};

type ToastRecord = ToastInput & {
  id: string;
  kind: ToastKind;
};

type ToastContextValue = {
  pushToast: (toast: ToastInput) => void;
  dismissToast: (id: string) => void;
};

const ToastContext = createContext<ToastContextValue | null>(null);

function defaultToastDuration(kind: ToastKind) {
  return kind === "error" ? 5000 : 2800;
}

export function ToastProvider({ children }: { children: ReactNode }) {
  const { t } = useTranslation();
  const [toasts, setToasts] = useState<ToastRecord[]>([]);
  const timersRef = useRef<Map<string, number>>(new Map());

  const dismissToast = useCallback((id: string) => {
    const timerId = timersRef.current.get(id);
    if (timerId) {
      window.clearTimeout(timerId);
      timersRef.current.delete(id);
    }
    setToasts((prev) => prev.filter((toast) => toast.id !== id));
  }, []);

  const pushToast = useCallback(
    ({ title, description, kind = "info", durationMs }: ToastInput) => {
      const id = crypto.randomUUID();
      const toast: ToastRecord = {
        id,
        title,
        description,
        kind,
        durationMs,
      };
      setToasts((prev) => [...prev, toast].slice(-4));
      const timerId = window.setTimeout(
        () => dismissToast(id),
        durationMs ?? defaultToastDuration(kind),
      );
      timersRef.current.set(id, timerId);
    },
    [dismissToast],
  );

  useEffect(() => {
    return () => {
      timersRef.current.forEach((timerId) => window.clearTimeout(timerId));
      timersRef.current.clear();
    };
  }, []);

  const contextValue = useMemo(
    () => ({ pushToast, dismissToast }),
    [dismissToast, pushToast],
  );

  return (
    <ToastContext.Provider value={contextValue}>
      {children}
      <div className="pointer-events-none fixed right-4 top-4 z-[70] flex w-[min(24rem,calc(100vw-2rem))] flex-col gap-3">
        {toasts.map((toast) => {
          const icon =
            toast.kind === "success" ? (
              <CheckCircle2 className="h-5 w-5" />
            ) : toast.kind === "error" ? (
              <AlertCircle className="h-5 w-5" />
            ) : (
              <Info className="h-5 w-5" />
            );
          const accentClass =
            toast.kind === "success"
              ? "border-emerald-500/20 text-emerald-600 bg-emerald-500/10"
              : toast.kind === "error"
                ? "border-destructive/20 text-destructive bg-destructive/10"
                : "border-primary/20 text-primary bg-primary/10";

          return (
            <div
              key={toast.id}
              className="pointer-events-auto animate-in fade-in slide-in-from-top-2 duration-200"
            >
              <div className="rounded-xl border bg-card shadow-lg">
                <div className="flex items-start gap-3 p-4">
                  <div
                    className={`mt-0.5 inline-flex h-9 w-9 shrink-0 items-center justify-center rounded-full border ${accentClass}`}
                  >
                    {icon}
                  </div>
                  <div className="min-w-0 flex-1">
                    <div className="text-sm font-semibold text-foreground">
                      {toast.title}
                    </div>
                    {toast.description ? (
                      <div className="mt-1 whitespace-pre-wrap break-words text-sm text-muted-foreground">
                        {toast.description}
                      </div>
                    ) : null}
                  </div>
                  <button
                    type="button"
                    onClick={() => dismissToast(toast.id)}
                    className="inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
                    aria-label={t("toastDismiss", "Dismiss")}
                    title={t("toastDismiss", "Dismiss")}
                  >
                    <X className="h-4 w-4" />
                  </button>
                </div>
              </div>
            </div>
          );
        })}
      </div>
    </ToastContext.Provider>
  );
}

export function useToast() {
  const context = useContext(ToastContext);
  if (!context) {
    throw new Error("useToast must be used within ToastProvider");
  }
  return context;
}
