import { useEffect, useLayoutEffect, useRef, useState } from "react";
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

type TextareaSelection = {
  start: number;
  end: number;
  direction: "forward" | "backward" | "none";
};

type PendingTextareaEdit = {
  rawValue: string;
  textareaValue: string;
  selection: TextareaSelection;
  inputType: string;
  data: string | null;
  pastedText: string | null;
  isComposing: boolean;
  followsCompositionEnd: boolean;
};

type CompositionEdit = {
  rawValue: string;
  textareaValue: string;
  selection: TextareaSelection;
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

function replaceDisplayedRange(
  rawValue: string,
  displayedStart: number,
  displayedEnd: number,
  replacement: string,
) {
  const rawStart = rawOffsetAt(rawValue, displayedStart);
  const rawEnd = rawOffsetAt(rawValue, displayedEnd);
  return `${rawValue.slice(0, rawStart)}${replacement}${rawValue.slice(rawEnd)}`;
}

function insertedTextForEdit(edit: PendingTextareaEdit, nextTextareaValue: string) {
  if (edit.inputType === "insertFromPaste" && edit.pastedText !== null) {
    return edit.pastedText;
  }
  if (edit.inputType === "insertLineBreak" || edit.inputType === "insertParagraph") {
    return edit.data ?? "\n";
  }
  if (edit.data !== null) return edit.data;

  const unchangedSuffixLength = edit.textareaValue.length - edit.selection.end;
  const insertedEnd = Math.max(edit.selection.start, nextTextareaValue.length - unchangedSuffixLength);
  return nextTextareaValue.slice(edit.selection.start, insertedEnd);
}

function applyTextareaEdit(
  edit: PendingTextareaEdit,
  nextTextareaValue: string,
  nextSelection: TextareaSelection,
) {
  if (edit.inputType.startsWith("delete")) {
    if (edit.selection.start !== edit.selection.end) {
      return replaceDisplayedRange(edit.rawValue, edit.selection.start, edit.selection.end, "");
    }

    const deletedLength = Math.max(0, edit.textareaValue.length - nextTextareaValue.length);
    const deletionStart = Math.min(nextSelection.start, edit.selection.start);
    return replaceDisplayedRange(edit.rawValue, deletionStart, deletionStart + deletedLength, "");
  }

  if (edit.inputType.startsWith("insert")) {
    return replaceDisplayedRange(
      edit.rawValue,
      edit.selection.start,
      edit.selection.end,
      insertedTextForEdit(edit, nextTextareaValue),
    );
  }

  return nextTextareaValue;
}

export function Md5EncryptionTool() {
  const { t } = useTranslation();
  const { pushToast } = useToast();
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const rawInputRef = useRef("");
  const pendingEditRef = useRef<PendingTextareaEdit | null>(null);
  const pendingPastedTextRef = useRef<string | null>(null);
  const compositionEditRef = useRef<CompositionEdit | null>(null);
  const completedCompositionValueRef = useRef<string | null>(null);
  const pendingSelectionRef = useRef<TextareaSelection | null>(null);
  const [input, setInput] = useState("");
  const [results, setResults] = useState<Md5Results | null>(null);

  const updateInput = (nextInput: string) => {
    rawInputRef.current = nextInput;
    setInput(nextInput);
  };

  const readSelection = (textarea: HTMLTextAreaElement): TextareaSelection => ({
    start: textarea.selectionStart,
    end: textarea.selectionEnd,
    direction: textarea.selectionDirection ?? "none",
  });

  useEffect(() => {
    const textarea = inputRef.current;
    if (!textarea) return;

    const captureBeforeInput = (event: InputEvent) => {
      const inputType = event.inputType || (event.data === null ? "" : "insertText");
      const followsCompositionEnd =
        completedCompositionValueRef.current === textarea.value &&
        (inputType === "insertCompositionText" || inputType === "insertFromComposition");
      pendingEditRef.current = {
        rawValue: rawInputRef.current,
        textareaValue: textarea.value,
        selection: readSelection(textarea),
        inputType,
        data: event.data,
        pastedText: inputType === "insertFromPaste" ? pendingPastedTextRef.current : null,
        isComposing: event.isComposing || compositionEditRef.current !== null,
        followsCompositionEnd,
      };
      if (!followsCompositionEnd && !event.isComposing && compositionEditRef.current === null) {
        completedCompositionValueRef.current = null;
      }
      if (inputType !== "insertFromPaste") pendingPastedTextRef.current = null;
    };

    textarea.addEventListener("beforeinput", captureBeforeInput);
    return () => textarea.removeEventListener("beforeinput", captureBeforeInput);
  }, []);

  useLayoutEffect(() => {
    if (pendingSelectionRef.current === null) return;
    const { start, end, direction } = pendingSelectionRef.current;
    inputRef.current?.setSelectionRange(start, end, direction);
    pendingSelectionRef.current = null;
  });

  const applyCompositionValue = (nextTextareaValue: string) => {
    const compositionEdit = compositionEditRef.current;
    if (!compositionEdit) return;

    const unchangedSuffixLength =
      compositionEdit.textareaValue.length - compositionEdit.selection.end;
    const compositionEnd = Math.max(
      compositionEdit.selection.start,
      nextTextareaValue.length - unchangedSuffixLength,
    );
    updateInput(
      replaceDisplayedRange(
        compositionEdit.rawValue,
        compositionEdit.selection.start,
        compositionEdit.selection.end,
        nextTextareaValue.slice(compositionEdit.selection.start, compositionEnd),
      ),
    );
  };

  const processTextareaInput = (textarea: HTMLTextAreaElement) => {
    const nextTextareaValue = textarea.value;
    const nextSelection = readSelection(textarea);
    const pendingEdit = pendingEditRef.current;
    pendingEditRef.current = null;

    if (compositionEditRef.current !== null || pendingEdit?.isComposing) {
      applyCompositionValue(nextTextareaValue);
      return;
    }

    if (
      pendingEdit?.followsCompositionEnd &&
      toTextareaValue(rawInputRef.current) === nextTextareaValue
    ) {
      completedCompositionValueRef.current = null;
      pendingSelectionRef.current = nextSelection;
      return;
    }

    if (pendingEdit) {
      updateInput(applyTextareaEdit(pendingEdit, nextTextareaValue, nextSelection));
      pendingSelectionRef.current = nextSelection;
      if (pendingEdit.inputType === "insertFromPaste") pendingPastedTextRef.current = null;
      return;
    }

    if (toTextareaValue(rawInputRef.current) !== nextTextareaValue) {
      updateInput(nextTextareaValue);
    }
  };
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
    updateInput("");
    setResults(null);
    inputRef.current?.focus();
  };

  const beginComposition = (event: React.CompositionEvent<HTMLTextAreaElement>) => {
    pendingEditRef.current = null;
    completedCompositionValueRef.current = null;
    compositionEditRef.current = {
      rawValue: rawInputRef.current,
      textareaValue: event.currentTarget.value,
      selection: readSelection(event.currentTarget),
    };
  };

  const finishComposition = (event: React.CompositionEvent<HTMLTextAreaElement>) => {
    applyCompositionValue(event.currentTarget.value);
    compositionEditRef.current = null;
    completedCompositionValueRef.current = event.currentTarget.value;
    pendingEditRef.current = null;
    pendingSelectionRef.current = readSelection(event.currentTarget);
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
            onInput={(event) => processTextareaInput(event.currentTarget)}
            onChange={(event) => processTextareaInput(event.currentTarget)}
            onPaste={(event) => {
              pendingPastedTextRef.current = event.clipboardData.getData("text/plain");
            }}
            onCompositionStart={beginComposition}
            onCompositionEnd={finishComposition}
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
