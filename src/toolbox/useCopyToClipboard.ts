import { useCallback, useEffect, useRef, useState } from "react";

export const COPIED_FEEDBACK_RESET_MS = 1600;

export type UseCopyToClipboardOptions = {
  onSuccess?: () => void;
  onError?: (error: unknown) => void;
  resetMs?: number;
};

export type UseCopyToClipboardResult = {
  copied: boolean;
  copy: (text: string) => Promise<boolean>;
};

export function useCopyToClipboard(
  options: UseCopyToClipboardOptions = {},
): UseCopyToClipboardResult {
  const { onSuccess, onError, resetMs = COPIED_FEEDBACK_RESET_MS } = options;
  const [copied, setCopied] = useState(false);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const onSuccessRef = useRef(onSuccess);
  const onErrorRef = useRef(onError);

  useEffect(() => {
    onSuccessRef.current = onSuccess;
  }, [onSuccess]);

  useEffect(() => {
    onErrorRef.current = onError;
  }, [onError]);

  const clearResetTimer = useCallback(() => {
    if (timerRef.current !== null) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
  }, []);

  useEffect(() => clearResetTimer, [clearResetTimer]);

  const copy = useCallback(
    async (text: string): Promise<boolean> => {
      try {
        await navigator.clipboard.writeText(text);
        setCopied(true);
        onSuccessRef.current?.();
        clearResetTimer();
        timerRef.current = setTimeout(() => {
          timerRef.current = null;
          setCopied(false);
        }, resetMs);
        return true;
      } catch (error) {
        onErrorRef.current?.(error);
        return false;
      }
    },
    [clearResetTimer, resetMs],
  );

  return { copied, copy };
}
