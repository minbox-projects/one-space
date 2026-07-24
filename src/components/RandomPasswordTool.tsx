import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Check, Copy, KeyRound, Minus, Plus, RefreshCw, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useToast } from "./ToastProvider";

const PASSWORD_HISTORY_KEY = "onespace:random-password-history";
const MIN_LENGTH = 1;
const MAX_LENGTH = 128;
const PASSWORD_BATCH_SIZE = 9;
const PASSWORD_HISTORY_LIMIT = 36;

const CHARACTER_GROUPS = [
  { id: "numbers", value: "0123456789" },
  { id: "lowercase", value: "abcdefghijklmnopqrstuvwxyz" },
  { id: "uppercase", value: "ABCDEFGHIJKLMNOPQRSTUVWXYZ" },
  { id: "symbols", value: "~!@#$%^&*()_+" },
] as const;

type CharacterGroupId = (typeof CHARACTER_GROUPS)[number]["id"];
type SelectedGroups = Record<CharacterGroupId, boolean>;

const DEFAULT_GROUPS: SelectedGroups = {
  numbers: true,
  lowercase: true,
  uppercase: true,
  symbols: true,
};

const DEFAULT_CHARACTERS = CHARACTER_GROUPS.map((group) => group.value).join("");

function parsePasswordHistory(stored: string) {
  try {
    const parsed: unknown = JSON.parse(stored);
    if (!Array.isArray(parsed) || !parsed.every((password) => typeof password === "string")) {
      return null;
    }
    return parsed.slice(0, PASSWORD_HISTORY_LIMIT);
  } catch {
    return null;
  }
}

function selectedGroupsFromCharacters(characters: string): SelectedGroups {
  return CHARACTER_GROUPS.reduce<SelectedGroups>((groups, group) => {
    groups[group.id] = group.value.split("").some((character) => characters.includes(character));
    return groups;
  }, { ...DEFAULT_GROUPS });
}

function rebuildCharacters(groups: SelectedGroups) {
  return CHARACTER_GROUPS.filter((group) => groups[group.id])
    .map((group) => group.value)
    .join("");
}

function randomIndex(max: number) {
  const limit = Math.floor(0x1_0000_0000 / max) * max;
  const value = new Uint32Array(1);
  do {
    crypto.getRandomValues(value);
  } while (value[0] >= limit);
  return value[0] % max;
}

function createPassword(length: number, characters: string, requiredGroups: readonly string[]) {
  const availableCharacters = Array.from(characters);
  const password = [
    ...requiredGroups.map((group) => group[randomIndex(group.length)]),
    ...Array.from(
      { length: length - requiredGroups.length },
      () => availableCharacters[randomIndex(availableCharacters.length)],
    ),
  ];

  for (let index = password.length - 1; index > 0; index -= 1) {
    const nextIndex = randomIndex(index + 1);
    [password[index], password[nextIndex]] = [password[nextIndex], password[index]];
  }

  return password.join("");
}

