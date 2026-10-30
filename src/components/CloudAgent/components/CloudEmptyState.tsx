import type { ComponentType, ReactNode } from "react";
import { Loader2 } from "lucide-react";

export interface CloudEmptyStateProps {
  icon: ComponentType<{ className?: string }>;
  title: string;
  description: string;
  action?: {
    label: string;
    icon?: ComponentType<{ className?: string }>;
    onClick: () => void;
    loading?: boolean;
    disabled?: boolean;
  };
  children?: ReactNode;
}

export function CloudEmptyState({
  icon: Icon,
  title,
  description,
  action,
  children,
}: CloudEmptyStateProps) {
  const ActionIcon = action?.icon;

  return (
    <div className="rounded-2xl border border-dashed border-slate-300 dark:border-slate-800 bg-white/50 dark:bg-slate-900/40 p-12 text-center">
      <div className="mx-auto mb-4 flex h-12 w-12 items-center justify-center rounded-2xl bg-emerald-500/10 text-emerald-600 dark:bg-emerald-500/20 dark:text-emerald-400">
        <Icon className="h-6 w-6" />
      </div>
      <h3 className="text-base font-semibold text-slate-800 dark:text-slate-200">
        {title}
      </h3>
      <p className="mt-1 text-xs text-slate-500 dark:text-slate-400 max-w-sm mx-auto leading-relaxed">
        {description}
      </p>

      {action && (
        <button
          type="button"
          disabled={action.disabled || action.loading}
          onClick={action.onClick}
          className="mt-5 inline-flex items-center gap-2 rounded-xl bg-emerald-600 px-4 py-2 text-xs font-medium text-white shadow-sm hover:bg-emerald-700 transition-all disabled:opacity-60 cursor-pointer"
        >
          {action.loading ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
          ) : ActionIcon ? (
            <ActionIcon className="h-3.5 w-3.5" />
          ) : null}
          {action.label}
        </button>
      )}

      {children}
    </div>
  );
}
