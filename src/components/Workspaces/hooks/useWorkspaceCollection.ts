import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { collectWorkspaceTags, filterWorkspacesByTags, normalizeWorkspaceView } from "../helpers/workspaceHelpers";
import type { ApiResp, WorkspaceView } from "../types";

export function useWorkspaceCollection(args: {
  isVisible: boolean;
  isTauri: boolean;
  activeWorkspaceId: string | null;
  onActiveWorkspaceRemoved: () => void;
  onRefreshActiveWorkspace: () => Promise<void>;
  setMessage: (message: { type: "success" | "error"; text: string } | null) => void;
}) {
  const { isVisible, isTauri, activeWorkspaceId, onActiveWorkspaceRemoved, onRefreshActiveWorkspace, setMessage } = args;
  const { t } = useTranslation();
  const [loading, setLoading] = useState(false);
  const [workspacesInitialized, setWorkspacesInitialized] = useState(false);
  const [workspaces, setWorkspaces] = useState<WorkspaceView[]>([]);
  const [allTags, setAllTags] = useState<string[]>([]);
  const [selectedTags, setSelectedTags] = useState<string[]>([]);

  const loadWorkspaces = useCallback(async () => {
    if (!isTauri) return;
    try {
      setLoading(true);
      const resp = await invoke<ApiResp<WorkspaceView[]>>("workspaces_list");
      const allWorkspaces = Array.isArray(resp.data) ? resp.data.map((item) => normalizeWorkspaceView(item)) : [];
      setWorkspaces(allWorkspaces);
      setAllTags(collectWorkspaceTags(allWorkspaces));
      if (activeWorkspaceId && !allWorkspaces.some((item) => item.workspace.id === activeWorkspaceId)) {
        onActiveWorkspaceRemoved();
      }
    } catch (e: any) {
      setMessage({
        type: "error",
        text: t("workspaceLoadFailed", "Failed to load workspaces: {{message}}", { message: String(e) }),
      });
    } finally {
      setWorkspacesInitialized(true);
      setLoading(false);
    }
  }, [activeWorkspaceId, isTauri, onActiveWorkspaceRemoved, t]);

  useEffect(() => {
    if (!isVisible) return;
    void loadWorkspaces();
  }, [isVisible, loadWorkspaces]);

  useEffect(() => {
    if (!isVisible) return;
    let unlistenRefresh: (() => void) | undefined;
    let unlistenSessions: (() => void) | undefined;
    let unlistenWorkspaces: (() => void) | undefined;

    const register = async () => {
      unlistenRefresh = await listen("refresh-counts", () => {
        void loadWorkspaces();
        void onRefreshActiveWorkspace();
      });
      unlistenSessions = await listen("sessions-updated", () => {
        void loadWorkspaces();
        void onRefreshActiveWorkspace();
      });
      unlistenWorkspaces = await listen("workspaces-updated", () => {
        void loadWorkspaces();
        void onRefreshActiveWorkspace();
      });
    };

    void register();

    return () => {
      unlistenRefresh?.();
      unlistenSessions?.();
      unlistenWorkspaces?.();
    };
  }, [isVisible, loadWorkspaces, onRefreshActiveWorkspace]);

  const visibleWorkspaces = useMemo(
    () => filterWorkspacesByTags(workspaces, selectedTags),
    [selectedTags, workspaces],
  );
  const selectedWorkspaceTags = useMemo(() => new Set(selectedTags), [selectedTags]);

  const toggleTagFilter = useCallback((tag: string) => {
    setSelectedTags((prev) => (prev.includes(tag) ? prev.filter((item) => item !== tag) : [...prev, tag]));
  }, []);

  return {
    loading,
    workspacesInitialized,
    workspaces,
    allTags,
    selectedTags,
    setSelectedTags,
    visibleWorkspaces,
    selectedWorkspaceTags,
    toggleTagFilter,
    loadWorkspaces,
  };
}
