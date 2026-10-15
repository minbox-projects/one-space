import { afterEach, describe, expect, it, vi } from "vitest";
import * as aiWorkflowProfiles from "@/lib/aiWorkflowProfiles";
import { invokeMock, resetTauriMocks } from "@/test/mocks/tauri";

describe("aiWorkflowProfiles absent API", () => {
  it("does not export the removed atomic save-and-activate wrapper", () => {
    const module = aiWorkflowProfiles as unknown as Record<string, unknown>;
    expect("saveAndActivateProfile" in module).toBe(false);
  });
});

const AI_WORKFLOW_PROFILE_UPDATED_EVENT = "ai-workflow-profile-updated";

describe("aiWorkflowProfiles profile-updated window event", () => {
  afterEach(() => {
    resetTauriMocks();
  });

  it("exports the profile-updated event constant", () => {
    const exported = (
      aiWorkflowProfiles as unknown as Record<string, unknown>
    ).AI_WORKFLOW_PROFILE_UPDATED_EVENT;
    expect(exported).toBe(AI_WORKFLOW_PROFILE_UPDATED_EVENT);
  });

  it("emits the profile-updated event after a successful activation", async () => {
    invokeMock.mockResolvedValueOnce({
      active_profile: "team-alpha",
      hosts: [],
      installations: [],
    });
    const listener = vi.fn();
    window.addEventListener(AI_WORKFLOW_PROFILE_UPDATED_EVENT, listener);
    try {
      await aiWorkflowProfiles.activateProfile("team-alpha");
    } finally {
      window.removeEventListener(AI_WORKFLOW_PROFILE_UPDATED_EVENT, listener);
    }
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it("does not emit the profile-updated event when activation fails", async () => {
    invokeMock.mockRejectedValueOnce(new Error("activation failed"));
    const listener = vi.fn();
    window.addEventListener(AI_WORKFLOW_PROFILE_UPDATED_EVENT, listener);
    try {
      await expect(
        aiWorkflowProfiles.activateProfile("team-alpha"),
      ).rejects.toThrow();
    } finally {
      window.removeEventListener(AI_WORKFLOW_PROFILE_UPDATED_EVENT, listener);
    }
    expect(listener).not.toHaveBeenCalled();
  });
});

// Step 2 guard (REQ-001): the typed IPC wrapper must pass the additive
// model-capability metadata through unchanged and must not synthesize an empty
// capability map for an omitted one. These are guard assertions, not RED.
describe("aiWorkflowProfiles model-capability IPC metadata", () => {
  afterEach(() => {
    resetTauriMocks();
  });

  it("preserves each column's capability map and warning through getModelSources", async () => {
    const sources = {
      opencode: {
        models: ["provider-a/deepseek/r1"],
        model_capabilities: {
          "provider-a/deepseek/r1": { efforts: ["low", "high"], default_effort: "high" },
        },
        capability_warning: "cache partial",
      },
      codex: { models: ["gpt-5"] },
      claude: { models: ["claude-3-7-sonnet"] },
    };
    invokeMock.mockResolvedValueOnce(sources);

    const result = await aiWorkflowProfiles.getModelSources();
    const opencode = result.opencode as unknown as Record<string, unknown>;
    expect(opencode.models).toEqual(["provider-a/deepseek/r1"]);
    expect(opencode.model_capabilities).toEqual({
      "provider-a/deepseek/r1": { efforts: ["low", "high"], default_effort: "high" },
    });
    expect(opencode.capability_warning).toBe("cache partial");
  });

  it("keeps an omitted capability map unknown instead of synthesizing an empty one", async () => {
    const sources = {
      opencode: { models: [] },
      codex: { models: [] },
      claude: { models: [] },
    };
    invokeMock.mockResolvedValueOnce(sources);

    const result = await aiWorkflowProfiles.getModelSources();
    for (const tool of ["opencode", "codex", "claude"] as const) {
      expect(
        (result[tool] as unknown as Record<string, unknown>).model_capabilities,
      ).toBeUndefined();
    }
  });
});
