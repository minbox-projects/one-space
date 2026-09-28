export type ToolEmptyStateProps = {
  title: string;
  description?: string;
  testId?: string;
  className?: string;
};

export function ToolEmptyState({
  title,
  description,
  testId,
  className,
}: ToolEmptyStateProps) {
  const classes = [
    "rounded-xl border border-dashed p-6 text-center text-sm text-muted-foreground",
    className,
  ]
    .filter(Boolean)
    .join(" ");

  return (
    <div data-testid={testId} className={classes}>
      <p className="font-medium text-foreground">{title}</p>
      {description ? (
        <p className="mt-1 text-muted-foreground">{description}</p>
      ) : null}
    </div>
  );
}
