/**
 * Core types and streaming utilities for WebSocket & SSE Playground.
 */

export type RealtimeProtocol = "websocket" | "sse";
export type ConnectionStatus = "disconnected" | "connecting" | "connected" | "error";

export interface RealtimeMessage {
  id: string;
  direction: "inbound" | "outbound";
  timestamp: number;
  timeStr: string;
  content: string;
  size: number;
  event?: string;
  deltaMs?: number;
}

export interface HeaderEntry {
  id: string;
  key: string;
  value: string;
  enabled: boolean;
}

export interface HeartbeatConfig {
  enabled: boolean;
  intervalSeconds: number;
  payload: string;
}

export interface SseSessionMetrics {
  ttftMs: number | null;
  chunkCount: number;
  totalBytes: number;
  startTime: number;
  durationMs: number;
}

export function formatTime(ts: number): string {
  const d = new Date(ts);
  const h = String(d.getHours()).padStart(2, "0");
  const m = String(d.getMinutes()).padStart(2, "0");
  const s = String(d.getSeconds()).padStart(2, "0");
  const ms = String(d.getMilliseconds()).padStart(3, "0");
  return `${h}:${m}:${s}.${ms}`;
}

/**
 * Parses raw SSE chunk text into structured events.
 * Standard SSE chunks end with double newline \n\n or \r\n\r\n.
 */
export function parseSseBlock(block: string): { event?: string; data: string; id?: string } | null {
  if (!block || !block.trim()) return null;

  const lines = block.split(/\r?\n/);
  let event: string | undefined = undefined;
  let id: string | undefined = undefined;
  const dataLines: string[] = [];

  for (const line of lines) {
    if (line.startsWith(":")) {
      // Comment line, ignore
      continue;
    }
    if (line.startsWith("event:")) {
      event = line.slice(6).trim();
    } else if (line.startsWith("data:")) {
      dataLines.push(line.slice(5).trimStart());
    } else if (line.startsWith("id:")) {
      id = line.slice(3).trim();
    }
  }

  if (dataLines.length === 0 && !event && !id) {
    return null;
  }

  return {
    event,
    data: dataLines.join("\n"),
    id,
  };
}

/**
 * Lightweight helper to detect if a text payload is valid JSON.
 */
export function tryParseJson(text: string): { isJson: boolean; formatted: string } {
  try {
    const parsed = JSON.parse(text);
    return {
      isJson: true,
      formatted: JSON.stringify(parsed, null, 2),
    };
  } catch {
    return {
      isJson: false,
      formatted: text,
    };
  }
}
