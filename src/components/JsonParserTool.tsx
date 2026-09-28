import { useState } from "react";
import { Check, Copy, WandSparkles } from "lucide-react";
import { useTranslation } from "react-i18next";
import { ToolShell } from "@/components/toolbox/ToolShell";
import { useToast } from "./ToastProvider";
import { getToolboxTool } from "@/toolbox/registry";
import { useCopyToClipboard } from "@/toolbox/useCopyToClipboard";

export function JsonParserTool() {
  const { t } = useTranslation();
  const { pushToast } = useToast();
  const { icon: ToolIcon, iconClassName } = getToolboxTool("json-parser")!;
  const [content, setContent] = useState("");
  const [indent, setIndent] = useState(2);
  const [error, setError] = useState("");
  const { copied, copy } = useCopyToClipboard({
    onSuccess: () => pushToast({ title: t("jsonParserCopied", "JSON copied"), kind: "success" }),
    onError: () => pushToast({ title: t("jsonParserCopyFailed", "Unable to copy JSON"), kind: "error" }),
  });

  const formatJson = () => {
    try {
      setContent(JSON.stringify(JSON.parse(content), null, indent));
      setError("");
    } catch (caughtError) {
      setError(caughtError instanceof Error ? caughtError.message : String(caughtError));
    }
  };

  return (
    <ToolShell
      titleId="json-parser-title"
      title={t("jsonParser", "JSON Parser")}
      description={t("jsonParserToolDesc", "Validate and format JSON locally in one editable workspace.")}
      error={error ? `${t("jsonParserError", "JSON parse error")}: ${error}` : null}
      icon={
        <div className={`rounded-lg p-2 ${iconClassName}`}>
          <ToolIcon className="h-5 w-5" />
        </div>
      }
    >
          <div className="space-y-4 rounded-lg border bg-card p-4">
            <div className="flex flex-wrap items-end justify-between gap-3">
              <label className="grid gap-1.5 text-sm font-medium" htmlFor="json-parser-indent">
                {t("jsonParserIndent", "Indentation")}
                <select
                  id="json-parser-indent"
                  value={indent}
                  onChange={(event) => setIndent(Number(event.target.value))}
                  className="h-10 rounded-md border bg-background px-3 text-sm outline-none"
                >
                  {[2, 4, 8].map((value) => (
                    <option key={value} value={value}>
                      {t("jsonParserSpaces", "{{count}} spaces", { count: value })}
                    </option>
                  ))}
                </select>
              </label>
              <div className="flex flex-wrap gap-2">
                <button
                  type="button"
                  onClick={formatJson}
                  className="inline-flex h-10 items-center gap-2 rounded-md bg-primary px-4 text-sm font-medium text-primary-foreground hover:bg-primary/90"
                >
                  <WandSparkles className="h-4 w-4" />
                  {t("jsonParserFormat", "Format JSON")}
                </button>
                <button
                  type="button"
                  onClick={() => void copy(content)}
                  className="inline-flex h-10 items-center gap-2 rounded-md border px-3 text-sm font-medium hover:bg-muted"
                >
                  {copied ? <Check className="h-4 w-4 text-emerald-600" /> : <Copy className="h-4 w-4" />}
                  {t("jsonParserCopy", "Copy JSON")}
                </button>
              </div>
            </div>

            <label className="sr-only" htmlFor="json-parser-content">
              {t("jsonParserInput", "JSON input")}
            </label>
            <textarea
              id="json-parser-content"
              value={content}
              onChange={(event) => {
                setContent(event.target.value);
                setError("");
              }}
              placeholder={t("jsonParserPlaceholder", "Paste or write JSON here...")}
              className="min-h-64 w-full resize rounded-md border bg-background p-3 font-mono text-sm leading-6 outline-none ring-offset-background focus-visible:ring-2 focus-visible:ring-ring"
              spellCheck={false}
            />
          </div>
        </ToolShell>
  );
}
