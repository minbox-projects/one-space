import { useMemo, useState } from "react";
import type { HeartbeatBarPoint } from "./types";

interface HeartbeatBarProps {
  heartbeats: HeartbeatBarPoint[];
  maxBars?: number;
}

export function HeartbeatBar({ heartbeats, maxBars = 60 }: HeartbeatBarProps) {
  const [hoveredPoint, setHoveredPoint] = useState<{
    point: HeartbeatBarPoint | null;
    x: number;
    y: number;
  } | null>(null);

  // 补齐或截断至 maxBars 根柱子
  const displayBars = useMemo(() => {
    const total = heartbeats.length;
    if (total >= maxBars) {
      return heartbeats.slice(total - maxBars);
    }
    const emptyCount = maxBars - total;
    const padding: Array<HeartbeatBarPoint | null> = Array(emptyCount).fill(null);
    return [...padding, ...heartbeats];
  }, [heartbeats, maxBars]);

  const formatTimestamp = (ts: number) => {
    const date = new Date(ts * 1000);
    return date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
  };

  return (
    <div className="relative w-full">
      <div className="flex items-center gap-[2px] h-7 w-full py-1">
        {displayBars.map((item, idx) => {
          if (!item) {
            return (
              <div
                key={`empty-${idx}`}
                className="flex-1 h-full min-w-[2px] rounded-[2px] bg-muted/40 transition-colors"
                title="No data"
              />
            );
          }

          let barColor = "bg-muted/40";
          if (item.status === "up") {
            barColor = "bg-emerald-500 hover:bg-emerald-400";
          } else if (item.status === "degraded") {
            barColor = "bg-amber-500 hover:bg-amber-400";
          } else if (item.status === "down") {
            barColor = "bg-rose-500 hover:bg-rose-400";
          } else if (item.status === "paused") {
            barColor = "bg-neutral-600/40";
          }

          return (
            <div
              key={`${item.timestamp}-${idx}`}
              className={`flex-1 h-full min-w-[2px] rounded-[2px] cursor-pointer transition-all ${barColor}`}
              onMouseEnter={(e) => {
                const rect = e.currentTarget.getBoundingClientRect();
                setHoveredPoint({
                  point: item,
                  x: rect.left + rect.width / 2,
                  y: rect.top - 8,
                });
              }}
              onMouseLeave={() => setHoveredPoint(null)}
            />
          );
        })}
      </div>

      {hoveredPoint && hoveredPoint.point && (
        <div
          className="fixed z-50 -translate-x-1/2 -translate-y-full px-2.5 py-1.5 rounded-md bg-popover text-popover-foreground text-xs shadow-md border border-border pointer-events-none whitespace-nowrap animate-in fade-in-0 zoom-in-95 duration-100"
          style={{ left: hoveredPoint.x, top: hoveredPoint.y }}
        >
          <div className="flex items-center gap-1.5 font-medium">
            <span
              className={`inline-block w-2 h-2 rounded-full ${
                hoveredPoint.point.status === "up"
                  ? "bg-emerald-500"
                  : hoveredPoint.point.status === "down"
                    ? "bg-rose-500"
                    : "bg-amber-500"
              }`}
            />
            <span>{formatTimestamp(hoveredPoint.point.timestamp)}</span>
            <span className="font-mono text-[11px] text-muted-foreground">
              {hoveredPoint.point.responseTimeMs}ms
            </span>
          </div>
          {hoveredPoint.point.statusCode && (
            <div className="text-[11px] text-muted-foreground mt-0.5">
              HTTP {hoveredPoint.point.statusCode}
            </div>
          )}
          {hoveredPoint.point.errorMessage && (
            <div className="text-[11px] text-rose-500 mt-0.5 max-w-[220px] truncate">
              {hoveredPoint.point.errorMessage}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
