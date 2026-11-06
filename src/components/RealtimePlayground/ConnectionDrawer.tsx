import { useState } from "react";
import { Plus, SlidersHorizontal, Trash2, X } from "lucide-react";
import type { HeaderEntry, HeartbeatConfig, RealtimeProtocol } from "@/lib/realtimeClient";

interface ConnectionDrawerProps {
  isOpen: boolean;
  onClose: () => void;
  protocol: RealtimeProtocol;
  headers: HeaderEntry[];
  onUpdateHeaders: (headers: HeaderEntry[]) => void;
  subprotocols: string;
  onUpdateSubprotocols: (val: string) => void;
  heartbeat: HeartbeatConfig;
  onUpdateHeartbeat: (val: HeartbeatConfig) => void;
}

export function ConnectionDrawer({
  isOpen,
  onClose,
  protocol,
  headers,
  onUpdateHeaders,
  subprotocols,
  onUpdateSubprotocols,
  heartbeat,
  onUpdateHeartbeat,
}: ConnectionDrawerProps) {
  const [newKey, setNewKey] = useState("");
  const [newValue, setNewValue] = useState("");

  if (!isOpen) return null;

  const handleAddHeader = (e: React.FormEvent) => {
    e.preventDefault();
    if (!newKey.trim()) return;

    onUpdateHeaders([
      ...headers,
      {
        id: Math.random().toString(36).slice(2, 8),
        key: newKey.trim(),
        value: newValue.trim(),
        enabled: true,
      },
    ]);
    setNewKey("");
    setNewValue("");
  };

  const handleToggleHeader = (id: string) => {
    onUpdateHeaders(
      headers.map((h) => (h.id === id ? { ...h, enabled: !h.enabled } : h)),
    );
  };

  const handleDeleteHeader = (id: string) => {
    onUpdateHeaders(headers.filter((h) => h.id !== id));
  };

  return (
    <div className="fixed inset-0 z-50 flex justify-end bg-black/40 backdrop-blur-xs transition-opacity animate-in fade-in">
      <div className="flex h-full w-full max-w-md flex-col border-l border-slate-200/80 bg-white p-6 shadow-2xl dark:border-slate-800 dark:bg-slate-900">
        {/* 抽屉头部 */}
        <div className="flex items-center justify-between border-b border-slate-100 pb-4 dark:border-slate-800">
          <div className="flex items-center gap-2">
            <SlidersHorizontal className="h-4 w-4 text-sky-500" />
            <h2 className="text-sm font-semibold text-slate-800 dark:text-slate-100">
              连接高级配置 ({protocol === "websocket" ? "WebSocket" : "SSE"})
            </h2>
          </div>
          <button
            type="button"
            onClick={onClose}
            className="rounded-lg p-1.5 text-slate-400 hover:bg-slate-100 hover:text-slate-700 dark:hover:bg-slate-800 dark:hover:text-slate-200"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        {/* 抽屉内容区 */}
        <div className="flex-1 space-y-6 overflow-y-auto py-5">
          {/* 请求头 Headers */}
          <div>
            <div className="mb-2 flex items-center justify-between">
              <label className="text-xs font-semibold text-slate-700 dark:text-slate-300">
                自定义请求头 (Headers)
              </label>
              <span className="text-[11px] text-slate-400">
                可传递鉴权 Authorization / Cookie / User-Agent
              </span>
            </div>

            <div className="space-y-2">
              {headers.map((header) => (
                <div
                  key={header.id}
                  className="flex items-center gap-2 rounded-lg border border-slate-200/80 bg-slate-50/50 p-2 dark:border-slate-800 dark:bg-slate-800/40"
                >
                  <input
                    type="checkbox"
                    checked={header.enabled}
                    onChange={() => handleToggleHeader(header.id)}
                    className="h-3.5 w-3.5 rounded border-slate-300 text-sky-600 focus:ring-sky-500 dark:border-slate-700 dark:bg-slate-800"
                  />
                  <span className="w-1/3 truncate font-mono text-xs font-medium text-slate-700 dark:text-slate-200">
                    {header.key}
                  </span>
                  <span className="flex-1 truncate font-mono text-xs text-slate-500 dark:text-slate-400">
                    {header.value}
                  </span>
                  <button
                    type="button"
                    onClick={() => handleDeleteHeader(header.id)}
                    className="p-1 text-slate-400 hover:text-rose-500"
                  >
                    <Trash2 className="h-3.5 w-3.5" />
                  </button>
                </div>
              ))}

              <form onSubmit={handleAddHeader} className="flex gap-2 pt-1">
                <input
                  type="text"
                  value={newKey}
                  onChange={(e) => setNewKey(e.target.value)}
                  placeholder="Key (如 Authorization)"
                  className="w-1/3 rounded-lg border border-slate-200 px-2.5 py-1.5 font-mono text-xs focus:border-sky-500 focus:outline-none dark:border-slate-700 dark:bg-slate-800"
                />
                <input
                  type="text"
                  value={newValue}
                  onChange={(e) => setNewValue(e.target.value)}
                  placeholder="Value (如 Bearer token)"
                  className="flex-1 rounded-lg border border-slate-200 px-2.5 py-1.5 font-mono text-xs focus:border-sky-500 focus:outline-none dark:border-slate-700 dark:bg-slate-800"
                />
                <button
                  type="submit"
                  disabled={!newKey.trim()}
                  className="rounded-lg bg-sky-600 px-2.5 py-1.5 text-xs text-white hover:bg-sky-500 disabled:opacity-40"
                >
                  <Plus className="h-3.5 w-3.5" />
                </button>
              </form>
            </div>
          </div>

          {/* WebSocket 专属：子协议 */}
          {protocol === "websocket" ? (
            <div>
              <label className="mb-1 block text-xs font-semibold text-slate-700 dark:text-slate-300">
                Sec-WebSocket-Protocol (子协议)
              </label>
              <input
                type="text"
                value={subprotocols}
                onChange={(e) => onUpdateSubprotocols(e.target.value)}
                placeholder="可选，多个协议以逗号分隔 (如 chat, v1)"
                className="w-full rounded-lg border border-slate-200/80 bg-slate-50/50 px-3 py-2 font-mono text-xs text-slate-800 focus:border-sky-500 focus:bg-white focus:outline-none dark:border-slate-700 dark:bg-slate-800/50 dark:text-slate-200"
              />
            </div>
          ) : null}

          {/* WebSocket 专属：心跳保活引擎配置 */}
          {protocol === "websocket" ? (
            <div className="rounded-xl border border-slate-200/80 bg-slate-50/60 p-4 dark:border-slate-800 dark:bg-slate-800/50">
              <div className="flex items-center justify-between">
                <div>
                  <h4 className="text-xs font-semibold text-slate-800 dark:text-slate-100">
                    自动心跳保活引擎 (Heartbeat Keeper)
                  </h4>
                  <p className="mt-0.5 text-[11px] text-slate-400">
                    连接建立后，按指定间隔自动发送 Ping 消息防断线
                  </p>
                </div>
                <input
                  type="checkbox"
                  checked={heartbeat.enabled}
                  onChange={(e) =>
                    onUpdateHeartbeat({ ...heartbeat, enabled: e.target.checked })
                  }
                  className="h-4 w-4 rounded border-slate-300 text-sky-600 focus:ring-sky-500 dark:border-slate-700 dark:bg-slate-800"
                />
              </div>

              {heartbeat.enabled ? (
                <div className="mt-3 space-y-3 border-t border-slate-200/60 pt-3 dark:border-slate-700/60">
                  <div className="flex items-center justify-between gap-3">
                    <span className="text-xs text-slate-600 dark:text-slate-300">
                      发送周期 (秒)
                    </span>
                    <input
                      type="number"
                      min={1}
                      max={300}
                      value={heartbeat.intervalSeconds}
                      onChange={(e) =>
                        onUpdateHeartbeat({
                          ...heartbeat,
                          intervalSeconds: Number(e.target.value) || 15,
                        })
                      }
                      className="w-20 rounded-lg border border-slate-200 px-2 py-1 text-right font-mono text-xs dark:border-slate-700 dark:bg-slate-800"
                    />
                  </div>

                  <div>
                    <span className="mb-1 block text-xs text-slate-600 dark:text-slate-300">
                      心跳包内容 (Payload)
                    </span>
                    <input
                      type="text"
                      value={heartbeat.payload}
                      onChange={(e) =>
                        onUpdateHeartbeat({ ...heartbeat, payload: e.target.value })
                      }
                      className="w-full rounded-lg border border-slate-200 px-2.5 py-1.5 font-mono text-xs dark:border-slate-700 dark:bg-slate-800"
                    />
                  </div>
                </div>
              ) : null}
            </div>
          ) : null}
        </div>

        {/* 底部完成按钮 */}
        <div className="border-t border-slate-100 pt-4 dark:border-slate-800">
          <button
            type="button"
            onClick={onClose}
            className="w-full rounded-xl bg-sky-600 py-2 text-xs font-semibold text-white shadow-xs hover:bg-sky-500"
          >
            保存并关闭
          </button>
        </div>
      </div>
    </div>
  );
}
