import type { ComponentProps } from "react";
import { fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "@/i18n";
import { ClaudeModelSelectionDialog } from "./ClaudeModelSelectionDialog";

/**
 * Step 4 (REQ-006 / REQ-007, AC-012 / AC-015) behavior contract for the Claude
 * four-choice model dialog. The public props boundary is fixed here: `open`,
 * `models` (selectable local mapping ids), `initialSelection`, `busy`,
 * `onSubmit(selection)` and `onCancel()`. `selection` uses the backend
 * `claude_models` snake_case shape shared with the terminal-target snapshot.
 */
type DialogProps = ComponentProps<typeof ClaudeModelSelectionDialog>;

const MODELS = ["cc-haiku", "cc-sonnet", "cc-opus"];

function renderDialog(overrides: Partial<DialogProps> = {}) {
  const onSubmit = overrides.onSubmit ?? vi.fn();
  const onCancel = overrides.onCancel ?? vi.fn();
  render(
    <ClaudeModelSelectionDialog
      open={overrides.open ?? true}
      models={overrides.models ?? MODELS}
      initialSelection={overrides.initialSelection ?? null}
      busy={overrides.busy}
      onSubmit={onSubmit}
      onCancel={onCancel}
    />,
  );
  return { onSubmit, onCancel };
}

const SELECT_TEST_IDS = {
  default: "claude-model-select-default",
  haiku: "claude-model-select-haiku",
  sonnet: "claude-model-select-sonnet",
  opus: "claude-model-select-opus",
} as const;

describe("ClaudeModelSelectionDialog 四模型选择对话框", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
  });

  it("渲染默认/haiku/sonnet/opus 四个选择器并列出候选本地模型 id", () => {
    renderDialog();

    expect(screen.getByTestId("claude-model-selection-dialog")).toBeInTheDocument();
    for (const label of [
      "Default model",
      "Haiku model",
      "Sonnet model",
      "Opus model",
    ]) {
      expect(screen.getByLabelText(label)).toBeInTheDocument();
    }

    const defaultSelect = screen.getByTestId(SELECT_TEST_IDS.default);
    const values = within(defaultSelect)
      .getAllByRole("option")
      .map((option) => (option as HTMLOptionElement).value);
    for (const model of MODELS) expect(values).toContain(model);
  });

  it("首次选择四项后提交回调携带四个本地模型 id", () => {
    const { onSubmit } = renderDialog();

    fireEvent.change(screen.getByTestId(SELECT_TEST_IDS.default), {
      target: { value: "cc-opus" },
    });
    fireEvent.change(screen.getByTestId(SELECT_TEST_IDS.haiku), {
      target: { value: "cc-haiku" },
    });
    fireEvent.change(screen.getByTestId(SELECT_TEST_IDS.sonnet), {
      target: { value: "cc-sonnet" },
    });
    fireEvent.change(screen.getByTestId(SELECT_TEST_IDS.opus), {
      target: { value: "cc-opus" },
    });

    fireEvent.click(screen.getByTestId("claude-model-selection-submit"));

    expect(onSubmit).toHaveBeenCalledTimes(1);
    expect(onSubmit).toHaveBeenCalledWith({
      default_model: "cc-opus",
      haiku: "cc-haiku",
      sonnet: "cc-sonnet",
      opus: "cc-opus",
    });
  });

  it("允许四个槽位选择同一本地模型 id", () => {
    const { onSubmit } = renderDialog();

    for (const testId of Object.values(SELECT_TEST_IDS)) {
      fireEvent.change(screen.getByTestId(testId), {
        target: { value: "cc-opus" },
      });
    }
    fireEvent.click(screen.getByTestId("claude-model-selection-submit"));

    expect(onSubmit).toHaveBeenCalledWith({
      default_model: "cc-opus",
      haiku: "cc-opus",
      sonnet: "cc-opus",
      opus: "cc-opus",
    });
  });

  it("存在空槽位时不提交", () => {
    const { onSubmit } = renderDialog();

    // Only the default slot is chosen; the three aliases stay blank.
    fireEvent.change(screen.getByTestId(SELECT_TEST_IDS.default), {
      target: { value: "cc-opus" },
    });
    fireEvent.click(screen.getByTestId("claude-model-selection-submit"));

    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("初始已存选择回填后可直接提交复用", () => {
    const { onSubmit } = renderDialog({
      initialSelection: {
        default_model: "cc-opus",
        haiku: "cc-haiku",
        sonnet: "cc-sonnet",
        opus: "cc-opus",
      },
    });

    expect(screen.getByTestId(SELECT_TEST_IDS.default)).toHaveValue("cc-opus");
    expect(screen.getByTestId(SELECT_TEST_IDS.haiku)).toHaveValue("cc-haiku");
    expect(screen.getByTestId(SELECT_TEST_IDS.sonnet)).toHaveValue("cc-sonnet");
    expect(screen.getByTestId(SELECT_TEST_IDS.opus)).toHaveValue("cc-opus");

    fireEvent.click(screen.getByTestId("claude-model-selection-submit"));
    expect(onSubmit).toHaveBeenCalledTimes(1);
  });

  it("取消只回调 onCancel，不触发任何提交", () => {
    const { onSubmit, onCancel } = renderDialog();

    fireEvent.click(screen.getByTestId("claude-model-selection-cancel"));

    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("中文下展示四选择文案", async () => {
    await i18n.changeLanguage("zh");
    renderDialog();

    expect(screen.getByText("选择 Claude 模型")).toBeInTheDocument();
    expect(screen.getByLabelText("默认模型")).toBeInTheDocument();
    expect(screen.getByLabelText("Haiku 模型")).toBeInTheDocument();
    expect(screen.getByLabelText("Sonnet 模型")).toBeInTheDocument();
    expect(screen.getByLabelText("Opus 模型")).toBeInTheDocument();
  });
});
