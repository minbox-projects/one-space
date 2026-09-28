import { describe, expect, it } from "vitest";
import * as aiWorkflowProfiles from "@/lib/aiWorkflowProfiles";

describe("aiWorkflowProfiles absent API", () => {
  it("does not export the removed atomic save-and-activate wrapper", () => {
    const module = aiWorkflowProfiles as unknown as Record<string, unknown>;
    expect("saveAndActivateProfile" in module).toBe(false);
  });
});
