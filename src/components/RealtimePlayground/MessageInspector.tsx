import { useState } from "react";
import { Check, Copy, FileCode, FileText } from "lucide-react";
import { useCopyToClipboard } from "@/toolbox/useCopyToClipboard";
import type { RealtimeMessage } from "@/lib/realtimeClient";
import { tryParseJson } from "@/lib/realtimeClient";

interface MessageInspectorProps {
  message: RealtimeMessage | null;
}

type ViewMode = "formatted" | "raw";

export function MessageInspector({ message }: MessageInspectorProps) {
  const [viewMode, setViewMode] = useState<ViewMode>("formatted");
  const { copy, copied } = useCopyToClipboard();

  if (!message) {
    return (
      <div className="flex h-full flex-col items-center justify-center rounded-2xl border border-slate-200/80 bg-white p-6 text-center text-xs text-slate-400 dark:border-slate-800/80 dark:bg-slate-900">
        <FileCode className="h-8 w-8 text-slate-300 dark:text-slate-600" />
        <p className="mt-2 font-medium">请从左侧选择一条消息</p>
        <p className="text-[11px]">查看完整的消息载荷、JSON 格式化与元数据</p>
      </div>
    );
  }

  const { isJson, formatted } = tryParseJson(message.content);

  return (
    <div className="flex h-full flex-col rounded-2xl border border-slate-200/80 bg-white shadow-xs dark:border-slate-800/80 dark:bg-slate-900">
      {/* 头部工具栏 */}
      <div className="flex items-center justify-between border-b border-slate-100 p-3.5 dark:border-slate-800">
        <div className="flex items-center gap-2">
          <span className="text-xs font-semibold text-slate-800 dark:text-slate-100">
            消息检查器
          </span>
          <span className="rounded bg-slate-100 px-1.5 py-0.5 font-mono text-[10px] text-slate-500 dark:bg-slate-800 dark:text-slate-400">
            {message.size} bytes
          </span>
          {isJson ? (
            <span className="rounded bg-emerald-500/10 px-1.5 py-0.5 text-[10px] font-semibold text-emerald-600 dark:bg-emerald-500/20 dark:text-emerald-400">
              JSON
            </span>
          ) : null}
        </div>

        <div className="flex items-center gap-1.5">
          {/* 模式切换 */}
          <div className="flex rounded-lg bg-slate-100 p-0.5 dark:bg-slate-800">
            <button
              type="button"
              onClick={() => setViewMode("formatted")}
              className={`flex items-center gap-1 rounded-md px-2 py-1 text-xs font-medium transition-all ${
                viewMode === "formatted"
                  ? "bg-white text-sky-600 shadow-xs dark:bg-slate-700 dark:text-sky-400"
                  : "text-slate-500 hover:text-slate-800 dark:text-slate-400 dark:hover:text-slate-200"
              }`}
            >
              <FileCode className="h-3 w-3" />
              <span>美化</span>
            </button>
            <button
              type="button"
              onClick={() => setViewMode("raw")}
              className={`flex items-center gap-1 rounded-md px-2 py-1 text-xs font-medium transition-all ${
                viewMode === "raw"
                  ? "bg-white text-sky-600 shadow-xs dark:bg-slate-700 dark:text-sky-400"
                  : "text-slate-500 hover:text-slate-800 dark:text-slate-400 dark:hover:text-slate-200"
              }`}
            >
              <FileText className="h-3 w-3" />
              <span>原始</span>
            </button>
          </div>

          {/* 复制 */}
          <button
            type="button"
            onClick={() => copy(message.content)}
            className="flex items-center gap-1 rounded-lg border border-slate-200 bg-white px-2.5 py-1 text-xs font-medium text-slate-700 hover:bg-slate-50 dark:border-slate-700 dark:bg-slate-800 dark:text-slate-300 dark:hover:bg-slate-700"
          >
            {copied ? (
              <>
                <Check className="h-3 w-3 text-emerald-500" />
                <span className="text-emerald-500">已复制</span>
              </>
            ) : (
              <>
                <Copy className="h-3 w-3" />
                <span>复制</span>
              </>
            )}
          </button>
        </div>
      </div>

      {/* 消息正文 */}
      <div className="flex-1 overflow-auto p-4 font-mono text-xs leading-relaxed">
        <pre className="whitespace-pre-wrap selection:bg-sky-500/20 text-slate-800 dark:text-slate-200">
          {viewMode === "formatted" ? formatted : message.content}
        </pre>
      </div>
    </div>
  );
}
