import type { Dispatch, RefObject, SetStateAction } from "react";
import type { TFunction } from "i18next";
import { X } from "lucide-react";
import type { SftpTailSession } from "../types";

type SftpLogConsoleProps = {
  t: TFunction;
  tailSession: SftpTailSession;
  setTailSession: Dispatch<SetStateAction<SftpTailSession | null>>;
  filteredLogLines: string[];
  copiedLog: boolean;
  copyLog: (text: string) => Promise<boolean>;
  logConsoleEndRef: RefObject<HTMLDivElement | null>;
};

export function SftpLogConsole({
  t,
  tailSession,
  setTailSession,
  filteredLogLines,
  copiedLog,
  copyLog,
  logConsoleEndRef,
}: SftpLogConsoleProps) {
  return (
    <div className="rounded-lg border bg-zinc-950 text-zinc-100 overflow-hidden shadow-xl space-y-2">
      {/* 控制台顶栏 */}
      <div className="flex items-center justify-between border-b border-zinc-800 bg-zinc-900/80 px-3 py-2 text-xs">
        <div className="flex items-center gap-2">
          <span className="flex h-2 w-2 rounded-full bg-emerald-500 animate-ping" />
          <span className="font-mono font-semibold">
            {tailSession.fileName}
          </span>
          <span className="text-[11px] text-zinc-400">
            ({tailSession.lines.length} lines)
          </span>
        </div>

        <div className="flex items-center gap-2">
          <input
            type="text"
            value={tailSession.filter}
            onChange={(e) =>
              setTailSession({ ...tailSession, filter: e.target.value })
            }
            placeholder={t("sftpLogFilter", "Filter keywords...")}
            className="h-6 w-32 rounded bg-zinc-800 px-2 font-mono text-[11px] text-zinc-200 outline-none"
          />

          <button
            type="button"
            onClick={() =>
              setTailSession({
                ...tailSession,
                autoScroll: !tailSession.autoScroll,
              })
            }
            className={`rounded px-2 py-0.5 text-[11px] ${
              tailSession.autoScroll
                ? "bg-emerald-500/20 text-emerald-400"
                : "bg-zinc-800 text-zinc-400"
            }`}
          >
            {t("sftpAutoScroll", "Auto-scroll")}
          </button>

          <button
            type="button"
            onClick={() => void copyLog(tailSession.lines.join("\n"))}
            className="rounded px-2 py-0.5 text-[11px] bg-zinc-800 text-zinc-300 hover:bg-zinc-700"
          >
            {copiedLog ? (
              t("sftpCopied", "Copied")
            ) : (
              t("sftpCopy", "Copy")
            )}
          </button>

          <button
            type="button"
            onClick={() =>
              setTailSession({ ...tailSession, lines: [] })
            }
            className="rounded px-2 py-0.5 text-[11px] bg-zinc-800 text-zinc-300 hover:bg-zinc-700"
          >
            {t("sftpClear", "Clear")}
          </button>

          <button
            type="button"
            onClick={() => setTailSession(null)}
            className="rounded p-1 text-zinc-400 hover:text-rose-400"
          >
            <X className="h-4 w-4" />
          </button>
        </div>
      </div>

      {/* 控制台日志内容 */}
      <div className="max-h-80 overflow-y-auto p-3 font-mono text-xs leading-5">
        {filteredLogLines.map((line, idx) => (
          <div
            key={idx}
            className="hover:bg-zinc-900/60 px-1 rounded break-all whitespace-pre-wrap"
          >
            {line}
          </div>
        ))}
        <div ref={logConsoleEndRef} />
      </div>
    </div>
  );
}