export function RandomPasswordTool() {
  const { t } = useTranslation();
  const { pushToast } = useToast();
  const [length, setLength] = useState("10");
  const [characters, setCharacters] = useState(DEFAULT_CHARACTERS);
  const [groups, setGroups] = useState<SelectedGroups>(DEFAULT_GROUPS);
  const [passwords, setPasswords] = useState<string[]>([]);
  const [history, setHistory] = useState<string[]>([]);
  const [historyLoading, setHistoryLoading] = useState(true);
  const [validationError, setValidationError] = useState("");
  const [copiedPassword, setCopiedPassword] = useState<string | null>(null);
  const historyRef = useRef<string[]>([]);
  const historyQueueRef = useRef<Promise<void>>(Promise.resolve());
  const historyVersionRef = useRef(0);
  const historyLoadDiscardedRef = useRef(false);
  const legacyHistoryPendingRef = useRef(false);

  const enqueueHistoryOperation = useCallback((operation: () => Promise<void>) => {
    const pending = historyQueueRef.current.then(operation);
    historyQueueRef.current = pending.catch(() => undefined);
    return pending;
  }, []);

  const groupLabels = useMemo<Record<CharacterGroupId, string>>(
    () => ({
      numbers: t("randomPasswordNumbers", "Numbers"),
      lowercase: t("randomPasswordLowercase", "Lowercase"),
      uppercase: t("randomPasswordUppercase", "Uppercase"),
      symbols: t("randomPasswordSymbols", "Common symbols"),
    }),
    [t],
  );

  const normalizedLength = () => {
    const parsed = Number(length);
    return Number.isFinite(parsed)
      ? Math.min(MAX_LENGTH, Math.max(MIN_LENGTH, Math.trunc(parsed)))
      : MIN_LENGTH;
  };

  const updateLength = (nextLength: number) => {
    setLength(String(Math.min(MAX_LENGTH, Math.max(MIN_LENGTH, nextLength))));
  };

  const toggleGroup = (groupId: CharacterGroupId) => {
    setGroups((currentGroups) => {
      const nextGroups = { ...currentGroups, [groupId]: !currentGroups[groupId] };
      setCharacters(rebuildCharacters(nextGroups));
      return nextGroups;
    });
    setValidationError("");
  };

  const updateCharacters = (nextCharacters: string) => {
    setCharacters(nextCharacters);
    setGroups(selectedGroupsFromCharacters(nextCharacters));
    setValidationError("");
  };

  useEffect(() => {
    let active = true;
    const loadVersion = historyVersionRef.current;

    const loadHistory = async () => {
      try {
        await enqueueHistoryOperation(async () => {
          try {
            const stored = await invoke<string | null>("get_secret", { key: PASSWORD_HISTORY_KEY });
            if (stored !== null) {
              const parsed = parsePasswordHistory(stored);
              if (parsed) {
                if (!historyLoadDiscardedRef.current) {
                  historyRef.current = parsed;
                  if (active && historyVersionRef.current === loadVersion) {
                    setHistory(parsed);
                  }
                }
                return;
              }

              if (historyVersionRef.current !== loadVersion) {
                return;
              }
              await invoke("delete_secret", { key: PASSWORD_HISTORY_KEY });
              if (active && historyVersionRef.current === loadVersion) {
                historyRef.current = [];
                setHistory([]);
                pushToast({
                  title: t("randomPasswordHistoryInvalid", "Copied password history was invalid and has been cleared."),
                  kind: "error",
                });
              }
              return;
            }

            const legacyHistory = localStorage.getItem(PASSWORD_HISTORY_KEY);
            if (legacyHistory === null) {
              return;
            }

            const parsed = parsePasswordHistory(legacyHistory);
            if (!parsed) {
              if (historyVersionRef.current !== loadVersion) {
                return;
              }
              localStorage.removeItem(PASSWORD_HISTORY_KEY);
              if (active) {
                pushToast({
                  title: t("randomPasswordHistoryInvalid", "Copied password history was invalid and has been cleared."),
                  kind: "error",
                });
              }
              return;
            }

            if (historyLoadDiscardedRef.current) {
              return;
            }
            historyRef.current = parsed;
            legacyHistoryPendingRef.current = true;
            if (historyVersionRef.current !== loadVersion) {
              return;
            }

            try {
              await invoke("save_secret", {
                key: PASSWORD_HISTORY_KEY,
                value: JSON.stringify(parsed),
              });
              localStorage.removeItem(PASSWORD_HISTORY_KEY);
              legacyHistoryPendingRef.current = false;
              if (active && historyVersionRef.current === loadVersion) {
                setHistory(parsed);
                pushToast({
                  title: t("randomPasswordHistoryMigrated", "Copied password history was moved to secure storage."),
                  kind: "success",
                });
              }
            } catch {
              if (active && historyVersionRef.current === loadVersion) {
                setHistory(parsed);
                pushToast({
                  title: t("randomPasswordHistoryMigrationFailed", "Unable to move copied password history to secure storage."),
                  kind: "error",
                });
              }
            }
          } catch {
            if (active && historyVersionRef.current === loadVersion) {
              pushToast({
                title: t("randomPasswordHistoryLoadFailed", "Unable to load copied password history."),
                kind: "error",
              });
            }
          } finally {
            if (active) {
              setHistoryLoading(false);
            }
          }
        });
      } catch {
        // 队列会吸收操作错误；此处仅满足异步调用约定。
      }
    };

    void loadHistory();
    return () => {
      active = false;
    };
  }, [enqueueHistoryOperation, pushToast, t]);

  const generatePasswords = () => {
    if (!characters.length) {
      setPasswords([]);
      setValidationError(t("randomPasswordCharactersRequired", "Choose at least one character before generating a password."));
      return;
    }

    const passwordLength = normalizedLength();
    const requiredGroups = CHARACTER_GROUPS.flatMap((group) => {
      const availableGroupCharacters = [...characters]
        .filter((character) => group.value.includes(character))
        .join("");
      return groups[group.id] && availableGroupCharacters ? [availableGroupCharacters] : [];
    });
    if (passwordLength < requiredGroups.length) {
      setPasswords([]);
      setValidationError(
        t("randomPasswordCoverageLengthRequired", {
          count: requiredGroups.length,
        }),
      );
      return;
    }

    setLength(String(passwordLength));
    setValidationError("");
    setPasswords(
      Array.from({ length: PASSWORD_BATCH_SIZE }, () =>
        createPassword(passwordLength, characters, requiredGroups),
      ),
    );
  };

  const copyPassword = async (password: string) => {
    try {
      await navigator.clipboard.writeText(password);
    } catch {
      pushToast({
        title: t("randomPasswordCopyFailed", "Unable to copy password"),
        kind: "error",
      });
      return;
    }

    historyVersionRef.current += 1;
    setCopiedPassword(password);
    void enqueueHistoryOperation(async () => {
      const nextHistory = [password, ...historyRef.current.filter((item) => item !== password)].slice(
        0,
        PASSWORD_HISTORY_LIMIT,
      );
      try {
        await invoke("save_secret", {
          key: PASSWORD_HISTORY_KEY,
          value: JSON.stringify(nextHistory),
        });
        if (legacyHistoryPendingRef.current) {
          localStorage.removeItem(PASSWORD_HISTORY_KEY);
          legacyHistoryPendingRef.current = false;
        }
        historyRef.current = nextHistory;
        setHistory(nextHistory);
        pushToast({
          title: t("randomPasswordCopied", "Password copied"),
          kind: "success",
        });
      } catch {
        pushToast({
          title: t("randomPasswordHistorySaveFailed", "Password copied, but history could not be saved."),
          kind: "error",
        });
      }
    });
  };

  const clearHistory = () => {
    historyVersionRef.current += 1;
    historyLoadDiscardedRef.current = true;
    void enqueueHistoryOperation(async () => {
      try {
        await invoke("delete_secret", { key: PASSWORD_HISTORY_KEY });
        try {
          localStorage.removeItem(PASSWORD_HISTORY_KEY);
        } catch {
          // 受保护存储已清除，旧浏览器存储清理失败不应阻断当前操作。
        }
        legacyHistoryPendingRef.current = false;
        historyRef.current = [];
        setHistory([]);
        pushToast({
          title: t("randomPasswordHistoryCleared", "Copied password history cleared."),
          kind: "success",
        });
      } catch {
        pushToast({
          title: t("randomPasswordHistoryClearFailed", "Unable to clear copied password history."),
          kind: "error",
        });
      }
    });
  };

  return (
    <section className="space-y-5 pb-5" aria-labelledby="random-password-title">
      <div className="flex items-start gap-3">
        <div className="rounded-lg bg-emerald-500/10 p-2 text-emerald-600">
          <KeyRound className="h-5 w-5" />
        </div>
        <div>
          <h2 id="random-password-title" className="text-lg font-semibold">
            {t("randomPassword", "Random Password")}
          </h2>
          <p className="text-sm text-muted-foreground">
            {t("randomPasswordToolDesc", "Generate passwords locally with the character groups you need.")}
          </p>
        </div>
      </div>

      <div className="grid gap-5 lg:grid-cols-[minmax(0,1fr)_minmax(18rem,0.8fr)]">
        <div className="space-y-4 rounded-lg border bg-card p-4">
          <div className="flex flex-wrap items-end gap-4">
            <label className="grid gap-1.5 text-sm font-medium" htmlFor="random-password-length">
              {t("randomPasswordLength", "Length")}
              <span className="flex h-10 overflow-hidden rounded-md border bg-background">
                <button
                  type="button"
                  className="inline-flex w-10 items-center justify-center border-r text-muted-foreground hover:bg-muted hover:text-foreground"
                  onClick={() => updateLength(normalizedLength() - 1)}
                  aria-label={t("randomPasswordDecreaseLength", "Decrease length")}
                  title={t("randomPasswordDecreaseLength", "Decrease length")}
                >
                  <Minus className="h-4 w-4" />
                </button>
                <input
                  id="random-password-length"
                  type="number"
                  min={MIN_LENGTH}
                  max={MAX_LENGTH}
                  value={length}
                  onChange={(event) => setLength(event.target.value)}
                  onBlur={() => setLength(String(normalizedLength()))}
                  className="w-16 bg-transparent text-center outline-none"
                />
                <button
                  type="button"
                  className="inline-flex w-10 items-center justify-center border-l text-muted-foreground hover:bg-muted hover:text-foreground"
                  onClick={() => updateLength(normalizedLength() + 1)}
                  aria-label={t("randomPasswordIncreaseLength", "Increase length")}
                  title={t("randomPasswordIncreaseLength", "Increase length")}
                >
                  <Plus className="h-4 w-4" />
                </button>
              </span>
            </label>
            <button
              type="button"
              onClick={generatePasswords}
              className="inline-flex h-10 items-center gap-2 rounded-md bg-primary px-4 text-sm font-medium text-primary-foreground hover:bg-primary/90"
            >
              <RefreshCw className="h-4 w-4" />
              {t("randomPasswordGenerate", "Generate")}
            </button>
          </div>

          <fieldset className="space-y-2">
            <legend className="text-sm font-medium">
              {t("randomPasswordCharacterGroups", "Character groups")}
            </legend>
            <div className="grid gap-2 sm:grid-cols-2">
              {CHARACTER_GROUPS.map((group) => (
                <label key={group.id} className="flex items-center gap-2 text-sm text-muted-foreground">
                  <input
                    type="checkbox"
                    checked={groups[group.id]}
                    onChange={() => toggleGroup(group.id)}
                    className="h-4 w-4 accent-primary"
                  />
                  {groupLabels[group.id]}
                </label>
              ))}
            </div>
          </fieldset>

          <label className="grid gap-1.5 text-sm font-medium" htmlFor="random-password-characters">
            {t("randomPasswordCharacters", "Characters used")}
            <input
              id="random-password-characters"
              value={characters}
              onChange={(event) => updateCharacters(event.target.value)}
              className="h-10 rounded-md border bg-background px-3 font-mono text-sm outline-none ring-offset-background focus-visible:ring-2 focus-visible:ring-ring"
              spellCheck={false}
            />
          </label>
          {validationError ? (
            <p role="alert" className="text-sm text-destructive">
              {validationError}
            </p>
          ) : null}
        </div>

        <div className="rounded-lg border bg-card p-4">
          <div className="flex items-center justify-between gap-3">
            <h3 className="text-sm font-semibold">{t("randomPasswordHistory", "Copied history")}</h3>
            {history.length || historyLoading ? (
              <button
                type="button"
                onClick={clearHistory}
                className="inline-flex h-8 items-center gap-1.5 rounded-md px-2 text-xs font-medium text-muted-foreground hover:bg-muted hover:text-foreground"
              >
                <Trash2 className="h-3.5 w-3.5" />
                {t("randomPasswordClearHistory", "Clear history")}
              </button>
            ) : null}
          </div>
          {historyLoading ? (
            <p className="mt-3 text-sm text-muted-foreground">
              {t("randomPasswordHistoryLoading", "Loading copied password history...")}
            </p>
          ) : history.length ? (
            <ul className="mt-3 max-h-60 space-y-2 overflow-y-auto font-mono text-xs">
              {history.map((password, index) => (
                <li key={`${password}-${index}`} className="break-all text-muted-foreground">
                  {password}
                </li>
              ))}
            </ul>
          ) : (
            <p className="mt-3 text-sm text-muted-foreground">
              {t("randomPasswordHistoryEmpty", "Copied passwords will appear here.")}
            </p>
          )}
        </div>
      </div>

      {passwords.length ? (
        <div className="grid gap-2 md:grid-cols-2 xl:grid-cols-3">
          {passwords.map((password, index) => (
            <div
              key={`${password}-${index}`}
              data-testid="generated-password-row"
              className="flex items-center gap-2 rounded-md border bg-card p-3"
            >
              <code data-testid="generated-password" className="min-w-0 flex-1 break-all text-sm">
                {password}
              </code>
              <button
                type="button"
                onClick={() => void copyPassword(password)}
                className="inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground"
                aria-label={t("randomPasswordCopy", "Copy password")}
                title={t("randomPasswordCopy", "Copy password")}
              >
                {copiedPassword === password ? <Check className="h-4 w-4 text-emerald-600" /> : <Copy className="h-4 w-4" />}
              </button>
            </div>
          ))}
        </div>
      ) : null}
    </section>
  );
}
