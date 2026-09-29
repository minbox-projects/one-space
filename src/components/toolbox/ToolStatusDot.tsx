export type ToolStatusDotTone = "success" | "warning" | "error" | "info";

const TONE_CLASSES: Record<ToolStatusDotTone, { ping: string; dot: string }> = {
  success: {
    ping: "bg-emerald-400",
    dot: "bg-emerald-500",
  },
  warning: {
    ping: "bg-amber-400",
    dot: "bg-amber-500",
  },
  error: {
    ping: "bg-destructive",
    dot: "bg-destructive",
  },
  info: {
    ping: "bg-blue-400",
    dot: "bg-blue-500",
  },
};

export interface ToolStatusDotProps {
  tone?: ToolStatusDotTone;
  ping?: boolean;
  className?: string;
  testId?: string;
}

export function ToolStatusDot({
  tone = "success",
  ping = true,
  className = "",
  testId,
}: ToolStatusDotProps) {
  const toneStyle = TONE_CLASSES[tone] ?? TONE_CLASSES.success;

  return (
    <span
      data-testid={testId}
      className={`absolute right-1 top-1 flex h-2 w-2 ${className}`.trim()}
    >
      {ping && (
        <span
          data-testid={testId ? `${testId}-ping` : undefined}
          className={`absolute inline-flex h-full w-full animate-ping rounded-full opacity-75 ${toneStyle.ping}`}
        />
      )}
      <span
        data-testid={testId ? `${testId}-core` : undefined}
        className={`relative inline-flex h-2 w-2 rounded-full ${toneStyle.dot}`}
      />
    </span>
  );
}
