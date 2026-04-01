import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Bot, Play, Plus, Save, Search, Trash2 } from "lucide-react";
import { useConfirmDialog } from "./ConfirmDialogProvider";
import {
  assistantAgentDelete,
  assistantAgentTestRun,
  assistantAgentsList,
  assistantAgentUpsert,
  assistantSettingsGet,
  type AgentDefinition,
  type AiAssistantSettings,
} from "@/lib/aiAssistant";

const PENDING_CONVERSATION_KEY = "onespace:pending-assistant-conversation";

function createAgent(): AgentDefinition {
  const now = Math.floor(Date.now() / 1000);
  return {
    id: "",
    name: "New Agent",
    description: "",
    system_prompt: "",
    default_model_profile_id: null,
    light_model_profile_id: null,
    tool_policy: {
      web_search: true,
      workspace_read: false,
      notes_search: false,
    },
    output_contract: "",
    created_at: now,
    updated_at: now,
  };
}

export function Agents({ isVisible = false }: { isVisible?: boolean }) {
  const { t } = useTranslation();
  const confirmDialog = useConfirmDialog();
  const [settings, setSettings] = useState<AiAssistantSettings | null>(null);
  const [agents, setAgents] = useState<AgentDefinition[]>([]);
  const [selectedAgentId, setSelectedAgentId] = useState<string | null>(null);
  const [draft, setDraft] = useState<AgentDefinition | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [loading, setLoading] = useState(false);
  const [testPrompt, setTestPrompt] = useState(
    "Summarize the current release risks and key action items.",
  );
  const [message, setMessage] = useState<string | null>(null);

  const loadData = async () => {
    setLoading(true);
    try {
      const [loadedSettings, loadedAgents] = await Promise.all([
        assistantSettingsGet(),
        assistantAgentsList(),
      ]);
      setSettings(loadedSettings);
      setAgents(loadedAgents);
      setSelectedAgentId((current) => current || loadedAgents[0]?.id || null);
    } catch (error) {
      console.error("Failed to load agents", error);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (!isVisible) return;
    void loadData();
  }, [isVisible]);

  useEffect(() => {
    const selected =
      agents.find((agent) => agent.id === selectedAgentId) || null;
    setDraft(
      selected
        ? { ...selected, tool_policy: { ...selected.tool_policy } }
        : null,
    );
  }, [agents, selectedAgentId]);

  const filteredAgents = useMemo(() => {
    const normalized = searchQuery.trim().toLowerCase();
    if (!normalized) return agents;
    return agents.filter((agent) => {
      const haystack =
        `${agent.name} ${agent.description} ${agent.system_prompt}`.toLowerCase();
      return haystack.includes(normalized);
    });
  }, [agents, searchQuery]);

  const setDraftPatch = (patch: Partial<AgentDefinition>) => {
    setDraft((current) => (current ? { ...current, ...patch } : current));
  };

  const handleSave = async () => {
    if (!draft) return;
    const saved = await assistantAgentUpsert(draft);
    await loadData();
    setSelectedAgentId(saved.id);
    setMessage(t("presetSaved", "Saved"));
    window.setTimeout(() => setMessage(null), 2000);
  };

  const handleDelete = async () => {
    if (!draft?.id) return;
    const confirmed = await confirmDialog(
      t(
        "assistantDeleteAgentConfirm",
        "Delete this agent definition? Existing schedules will need manual reassignment.",
      ),
      {
        title: t("assistantDeleteAgent", "Delete Agent"),
        okLabel: t("delete", "Delete"),
      },
    );
    if (!confirmed) return;
    await assistantAgentDelete(draft.id);
    await loadData();
    setMessage(t("deleted", "Deleted"));
    window.setTimeout(() => setMessage(null), 2000);
  };

  const handleTestRun = async () => {
    if (!draft?.id) return;
    const result = await assistantAgentTestRun({
      agent_id: draft.id,
      prompt: testPrompt.trim() || "Run a quick capability check.",
    });
    window.localStorage.setItem(
      PENDING_CONVERSATION_KEY,
      result.conversation_id,
    );
    const appWindow = window as Window & {
      setActiveTab?: (tab: string) => void;
    };
    appWindow.setActiveTab?.("ai-assistants");
  };

  return (
    <div className="h-full">
      <div className="grid h-full gap-6 xl:grid-cols-[320px,minmax(0,1fr)]">
        <div className="flex min-h-0 flex-col rounded-2xl border bg-card">
          <div className="border-b px-4 py-4">
            <div className="flex items-center justify-between gap-3">
              <div>
                <div className="text-base font-semibold">
                  {t("agents", "智能体")}
                </div>
                <div className="text-xs text-muted-foreground">
                  {t(
                    "assistantAgentsDesc",
                    "Design reusable agent definitions with stable model and tool defaults.",
                  )}
                </div>
              </div>
              <button
                type="button"
                onClick={() => {
                  const created = createAgent();
                  setAgents((current) => [created, ...current]);
                  setSelectedAgentId(created.id);
                  setDraft(created);
                }}
                className="inline-flex items-center gap-2 rounded-lg border px-3 py-2 text-sm hover:bg-muted"
              >
                <Plus className="h-4 w-4" />
                {t("add", "Add")}
              </button>
            </div>
            <div className="mt-4 flex items-center gap-2 rounded-xl border bg-background px-3 py-2">
              <Search className="h-4 w-4 text-muted-foreground" />
              <input
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                placeholder={t("assistantSearchAgents", "搜索 Agent...")}
                className="w-full bg-transparent text-sm outline-none"
              />
            </div>
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto p-4">
            {loading ? (
              <div className="text-sm text-muted-foreground">
                {t("loading", "Loading...")}
              </div>
            ) : (
              <div className="space-y-2">
                {filteredAgents.map((agent) => (
                  <button
                    key={agent.id || agent.name}
                    type="button"
                    onClick={() => setSelectedAgentId(agent.id)}
                    className={`w-full rounded-xl border px-3 py-3 text-left transition-colors ${
                      draft?.id === agent.id
                        ? "border-primary bg-primary/5"
                        : "hover:bg-muted/30"
                    }`}
                  >
                    <div className="flex items-start gap-3">
                      <div className="rounded-full bg-primary/10 p-2 text-primary">
                        <Bot className="h-4 w-4" />
                      </div>
                      <div className="min-w-0">
                        <div className="truncate text-sm font-medium">
                          {agent.name}
                        </div>
                        <div className="mt-1 line-clamp-2 text-xs text-muted-foreground">
                          {agent.description ||
                            agent.output_contract ||
                            agent.system_prompt}
                        </div>
                      </div>
                    </div>
                  </button>
                ))}
              </div>
            )}
          </div>
        </div>

        <div className="min-h-0 rounded-2xl border bg-card">
          <div className="border-b px-6 py-4">
            <div className="flex items-center justify-between gap-3">
              <div>
                <div className="text-lg font-semibold">
                  {draft?.name || t("assistantEmptyAgent", "选择一个 Agent")}
                </div>
                <div className="text-sm text-muted-foreground">
                  {t(
                    "assistantAgentEditorDesc",
                    "Bind prompt, output contract, and model defaults in one focused editor.",
                  )}
                </div>
              </div>
              <div className="flex items-center gap-2">
                {message ? (
                  <span className="text-xs text-muted-foreground">
                    {message}
                  </span>
                ) : null}
                <button
                  type="button"
                  onClick={() => void handleDelete()}
                  disabled={!draft?.id}
                  className="inline-flex items-center gap-2 rounded-lg border border-destructive/30 px-3 py-2 text-sm text-destructive hover:bg-destructive/5 disabled:opacity-50"
                >
                  <Trash2 className="h-4 w-4" />
                  {t("delete", "Delete")}
                </button>
                <button
                  type="button"
                  onClick={() => void handleSave()}
                  disabled={!draft}
                  className="inline-flex items-center gap-2 rounded-lg bg-primary px-3 py-2 text-sm text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
                >
                  <Save className="h-4 w-4" />
                  {t("saveCurrentTab", "Save")}
                </button>
              </div>
            </div>
          </div>

          <div className="min-h-0 overflow-y-auto px-6 py-5">
            {draft ? (
              <div className="space-y-6">
                <div className="grid gap-4 md:grid-cols-2">
                  <label className="space-y-2">
                    <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                      {t("name", "Name")}
                    </span>
                    <input
                      value={draft.name}
                      onChange={(e) => setDraftPatch({ name: e.target.value })}
                      className="w-full rounded-lg border bg-background px-3 py-2 text-sm"
                    />
                  </label>
                  <label className="space-y-2">
                    <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                      {t("description", "Description")}
                    </span>
                    <input
                      value={draft.description}
                      onChange={(e) =>
                        setDraftPatch({ description: e.target.value })
                      }
                      className="w-full rounded-lg border bg-background px-3 py-2 text-sm"
                    />
                  </label>
                </div>

                <div className="grid gap-4 md:grid-cols-2">
                  <label className="space-y-2">
                    <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                      {t("assistantDefaultAgentProfile", "默认 Agent 模型")}
                    </span>
                    <select
                      value={draft.default_model_profile_id || ""}
                      onChange={(e) =>
                        setDraftPatch({
                          default_model_profile_id: e.target.value || null,
                        })
                      }
                      className="w-full rounded-lg border bg-background px-3 py-2 text-sm"
                    >
                      <option value="">
                        {t("selectModel", "Select model")}
                      </option>
                      {(settings?.profiles || []).map((profile) => (
                        <option key={profile.id} value={profile.id}>
                          {profile.name} / {profile.model_id}
                        </option>
                      ))}
                    </select>
                  </label>
                  <label className="space-y-2">
                    <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                      {t("assistantDefaultSummaryProfile", "轻量模型")}
                    </span>
                    <select
                      value={draft.light_model_profile_id || ""}
                      onChange={(e) =>
                        setDraftPatch({
                          light_model_profile_id: e.target.value || null,
                        })
                      }
                      className="w-full rounded-lg border bg-background px-3 py-2 text-sm"
                    >
                      <option value="">
                        {t("selectModel", "Select model")}
                      </option>
                      {(settings?.profiles || []).map((profile) => (
                        <option key={profile.id} value={profile.id}>
                          {profile.name} / {profile.model_id}
                        </option>
                      ))}
                    </select>
                  </label>
                </div>

                <label className="space-y-2 block">
                  <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                    {t("systemPrompt", "System Prompt")}
                  </span>
                  <textarea
                    value={draft.system_prompt}
                    onChange={(e) =>
                      setDraftPatch({ system_prompt: e.target.value })
                    }
                    className="min-h-[180px] w-full rounded-xl border bg-background px-3 py-3 text-sm leading-6"
                  />
                </label>

                <label className="space-y-2 block">
                  <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                    {t("assistantOutputContract", "输出契约")}
                  </span>
                  <input
                    value={draft.output_contract}
                    onChange={(e) =>
                      setDraftPatch({ output_contract: e.target.value })
                    }
                    className="w-full rounded-lg border bg-background px-3 py-2 text-sm"
                    placeholder="summary + risks + action_items"
                  />
                </label>

                <div className="grid gap-3 md:grid-cols-3">
                  {[
                    ["web_search", t("assistantWebSearch", "联网搜索")],
                    ["workspace_read", t("workspaceRead", "Workspace Read")],
                    ["notes_search", t("notes", "Notes Search")],
                  ].map(([key, label]) => (
                    <label
                      key={key}
                      className="flex items-center justify-between rounded-xl border bg-muted/10 px-4 py-3"
                    >
                      <span className="text-sm font-medium">{label}</span>
                      <input
                        type="checkbox"
                        checked={
                          draft.tool_policy[
                            key as keyof typeof draft.tool_policy
                          ]
                        }
                        onChange={(e) =>
                          setDraft((current) =>
                            current
                              ? {
                                  ...current,
                                  tool_policy: {
                                    ...current.tool_policy,
                                    [key]: e.target.checked,
                                  },
                                }
                              : current,
                          )
                        }
                      />
                    </label>
                  ))}
                </div>

                <div className="rounded-2xl border bg-muted/10 p-4">
                  <div className="mb-3 flex items-center gap-2 text-sm font-semibold">
                    <Play className="h-4 w-4" />
                    {t("assistantAgentTestRun", "测试运行")}
                  </div>
                  <textarea
                    value={testPrompt}
                    onChange={(e) => setTestPrompt(e.target.value)}
                    className="min-h-[120px] w-full rounded-xl border bg-background px-3 py-3 text-sm leading-6"
                  />
                  <div className="mt-3">
                    <button
                      type="button"
                      onClick={() => void handleTestRun()}
                      disabled={!draft.id}
                      className="inline-flex items-center gap-2 rounded-lg border px-4 py-2 text-sm hover:bg-muted disabled:opacity-50"
                    >
                      <Play className="h-4 w-4" />
                      {t("assistantAgentTestRun", "测试运行")}
                    </button>
                  </div>
                </div>
              </div>
            ) : (
              <div className="text-sm text-muted-foreground">
                {t(
                  "assistantSelectAgentHint",
                  "从左侧选择一个 Agent，或新建一个。",
                )}
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
