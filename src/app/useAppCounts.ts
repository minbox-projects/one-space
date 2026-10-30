import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getUnreadEmailCount } from "@/lib/gmail";
import { getUnreadMessageCount } from "@/lib/messages";
import { sessionsUsageDayStats } from "@/lib/aiUsage";
import {
  cloudAgentListSessions,
  CLOUD_AGENT_SESSIONS_COUNT_EVENT,
} from "@/lib/cloudAgent";
import { localDateString } from "./helpers";
import type { ApiResp, AppCounts, DashboardCounts } from "./types";

type UseAppCountsParams = {
  setStorageType: (storageType: "local" | "git" | "icloud") => void;
  setMessageUnreadCount: (count: number) => void;
};

export function useAppCounts({
  setStorageType,
  setMessageUnreadCount,
}: UseAppCountsParams) {
  const [counts, setCounts] = useState<AppCounts>({
    launcher: 0,
    workspaces: 0,
    sessions: 0,
    ssh: 0,
    snippets: 0,
    bookmarks: 0,
    notes: 0,
    aiNews: 0,
    mail: 0,
    environments: 0,
    skills: 0,
    subagents: 0,
    mcpServers: 0,
    cloudAgent: 0,
  });
  const [usageTodayTokens, setUsageTodayTokens] = useState(0);
  const loadCountsInFlightRef = useRef<Promise<void> | null>(null);
  const countsRefreshTimerRef = useRef<number | null>(null);

  useEffect(() => {
    const handleCountUpdate = (e: Event) => {
      const customEvent = e as CustomEvent<{ count: number }>;
      if (typeof customEvent.detail?.count === "number") {
        setCounts((prev) => ({ ...prev, cloudAgent: customEvent.detail.count }));
      }
    };
    window.addEventListener(
      CLOUD_AGENT_SESSIONS_COUNT_EVENT,
      handleCountUpdate,
    );
    return () => {
      window.removeEventListener(
        CLOUD_AGENT_SESSIONS_COUNT_EVENT,
        handleCountUpdate,
      );
    };
  }, []);

  const isTauri = "__TAURI_INTERNALS__" in window;

  const loadCounts = async () => {
    if (!isTauri) return;

    if (loadCountsInFlightRef.current) {
      await loadCountsInFlightRef.current;
      return;
    }

    const task = (async () => {
      try {
        const res = await invoke<ApiResp<DashboardCounts>>("dashboard_counts");
        const data = res.data;
        setCounts((prev) => ({
          ...prev,
          launcher: data.launcher || 0,
          workspaces: data.workspaces || 0,
          sessions: data.sessions || 0,
          ssh: data.ssh || 0,
          snippets: data.snippets || 0,
          bookmarks: data.bookmarks || 0,
          notes: data.notes || 0,
          aiNews: data.ai_news || 0,
          environments: data.environments || 0,
          skills: data.skills || 0,
          subagents: data.subagents || 0,
          mcpServers: data.mcp_servers || 0,
        }));
        if (data.storage_type) {
          setStorageType(data.storage_type);
        }
      } catch (e) {
        console.error("Failed to load local counts", e);
      }

      getUnreadEmailCount()
        .then((mailCount) => {
          setCounts((prev) => ({ ...prev, mail: mailCount }));
        })
        .catch(() => {});

      cloudAgentListSessions()
        .then((cloudSessions) => {
          if (Array.isArray(cloudSessions)) {
            const activeCount = cloudSessions.filter(
              (s) => s.is_active !== false,
            ).length;
            setCounts((prev) => ({ ...prev, cloudAgent: activeCount }));
          }
        })
        .catch(() => {});
    })();

    loadCountsInFlightRef.current = task.finally(() => {
      loadCountsInFlightRef.current = null;
    });
    await loadCountsInFlightRef.current;
  };

  const scheduleLoadCounts = (delayMs = 180) => {
    if (!isTauri) return;
    if (countsRefreshTimerRef.current !== null) {
      clearTimeout(countsRefreshTimerRef.current);
    }
    countsRefreshTimerRef.current = setTimeout(() => {
      countsRefreshTimerRef.current = null;
      void loadCounts();
    }, delayMs) as unknown as number;
  };

  const refreshMessageUnreadCount = async () => {
    if (!isTauri) return;
    try {
      const count = await getUnreadMessageCount();
      setMessageUnreadCount(count);
    } catch (e) {
      console.error("Failed to load message unread count", e);
    }
  };

  const loadUsageTodayTokens = async () => {
    if (!isTauri) return;
    try {
      const stats = await sessionsUsageDayStats(localDateString());
      setUsageTodayTokens(stats?.total_tokens ?? 0);
    } catch (e) {
      console.error("Failed to load today's AI usage tokens", e);
    }
  };

  return {
    counts,
    setCounts,
    usageTodayTokens,
    isTauri,
    countsRefreshTimerRef,
    loadCounts,
    scheduleLoadCounts,
    refreshMessageUnreadCount,
    loadUsageTodayTokens,
  };
}
