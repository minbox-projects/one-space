import type { ReactNode } from "react";

import { ToolErrorBanner } from "./ToolErrorBanner";

export type ToolShellProps = {
  titleId?: string;
  title: string;
  description?: string;
  icon?: ReactNode;
  actions?: ReactNode;
  error?: string | null;
  children: ReactNode;
};

export function ToolShell({
  titleId,
  title,
  description,
  icon,
  actions,
  error,
  children,
}: ToolShellProps) {
  return (
    <section className="space-y-5 pb-5" aria-labelledby={titleId}>
      <div className="flex items-start justify-between gap-4">
        <div className="flex items-start gap-3">
          {icon ? icon : null}
          <div>
            <h2 id={titleId} className="text-xl font-bold tracking-tight">
              {title}
            </h2>
            {description ? (
              <p className="mt-1 text-sm text-muted-foreground">{description}</p>
            ) : null}
          </div>
        </div>
        {actions ? <div className="flex flex-wrap gap-2">{actions}</div> : null}
      </div>
      <ToolErrorBanner message={error} />
      {children}
    </section>
  );
}
