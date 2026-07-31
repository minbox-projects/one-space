import { fireEvent, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterAll, beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "@/i18n";
import { md5Hex } from "@/lib/md5";
import { Md5EncryptionTool } from "@/components/Md5EncryptionTool";
import { renderWithProviders } from "@/test/mocks/render";

vi.mock("@/lib/md5", async (importOriginal) => {
  const original = await importOriginal<typeof import("@/lib/md5")>();
  return { ...original, md5Hex: vi.fn(original.md5Hex) };
});

const RESULT_LABELS = {
  lower32: "32-bit lowercase",
  upper32: "32-bit uppercase",
  lower16: "16-bit lowercase",
  upper16: "16-bit uppercase",
} as const;

const ABC_RESULTS = {
  lower32: "900150983cd24fb0d6963f7d28e17f72",
  upper32: "900150983CD24FB0D6963F7D28E17F72",
  lower16: "3cd24fb0d6963f7d",
  upper16: "3CD24FB0D6963F7D",
} as const;

type TextareaEdit = {
  inputType: string;
  data?: string | null;
  value: string;
  selectionStart: number;
  selectionEnd?: number;
  nextSelectionStart: number;
  nextSelectionEnd?: number;
  clipboardText?: string;
  isComposing?: boolean;
};

function dispatchBeforeInput(textarea: HTMLTextAreaElement, edit: TextareaEdit) {
  const event = new textarea.ownerDocument.defaultView!.Event("beforeinput", {
    bubbles: true,
    cancelable: true,
  });
  Object.defineProperties(event, {
    inputType: { value: edit.inputType },
    data: { value: edit.data ?? null },
    isComposing: { value: edit.isComposing ?? false },
  });
  fireEvent(textarea, event);
}

function editTextarea(textarea: HTMLTextAreaElement, edit: TextareaEdit) {
  textarea.setSelectionRange(
    edit.selectionStart,
    edit.selectionEnd ?? edit.selectionStart,
  );
  if (edit.clipboardText !== undefined) {
    fireEvent.paste(textarea, {
      clipboardData: { getData: () => edit.clipboardText },
    });
  }
  dispatchBeforeInput(textarea, edit);
  fireEvent.input(textarea, {
    target: {
      value: edit.value,
      selectionStart: edit.nextSelectionStart,
      selectionEnd: edit.nextSelectionEnd ?? edit.nextSelectionStart,
    },
    inputType: edit.inputType,
    data: edit.data ?? null,
    isComposing: edit.isComposing ?? false,
  });
}

function pasteTextarea(
  textarea: HTMLTextAreaElement,
  rawText: string,
  selectionStart = textarea.selectionStart,
  selectionEnd = textarea.selectionEnd,
) {
  const displayedText = rawText.replaceAll("\r\n", "\n");
  const nextValue = `${textarea.value.slice(0, selectionStart)}${displayedText}${textarea.value.slice(
    selectionEnd,
  )}`;
  const nextSelection = selectionStart + displayedText.length;
  editTextarea(textarea, {
    inputType: "insertFromPaste",
    value: nextValue,
    selectionStart,
    selectionEnd,
    nextSelectionStart: nextSelection,
    clipboardText: rawText,
  });
}

function resultRow(resultKey: keyof typeof RESULT_LABELS) {
  return screen.getByTestId(`md5-result-${resultKey}`);
}

function expectResults(expected: Record<keyof typeof RESULT_LABELS, string>) {
  for (const resultKey of Object.keys(RESULT_LABELS) as (keyof typeof RESULT_LABELS)[]) {
    expect(within(resultRow(resultKey)).getByText(expected[resultKey])).toBeInTheDocument();
  }
}

describe("Md5EncryptionTool", () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    await i18n.changeLanguage("en");
  });

  afterAll(async () => {
    await i18n.changeLanguage("zh");
  });

  it("distinguishes the initial state from an explicitly calculated empty string", async () => {
    const user = userEvent.setup();
    renderWithProviders(<Md5EncryptionTool />);

    expect(screen.getByText("Enter text and calculate to see MD5 results.")).toBeInTheDocument();
    expect(screen.queryAllByRole("button", { name: /^Copy / })).toHaveLength(0);

    await user.click(screen.getByRole("button", { name: "Calculate MD5" }));

    expect(md5Hex).toHaveBeenCalledOnce();
    expect(md5Hex).toHaveBeenCalledWith("");
    expectResults({
      lower32: "d41d8cd98f00b204e9800998ecf8427e",
      upper32: "D41D8CD98F00B204E9800998ECF8427E",
      lower16: "8f00b204e9800998",
      upper16: "8F00B204E9800998",
    });
  });

  it("calculates only on command, derives all formats, and replaces them together", async () => {
    const user = userEvent.setup();
    renderWithProviders(<Md5EncryptionTool />);
    const input = screen.getByLabelText("Text input") as HTMLTextAreaElement;

    fireEvent.change(input, { target: { value: "abc" } });
    expect(md5Hex).not.toHaveBeenCalled();
    expect(screen.queryByText(ABC_RESULTS.lower32)).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Calculate MD5" }));
    expect(md5Hex).toHaveBeenCalledOnce();
    expectResults(ABC_RESULTS);

    fireEvent.change(input, { target: { value: "a" } });
    expectResults(ABC_RESULTS);
    expect(md5Hex).toHaveBeenCalledOnce();

    await user.click(screen.getByRole("button", { name: "Calculate MD5" }));
    expect(md5Hex).toHaveBeenCalledTimes(2);
    expect(md5Hex).toHaveBeenLastCalledWith("a");
    expectResults({
      lower32: "0cc175b9c0f1b6a831c399e269772661",
      upper32: "0CC175B9C0F1B6A831C399E269772661",
      lower16: "c0f1b6a831c399e2",
      upper16: "C0F1B6A831C399E2",
    });
    expect(screen.queryByText(ABC_RESULTS.lower32)).not.toBeInTheDocument();
  });

  it.each([
    [" abc ", "01c9a8945abead949b46c77cf3245b8a"],
    ["\t", "5e732a1878be2342dbfeff5fe3ca5aa3"],
    ["line\nend", "98143b220546868e4edba99b20f1ff97"],
    ["line\r\nend", "2dc8f4b282f6dec7471173938ea4bd41"],
    ["中文", "a7bac2239fcdcb3a067903d8077c4a07"],
    ["é", "66ddcd97cfdeabb2f6fb8a999b4bc76f"],
    ["e\u0301", "5526861fbb1e71a1bda6ac364310a807"],
  ])("preserves the original UTF-8 input %j", async (value, expected) => {
    const user = userEvent.setup();
    renderWithProviders(<Md5EncryptionTool />);

    const input = screen.getByLabelText("Text input") as HTMLTextAreaElement;
    if (value.includes("\r")) {
      pasteTextarea(input, value);
    } else {
      fireEvent.change(input, { target: { value } });
    }
    await user.click(screen.getByRole("button", { name: "Calculate MD5" }));

    expect(md5Hex).toHaveBeenCalledWith(value);
    expect(within(resultRow("lower32")).getByText(expected)).toBeInTheDocument();
  });

  it.each([
    [
      "inserts at the beginning",
      "beginning",
      ">left\nright",
      ">left\r\nright",
      "f1fc27a815fefab0a86ad8b31cc211cf",
    ],
    [
      "replaces a selection in the middle",
      "middle",
      "left\nriXXt",
      "left\r\nriXXt",
      "1c9d434533472a882531047ccf19e7d9",
    ],
    [
      "deletes at the end",
      "end",
      "left\nrigh",
      "left\r\nrigh",
      "0cbece7fc76742d26d2f9bf8f5ae903a",
    ],
  ])(
    "preserves pasted CRLF when the user %s",
    async (_description, editPosition, editedValue, expectedRawValue, expectedDigest) => {
      const user = userEvent.setup();
      renderWithProviders(<Md5EncryptionTool />);
      const input = screen.getByLabelText("Text input") as HTMLTextAreaElement;

      pasteTextarea(input, "left\r\nright");
      if (editPosition === "beginning") {
        await user.type(input, ">", { initialSelectionStart: 0, initialSelectionEnd: 0 });
      } else if (editPosition === "middle") {
        await user.type(input, "XX", { initialSelectionStart: 7, initialSelectionEnd: 9 });
      } else {
        await user.type(input, "{Backspace}", { initialSelectionStart: 10, initialSelectionEnd: 10 });
      }
      await user.click(screen.getByRole("button", { name: "Calculate MD5" }));

      expect(input).toHaveValue(editedValue);
      expect(md5Hex).toHaveBeenCalledOnce();
      expect(md5Hex).toHaveBeenCalledWith(expectedRawValue);
      expect(within(resultRow("lower32")).getByText(expectedDigest)).toBeInTheDocument();
    },
  );

  it("uses the selected repeated occurrence instead of guessing from equal prefixes and suffixes", async () => {
    const user = userEvent.setup();
    renderWithProviders(<Md5EncryptionTool />);
    const input = screen.getByLabelText("Text input") as HTMLTextAreaElement;
    pasteTextarea(input, "same\r\nsame\nsame\r\nsame");

    editTextarea(input, {
      inputType: "insertText",
      data: "X",
      value: "same\nsame\nX\nsame",
      selectionStart: 10,
      selectionEnd: 14,
      nextSelectionStart: 11,
    });
    await user.click(screen.getByRole("button", { name: "Calculate MD5" }));

    expect(md5Hex).toHaveBeenCalledWith("same\r\nsame\nX\r\nsame");
    expect(input).toHaveValue("same\nsame\nX\nsame");
    expect(input.selectionStart).toBe(11);
    expect(input.selectionEnd).toBe(11);
  });

  it("applies Backspace and Delete to raw ranges while preserving untouched mixed newlines", async () => {
    const user = userEvent.setup();
    renderWithProviders(<Md5EncryptionTool />);
    const input = screen.getByLabelText("Text input") as HTMLTextAreaElement;
    pasteTextarea(input, "A\r\nB\nC\r\nD");

    editTextarea(input, {
      inputType: "deleteContentBackward",
      value: "A\nB\n\nD",
      selectionStart: 5,
      nextSelectionStart: 4,
    });
    editTextarea(input, {
      inputType: "deleteContentForward",
      value: "A\nB\nD",
      selectionStart: 4,
      nextSelectionStart: 4,
    });
    await user.click(screen.getByRole("button", { name: "Calculate MD5" }));

    expect(md5Hex).toHaveBeenCalledWith("A\r\nB\nD");
    expect(input.selectionStart).toBe(4);
  });

  it("deletes a cross-line selection using its exact raw boundaries", async () => {
    const user = userEvent.setup();
    renderWithProviders(<Md5EncryptionTool />);
    const input = screen.getByLabelText("Text input") as HTMLTextAreaElement;
    pasteTextarea(input, "a\r\nb\nc\r\nd");

    editTextarea(input, {
      inputType: "deleteContentBackward",
      value: "a\n\nd",
      selectionStart: 2,
      selectionEnd: 5,
      nextSelectionStart: 2,
    });
    await user.click(screen.getByRole("button", { name: "Calculate MD5" }));

    expect(md5Hex).toHaveBeenCalledWith("a\r\n\r\nd");
    expect(input).toHaveValue("a\n\nd");
  });

  it("replaces a visually equal CRLF with LF when Enter explicitly edits it", async () => {
    const user = userEvent.setup();
    renderWithProviders(<Md5EncryptionTool />);
    const input = screen.getByLabelText("Text input") as HTMLTextAreaElement;
    pasteTextarea(input, "a\r\nb");

    editTextarea(input, {
      inputType: "insertLineBreak",
      value: "a\nb",
      selectionStart: 1,
      selectionEnd: 2,
      nextSelectionStart: 2,
    });
    await user.click(screen.getByRole("button", { name: "Calculate MD5" }));

    expect(md5Hex).toHaveBeenCalledWith("a\nb");
    expect(input.selectionStart).toBe(2);
  });

  it("uses original clipboard text for equal and consecutive pastes", async () => {
    const user = userEvent.setup();
    renderWithProviders(<Md5EncryptionTool />);
    const input = screen.getByLabelText("Text input") as HTMLTextAreaElement;
    pasteTextarea(input, "a\r\nb");

    pasteTextarea(input, "\n", 1, 2);
    pasteTextarea(input, "x\r\n", 3, 3);
    pasteTextarea(input, "x\r\n", 5, 5);
    await user.click(screen.getByRole("button", { name: "Calculate MD5" }));

    expect(input).toHaveValue("a\nbx\nx\n");
    expect(md5Hex).toHaveBeenCalledWith("a\nbx\r\nx\r\n");
    expect(input.selectionStart).toBe(7);
  });

  it("deletes a Unicode code point without corrupting adjacent raw CRLF", async () => {
    const user = userEvent.setup();
    renderWithProviders(<Md5EncryptionTool />);
    const input = screen.getByLabelText("Text input") as HTMLTextAreaElement;
    pasteTextarea(input, "A😀\r\n中");

    editTextarea(input, {
      inputType: "deleteContentBackward",
      value: "A\n中",
      selectionStart: 3,
      nextSelectionStart: 1,
    });
    await user.click(screen.getByRole("button", { name: "Calculate MD5" }));

    expect(md5Hex).toHaveBeenCalledWith("A\r\n中");
    expect(input.selectionStart).toBe(1);
  });

  it("keeps composition updates inside the original raw selection", async () => {
    const user = userEvent.setup();
    renderWithProviders(<Md5EncryptionTool />);
    const input = screen.getByLabelText("Text input") as HTMLTextAreaElement;
    pasteTextarea(input, "left\r\n--\r\nright");
    input.setSelectionRange(5, 7);
    fireEvent.compositionStart(input);

    editTextarea(input, {
      inputType: "insertCompositionText",
      data: "你",
      value: "left\n你\nright",
      selectionStart: 5,
      selectionEnd: 7,
      nextSelectionStart: 6,
      isComposing: true,
    });
    editTextarea(input, {
      inputType: "insertCompositionText",
      data: "你好",
      value: "left\n你好\nright",
      selectionStart: 5,
      selectionEnd: 6,
      nextSelectionStart: 7,
      isComposing: true,
    });
    fireEvent.compositionEnd(input, { data: "你好" });
    editTextarea(input, {
      inputType: "insertFromComposition",
      data: "你好",
      value: "left\n你好\nright",
      selectionStart: 7,
      nextSelectionStart: 7,
    });
    await user.click(screen.getByRole("button", { name: "Calculate MD5" }));

    expect(input).toHaveValue("left\n你好\nright");
    expect(md5Hex).toHaveBeenCalledWith("left\r\n你好\r\nright");
    expect(input.selectionStart).toBe(7);
  });

  it("copies each exact result and shows a result-specific success toast", async () => {
    const user = userEvent.setup();
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    renderWithProviders(<Md5EncryptionTool />);

    fireEvent.change(screen.getByLabelText("Text input"), { target: { value: "abc" } });
    await user.click(screen.getByRole("button", { name: "Calculate MD5" }));

    for (const resultKey of Object.keys(RESULT_LABELS) as (keyof typeof RESULT_LABELS)[]) {
      const label = RESULT_LABELS[resultKey];
      await user.click(screen.getByRole("button", { name: `Copy ${label}` }));
      expect(writeText).toHaveBeenLastCalledWith(ABC_RESULTS[resultKey]);
      expect(screen.getByText(`${label} copied.`)).toBeInTheDocument();
    }
    expect(writeText).toHaveBeenCalledTimes(4);
  });

  it.each(Object.keys(RESULT_LABELS) as (keyof typeof RESULT_LABELS)[])(
    "keeps all state and operation ability after copying %s fails",
    async (resultKey) => {
      const user = userEvent.setup();
      const writeText = vi.fn().mockRejectedValueOnce(new Error("denied")).mockResolvedValue(undefined);
      Object.defineProperty(navigator, "clipboard", {
        configurable: true,
        value: { writeText },
      });
      renderWithProviders(<Md5EncryptionTool />);

      const input = screen.getByLabelText("Text input");
      fireEvent.change(input, { target: { value: "abc" } });
      await user.click(screen.getByRole("button", { name: "Calculate MD5" }));

      const copyButton = screen.getByRole("button", { name: `Copy ${RESULT_LABELS[resultKey]}` });
      await user.click(copyButton);

      expect(screen.getByText(`Unable to copy ${RESULT_LABELS[resultKey]}.`)).toBeInTheDocument();
      expect(input).toHaveValue("abc");
      expectResults(ABC_RESULTS);
      expect(copyButton).toBeEnabled();

      await user.click(copyButton);
      expect(writeText).toHaveBeenCalledTimes(2);
      expect(screen.getByText(`${RESULT_LABELS[resultKey]} copied.`)).toBeInTheDocument();
    },
  );

  it("clears input and results, then restores input focus", async () => {
    const user = userEvent.setup();
    renderWithProviders(<Md5EncryptionTool />);
    const input = screen.getByLabelText("Text input");

    fireEvent.change(input, { target: { value: "abc" } });
    await user.click(screen.getByRole("button", { name: "Calculate MD5" }));
    await user.click(screen.getByRole("button", { name: "Clear" }));

    expect(input).toHaveValue("");
    expect(input).toHaveFocus();
    for (const resultKey of Object.keys(RESULT_LABELS)) {
      expect(screen.queryByTestId(`md5-result-${resultKey}`)).not.toBeInTheDocument();
    }
    expect(screen.queryAllByRole("button", { name: /^Copy / })).toHaveLength(0);
    expect(screen.getByText("Enter text and calculate to see MD5 results.")).toBeInTheDocument();
  });

  it("provides section semantics, distinct accessible controls, and stable narrow-screen rows", async () => {
    const user = userEvent.setup();
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    renderWithProviders(<Md5EncryptionTool />);

    expect(screen.getByRole("region", { name: "MD5 Encryption" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "MD5 results" })).toBeInTheDocument();
    const input = screen.getByLabelText("Text input");
    expect(input).toHaveAttribute("id", "md5-encryption-input");

    await user.tab();
    expect(input).toHaveFocus();
    await user.tab();
    expect(screen.getByRole("button", { name: "Calculate MD5" })).toHaveFocus();
    await user.keyboard("{Enter}");
    const copyButtons = Object.values(RESULT_LABELS).map((label) =>
      screen.getByRole("button", { name: `Copy ${label}` }),
    );
    expect(new Set(copyButtons.map((button) => button.getAttribute("aria-label"))).size).toBe(4);
    for (const button of copyButtons) expect(button).toHaveClass("h-8", "w-8", "shrink-0");
    for (const resultKey of Object.keys(RESULT_LABELS) as (keyof typeof RESULT_LABELS)[]) {
      expect(resultRow(resultKey)).toHaveClass("min-h-16", "min-w-0");
      expect(within(resultRow(resultKey)).getByText(/^[0-9A-Fa-f]{16,32}$/)).toHaveClass("break-all");
    }

    await user.tab();
    expect(screen.getByRole("button", { name: "Clear" })).toHaveFocus();
    await user.tab();
    expect(copyButtons[0]).toHaveFocus();
    await user.keyboard("{Enter}");
    expect(writeText).toHaveBeenCalledWith("d41d8cd98f00b204e9800998ecf8427e");
  });

  it.each(["en", "zh"] as const)("resolves all required %s translations", async (language) => {
    await i18n.changeLanguage(language);
    const keys = [
      "title",
      "description",
      "securityNotice",
      "inputLabel",
      "inputPlaceholder",
      "calculate",
      "clear",
      "resultsTitle",
      "emptyState",
      "copyResult",
      "copySuccess",
      "copyFailed",
      "results.lower32",
      "results.upper32",
      "results.lower16",
      "results.upper16",
    ];

    for (const key of keys) {
      const fullKey = `md5Encryption.${key}`;
      expect(i18n.t(fullKey, { label: "result" })).not.toBe(fullKey);
    }
    expect(i18n.t("md5Encryption.securityNotice")).toMatch(
      language === "en"
        ? /irreversible hash.*not suitable for password storage or secure encryption/i
        : /不可逆哈希.*不适合密码存储或安全加密/,
    );
  });
});
