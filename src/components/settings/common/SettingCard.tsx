import type { ReactNode } from "react";
import type { LucideIcon } from "lucide-react";

export interface SettingCardProps {
  id?: string;
  icon?: LucideIcon;
  title: ReactNode;
  description?: ReactNode;
  headerAction?: ReactNode;
  children: ReactNode;
  className?: string;
}

export function SettingCard({
  id,
  icon: Icon,
  title,
  description,
  headerAction,
  children,
  className = "",
}: SettingCardProps) {
  return (
    <section id={id} className={`space-y-4 ${className}`}>
      <div className="flex items-center justify-between gap-3">
        <div className="flex flex-col gap-1">
          <div className="flex items-center gap-2">
            {Icon && <Icon className="w-5 h-5 text-primary shrink-0" />}
            <h2 className="text-lg font-semibold tracking-tight">{title}</h2>
          </div>
          {description && (
            <p className="text-sm text-muted-foreground">{description}</p>
          )}
        </div>
        {headerAction && <div className="shrink-0">{headerAction}</div>}
      </div>

      <div className="bg-card border rounded-2xl p-6 shadow-sm space-y-6">
        {children}
      </div>
    </section>
  );
}
