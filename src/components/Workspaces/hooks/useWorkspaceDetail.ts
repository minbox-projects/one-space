import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  createOptimisticWorkspaceDetail,
  normalizeWorkspaceDetail,
} from "../helpers/workspaceHelpers";
import {
  DEFAULT_WORKSPACE_SESSIONS_QUERY,
  TAB_LOADING_MIN_MS,
  type ApiResp,
  type WorkspaceDetail,
  type WorkspaceSessionsListData,
  type WorkspaceView,
} from "../types";
import type { AiSessionListItem, AiSessionsQueryState } from "../../AiSessionsList";

export function useWorkspaceDetail(args: {
  isVisible: boolean;
  isTauri: boolean;
  activeTab: "sessions" | "mcp" | "skills" | "subagents";
  setMessage: (message: { type: "success" | "error"; text: string } | null) => void;
}) {
  const { isVisible, isTauri, activeTab, setMessage } = args;
  const { t } = useTranslation();
  const [activeWorkspaceId, setActiveWorkspaceId] = useState<string | null>(null);
  const [activeDetail, setActiveDetail] = useState<WorkspaceDetail | null>(null);
  const [activeSessions, setActiveSessions] = useState<AiSessionListItem[]>([]);
  const [sessionsLoading, setSessionsLoading] = useState(false);
  const [sessionsInitialized, setSessionsInitialized] = useState(false);
  const [sessionsTotal, setSessionsTotal] = useState(0);
  const [sessionToolOptions, setSessionToolOptions] = useState<string[]>([]);
  const [sessionModelOptions, setSessionModelOptions] = useState<string[]>([]);
  const [sessionQuery, setSessionQuery] = useState<AiSessionsQueryState>(DEFAULT_WORKSPACE_SESSIONS_QUERY);
  const [debouncedSessionNameFilter, setDebouncedSessionNameFilter] = useState("");
  const [detailLoading, setDetailLoading] = useState(false);
  const sessionsRequestSeqRef = useRef(0);
  const detailRequestSeqRef = useRef(0);
  const isMountedRef = useRef(true);

  useEffect(() => {
    return () => {
      isMountedRef.current = false;
    };
  }, []);

  const requestedSessionQuery = useMemo<AiSessionsQueryState>(
    () => ({
      ...sessionQuery,
      nameFilter: debouncedSessionNameFilter,
    }),
    [debouncedSessionNameFilter, sessionQuery],
  );

  const ensureMinimumLoadingDuration = useCallback(async (startedAt: number) => {
    const elapsed = Date.now() - startedAt;
    if (elapsed >= TAB_LOADING_MIN_MS) return;
    await new Promise((resolve) => window.setTimeout(resolve, TAB_LOADING_MIN_MS - elapsed));
  }, []);

  const clearActiveWorkspace = useCallback(() => {
    setActiveWorkspaceId(null);
    setActiveDetail(null);
    setActiveSessions([]);
    setSessionsTotal(0);
    setSessionToolOptions([]);
    setSessionModelOptions([]);
    setSessionsInitialized(false);
  }, []);

  const loadWorkspaceDetail = useCallback(
    async (workspaceId: string, optimisticView?: WorkspaceView) => {
      if (!isTauri || !isMountedRef.current) return;
      const requestId = detailRequestSeqRef.current + 1;
      detailRequestSeqRef.current = requestId;
      try {
        setDetailLoading(true);
        const switchingWorkspace = workspaceId !== activeWorkspaceId;
        setActiveWorkspaceId(workspaceId);
        if (optimisticView) {
          setActiveDetail((prev) => createOptimisticWorkspaceDetail(optimisticView, prev));
        } else if (switchingWorkspace) {
          setActiveDetail(null);
        }
        if (switchingWorkspace) {
          sessionsRequestSeqRef.current += 1;
          setActiveSessions([]);
          setSessionsTotal(0);
          setSessionToolOptions([]);
          setSessionModelOptions([]);
          setSessionsInitialized(false);
          setSessionsLoading(false);
          setSessionQuery(DEFAULT_WORKSPACE_SESSIONS_QUERY);
          setDebouncedSessionNameFilter("");
        }
        const detailResp = await invoke<ApiResp<WorkspaceDetail>>("workspace_get", { workspaceId });
        if (requestId !== detailRequestSeqRef.current || !isMountedRef.current) {
          return;
        }
        setActiveWorkspaceId(workspaceId);
        setActiveDetail(normalizeWorkspaceDetail(detailResp.data));
      } catch (e: any) {
        if (requestId === detailRequestSeqRef.current && isMountedRef.current) {
          setMessage({
            type: "error",
            text: t("workspaceDetailLoadFailed", "Failed to load workspace detail: {{message}}", {
              message: String(e),
            }),
          });
        }
      } finally {
        if (requestId === detailRequestSeqRef.current && isMountedRef.current) {
          setDetailLoading(false);
        }
      }
    },
    [activeWorkspaceId, isTauri, setMessage, t],
  );

  const loadWorkspaceSessions = useCallback(
    async (workspaceId: string, query: AiSessionsQueryState, { silent = false }: { silent?: boolean } = {}) => {
      if (!isTauri || !isMountedRef.current) return;
      const requestId = sessionsRequestSeqRef.current + 1;
      sessionsRequestSeqRef.current = requestId;
      const startedAt = Date.now();
      try {
        if (!silent) {
          setSessionsLoading(true);
        }
        const resp = await invoke<ApiResp<WorkspaceSessionsListData>>("workspace_sessions_list", {
          workspaceId,
          tool: query.toolFilter === "all" ? null : query.toolFilter,
          modelName: query.modelFilter === "all" ? null : query.modelFilter,
          query: query.nameFilter.trim() ? query.nameFilter.trim() : null,
        });
        if (requestId !== sessionsRequestSeqRef.current || !isMountedRef.current) return;
        const nextData = resp.data;
        setActiveSessions(Array.isArray(nextData?.items) ? nextData.items : []);
        setSessionsTotal(Number(nextData?.total) || 0);
        setSessionToolOptions(Array.isArray(nextData?.tool_options) ? nextData.tool_options : []);
        setSessionModelOptions(Array.isArray(nextData?.model_options) ? nextData.model_options : []);
        setSessionsInitialized(true);
      } catch (e: any) {
        if (requestId !== sessionsRequestSeqRef.current || !isMountedRef.current) return;
        setMessage({
          type: "error",
          text: t("workspaceSessionsLoadFailed", "Failed to load workspace sessions: {{message}}", {
            message: String(e),
          }),
        });
        setSessionsInitialized(true);
      } finally {
        if (requestId === sessionsRequestSeqRef.current && isMountedRef.current) {
          if (!silent) {
            await ensureMinimumLoadingDuration(startedAt);
          }
          if (requestId === sessionsRequestSeqRef.current && isMountedRef.current) {
            setSessionsLoading(false);
          }
        }
      }
    },
    [ensureMinimumLoadingDuration, isTauri, setMessage, t],
  );

  const refreshActiveWorkspace = useCallback(async () => {
    if (!activeWorkspaceId) return;
    await loadWorkspaceDetail(activeWorkspaceId);
    if (activeTab === "sessions") {
      await loadWorkspaceSessions(activeWorkspaceId, requestedSessionQuery, { silent: true });
    }
  }, [activeTab, activeWorkspaceId, loadWorkspaceDetail, loadWorkspaceSessions, requestedSessionQuery]);

  useEffect(() => {
    const timer = window.setTimeout(() => {
      setDebouncedSessionNameFilter(sessionQuery.nameFilter);
    }, 250);
    return () => window.clearTimeout(timer);
  }, [sessionQuery.nameFilter]);

  useEffect(() => {
    if (sessionQuery.modelFilter === "all") return;
    if (sessionModelOptions.includes(sessionQuery.modelFilter)) return;
    setSessionQuery((prev) => ({ ...prev, modelFilter: "all" }));
  }, [sessionModelOptions, sessionQuery.modelFilter]);

  useEffect(() => {
    if (!isVisible || activeTab !== "sessions" || !activeWorkspaceId) return;
    void loadWorkspaceSessions(activeWorkspaceId, requestedSessionQuery);
  }, [activeTab, activeWorkspaceId, isVisible, loadWorkspaceSessions, requestedSessionQuery]);

  return {
    activeWorkspaceId,
    setActiveWorkspaceId,
    activeDetail,
    setActiveDetail,
    activeSessions,
    setActiveSessions,
    sessionsLoading,
    setSessionsLoading,
    sessionsInitialized,
    sessionsTotal,
    sessionToolOptions,
    sessionModelOptions,
    sessionQuery,
    setSessionQuery,
    requestedSessionQuery,
    detailLoading,
    loadWorkspaceDetail,
    loadWorkspaceSessions,
    refreshActiveWorkspace,
    clearActiveWorkspace,
  };
}
