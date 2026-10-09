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
