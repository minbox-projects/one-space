import { useState } from "react";
import { CornerDownLeft, Sparkles, Wand2 } from "lucide-react";
import { tryParseJson } from "@/lib/realtimeClient";

interface MessageSenderProps {
  onSendMessage: (content: string) => void;
  disabled: boolean;
}

const PRESETS = [
  { label: "Ping 心跳", payload: '{"type": "ping"}' },
  { label: "订阅消息", payload: '{"action": "subscribe", "channel": "events"}' },
  { label: "问候消息", payload: '{"event": "hello", "client": "OneSpace"}' },
];

export function MessageSender({ onSendMessage, disabled }: MessageSenderProps) {
  const [input, setInput] = useState('{"message": "Hello from OneSpace"}');

  const handleSend = () => {
    if (!input.trim() || disabled) return;
    onSendMessage(input.trim());
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
      e.preventDefault();
      handleSend();
    }
  };

  const handleFormatJson = () => {
    const { isJson, formatted } = tryParseJson(input);
    if (isJson) {
      setInput(formatted);
    }
  };

  return (
    <div className="rounded-2xl border border-slate-200/80 bg-white p-4 shadow-xs dark:border-slate-800/80 dark:bg-slate-900">
      {/* 预设模板与格式化 */}
      <div className="mb-2 flex items-center justify-between">
        <div className="flex items-center gap-1.5">
          <Sparkles className="h-3.5 w-3.5 text-sky-500" />
          <span className="text-[11px] font-medium text-slate-500 dark:text-slate-400">
            预设模版:
          </span>
          {PRESETS.map((p) => (
            <button
              key={p.label}
              type="button"
              onClick={() => setInput(p.payload)}
              disabled={disabled}
              className="rounded-md border border-slate-200/80 bg-slate-50/50 px-2 py-0.5 text-[11px] text-slate-600 hover:bg-slate-100 disabled:opacity-40 dark:border-slate-800 dark:bg-slate-800 dark:text-slate-300 dark:hover:bg-slate-700"
            >
              {p.label}
            </button>
          ))}
        </div>

        <button
          type="button"
          onClick={handleFormatJson}
          disabled={disabled}
          title="美化 JSON 格式"
          className="flex items-center gap-1 rounded-md px-2 py-0.5 text-[11px] text-sky-600 hover:bg-sky-50 disabled:opacity-40 dark:text-sky-400 dark:hover:bg-sky-950/40"
        >
          <Wand2 className="h-3 w-3" />
          <span>美化 JSON</span>
        </button>
      </div>

      {/* 消息输入框 */}
      <textarea
        rows={3}
        value={input}
        onChange={(e) => setInput(e.target.value)}
        onKeyDown={handleKeyDown}
        disabled={disabled}
        placeholder="输入要发送的 WebSocket 文本或 JSON，支持 ⌘+Enter 快速发送..."
        className="w-full resize-none rounded-xl border border-slate-200/80 bg-slate-50/50 p-2.5 font-mono text-xs text-slate-800 placeholder-slate-400 focus:border-sky-500 focus:bg-white focus:outline-none disabled:opacity-40 dark:border-slate-800 dark:bg-slate-800/50 dark:text-slate-100 dark:focus:border-sky-400 dark:focus:bg-slate-800"
      />

      {/* 发送按钮 */}
      <div className="mt-2 flex items-center justify-between">
        <span className="text-[11px] text-slate-400">
          快捷键: <kbd className="rounded bg-slate-100 px-1 py-0.5 dark:bg-slate-800">⌘+Enter</kbd> 发送
        </span>

        <button
          type="button"
          onClick={handleSend}
          disabled={disabled || !input.trim()}
          className="flex items-center gap-1.5 rounded-xl bg-sky-600 px-4 py-1.5 text-xs font-semibold text-white shadow-xs transition-all hover:bg-sky-500 disabled:opacity-40"
        >
          <CornerDownLeft className="h-3.5 w-3.5" />
          <span>发送消息</span>
        </button>
      </div>
    </div>
  );
}
