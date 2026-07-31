import { useLayoutEffect, useRef, useState } from "react";
import { AlertTriangle, Copy, Hash, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useToast } from "./ToastProvider";
import { md5Hex } from "@/lib/md5";

type Md5Results = {
  lower32: string;
  upper32: string;
  lower16: string;
  upper16: string;
};

const RESULT_ROWS = ["lower32", "upper32", "lower16", "upper16"] as const;

function toTextareaValue(rawValue: string) {
  let textareaValue = "";
  for (let index = 0; index < rawValue.length; index += 1) {
    if (rawValue[index] === "\r" && rawValue[index + 1] === "\n") {
      textareaValue += "\n";
      index += 1;
    } else {
      textareaValue += rawValue[index];
    }
  }
  return textareaValue;
}

function rawOffsetAt(rawValue: string, textareaOffset: number) {
  let rawOffset = 0;
  let displayedOffset = 0;

  while (rawOffset < rawValue.length && displayedOffset < textareaOffset) {
    rawOffset += rawValue[rawOffset] === "\r" && rawValue[rawOffset + 1] === "\n" ? 2 : 1;
    displayedOffset += 1;
  }

  return rawOffset;
}

function applyTextareaChange(rawValue: string, nextTextareaValue: string) {
  const previousTextareaValue = toTextareaValue(rawValue);
  let prefixLength = 0;

  while (
    prefixLength < previousTextareaValue.length &&
    prefixLength < nextTextareaValue.length &&
    previousTextareaValue[prefixLength] === nextTextareaValue[prefixLength]
  ) {
    prefixLength += 1;
  }

  let previousSuffixStart = previousTextareaValue.length;
  let nextSuffixStart = nextTextareaValue.length;
  while (
    previousSuffixStart > prefixLength &&
    nextSuffixStart > prefixLength &&
    previousTextareaValue[previousSuffixStart - 1] === nextTextareaValue[nextSuffixStart - 1]
  ) {
    previousSuffixStart -= 1;
    nextSuffixStart -= 1;
  }

  const rawChangeStart = rawOffsetAt(rawValue, prefixLength);
  const rawChangeEnd = rawOffsetAt(rawValue, previousSuffixStart);
  return `${rawValue.slice(0, rawChangeStart)}${nextTextareaValue.slice(
    prefixLength,
    nextSuffixStart,
  )}${rawValue.slice(rawChangeEnd)}`;
}

