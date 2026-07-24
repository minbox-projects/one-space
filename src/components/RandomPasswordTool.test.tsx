import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { RandomPasswordTool } from "@/components/RandomPasswordTool";
import { renderWithProviders } from "@/test/mocks/render";
import { invokeMock, resetTauriMocks } from "@/test/mocks/tauri";

const PASSWORD_HISTORY_KEY = "onespace:random-password-history";

function deferred<T>() {
  let resolve: (value: T) => void;
  const promise = new Promise<T>((nextResolve) => {
    resolve = nextResolve;
  });
  return { promise, resolve: resolve! };
}

describe("RandomPasswordTool", () => {
  beforeEach(() => {
    let toastId = 0;
    localStorage.clear();
    resetTauriMocks();
    invokeMock.mockResolvedValue(null);
    vi.stubGlobal("crypto", {
      getRandomValues: (values: Uint32Array) => {
        values[0] = 0;
        return values;
      },
      randomUUID: () => `test-toast-id-${toastId++}`,
    });
  });

  it("generates exactly nine passwords using the selected character groups", async () => {
    const user = userEvent.setup();
    renderWithProviders(<RandomPasswordTool />);

    fireEvent.change(screen.getByRole("spinbutton"), { target: { value: "4" } });
    await user.click(screen.getByRole("checkbox", { name: /Lowercase|小写字母/ }));
    await user.click(screen.getByRole("checkbox", { name: /Uppercase|大写字母/ }));
    await user.click(screen.getByRole("checkbox", { name: /Common symbols|常用符号/ }));
    await user.click(screen.getByRole("button", { name: /Generate|生成/ }));

    const passwords = screen.getAllByTestId("generated-password");
    expect(passwords).toHaveLength(9);
    expect(passwords.every((item) => /^\d{4}$/.test(item.textContent || ""))).toBe(true);
    expect(screen.getByLabelText(/Characters used|所用字符/)).toHaveValue("0123456789");
  });

  it("keeps character groups synchronized and blocks generation for an empty character set", async () => {
    const user = userEvent.setup();
    renderWithProviders(<RandomPasswordTool />);

    const characters = screen.getByLabelText(/Characters used|所用字符/);
    await user.clear(characters);

    expect(screen.getByRole("checkbox", { name: /Numbers|数字/ })).not.toBeChecked();
    expect(screen.getByRole("checkbox", { name: /Lowercase|小写字母/ })).not.toBeChecked();

    await user.click(screen.getByRole("button", { name: /Generate|生成/ }));

    expect(screen.getByRole("alert")).toHaveTextContent(/Choose at least one character|请至少选择一个字符/);
    expect(screen.queryAllByTestId("generated-password")).toHaveLength(0);
  });

  it("ensures every enabled group appears in every generated password", async () => {
    const user = userEvent.setup();
    renderWithProviders(<RandomPasswordTool />);

    fireEvent.change(screen.getByRole("spinbutton"), { target: { value: "4" } });
    await user.click(screen.getByRole("button", { name: /Generate|生成/ }));

    const passwords = screen.getAllByTestId("generated-password");
    expect(passwords).toHaveLength(9);
    expect(passwords.every((item) => /\d/.test(item.textContent || ""))).toBe(true);
    expect(passwords.every((item) => /[a-z]/.test(item.textContent || ""))).toBe(true);
    expect(passwords.every((item) => /[A-Z]/.test(item.textContent || ""))).toBe(true);
    expect(passwords.every((item) => /[~!@#$%^&*()_+]/.test(item.textContent || ""))).toBe(true);
  });

  it("blocks generation when the password length cannot cover every enabled group", async () => {
    const user = userEvent.setup();
    renderWithProviders(<RandomPasswordTool />);

    fireEvent.change(screen.getByRole("spinbutton"), { target: { value: "3" } });
    await user.click(screen.getByRole("button", { name: /Generate|生成/ }));

    expect(screen.getByRole("alert")).toHaveTextContent(/at least 4|至少为 4/);
    expect(screen.queryAllByTestId("generated-password")).toHaveLength(0);
  });

  it("does not add unavailable group characters after manual character edits", async () => {
    const user = userEvent.setup();
    renderWithProviders(<RandomPasswordTool />);

    const characters = screen.getByLabelText(/Characters used|所用字符/);
    await user.clear(characters);
    await user.type(characters, "0123abc");
    await user.click(screen.getByRole("button", { name: /Generate|生成/ }));

    expect(
      screen.getAllByTestId("generated-password").every((item) =>
        /^[0123abc]+$/.test(item.textContent || ""),
      ),
    ).toBe(true);
  });

  it("covers every enabled group from its actual manual character subset", async () => {
    const user = userEvent.setup();
    renderWithProviders(<RandomPasswordTool />);

    const characters = screen.getByLabelText(/Characters used|所用字符/);
    await user.clear(characters);
    await user.type(characters, "aA!");
    fireEvent.change(screen.getByRole("spinbutton"), { target: { value: "3" } });
    await user.click(screen.getByRole("button", { name: /Generate|生成/ }));

    expect(screen.getByRole("checkbox", { name: /Lowercase|小写字母/ })).toBeChecked();
    expect(screen.getByRole("checkbox", { name: /Uppercase|大写字母/ })).toBeChecked();
    expect(screen.getByRole("checkbox", { name: /Common symbols|常用符号/ })).toBeChecked();
    expect(
      screen.getAllByTestId("generated-password").every((item) =>
        /a/.test(item.textContent || "") && /A/.test(item.textContent || "") && /!/.test(item.textContent || ""),
      ),
    ).toBe(true);

    fireEvent.change(screen.getByRole("spinbutton"), { target: { value: "2" } });
    await user.click(screen.getByRole("button", { name: /Generate|生成/ }));

    expect(screen.getByRole("alert")).toHaveTextContent(/at least 3|至少为 3/);
  });

  it("loads copied history from protected secrets storage", async () => {
    invokeMock.mockImplementation(async (command: string) =>
      command === "get_secret" ? JSON.stringify(["stored-password"]) : null,
    );
    renderWithProviders(<RandomPasswordTool />);

    expect(await screen.findByText("stored-password")).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("get_secret", { key: PASSWORD_HISTORY_KEY });
  });

  it("migrates valid legacy history only after saving it to protected storage", async () => {
    localStorage.setItem(PASSWORD_HISTORY_KEY, JSON.stringify(["legacy-password"]));
    renderWithProviders(<RandomPasswordTool />);

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("save_secret", {
        key: PASSWORD_HISTORY_KEY,
        value: JSON.stringify(["legacy-password"]),
      }),
    );

    expect(localStorage.getItem(PASSWORD_HISTORY_KEY)).toBeNull();
    expect(screen.getByText("legacy-password")).toBeInTheDocument();
  });

  it("saves copied passwords to protected secrets storage without writing localStorage", async () => {
    const user = userEvent.setup();
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    renderWithProviders(<RandomPasswordTool />);

    await user.click(screen.getByRole("button", { name: /Generate|生成/ }));
    const firstPassword = screen.getAllByTestId("generated-password")[0].textContent!;
    await user.click(
      within(screen.getAllByTestId("generated-password-row")[0]).getByRole("button", {
        name: /Copy password|复制密码/,
      }),
    );

    expect(writeText).toHaveBeenCalledWith(firstPassword);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("save_secret", {
        key: PASSWORD_HISTORY_KEY,
        value: JSON.stringify([firstPassword]),
      }),
    );
    expect(localStorage.getItem(PASSWORD_HISTORY_KEY)).toBeNull();
  });

  it("preserves loaded history when copying before a delayed initial load completes", async () => {
    const user = userEvent.setup();
    const getSecret = deferred<string | null>();
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    invokeMock.mockImplementation((command: string) =>
      command === "get_secret" ? getSecret.promise : Promise.resolve(null),
    );
    renderWithProviders(<RandomPasswordTool />);

    await user.click(screen.getByRole("button", { name: /Generate|生成/ }));
    const password = screen.getAllByTestId("generated-password")[0].textContent!;
    await user.click(
      within(screen.getAllByTestId("generated-password-row")[0]).getByRole("button", {
        name: /Copy password|复制密码/,
      }),
    );
    await waitFor(() => expect(writeText).toHaveBeenCalledWith(password));
    getSecret.resolve(JSON.stringify(["stored-password"]));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("save_secret", {
        key: PASSWORD_HISTORY_KEY,
        value: JSON.stringify([password, "stored-password"]),
      }),
    );
    const historyList = screen.getByRole("list");
    expect(within(historyList).getByText(password)).toBeInTheDocument();
    expect(within(historyList).getByText("stored-password")).toBeInTheDocument();
  });

  it("keeps history cleared when a delayed initial load resolves after clear", async () => {
    const user = userEvent.setup();
    const getSecret = deferred<string | null>();
    invokeMock.mockImplementation((command: string) =>
      command === "get_secret" ? getSecret.promise : Promise.resolve(null),
    );
    renderWithProviders(<RandomPasswordTool />);

    await user.click(screen.getByRole("button", { name: /Clear history|清除历史/ }));
    getSecret.resolve(JSON.stringify(["stored-password"]));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("delete_secret", { key: PASSWORD_HISTORY_KEY }),
    );
    expect(screen.queryByText("stored-password")).not.toBeInTheDocument();
  });

  it("serializes consecutive copies without dropping earlier history", async () => {
    const user = userEvent.setup();
    const firstSave = deferred<null>();
    const writeText = vi.fn().mockResolvedValue(undefined);
    let saveCount = 0;
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    invokeMock.mockImplementation((command: string) => {
      if (command === "get_secret") {
        return Promise.resolve(null);
      }
      if (command === "save_secret") {
        saveCount += 1;
        return saveCount === 1 ? firstSave.promise : Promise.resolve(null);
      }
      return Promise.resolve(null);
    });
    renderWithProviders(<RandomPasswordTool />);

    await user.click(screen.getByRole("button", { name: /Generate|生成/ }));
    const firstPassword = screen.getAllByTestId("generated-password")[0].textContent!;
    await user.click(
      within(screen.getAllByTestId("generated-password-row")[0]).getByRole("button", {
        name: /Copy password|复制密码/,
      }),
    );
    await waitFor(() => expect(saveCount).toBe(1));

    const characters = screen.getByLabelText(/Characters used|所用字符/);
    await user.clear(characters);
    await user.type(characters, "a");
    await user.click(screen.getByRole("button", { name: /Generate|生成/ }));
    const secondPassword = screen.getAllByTestId("generated-password")[0].textContent!;
    await user.click(
      within(screen.getAllByTestId("generated-password-row")[0]).getByRole("button", {
        name: /Copy password|复制密码/,
      }),
    );
    expect(saveCount).toBe(1);
    firstSave.resolve(null);

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("save_secret", {
        key: PASSWORD_HISTORY_KEY,
        value: JSON.stringify([secondPassword, firstPassword]),
      }),
    );
    const historyList = screen.getByRole("list");
    expect(within(historyList).getByText(firstPassword)).toBeInTheDocument();
    expect(within(historyList).getByText(secondPassword)).toBeInTheDocument();
  });

  it("clears protected history only after deleting its secret", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation(async (command: string) =>
      command === "get_secret" ? JSON.stringify(["stored-password"]) : null,
    );
    renderWithProviders(<RandomPasswordTool />);

    expect(await screen.findByText("stored-password")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Clear history|清除历史/ }));

    expect(invokeMock).toHaveBeenCalledWith("delete_secret", { key: PASSWORD_HISTORY_KEY });
    expect(screen.queryByText("stored-password")).not.toBeInTheDocument();
  });
});
