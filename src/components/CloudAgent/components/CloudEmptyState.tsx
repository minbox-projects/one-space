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
    <div className="rounded-xl border border-dashed border-border bg-card/60 p-10 text-center">
      <div className="mx-auto mb-3 flex h-11 w-11 items-center justify-center rounded-xl bg-primary/10 text-primary">
        <Icon className="h-5 w-5" />
      </div>
      <h3 className="text-sm font-semibold text-foreground">
        {title}
      </h3>
      <p className="mt-1 text-xs text-muted-foreground max-w-md mx-auto leading-relaxed">
        {description}
      </p>

      {action && (
        <button
          type="button"
          disabled={action.disabled || action.loading}
          onClick={action.onClick}
          className="mt-4 inline-flex h-8 items-center gap-1.5 rounded-lg bg-primary px-3.5 text-xs font-medium text-primary-foreground shadow-2xs hover:bg-primary/90 transition-all disabled:opacity-50 cursor-pointer"
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