export function Md5EncryptionTool() {
  const { t } = useTranslation();
  const { pushToast } = useToast();
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const pendingSelectionRef = useRef<number | null>(null);
  const [input, setInput] = useState("");
  const [results, setResults] = useState<Md5Results | null>(null);

  useLayoutEffect(() => {
    if (pendingSelectionRef.current === null) return;
    inputRef.current?.setSelectionRange(pendingSelectionRef.current, pendingSelectionRef.current);
    pendingSelectionRef.current = null;
  });

  const calculate = () => {
    const lower32 = md5Hex(input);
    const lower16 = lower32.slice(8, 24);
    setResults({
      lower32,
      upper32: lower32.toUpperCase(),
      lower16,
      upper16: lower16.toUpperCase(),
    });
  };

  const copyResult = async (resultKey: keyof Md5Results) => {
    if (!results) return;

    const label = t(`md5Encryption.results.${resultKey}`);
    try {
      await navigator.clipboard.writeText(results[resultKey]);
      pushToast({
        title: t("md5Encryption.copySuccess", { label }),
        kind: "success",
      });
    } catch {
      pushToast({
        title: t("md5Encryption.copyFailed", { label }),
        kind: "error",
      });
    }
  };

  const clear = () => {
    setInput("");
    setResults(null);
    inputRef.current?.focus();
  };

  const preservePastedText = (event: React.ClipboardEvent<HTMLTextAreaElement>) => {
    const pastedText = event.clipboardData.getData("text/plain");
    const { selectionStart, selectionEnd } = event.currentTarget;
    event.preventDefault();
    pendingSelectionRef.current = selectionStart + toTextareaValue(pastedText).length;
    setInput((currentInput) => {
      const rawSelectionStart = rawOffsetAt(currentInput, selectionStart);
      const rawSelectionEnd = rawOffsetAt(currentInput, selectionEnd);
      return `${currentInput.slice(0, rawSelectionStart)}${pastedText}${currentInput.slice(rawSelectionEnd)}`;
    });
  };

  return (
    <section className="space-y-5 pb-5" aria-labelledby="md5-encryption-title">
      <div className="flex items-start gap-3">
        <div className="rounded-lg bg-teal-500/10 p-2 text-teal-600">
          <Hash className="h-5 w-5" aria-hidden="true" />
        </div>
        <div className="min-w-0">
          <h2 id="md5-encryption-title" className="text-lg font-semibold">
            {t("md5Encryption.title")}
          </h2>
          <p className="text-sm text-muted-foreground">
            {t("md5Encryption.description")}
          </p>
        </div>
      </div>

      <div className="flex items-start gap-2 rounded-md border border-amber-500/30 bg-amber-500/10 p-3 text-sm">
        <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-amber-600" aria-hidden="true" />
        <p>{t("md5Encryption.securityNotice")}</p>
      </div>

      <div className="space-y-4 rounded-lg border bg-card p-4">
        <label className="grid gap-1.5 text-sm font-medium" htmlFor="md5-encryption-input">
          {t("md5Encryption.inputLabel")}
          <textarea
            ref={inputRef}
            id="md5-encryption-input"
            value={toTextareaValue(input)}
            onChange={(event) =>
              setInput((currentInput) => applyTextareaChange(currentInput, event.target.value))
            }
            onPaste={preservePastedText}
            placeholder={t("md5Encryption.inputPlaceholder")}
            className="min-h-36 w-full resize-y rounded-md border bg-background p-3 font-mono text-sm leading-6 outline-none ring-offset-background focus-visible:ring-2 focus-visible:ring-ring"
            spellCheck={false}
          />
        </label>

        <div className="flex flex-wrap gap-2">
          <button
            type="button"
            onClick={calculate}
            className="inline-flex h-10 items-center gap-2 rounded-md bg-primary px-4 text-sm font-medium text-primary-foreground hover:bg-primary/90"
          >
            <Hash className="h-4 w-4" aria-hidden="true" />
            {t("md5Encryption.calculate")}
          </button>
          <button
            type="button"
            onClick={clear}
            className="inline-flex h-10 items-center gap-2 rounded-md border px-4 text-sm font-medium hover:bg-muted"
          >
            <Trash2 className="h-4 w-4" aria-hidden="true" />
            {t("md5Encryption.clear")}
          </button>
        </div>
      </div>

      <section className="space-y-3" aria-labelledby="md5-encryption-results-title">
        <h3 id="md5-encryption-results-title" className="text-sm font-semibold">
          {t("md5Encryption.resultsTitle")}
        </h3>
        {results ? (
          <div className="grid min-w-0 gap-2">
            {RESULT_ROWS.map((resultKey) => {
              const label = t(`md5Encryption.results.${resultKey}`);
              return (
                <div
                  key={resultKey}
                  data-testid={`md5-result-${resultKey}`}
                  className="flex min-h-16 min-w-0 items-center gap-3 rounded-md border bg-card p-3"
                >
                  <div className="min-w-0 flex-1">
                    <p className="text-xs font-medium text-muted-foreground">{label}</p>
                    <code className="mt-1 block min-w-0 break-all text-sm">{results[resultKey]}</code>
                  </div>
                  <button
                    type="button"
                    onClick={() => void copyResult(resultKey)}
                    className="inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground"
                    aria-label={t("md5Encryption.copyResult", { label })}
                    title={t("md5Encryption.copyResult", { label })}
                  >
                    <Copy className="h-4 w-4" aria-hidden="true" />
                  </button>
                </div>
              );
            })}
          </div>
        ) : (
          <p className="rounded-md border border-dashed p-4 text-sm text-muted-foreground">
            {t("md5Encryption.emptyState")}
          </p>
        )}
      </section>
    </section>
  );
}
