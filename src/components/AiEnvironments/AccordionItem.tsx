import { ChevronDown } from 'lucide-react';
import { type ReactNode } from 'react';

interface AccordionItemProps {
  id: string;
  isOpen: boolean;
  onToggle: (id: string) => void;
  chevron?: ReactNode;
  avatar?: ReactNode;
  nameRow?: ReactNode;
  badges?: ReactNode;
  meta?: ReactNode;
  actions?: ReactNode;
  panel: ReactNode;
  /** 是否为 Claude Profile 使用新的紧凑布局 */
  compact?: boolean;
}

export function AccordionItem({
  id,
  isOpen,
  onToggle,
  chevron,
  avatar,
  nameRow,
  badges,
  meta,
  actions,
  panel,
  compact,
}: AccordionItemProps) {
  if (compact) {
    return (
      <div className={`acc-item ${isOpen ? 'open' : ''}`}>
        <button
          type="button"
          className="acc-row w-full text-left"
          onClick={() => onToggle(id)}
        >
          {chevron ?? (
            <ChevronDown
              className="acc-chevron"
              strokeWidth={1.5}
              strokeLinecap="round"
            />
          )}
          {avatar && <div className="shrink-0">{avatar}</div>}
          {/* 紧凑布局：info 区域纵向排列 name / badges / meta */}
          {nameRow && (
            <div className="acc-info">
              <div className="acc-name-row">{nameRow}</div>
              {badges && <div className="acc-badges">{badges}</div>}
              {meta && <div className="acc-meta">{meta}</div>}
            </div>
          )}
          <div className="flex-1" />
          {actions && <div className="acc-actions">{actions}</div>}
        </button>
        <div className="acc-panel">
          {isOpen && (
            <div className="bg-card border rounded-lg p-5 mx-4 mb-3">
              {panel}
            </div>
          )}
        </div>
      </div>
    );
  }

  // 默认布局（保持向后兼容）
  return (
    <div className={`acc-item ${isOpen ? 'open' : ''}`}>
      <button
        type="button"
        className="acc-row w-full text-left flex items-center px-4 py-3.5 gap-3.5 min-h-[60px] transition-colors hover:bg-muted/30"
        onClick={() => onToggle(id)}
      >
        {chevron ?? (
          <ChevronDown
            className={`w-4 h-4 text-muted-foreground shrink-0 transition-transform duration-200 ${
              isOpen ? 'rotate-180' : ''
            }`}
          />
        )}
        {avatar && <div className="shrink-0">{avatar}</div>}
        {nameRow && <div className="min-w-0 flex-1">{nameRow}</div>}
        {badges && <div className="flex items-center gap-1.5 shrink-0">{badges}</div>}
        {meta && <div className="text-xs text-muted-foreground shrink-0">{meta}</div>}
        <div className="flex-1" />
        {actions && <div className="flex items-center gap-1.5 shrink-0">{actions}</div>}
      </button>
      <div className="acc-panel">
        {isOpen && (
          <div className="bg-card border rounded-lg p-5 mx-4 mb-3">
            {panel}
          </div>
        )}
      </div>
    </div>
  );
}
