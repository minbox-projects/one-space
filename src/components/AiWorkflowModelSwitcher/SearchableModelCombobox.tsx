import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type FC,
  type KeyboardEvent,
} from "react";
import { Check, ChevronDown, Search, X } from "lucide-react";
import { useTranslation } from "react-i18next";

export interface SearchableModelComboboxProps {
  value: string;
  onChange: (value: string) => void;
  candidates: string[];
  placeholder?: string;
  testId?: string;
  className?: string;
  inputClassName?: string;
  autoFocus?: boolean;
}

export const SearchableModelCombobox: FC<SearchableModelComboboxProps> = ({
  value,
  onChange,
  candidates,
  placeholder,
  testId,
  className = "",
  inputClassName = "",
  autoFocus = false,
}) => {
  const { t } = useTranslation();
  const [isOpen, setIsOpen] = useState(false);
  const [query, setQuery] = useState(value || "");
  const containerRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  // 同步外部 value
  useEffect(() => {
    setQuery(value || "");
  }, [value]);

  // 点击外部关闭下拉列表
  useEffect(() => {
    const handlePointerDown = (event: MouseEvent) => {
      if (
        containerRef.current &&
        !containerRef.current.contains(event.target as Node)
      ) {
        setIsOpen(false);
      }
    };
    document.addEventListener("mousedown", handlePointerDown);
    return () => document.removeEventListener("mousedown", handlePointerDown);
  }, []);

  // 模糊检索过滤候选模型
  const filteredCandidates = useMemo(() => {
    const trimmed = query.trim().toLowerCase();
    if (!trimmed) {
      return candidates;
    }
    return candidates.filter((item) => item.toLowerCase().includes(trimmed));
  }, [candidates, query]);

  const handleSelect = (selectedModel: string) => {
    setQuery(selectedModel);
    onChange(selectedModel);
    setIsOpen(false);
  };

  const handleInputChange = (newVal: string) => {
    setQuery(newVal);
    onChange(newVal);
    if (!isOpen) {
      setIsOpen(true);
    }
  };

  const handleClear = () => {
    setQuery("");
    onChange("");
    inputRef.current?.focus();
  };

  const handleKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Escape") {
      setIsOpen(false);
    } else if (e.key === "Enter") {
      if (filteredCandidates.length > 0 && isOpen) {
        handleSelect(filteredCandidates[0]);
      } else {
        setIsOpen(false);
      }
    }
  };

  const isExactMatch = candidates.includes(query.trim());

  return (
    <div ref={containerRef} className={`relative w-full ${className}`}>
      <div className="relative flex items-center">
        <Search className="pointer-events-none absolute left-2 h-3.5 w-3.5 text-muted-foreground opacity-60" />
        <input
          ref={inputRef}
          type="text"
          data-testid={testId}
          value={query}
          autoFocus={autoFocus}
          onFocus={() => setIsOpen(true)}
          onChange={(e) => handleInputChange(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder={placeholder || t("aiWorkflow.searchOrEnterModel", "搜索或输入模型...")}
          className={`h-7 w-full rounded border bg-background pl-7 pr-12 font-mono text-xs focus:outline-none focus:ring-1 focus:ring-primary ${inputClassName}`}
        />
        <div className="absolute right-1 flex items-center gap-0.5">
          {query ? (
            <button
              type="button"
              tabIndex={-1}
              onClick={handleClear}
              className="rounded p-0.5 text-muted-foreground hover:text-foreground"
              title={t("aiWorkflow.clear", "清空")}
            >
              <X className="h-3 w-3" />
            </button>
          ) : null}
          <button
            type="button"
            tabIndex={-1}
            onClick={() => setIsOpen((prev) => !prev)}
            className="rounded p-0.5 text-muted-foreground hover:text-foreground"
          >
            <ChevronDown className="h-3.5 w-3.5 opacity-60" />
          </button>
        </div>
      </div>

      {isOpen && (
        <div
          role="listbox"
          data-testid={testId ? `${testId}-listbox` : undefined}
          className="absolute left-0 top-full z-50 mt-1 max-h-48 w-full min-w-[200px] overflow-y-auto rounded-md border bg-popover p-1 shadow-lg"
        >
          {query.trim() && !isExactMatch && (
            <div
              role="option"
              aria-selected={false}
              onClick={() => handleSelect(query.trim())}
              className="flex cursor-pointer items-center justify-between rounded px-2 py-1.5 font-mono text-xs text-primary hover:bg-muted"
            >
              <span className="truncate">
                {t("aiWorkflow.customModelLabel", "使用自定义模型")}: <strong>{query.trim()}</strong>
              </span>
            </div>
          )}

          {filteredCandidates.length > 0 ? (
            filteredCandidates.map((model) => {
              const isSelected = value === model;
              return (
                <div
                  key={model}
                  role="option"
                  aria-selected={isSelected}
                  data-testid={testId ? `${testId}-option-${model}` : undefined}
                  onClick={() => handleSelect(model)}
                  className={`flex cursor-pointer items-center justify-between rounded px-2 py-1.5 font-mono text-xs transition-colors ${
                    isSelected
                      ? "bg-primary text-primary-foreground font-semibold"
                      : "hover:bg-muted text-foreground"
                  }`}
                >
                  <span className="truncate">{model}</span>
                  {isSelected && <Check className="ml-2 h-3.5 w-3.5 shrink-0" />}
                </div>
              );
            })
          ) : (
            <div className="px-2 py-2 text-center text-[11px] text-muted-foreground">
              {t("aiWorkflow.noMatchingModels", "无匹配候选模型（支持自定义输入）")}
            </div>
          )}
        </div>
      )}
    </div>
  );
};
