import React from 'react';
import bailianPng from '@/assets/provider-icons/bailian.png';
import tencentPng from '@/assets/provider-icons/tencent.png';
import baiduPng from '@/assets/provider-icons/baidu.png';
import volcenginePng from '@/assets/provider-icons/volcengine.png';
import doubaoPng from '@/assets/provider-icons/doubao.png';
import deepseekPng from '@/assets/provider-icons/deepseek.png';
import zhipuPng from '@/assets/provider-icons/zhipu.png';
import kimiIco from '@/assets/provider-icons/kimi.ico';
import minimaxPng from '@/assets/provider-icons/minimax.png';
import stepfunSvg from '@/assets/provider-icons/stepfun.svg';
import xfyunIco from '@/assets/provider-icons/xfyun.ico';
import sensenovaPng from '@/assets/provider-icons/sensenova.png';
import lingyiPng from '@/assets/provider-icons/lingyi.png';

export const ClaudeIcon = ({ className, ...props }: React.SVGProps<SVGSVGElement>) => (
  <svg
    viewBox="0 0 24 24"
    fill="currentColor"
    className={className}
    {...props}
  >
    <path d="M17.3041 3.541h-3.6718l6.696 16.918H24Zm-10.6082 0L0 20.459h3.7442l1.3693-3.5527h7.0052l1.3693 3.5528h3.7442L10.5363 3.5409Zm-.3712 10.2232 2.2914-5.9456 2.2914 5.9456Z" />
  </svg>
);

export const OpenAIIcon = ({ className, ...props }: React.SVGProps<SVGSVGElement>) => (
  <svg
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="2"
    strokeLinecap="round"
    strokeLinejoin="round"
    className={className}
    {...props}
  >
    <path d="M11.217 19.384a3.501 3.501 0 0 0 6.783 -1.217v-5.167l-6 -3.35" />
    <path d="M5.214 15.014a3.501 3.501 0 0 0 4.446 5.266l4.34 -2.534v-6.946" />
    <path d="M6 7.63c-1.391 -.236 -2.787 .395 -3.534 1.689a3.474 3.474 0 0 0 1.271 4.745l4.263 2.514l6 -3.348" />
    <path d="M12.783 4.616a3.501 3.501 0 0 0 -6.783 1.217v5.067l6 3.45" />
    <path d="M18.786 8.986a3.501 3.501 0 0 0 -4.446 -5.266l-4.34 2.534v6.946" />
    <path d="M18 16.302c1.391 .236 2.787 -.395 3.534 -1.689a3.474 3.474 0 0 0 -1.271 -4.745l-4.308 -2.514l-5.955 3.42" />
  </svg>
);

export const GeminiIcon = ({ className, ...props }: React.SVGProps<SVGSVGElement>) => (
  <svg
    viewBox="0 0 24 24"
    fill="currentColor"
    className={className}
    {...props}
  >
    <path d="M11.04 19.32Q12 21.51 12 24q0-2.49.93-4.68.96-2.19 2.58-3.81t3.81-2.55Q21.51 12 24 12q-2.49 0-4.68-.93a12.3 12.3 0 0 1-3.81-2.58 12.3 12.3 0 0 1-2.58-3.81Q12 2.49 12 0q0 2.49-.96 4.68-.93 2.19-2.55 3.81a12.3 12.3 0 0 1-3.81 2.58Q2.49 12 0 12q2.49 0 4.68.96 2.19.93 3.81 2.55t2.55 3.81"/>
  </svg>
);

export const OpenCodeIcon = ({ className, ...props }: React.SVGProps<SVGSVGElement>) => (
  <svg
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="2"
    strokeLinecap="round"
    strokeLinejoin="round"
    className={className}
    {...props}
  >
    <polyline points="16 18 22 12 16 6" />
    <polyline points="8 6 2 12 8 18" />
    <line x1="12" y1="2" x2="12" y2="22" />
  </svg>
);

function createImageIcon(src: string, alt: string) {
  return ({ className }: { className?: string }) => (
    <img
      src={src}
      alt={alt}
      className={className}
      draggable={false}
    />
  );
}

export const BailianIcon = createImageIcon(bailianPng, '阿里百炼');
export const TencentIcon = createImageIcon(tencentPng, '腾讯混元');
export const BaiduIcon = createImageIcon(baiduPng, '百度');
export const VolcengineIcon = createImageIcon(volcenginePng, '火山引擎');
export const DoubaoIcon = createImageIcon(doubaoPng, '豆包');
export const DeepSeekIcon = createImageIcon(deepseekPng, 'DeepSeek');
export const ZhipuIcon = createImageIcon(zhipuPng, '智谱');
export const KimiIcon = createImageIcon(kimiIco, 'Kimi');
export const MiniMaxIcon = createImageIcon(minimaxPng, 'MiniMax');
export const StepFunIcon = createImageIcon(stepfunSvg, '阶跃星辰');
export const XFYunIcon = createImageIcon(xfyunIco, '讯飞星火');
export const SenseNovaIcon = createImageIcon(sensenovaPng, '商汤日日新');
export const LingyiIcon = createImageIcon(lingyiPng, '零一万物');

export const BUILTIN_PROVIDER_ICON_MAP = {
  'builtin:claude': ClaudeIcon,
  'builtin:chatgpt': OpenAIIcon,
  'builtin:gemini': GeminiIcon,
  'builtin:opencode': OpenCodeIcon,
  'builtin:bailian': BailianIcon,
  'builtin:tencent': TencentIcon,
  'builtin:baidu': BaiduIcon,
  'builtin:volcengine': VolcengineIcon,
  'builtin:doubao': DoubaoIcon,
  'builtin:deepseek': DeepSeekIcon,
  'builtin:zhipu': ZhipuIcon,
  'builtin:kimi': KimiIcon,
  'builtin:minimax': MiniMaxIcon,
  'builtin:stepfun': StepFunIcon,
  'builtin:xfyun': XFYunIcon,
  'builtin:sensenova': SenseNovaIcon,
  'builtin:lingyi': LingyiIcon,
} as const;

export type BuiltinProviderIconKey = keyof typeof BUILTIN_PROVIDER_ICON_MAP;

export function isBuiltinProviderIcon(icon?: string): icon is BuiltinProviderIconKey {
  return !!icon && icon in BUILTIN_PROVIDER_ICON_MAP;
}

export function BuiltinProviderIcon({
  icon,
  className,
}: {
  icon: BuiltinProviderIconKey;
  className?: string;
}) {
  const IconComponent = BUILTIN_PROVIDER_ICON_MAP[icon];
  return <IconComponent className={className} />;
}

const TOOL_ICON_MAP = {
  claude: ClaudeIcon,
  codex: OpenAIIcon,
  gemini: GeminiIcon,
  opencode: OpenCodeIcon,
} as const;

type ToolKey = keyof typeof TOOL_ICON_MAP;

interface ToolAvatarIconProps {
  tool: string;
  className?: string;
}

/**
 * 根据工具类型返回对应的 SVG 图标组件。
 * 用于 provider 列表头像（Claude profile / Codex / Gemini / OpenCode）以及同步设备列表。
 */
export const ToolAvatarIcon = ({ tool, className }: ToolAvatarIconProps) => {
  const IconComponent = TOOL_ICON_MAP[tool as ToolKey];
  if (!IconComponent) return null;
  return <IconComponent className={className} />;
};
