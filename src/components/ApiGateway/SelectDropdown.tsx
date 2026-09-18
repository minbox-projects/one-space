import { useEffect, useRef, useState, type ReactNode } from "react";
import { ChevronDown } from "lucide-react";

export interface SelectDropdownOption<T extends string | number> {
  value: T;
  label: string;
}

export interface SelectDropdownProps<T extends string | number> {
  value: T;
  options: SelectDropdownOption<T>[];
  onChange: (value: T) => void;
  ariaLabel?: string;
  testId?: string;
  className?: string;
  buttonClassName?: string;
  menuClassName?: string;
  align?: "left" | "right";
  icon?: ReactNode;
}

export function SelectDropdown<T extends string | number>({
  value,
  options,
  onChange,
  ariaLabel,
  testId,
  className = "",
  buttonClassName = "",
  menuClassName = "",
  align = "left",
  icon,
}: SelectDropdownProps<T>) {
  const [open, setOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  const currentOption = options.find((opt) => opt.value === value);
  const displayLabel = currentOption ? currentOption.label : String(value);

  useEffect(() => {
    if (!open) return;

    function handleClickOutside(event: MouseEvent) {
      if (
        containerRef.current &&
        !containerRef.current.contains(event.target as Node)
      ) {
        setOpen(false);
      }
    }

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        setOpen(false);
      }
    }

    document.addEventListener("mousedown", handleClickOutside);
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("mousedown", handleClickOutside);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [open]);

  return (
    <div className={`relative inline-block text-left ${className}`} ref={containerRef}>
      <button
        type="button"
        data-testid={testId ? `${testId}-trigger` : undefined}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={ariaLabel || displayLabel}
        onClick={() => setOpen((prev) => !prev)}
        className={`inline-flex h-8 items-center gap-1.5 rounded-lg border bg-background px-2.5 text-xs font-medium transition hover:bg-muted focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring ${buttonClassName}`}
      >
        {icon}
        <span>{displayLabel}</span>
        <ChevronDown
          className={`h-3.5 w-3.5 text-muted-foreground transition-transform duration-200 ${
            open ? "rotate-180" : ""
          }`}
        />
      </button>

      {open ? (
        <div
          role="listbox"
          data-testid={testId ? `${testId}-menu` : undefined}
          className={`absolute top-full z-30 mt-1 min-w-[120px] rounded-lg border bg-popover p-1 text-popover-foreground shadow-md animate-in fade-in-0 zoom-in-95 ${
            align === "right" ? "right-0" : "left-0"
          } ${menuClassName}`}
        >
          {options.map((option) => {
            const isSelected = option.value === value;
            return (
              <button
                key={String(option.value)}
                type="button"
                role="option"
                aria-selected={isSelected}
                onClick={() => {
                  onChange(option.value);
                  setOpen(false);
                }}
                className={`flex w-full items-center justify-between rounded-md px-2.5 py-1.5 text-xs transition-colors ${
                  isSelected
                    ? "bg-accent font-medium text-accent-foreground"
                    : "text-muted-foreground hover:bg-muted hover:text-foreground"
                }`}
              >
                <span>{option.label}</span>
              </button>
            );
          })}
        </div>
      ) : null}
    </div>
  );
}
