import { useEffect, useState } from "react";
import {
  Bot,
  ChevronRight,
  Loader2,
  Plus,
  Save,
  Search,
  Trash2,
  X,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { useConfirmDialog } from "../ConfirmDialogProvider";
import type {
  AiWorkspaceSettings,
  AssistantPreset,
} from "@/lib/aiWorkspace";
import {
  workspaceAssistantDelete,
  workspaceAssistantUpsert,
} from "@/lib/aiWorkspace";

interface AssistantLibraryDrawerProps {
  open: boolean;
  onClose: () => void;
  assistants: AssistantPreset[];
  onAssistantsChange: (assistants: AssistantPreset[]) => void;
  settings: AiWorkspaceSettings | null;
  currentAssistantId: string | null;
  onAssistantSelect: (id: string) => void;
}

function createAssistantDraft(
  settings: AiWorkspaceSettings | null,
  t: (key: string, defaultValue: string) => string,
): AssistantPreset {
  const now = Math.floor(Date.now() / 1000);
  return {
    id: "",
    name: t("aiWorkspaceNewAssistantName", "New Assistant"),
    avatar_emoji: "🤖",
    description: "",
    system_prompt: "",
    primary_model_id:
      settings?.role_bindings.find((binding) => binding.role === "assistant")
        ?.model_id || null,
    light_model_id:
      settings?.role_bindings.find((binding) => binding.role === "summary")
        ?.model_id || null,
    default_model_profile_id: null,
    light_model_profile_id: null,
    tool_policy: {
      web_search: true,
      workspace_read: false,
      notes_search: false,
    },
    knowledge_base_ids: [],
    mcp_server_ids: [],
    memory_enabled: false,
    output_contract: "",
    created_at: now,
    updated_at: now,
  };
}

export function AssistantLibraryDrawer({
  open,
  onClose,
  assistants,
  onAssistantsChange,
  settings,
  currentAssistantId,
  onAssistantSelect,
}: AssistantLibraryDrawerProps) {
  const { t } = useTranslation();
  const confirmDialog = useConfirmDialog();
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [draft, setDraft] = useState<AssistantPreset | null>(null);
  const [saving, setSaving] = useState(false);
  const [deleting, setDeleting] = useState(false);

  // 选择第一个助手或当前助手
  useEffect(() => {
    if (open && !selectedId) {
      setSelectedId(currentAssistantId || assistants[0]?.id || null);
      if (assistants[0]) {
        setDraft(assistants[0]);
      }
    }
  }, [open, selectedId, currentAssistantId, assistants]);

  // 当选择改变时更新draft
  useEffect(() => {
    if (selectedId) {
      const assistant = assistants.find((a) => a.id === selectedId);
      if (assistant) {
        setDraft(assistant);
      }
    }
  }, [selectedId, assistants]);

  const filteredAssistants = assistants.filter(
    (a) =>
      a.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
      (a.description && a.description.toLowerCase().includes(searchQuery.toLowerCase())),
  );

  const handleCreateNew = () => {
    const newDraft = createAssistantDraft(settings, t);
    onAssistantsChange([newDraft, ...assistants]);
    setSelectedId(newDraft.id);
    setDraft(newDraft);
  };

  const handleSave = async () => {
    if (!draft) return;
    setSaving(true);
    try {
      const saved = await workspaceAssistantUpsert(draft);
      const updatedList = assistants.map((a) =>
        a.id === draft.id ? saved : a,
      );
      if (!draft.id) {
        // 新创建的助手
        onAssistantsChange([saved, ...assistants.filter((a) => !a.id)]);
        setSelectedId(saved.id);
      } else {
        onAssistantsChange(updatedList);
      }
      setDraft(saved);
    } catch (error) {
      console.error("Failed to save assistant:", error);
    } finally {
      setSaving(false);
    }
  };

  const handleDelete = async () => {
    if (!draft?.id) return;
    const confirmed = await confirmDialog(
      t(
        "deleteAssistantMessage",
        "Are you sure you want to delete this assistant? This action cannot be undone.",
      ),
      { title: t("deleteAssistantTitle", "Delete Assistant") },
    );
    if (!confirmed) return;

    setDeleting(true);
    try {
      await workspaceAssistantDelete(draft.id);
      const updatedList = assistants.filter((a) => a.id !== draft.id);
      onAssistantsChange(updatedList);
      setSelectedId(updatedList[0]?.id || null);
      if (updatedList[0]) {
        setDraft(updatedList[0]);
      } else {
        setDraft(null);
      }
    } catch (error) {
      console.error("Failed to delete assistant:", error);
    } finally {
      setDeleting(false);
    }
  };

  const handleSelect = (id: string) => {
    setSelectedId(id);
    const assistant = assistants.find((a) => a.id === id);
    if (assistant) {
      setDraft(assistant);
    }
  };

  const handleUseAssistant = () => {
    if (selectedId) {
      onAssistantSelect(selectedId);
      onClose();
    }
  };

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50">
      {/* 背景遮罩 */}
      <div
        className="absolute inset-0 bg-black/30"
        onClick={onClose}
      />

      {/* 抽屉面板 */}
      <div className="absolute right-0 top-0 h-full w-[400px] bg-card shadow-xl">
        {/* 标题栏 */}
        <div className="flex items-center justify-between border-b px-4 py-3">
          <div className="flex items-center gap-2">
            <Bot className="h-4 w-4" />
            <span className="text-sm font-semibold">
              {t("assistantLibrary", "Assistant Library")}
            </span>
          </div>
          <button
            type="button"
            onClick={onClose}
            className="rounded-lg p-1 hover:bg-muted"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        {/* 搜索和新建 */}
        <div className="border-b px-4 py-3">
          <div className="flex items-center gap-2">
            <div className="relative flex-1">
              <Search className="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
              <input
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                placeholder={t("searchAssistants", "Search...")}
                className="w-full rounded-xl border bg-background py-2 pl-10 pr-3 text-sm outline-none"
              />
            </div>
            <button
              type="button"
              onClick={handleCreateNew}
              className="inline-flex h-9 items-center gap-1 rounded-xl border px-3 text-sm hover:bg-muted"
            >
              <Plus className="h-4 w-4" />
              {t("newLabel", "New")}
            </button>
          </div>
        </div>

        {/* 助手列表 */}
        <div className="flex min-h-0 flex-1" style={{ height: "calc(100% - 120px)" }}>
          {/* 左侧列表 */}
          <div className="w-[140px] border-r overflow-y-auto p-2">
            {filteredAssistants.map((assistant) => (
              <button
                key={assistant.id || assistant.name}
                type="button"
                onClick={() => handleSelect(assistant.id)}
                className={`w-full rounded-lg px-2 py-2 text-left text-sm transition-colors ${
                  selectedId === assistant.id
                    ? "bg-primary/5 text-primary"
                    : "hover:bg-muted"
                }`}
              >
                <div className="flex items-center gap-2">
                  <span className="text-xs">
                    {assistant.avatar_emoji || "🤖"}
                  </span>
                  <span className="truncate font-medium">
                    {assistant.name}
                  </span>
                </div>
              </button>
            ))}
          </div>

          {/* 右侧编辑器 */}
          <div className="flex-1 overflow-y-auto p-4">
            {draft ? (
              <div className="space-y-4">
                {/* 基本信息 */}
                <div>
                  <label className="text-xs font-medium text-muted-foreground">
                    {t("nameLabel", "Name")}
                  </label>
                  <input
                    value={draft.name}
                    onChange={(e) =>
                      setDraft({ ...draft, name: e.target.value })
                    }
                    className="mt-1 w-full rounded-xl border bg-background px-3 py-2 text-sm outline-none"
                  />
                </div>

                <div>
                  <label className="text-xs font-medium text-muted-foreground">
                    {t("avatarLabel", "Avatar")}
                  </label>
                  <input
                    value={draft.avatar_emoji || ""}
                    onChange={(e) =>
                      setDraft({ ...draft, avatar_emoji: e.target.value })
                    }
                    placeholder="🤖"
                    className="mt-1 w-full rounded-xl border bg-background px-3 py-2 text-sm outline-none"
                  />
                </div>

                <div>
                  <label className="text-xs font-medium text-muted-foreground">
                    {t("descriptionLabel", "Description")}
                  </label>
                  <textarea
                    value={draft.description}
                    onChange={(e) =>
                      setDraft({ ...draft, description: e.target.value })
                    }
                    className="mt-1 w-full rounded-xl border bg-background px-3 py-2 text-sm outline-none"
                    rows={2}
                  />
                </div>

                {/* System Prompt */}
                <div>
                  <label className="text-xs font-medium text-muted-foreground">
                    {t("systemPromptLabel", "System Prompt")}
                  </label>
                  <textarea
                    value={draft.system_prompt}
                    onChange={(e) =>
                      setDraft({ ...draft, system_prompt: e.target.value })
                    }
                    className="mt-1 w-full rounded-xl border bg-background px-3 py-2 text-sm outline-none"
                    rows={4}
                  />
                </div>

                {/* Tool Policy */}
                <div>
                  <label className="text-xs font-medium text-muted-foreground">
                    {t("toolPolicyLabel", "Tool Policy")}
                  </label>
                  <div className="mt-2 flex flex-wrap gap-2">
                    <button
                      type="button"
                      onClick={() =>
                        setDraft({
                          ...draft,
                          tool_policy: {
                            ...draft.tool_policy,
                            web_search: !draft.tool_policy.web_search,
                          },
                        })
                      }
                      className={`rounded-full border px-2 py-0.5 text-[11px] font-medium ${
                        draft.tool_policy.web_search
                          ? "border-primary bg-primary/5 text-primary"
                          : "text-muted-foreground"
                      }`}
                    >
                      WEB
                    </button>
                    <button
                      type="button"
                      onClick={() =>
                        setDraft({
                          ...draft,
                          tool_policy: {
                            ...draft.tool_policy,
                            workspace_read: !draft.tool_policy.workspace_read,
                          },
                        })
                      }
                      className={`rounded-full border px-2 py-0.5 text-[11px] font-medium ${
                        draft.tool_policy.workspace_read
                          ? "border-primary bg-primary/5 text-primary"
                          : "text-muted-foreground"
                      }`}
                    >
                      WS
                    </button>
                    <button
                      type="button"
                      onClick={() =>
                        setDraft({
                          ...draft,
                          tool_policy: {
                            ...draft.tool_policy,
                            notes_search: !draft.tool_policy.notes_search,
                          },
                        })
                      }
                      className={`rounded-full border px-2 py-0.5 text-[11px] font-medium ${
                        draft.tool_policy.notes_search
                          ? "border-primary bg-primary/5 text-primary"
                          : "text-muted-foreground"
                      }`}
                    >
                      NOTE
                    </button>
                  </div>
                </div>

                {/* 操作按钮 */}
                <div className="flex items-center gap-2 pt-4">
                  <button
                    type="button"
                    onClick={handleSave}
                    disabled={saving}
                    className="inline-flex items-center gap-2 rounded-xl bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
                  >
                    {saving ? (
                      <Loader2 className="h-4 w-4 animate-spin" />
                    ) : (
                      <Save className="h-4 w-4" />
                    )}
                    {t("save", "Save")}
                  </button>
                  <button
                    type="button"
                    onClick={handleUseAssistant}
                    className="inline-flex items-center gap-2 rounded-xl border px-4 py-2 text-sm hover:bg-muted"
                  >
                    <ChevronRight className="h-4 w-4" />
                    {t("useThisAssistant", "Use")}
                  </button>
                  {draft.id ? (
                    <button
                      type="button"
                      onClick={handleDelete}
                      disabled={deleting}
                      className="inline-flex items-center gap-2 rounded-xl border border-destructive/30 px-4 py-2 text-sm text-destructive hover:bg-destructive/5 disabled:opacity-50"
                    >
                      {deleting ? (
                        <Loader2 className="h-4 w-4 animate-spin" />
                      ) : (
                        <Trash2 className="h-4 w-4" />
                      )}
                      {t("delete", "Delete")}
                    </button>
                  ) : null}
                </div>
              </div>
            ) : (
              <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
                {t("noAssistantSelected", "No assistant selected")}
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}