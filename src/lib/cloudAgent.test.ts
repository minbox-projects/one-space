import { describe, expect, it } from "vitest";
import {
  formatWorkingDirTail,
  formatFriendlyDuration,
  extractPrNumber,
  formatGitCheckoutCommand,
  formatIssueLabelTail,
} from "./cloudAgent";

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

describe("formatFriendlyDuration", () => {
  it("formats seconds accurately in Chinese", () => {
    expect(formatFriendlyDuration(0, "zh")).toBe("0秒");
    expect(formatFriendlyDuration(45, "zh")).toBe("45秒");
    expect(formatFriendlyDuration(125, "zh")).toBe("2分5秒");
    expect(formatFriendlyDuration(3665, "zh")).toBe("1小时1分");
  });

  it("formats seconds accurately in English", () => {
    expect(formatFriendlyDuration(0, "en")).toBe("0s");
    expect(formatFriendlyDuration(45, "en")).toBe("45s");
    expect(formatFriendlyDuration(125, "en")).toBe("2m 5s");
    expect(formatFriendlyDuration(3665, "en")).toBe("1h 1m");
  });

  it("handles null or negative gracefully", () => {
    expect(formatFriendlyDuration(null, "zh")).toBe("0秒");
    expect(formatFriendlyDuration(undefined, "en")).toBe("0s");
  });
});

describe("extractPrNumber", () => {
  it("extracts PR number from GitHub PR URL", () => {
    expect(extractPrNumber("https://github.com/org/repo/pull/105")).toBe("#105");
    expect(extractPrNumber("https://github.com/org/repo/pull/42/files")).toBe("#42");
  });

  it("extracts MR number from GitLab MR URL", () => {
    expect(extractPrNumber("https://gitlab.com/org/repo/-/merge_requests/88")).toBe("#88");
  });

  it("returns null for non-PR URL or falsy values", () => {
    expect(extractPrNumber(null)).toBeNull();
    expect(extractPrNumber("https://github.com/org/repo")).toBeNull();
  });
});

describe("formatGitCheckoutCommand", () => {
  it("formats git checkout command with origin fetch", () => {
    expect(formatGitCheckoutCommand("bot/issue-102")).toBe(
      "git fetch origin bot/issue-102 && git checkout bot/issue-102"
    );
  });

  it("returns empty string for empty branch", () => {
    expect(formatGitCheckoutCommand("")).toBe("");
  });
});

describe("formatIssueLabelTail", () => {
  it("keeps short simple repo intact", () => {
    expect(formatIssueLabelTail("one-space", 101)).toBe("one-space#101");
  });

  it("hides front organization prefix and displays tail repo and issue full", () => {
    expect(formatIssueLabelTail("minbox-projects/onespace-app", 102)).toBe("…/onespace-app#102");
  });

  it("hides front https url prefix and displays tail repo and issue full", () => {
    expect(formatIssueLabelTail("https://github.com/my-org/custom-repo.git", 103)).toBe("…/custom-repo#103");
  });

  it("hides front ssh prefix and displays tail repo and issue full", () => {
    expect(formatIssueLabelTail("git@github.com:torvalds/linux.git", 789)).toBe("…/linux#789");
  });

  it("truncates front of long standalone repo and keeps tail and issue number", () => {
    expect(formatIssueLabelTail("super-long-repository-name-exceeding-limit", 42, 15)).toBe("…exceeding-limit#42");
  });
});
