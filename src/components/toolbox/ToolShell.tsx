import type { ReactNode } from "react";

import { ToolErrorBanner } from "./ToolErrorBanner";

export type ToolShellProps = {
  titleId?: string;
  title: string;
  description?: string;
  actions?: ReactNode;
  error?: string | null;
  children: ReactNode;
};

export function ToolShell({
  titleId,
  title,
  description,
  actions,
  error,
  children,
}: ToolShellProps) {
  return (
    <section className="space-y-5 pb-5" aria-labelledby={titleId}>
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <h2 id={titleId} className="text-lg font-semibold">
            {title}
          </h2>
          {description ? (
            <p className="text-sm text-muted-foreground">{description}</p>
          ) : null}
        </div>
        {actions ? <div className="flex flex-wrap gap-2">{actions}</div> : null}
      </div>
      <ToolErrorBanner message={error} />
      {children}
    </section>
  );
}
