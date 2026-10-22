import type { ReactNode } from "react";

export interface SettingRowProps {
  label: ReactNode;
  description?: ReactNode;
  children: ReactNode;
  className?: string;
  vertical?: boolean;
}

export function SettingRow({
  label,
  description,
  children,
  className = "",
  vertical = false,
}: SettingRowProps) {
  if (vertical) {
    return (
      <div className={`space-y-2 ${className}`}>
        <div className="space-y-0.5">
          <label className="text-sm font-medium">{label}</label>
          {description && (
            <div className="text-xs text-muted-foreground">{description}</div>
          )}
        </div>
        <div>{children}</div>
      </div>
    );
  }

  return (
    <div
      className={`flex flex-col sm:flex-row sm:items-center justify-between gap-4 ${className}`}
    >
      <div className="space-y-0.5 max-w-xl">
        <h3 className="text-sm font-medium">{label}</h3>
        {description && (
          <div className="text-xs text-muted-foreground leading-relaxed">
            {description}
          </div>
        )}
      </div>
      <div className="shrink-0 flex items-center">{children}</div>
    </div>
  );
}
