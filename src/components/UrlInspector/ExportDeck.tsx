import { useState } from "react";
import { Check, Copy, Terminal } from "lucide-react";
import { useCopyToClipboard } from "@/toolbox/useCopyToClipboard";
import type { QueryParamItem } from "@/lib/urlInspector";
import { generateExportCode } from "@/lib/urlInspector";

interface ExportDeckProps {
  fullUrl: string;
  params: QueryParamItem[];
}

type ExportFormat = "curl" | "fetch" | "python" | "json" | "markdown";

export function ExportDeck({ fullUrl, params }: ExportDeckProps) {
  const [format, setFormat] = useState<ExportFormat>("curl");
  const { copy, copied } = useCopyToClipboard();

  const code = generateExportCode(format, fullUrl, params);

  const tabs: Array<{ id: ExportFormat; label: string }> = [
    { id: "curl", label: "cURL" },
    { id: "fetch", label: "Fetch (JS)" },
    { id: "python", label: "Python" },
    { id: "json", label: "JSON" },
    { id: "markdown", label: "Markdown" },
  ];

  return (
    <div className="rounded-2xl border border-slate-200/80 bg-white p-5 shadow-xs dark:border-slate-800/80 dark:bg-slate-900">
      <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
        <div className="flex items-center gap-2">
          <Terminal className="h-4 w-4 text-violet-500" />
          <h3 className="text-xs font-semibold text-slate-800 dark:text-slate-100">
            多语言代码与参数快捷导出
          </h3>
        </div>

        {/* 格式切换 Tab */}
        <div className="flex rounded-xl bg-slate-100 p-0.5 dark:bg-slate-800">
          {tabs.map((tab) => (
            <button
              key={tab.id}
              type="button"
              onClick={() => setFormat(tab.id)}
              className={`rounded-lg px-2.5 py-1 text-xs font-medium transition-all ${
                format === tab.id
                  ? "bg-white text-violet-600 shadow-xs dark:bg-slate-700 dark:text-violet-400"
                  : "text-slate-500 hover:text-slate-800 dark:text-slate-400 dark:hover:text-slate-200"
              }`}
            >
              {tab.label}
            </button>
          ))}
        </div>
      </div>

      {/* 代码预览与复制 */}
      <div className="relative rounded-xl border border-slate-200/80 bg-slate-900 p-4 font-mono text-xs text-slate-200 dark:border-slate-800">
        <button
          type="button"
          onClick={() => copy(code)}
          className="absolute right-3 top-3 flex items-center gap-1.5 rounded-lg border border-slate-700 bg-slate-800/90 px-2.5 py-1 text-xs font-medium text-slate-300 shadow-sm backdrop-blur-xs transition-all hover:bg-slate-700 hover:text-white"
        >
          {copied ? (
            <>
              <Check className="h-3.5 w-3.5 text-emerald-400" />
              <span className="text-emerald-400">已复制</span>
            </>
          ) : (
            <>
              <Copy className="h-3.5 w-3.5" />
              <span>复制代码</span>
            </>
          )}
        </button>

        <pre className="max-h-56 overflow-auto whitespace-pre-wrap pr-20 selection:bg-violet-500/30">
          {code}
        </pre>
      </div>
    </div>
  );
}
