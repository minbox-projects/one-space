import type { SVGProps } from "react";

export interface TelegramIconProps extends SVGProps<SVGSVGElement> {
  size?: number | string;
  strokeWidth?: number | string;
}

/**
 * 统一的 Telegram Outline 线性风格矢量图标
 * 严格对齐 Lucide 图标体系的 24x24 描边规范（2px 描边、通透镂空、圆角拐角），与其他顶栏及设置项图标完美协调
 */
export function TelegramIcon({
  size = 20,
  className = "w-5 h-5",
  width,
  height,
  strokeWidth = 2,
  ...props
}: TelegramIconProps) {
  return (
    <svg
      viewBox="0 0 24 24"
      width={width ?? size}
      height={height ?? size}
      fill="none"
      stroke="currentColor"
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
