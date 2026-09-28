export type ToolErrorBannerProps = {
  message: string | null | undefined;
};

export function ToolErrorBanner({ message }: ToolErrorBannerProps) {
  if (!message) return null;

  return (
    <div
      role="alert"
      className="rounded-md border border-destructive/30 bg-destructive/10 p-3 text-sm text-destructive break-words"
    >
      {message}
    </div>
  );
}
