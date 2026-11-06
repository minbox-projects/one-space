import { useCallback, useEffect, useRef, useState } from "react";
import {
  Activity,
  Globe,
  Layers,
  Link,
  Power,
  Radio,
  SlidersHorizontal,
} from "lucide-react";
import { useToast } from "@/components/ToastProvider";
import type {
  ConnectionStatus,
  HeaderEntry,
  HeartbeatConfig,
  RealtimeMessage,
  RealtimeProtocol,
  SseSessionMetrics,
} from "@/lib/realtimeClient";
import {
  formatTime,
  parseSseBlock,
} from "@/lib/realtimeClient";

import { ConnectionDrawer } from "./ConnectionDrawer";
import { MessageInspector } from "./MessageInspector";
import { MessageSender } from "./MessageSender";
import { PublicTunnelPanel } from "./PublicTunnelPanel";
import { TimelineStream } from "./TimelineStream";

const DEFAULT_WS_URL = "wss://echo.websocket.org";
const DEFAULT_SSE_URL = "https://html5demos.com/sse/";

type PlaygroundTab = "client" | "tunnel";

export function RealtimePlaygroundTool() {
  const { pushToast } = useToast();

  const [activeTab, setActiveTab] = useState<PlaygroundTab>("client");
  const [protocol, setProtocol] = useState<RealtimeProtocol>("websocket");
  const [url, setUrl] = useState(DEFAULT_WS_URL);
  const [status, setStatus] = useState<ConnectionStatus>("disconnected");

  // 消息列表与当前选中消息
  const [messages, setMessages] = useState<RealtimeMessage[]>([]);
  const [selectedMessage, setSelectedMessage] = useState<RealtimeMessage | null>(null);

  // 搜索与滚动
  const [searchFilter, setSearchFilter] = useState("");
  const [autoScroll, setAutoScroll] = useState(true);

  // 高级配置抽屉
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [headers, setHeaders] = useState<HeaderEntry[]>([]);
  const [subprotocols, setSubprotocols] = useState("");
  const [heartbeat, setHeartbeat] = useState<HeartbeatConfig>({
    enabled: false,
    intervalSeconds: 15,
    payload: '{"type": "ping"}',
  });

  // SSE 指标
  const [sseMetrics, setSseMetrics] = useState<SseSessionMetrics | null>(null);

  // 运行时引用
  const wsRef = useRef<WebSocket | null>(null);
  const abortControllerRef = useRef<AbortController | null>(null);
  const heartbeatTimerRef = useRef<number | null>(null);
  const lastMessageTimeRef = useRef<number>(Date.now());

  // 切换协议时更新默认 URL
  const handleProtocolChange = (p: RealtimeProtocol) => {
    if (status === "connected" || status === "connecting") {
      handleDisconnect();
    }
    setProtocol(p);
    setUrl(p === "websocket" ? DEFAULT_WS_URL : DEFAULT_SSE_URL);
    setMessages([]);
    setSelectedMessage(null);
    setSseMetrics(null);
  };

  // 添加一条消息到时间轴
  const appendMessage = useCallback(
    (direction: "inbound" | "outbound", content: string, eventName?: string) => {
      const now = Date.now();
      const deltaMs = lastMessageTimeRef.current ? now - lastMessageTimeRef.current : 0;
      lastMessageTimeRef.current = now;

      const newMsg: RealtimeMessage = {
        id: Math.random().toString(36).slice(2, 10),
        direction,
        timestamp: now,
        timeStr: formatTime(now),
        content,
        size: new TextEncoder().encode(content).length,
        event: eventName,
        deltaMs: deltaMs > 0 && deltaMs < 60000 ? deltaMs : undefined,
      };

      setMessages((prev) => [...prev, newMsg]);
    },
    [],
  );

  // 断开连接清理
  const handleDisconnect = useCallback(() => {
    if (wsRef.current) {
      wsRef.current.close();
      wsRef.current = null;
    }
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
      abortControllerRef.current = null;
    }
    if (heartbeatTimerRef.current) {
      clearInterval(heartbeatTimerRef.current);
      heartbeatTimerRef.current = null;
    }
    setStatus("disconnected");
  }, []);

  // 启动自动心跳
  const startHeartbeat = useCallback(() => {
    if (heartbeatTimerRef.current) {
      clearInterval(heartbeatTimerRef.current);
    }
    if (!heartbeat.enabled || protocol !== "websocket") return;

    heartbeatTimerRef.current = window.setInterval(() => {
      if (wsRef.current && wsRef.current.readyState === WebSocket.OPEN) {
        wsRef.current.send(heartbeat.payload);
        appendMessage("outbound", heartbeat.payload, "heartbeat");
      }
    }, heartbeat.intervalSeconds * 1000);
  }, [heartbeat, protocol, appendMessage]);

  // 发起连接
  const handleConnect = async () => {
    if (!url.trim()) return;
    handleDisconnect();
    setStatus("connecting");

    if (protocol === "websocket") {
      try {
        const protocols = subprotocols.trim()
          ? subprotocols.split(",").map((s) => s.trim())
          : undefined;

        const ws = new WebSocket(url.trim(), protocols);
        wsRef.current = ws;

        ws.onopen = () => {
          setStatus("connected");
          pushToast({ title: "WebSocket 连接成功", kind: "success" });
          startHeartbeat();
        };

        ws.onmessage = (event) => {
          const content = typeof event.data === "string" ? event.data : "[Binary data]";
          appendMessage("inbound", content);
        };

        ws.onerror = () => {
          setStatus("error");
          pushToast({ title: "WebSocket 连接错误", kind: "error" });
        };

        ws.onclose = () => {
          setStatus("disconnected");
          if (heartbeatTimerRef.current) {
            clearInterval(heartbeatTimerRef.current);
          }
        };
      } catch (err) {
        setStatus("error");
        pushToast({
          title: "连接失败: " + (err instanceof Error ? err.message : String(err)),
          kind: "error",
        });
      }
    } else {
      // SSE 模式：使用 fetch + ReadableStream
      try {
        const controller = new AbortController();
        abortControllerRef.current = controller;

        const headerObj: Record<string, string> = {
          Accept: "text/event-stream",
        };
        for (const h of headers.filter((h) => h.enabled)) {
          headerObj[h.key] = h.value;
        }

        const startTime = Date.now();
        setSseMetrics({
          ttftMs: null,
          chunkCount: 0,
          totalBytes: 0,
          startTime,
          durationMs: 0,
        });

        const res = await fetch(url.trim(), {
          headers: headerObj,
          signal: controller.signal,
        });

        if (!res.ok) {
          throw new Error(`HTTP ${res.status} ${res.statusText}`);
        }

        setStatus("connected");
        pushToast({ title: "SSE 连接已建立，正在接收事件流...", kind: "success" });

        const reader = res.body?.getReader();
        if (!reader) {
          throw new Error("无法读取响应体流");
        }

        const decoder = new TextDecoder();
        let buffer = "";
        let chunks = 0;
        let bytes = 0;
        let firstChunk = true;

        while (true) {
          const { done, value } = await reader.read();
          if (done) break;

          const now = Date.now();
          if (firstChunk) {
            firstChunk = false;
            setSseMetrics((m) =>
              m ? { ...m, ttftMs: now - startTime } : null,
            );
          }

          chunks += 1;
          bytes += value?.length || 0;
          setSseMetrics((m) =>
            m
              ? {
                  ...m,
                  chunkCount: chunks,
                  totalBytes: bytes,
                  durationMs: now - startTime,
                }
              : null,
          );

          buffer += decoder.decode(value, { stream: true });
          const parts = buffer.split(/\r?\n\r?\n/);
          buffer = parts.pop() || "";

          for (const part of parts) {
            const parsed = parseSseBlock(part);
            if (parsed) {
              appendMessage("inbound", parsed.data, parsed.event);
            }
          }
        }

        setStatus("disconnected");
        pushToast({ title: "SSE 事件流已接收完毕", kind: "info" });
      } catch (err: unknown) {
        if ((err as Error)?.name === "AbortError") {
          setStatus("disconnected");
        } else {
          setStatus("error");
          pushToast({
            title: "SSE 连接中断: " + (err instanceof Error ? err.message : String(err)),
            kind: "error",
          });
        }
      }
    }
  };

  // 发送 WebSocket 消息
  const handleSendMessage = (content: string) => {
    if (wsRef.current && wsRef.current.readyState === WebSocket.OPEN) {
      wsRef.current.send(content);
      appendMessage("outbound", content);
    } else {
      pushToast({ title: "未连接到 WebSocket 服务", kind: "error" });
    }
  };

  // 组件卸载时释放连接
  useEffect(() => {
    return () => {
      handleDisconnect();
    };
  }, [handleDisconnect]);

  const isConnected = status === "connected";
  const isConnecting = status === "connecting";

  const handleTestInPlayground = (publicUrl: string, targetProtocol: "websocket" | "sse") => {
    setActiveTab("client");
    setProtocol(targetProtocol);
    setUrl(publicUrl);
    pushToast({
      title: `已带入公网 ${targetProtocol.toUpperCase()} 地址到演练台，可立即发起连接`,
      kind: "success",
    });
  };

  return (
    <div className="flex h-full flex-col bg-slate-50 text-slate-800 dark:bg-slate-950 dark:text-slate-100">
      {/* 顶部主工具栏 (Header) */}
      <div className="border-b border-slate-200/80 bg-white/70 px-6 py-4 backdrop-blur-md dark:border-slate-800/80 dark:bg-slate-900/70">
        <div className="flex flex-wrap items-center justify-between gap-4">
          <div className="flex items-center gap-3">
            <div className="flex h-10 w-10 items-center justify-center rounded-xl bg-sky-500/10 text-sky-600 shadow-xs dark:bg-sky-500/20 dark:text-sky-400">
              <Activity className="h-6 w-6" />
            </div>
            <div>
              <h1 className="text-lg font-semibold tracking-tight text-slate-900 dark:text-white">
                WebSocket & SSE
              </h1>
              <p className="text-xs text-slate-500 dark:text-slate-400">
                大模型流式事件监听 (SSE) · WebSocket 双向交互 · 心跳保活 · 本地服务公网穿透
              </p>
            </div>
          </div>

          {/* 顶层模式切换 Tabs */}
          <div className="flex rounded-xl bg-slate-100 p-1 dark:bg-slate-800">
            <button
              type="button"
              onClick={() => setActiveTab("client")}
              className={`flex items-center gap-1.5 rounded-lg px-3.5 py-1.5 text-xs font-medium transition-all ${
                activeTab === "client"
                  ? "bg-white text-sky-600 shadow-xs dark:bg-slate-700 dark:text-sky-400"
                  : "text-slate-600 hover:text-slate-900 dark:text-slate-300 dark:hover:text-white"
              }`}
            >
              <Activity className="h-3.5 w-3.5" />
              <span>实时通讯演练</span>
            </button>
            <button
              type="button"
              onClick={() => setActiveTab("tunnel")}
              className={`flex items-center gap-1.5 rounded-lg px-3.5 py-1.5 text-xs font-medium transition-all ${
                activeTab === "tunnel"
                  ? "bg-white text-sky-600 shadow-xs dark:bg-slate-700 dark:text-sky-400"
                  : "text-slate-600 hover:text-slate-900 dark:text-slate-300 dark:hover:text-white"
              }`}
            >
              <Globe className="h-3.5 w-3.5" />
              <span>本地服务公网映射</span>
            </button>
          </div>
        </div>

        {/* 仅在客户端演练模式下显示协议与主控条 */}
        {activeTab === "client" && (
          <div className="mt-4 flex flex-wrap items-center gap-2">
            {/* 协议切换 Tabs */}
            <div className="flex rounded-xl bg-slate-100 p-1 dark:bg-slate-800">
              <button
                type="button"
                onClick={() => handleProtocolChange("websocket")}
                className={`flex items-center gap-1.5 rounded-lg px-3 py-1 text-xs font-medium transition-all ${
                  protocol === "websocket"
                    ? "bg-white text-sky-600 shadow-xs dark:bg-slate-700 dark:text-sky-400"
                    : "text-slate-600 hover:text-slate-900 dark:text-slate-300 dark:hover:text-white"
                }`}
              >
                <Radio className="h-3.5 w-3.5" />
                <span>WebSocket</span>
              </button>
              <button
                type="button"
                onClick={() => handleProtocolChange("sse")}
                className={`flex items-center gap-1.5 rounded-lg px-3 py-1 text-xs font-medium transition-all ${
                  protocol === "sse"
                    ? "bg-white text-sky-600 shadow-xs dark:bg-slate-700 dark:text-sky-400"
                    : "text-slate-600 hover:text-slate-900 dark:text-slate-300 dark:hover:text-white"
                }`}
              >
                <Layers className="h-3.5 w-3.5" />
                <span>SSE</span>
              </button>
            </div>

            {/* 状态微光呼吸灯 */}
            <div className="flex items-center gap-2 rounded-xl border border-slate-200/80 bg-slate-50/60 px-3 py-2 text-xs font-medium text-slate-700 dark:border-slate-800 dark:bg-slate-800/50 dark:text-slate-300">
              <span
                className={`h-2.5 w-2.5 rounded-full transition-all ${
                  isConnected
                    ? "bg-emerald-500 shadow-xs shadow-emerald-500/50"
                    : isConnecting
                    ? "animate-pulse bg-amber-500 shadow-xs shadow-amber-500/50"
                    : status === "error"
                    ? "bg-rose-500"
                    : "bg-slate-300 dark:bg-slate-600"
                }`}
              />
              <span className="capitalize">
                {isConnected
                  ? "已连接"
                  : isConnecting
                  ? "连接中..."
                  : status === "error"
                  ? "连接异常"
                  : "未连接"}
              </span>
            </div>

            {/* URL 输入框 */}
            <div className="relative min-w-80 flex-1">
              <Link className="absolute left-3 top-2.5 h-4 w-4 text-slate-400" />
              <input
                type="text"
                value={url}
                onChange={(e) => setUrl(e.target.value)}
                disabled={isConnected || isConnecting}
                placeholder={protocol === "websocket" ? "wss://example.com/ws" : "https://example.com/events"}
                className="w-full rounded-xl border border-slate-200/80 bg-white py-2 pl-9 pr-3 font-mono text-xs text-slate-800 placeholder-slate-400 shadow-xs focus:border-sky-500 focus:outline-none disabled:bg-slate-50 disabled:text-slate-500 dark:border-slate-800 dark:bg-slate-900 dark:text-slate-100 dark:focus:border-sky-400 dark:disabled:bg-slate-800"
              />
            </div>

            {/* 高级配置抽屉按钮 */}
            <button
              type="button"
              onClick={() => setDrawerOpen(true)}
              className="flex items-center gap-1.5 rounded-xl border border-slate-200/80 bg-white px-3 py-2 text-xs font-medium text-slate-700 shadow-xs hover:bg-slate-50 dark:border-slate-800 dark:bg-slate-900 dark:text-slate-300 dark:hover:bg-slate-800"
            >
              <SlidersHorizontal className="h-3.5 w-3.5 text-slate-500" />
              <span>高级配置</span>
              {headers.filter((h) => h.enabled).length > 0 && (
                <span className="rounded-full bg-sky-500/10 px-1.5 py-0.2 text-[10px] font-semibold text-sky-600 dark:bg-sky-500/20 dark:text-sky-400">
                  {headers.filter((h) => h.enabled).length}
                </span>
              )}
            </button>

            {/* 连接 / 断开 主控按钮 */}
            {isConnected || isConnecting ? (
              <button
                type="button"
                onClick={handleDisconnect}
                className="flex items-center gap-1.5 rounded-xl bg-rose-600 px-4 py-2 text-xs font-semibold text-white shadow-xs transition-all hover:bg-rose-500"
              >
                <Power className="h-3.5 w-3.5" />
                <span>断开连接</span>
              </button>
            ) : (
              <button
                type="button"
                onClick={handleConnect}
                disabled={!url.trim()}
                className="flex items-center gap-1.5 rounded-xl bg-sky-600 px-4 py-2 text-xs font-semibold text-white shadow-xs transition-all hover:bg-sky-500 disabled:opacity-40"
              >
                <Power className="h-3.5 w-3.5" />
                <span>发起连接</span>
              </button>
            )}
          </div>
        )}
      </div>

      {/* 主工作区 */}
      {activeTab === "client" ? (
        <div className="flex flex-1 gap-5 overflow-hidden p-6">
          {/* 左侧：实时时间轴瀑布流 */}
          <div className="flex w-1/2 flex-col">
            <TimelineStream
              messages={messages}
              selectedMessageId={selectedMessage?.id || null}
              onSelectMessage={setSelectedMessage}
              onClear={() => {
                setMessages([]);
                setSelectedMessage(null);
              }}
              searchFilter={searchFilter}
              onSearchChange={setSearchFilter}
              autoScroll={autoScroll}
              onToggleAutoScroll={() => setAutoScroll((v) => !v)}
              sseMetrics={sseMetrics}
              isSse={protocol === "sse"}
            />
          </div>

          {/* 右侧：检查器 + WS 发送面板 */}
          <div className="flex w-1/2 flex-col gap-4">
            <div className="flex-1 overflow-hidden">
              <MessageInspector message={selectedMessage} />
            </div>

            {protocol === "websocket" ? (
              <MessageSender
                onSendMessage={handleSendMessage}
                disabled={!isConnected}
              />
            ) : null}
          </div>
        </div>
      ) : (
        <div className="flex-1 overflow-y-auto p-6">
          <PublicTunnelPanel onTestInPlayground={handleTestInPlayground} />
        </div>
      )}

      {/* 高级配置抽屉 */}
      <ConnectionDrawer
        isOpen={drawerOpen}
        onClose={() => setDrawerOpen(false)}
        protocol={protocol}
        headers={headers}
        onUpdateHeaders={setHeaders}
        subprotocols={subprotocols}
        onUpdateSubprotocols={setSubprotocols}
        heartbeat={heartbeat}
        onUpdateHeartbeat={setHeartbeat}
      />
    </div>
  );
}
