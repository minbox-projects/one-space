import { forwardRef } from "react";
import type { LucideProps } from "lucide-react";

export type TelegramIconProps = LucideProps;

/**
 * 统一的 Telegram Outline 线性风格矢量图标
 * 严格对齐 Lucide 图标体系规范与 LucideIcon 类型定义（24x24 描边网格、2px 描边、通透镂空、圆角拐角、forwardRef 支持）
 */
export const TelegramIcon = forwardRef<SVGSVGElement, LucideProps>(
  (
    {
      color = "currentColor",
      size = 20,
      strokeWidth = 2,
      className = "w-5 h-5",
      ...props
    },
    ref
  ) => {
    return (
      <svg
        ref={ref}
        viewBox="0 0 24 24"
        width={size}
        height={size}
        fill="none"
        stroke={color}
        strokeWidth={strokeWidth}
        strokeLinecap="round"
        strokeLinejoin="round"
        className={className}
        xmlns="http://www.w3.org/2000/svg"
        {...props}
      >
        <path d="M15 10l-4 4l6 6l4 -16l-18 7l4 2l2 6l3 -4" />
      </svg>
    );
  }
);

TelegramIcon.displayName = "TelegramIcon";
