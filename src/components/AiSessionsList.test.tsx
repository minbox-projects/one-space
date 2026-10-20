import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "@/i18n";
import { AiSessionsList, type AiSessionListItem } from "@/components/AiSessionsList";
import { renderWithProviders } from "@/test/mocks/render";

const mockSessions: AiSessionListItem[] = [
  {
    id: "s-1",
    name: "Refactor API Gateway",
    working_dir: "/Users/dev/project-one",
    model_type: "opencode",
    model_name: "claude-3-7-sonnet",
    tool_session_id: "ses_main_123",
    status: "active",
    created_at: 1700000000,
    last_used_at: 1700001000,
    is_main_agent: true,
  },
  {
    id: "s-2",
    name: "Build frontend UI (@frontend subagent)",
    working_dir: "/Users/dev/project-one",
    model_type: "opencode",
    model_name: "claude-3-7-sonnet",
    tool_session_id: "ses_sub_456",
    status: "active",
    created_at: 1700000100,
    last_used_at: 1700000900,
    is_main_agent: false,
  },
  {
    id: "s-3",
    name: "Claude Docs Research",
    working_dir: "/Users/dev/docs",
    model_type: "claude",
    model_name: "claude-3-5-sonnet",
    tool_session_id: "claude-session-789",
    status: "active",
    created_at: 1700000200,
    last_used_at: 1700000800,
    is_main_agent: true,
  },
];

describe("AiSessionsList", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
  });

  it("renders redesigned query bar with search input and filters", () => {
    renderWithProviders(
      <AiSessionsList
        sessions={mockSessions}
        onLaunch={vi.fn()}
        onDelete={vi.fn()}
        onRename={vi.fn()}
      />,
    );

    expect(
      screen.getByPlaceholderText("Search sessions by name, path, ID or model..."),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Main agent only/i })).toBeInTheDocument();
    expect(screen.getByText("Refactor API Gateway")).toBeInTheDocument();
  });

  it("filters out subagents by default and shows main agents only", () => {
    renderWithProviders(
      <AiSessionsList
        sessions={mockSessions}
        onLaunch={vi.fn()}
        onDelete={vi.fn()}
        onRename={vi.fn()}
      />,
    );

    // Main agents should be displayed
    expect(screen.getByText("Refactor API Gateway")).toBeInTheDocument();
    expect(screen.getByText("Claude Docs Research")).toBeInTheDocument();

    // Subagent should be filtered out by default
    expect(screen.queryByText("Build frontend UI (@frontend subagent)")).not.toBeInTheDocument();

    // Summary count shows 2 of 3 sessions and 2 main
    expect(screen.getByText("Showing 2 of 3 sessions")).toBeInTheDocument();
    expect(screen.getByText("(2 main)")).toBeInTheDocument();
  });

  it("toggles main agent filter to display subagents and shows subagent badge", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <AiSessionsList
        sessions={mockSessions}
        onLaunch={vi.fn()}
        onDelete={vi.fn()}
        onRename={vi.fn()}
      />,
    );

    const toggleButton = screen.getByRole("button", { name: /Main agent only/i });
    await user.click(toggleButton);

    // Now all 3 sessions should be visible
    expect(screen.getByText("Refactor API Gateway")).toBeInTheDocument();
    expect(screen.getByText("Claude Docs Research")).toBeInTheDocument();
    expect(screen.getByText("Build frontend UI (@frontend subagent)")).toBeInTheDocument();

    // Subagent badge should be shown on the subagent session
    expect(screen.getByText("Subagent")).toBeInTheDocument();
  });

  it("searches by session id and working directory", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <AiSessionsList
        sessions={mockSessions}
        onLaunch={vi.fn()}
        onDelete={vi.fn()}
        onRename={vi.fn()}
      />,
    );

    const searchInput = screen.getByPlaceholderText("Search sessions by name, path, ID or model...");
    await user.type(searchInput, "docs");

    expect(screen.getByText("Claude Docs Research")).toBeInTheDocument();
    expect(screen.queryByText("Refactor API Gateway")).not.toBeInTheDocument();

    // Clear search with clear button
    const clearButton = screen.getByTitle("Clear");
    await user.click(clearButton);

    expect(screen.getByText("Refactor API Gateway")).toBeInTheDocument();
    expect(screen.getByText("Claude Docs Research")).toBeInTheDocument();
  });

  it("resets all filters when clicking reset button", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <AiSessionsList
        sessions={mockSessions}
        onLaunch={vi.fn()}
        onDelete={vi.fn()}
        onRename={vi.fn()}
      />,
    );

    const searchInput = screen.getByPlaceholderText("Search sessions by name, path, ID or model...");
    await user.type(searchInput, "Refactor");

    // Reset button should now be visible
    const resetButton = screen.getByTitle("Reset filters");
    expect(resetButton).toBeInTheDocument();

    await user.click(resetButton);

    // Both main sessions visible again
    expect(screen.getByText("Refactor API Gateway")).toBeInTheDocument();
    expect(screen.getByText("Claude Docs Research")).toBeInTheDocument();
  });
});
