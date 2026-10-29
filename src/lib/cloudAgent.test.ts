import { describe, expect, it } from "vitest";
import { formatWorkingDirTail } from "./cloudAgent";

describe("formatWorkingDirTail", () => {
  it("handles empty or falsy paths", () => {
    expect(formatWorkingDirTail("")).toEqual({
      display: "—",
      isTruncated: false,
      fullPath: "",
    });
  });

  it("returns full path when shorter than maxLen", () => {
    const path = "/home/user/project";
    const res = formatWorkingDirTail(path, 30);
    expect(res.display).toBe(path);
    expect(res.isTruncated).toBe(false);
    expect(res.fullPath).toBe(path);
  });

  it("truncates front and keeps rear directory when exceeding maxLen", () => {
    const path = "/root/cloud-agent-workspaces/one-space/worktrees/feat-auth";
    const res = formatWorkingDirTail(path, 30);
    expect(res.isTruncated).toBe(true);
    expect(res.display.startsWith("…")).toBe(true);
    expect(res.display.endsWith("feat-auth")).toBe(true);
  });
});
