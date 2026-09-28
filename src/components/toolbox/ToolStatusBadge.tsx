export type ToolStatusTone = "success" | "warning" | "error" | "neutral";

const BASE_BADGE_CLASSES = [
  "rounded-full",
  "border",
  "px-2",
  "py-0.5",
  "text-[11px]",
  "font-medium",
];

const TONE_CLASSES: Record<ToolStatusTone, string> = {
  success: "border-emerald-500/20 bg-emerald-500/10 text-emerald-600",
  warning: "border-amber-500/20 bg-amber-500/10 text-amber-600",
  error: "border-destructive/20 bg-destructive/10 text-destructive",
  neutral: "border-border bg-muted text-muted-foreground",
};

export type ToolStatusBadgeProps = {
  tone: ToolStatusTone;
  label: string;
  testId?: string;
  className?: string;
};

export function ToolStatusBadge({
  tone,
  label,
  testId,
  className,
}: ToolStatusBadgeProps) {
  const classes = [...BASE_BADGE_CLASSES, TONE_CLASSES[tone], className]
    .filter(Boolean)
    .join(" ");

  return (
    <span data-testid={testId} className={classes}>
      {label}
    </span>
  );
}
