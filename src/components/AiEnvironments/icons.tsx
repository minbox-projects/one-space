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

export const AntigravityIcon = ({ className, ...props }: React.SVGProps<SVGSVGElement>) => (
  <svg
    viewBox="0 0 24 24"
    fill="currentColor"
    fillRule="evenodd"
    clipRule="evenodd"
    className={className}
    {...props}
  >
    <path d="M21.751 22.607c1.34 1.005 3.35.335 1.508-1.508C17.73 15.74 18.904 1 12.037 1 5.17 1 6.342 15.74.815 21.1c-2.01 2.009.167 2.511 1.507 1.506 5.192-3.517 4.857-9.714 9.715-9.714 4.857 0 4.522 6.197 9.714 9.715z" />
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

export const CommandCodeIcon = ({ className, ...props }: React.SVGProps<SVGSVGElement>) => (
  <svg
    viewBox="0 0 700 700"
    fill="currentColor"
    className={className}
    {...props}
  >
    <g transform="translate(0.000000,700.000000) scale(0.100000,-0.100000)" stroke="none">
      <path d="M2305 6994 c-371 -13 -682 -39 -893 -74 -598 -103 -963 -350 -1172 -795 -126 -267 -186 -576 -222 -1130 -19 -287 -18 -2669 0 -2950 53 -794 172 -1175 464 -1481 286 -298 672 -437 1367 -489 616 -47 2694 -46 3267 0 685 56 1056 186 1339 470 289 289 424 685 475 1395 22 310 32 1055 27 1915 -6 868 -13 1102 -42 1417 -55 589 -188 950 -448 1216 -305 311 -678 435 -1487 493 -141 10 -2428 21 -2675 13z m33 -1350 c322 -66 580 -324 646 -646 12 -57 16 -136 16 -303 l0 -225 474 0 474 0 5 243 c4 201 8 255 25 318 67 248 222 437 447 545 138 67 195 79 370 79 143 -1 154 -2 245 -33 279 -96 476 -307 552 -587 29 -111 29 -297 -1 -410 -69 -261 -263 -478 -511 -572 -113 -43 -194 -53 -431 -53 l-219 0 0 -474 0 -474 238 -5 c253 -5 310 -14 432 -64 243 -99 438 -327 495 -576 47 -209 17 -419 -88 -607 -57 -102 -205 -250 -307 -307 -433 -242 -960 -71 -1169 379 -63 134 -74 200 -79 466 l-4 232 -474 0 -474 0 0 -219 c0 -149 -5 -243 -14 -294 -60 -312 -299 -565 -611 -649 -112 -30 -298 -30 -410 0 -519 139 -776 708 -534 1185 106 210 301 365 538 429 63 17 117 21 319 25 l242 5 0 474 0 474 -225 0 c-252 0 -329 11 -452 62 -485 201 -664 796 -372 1232 116 174 310 306 514 349 93 20 248 21 343 1z" />
      <path d="M2080 5174 c-187 -50 -302 -241 -256 -425 31 -119 118 -216 231 -256 42 -15 84 -18 260 -18 l210 0 0 210 c0 176 -3 218 -18 260 -39 111 -136 200 -252 230 -72 18 -103 18 -175 -1z" />
      <path d="M4705 5176 c-75 -19 -125 -49 -178 -105 -86 -92 -91 -112 -95 -373 l-3 -228 185 0 c264 0 337 20 429 119 74 79 92 127 92 241 0 83 -3 102 -27 150 -74 150 -249 236 -403 196z" />
      <path d="M3000 3525 l0 -475 475 0 475 0 0 475 0 475 -475 0 -475 0 0 -475z" />
      <path d="M2051 2550 c-59 -22 -68 -27 -129 -84 -178 -167 -127 -463 98 -574 48 -24 67 -27 150 -27 114 0 162 18 241 92 99 92 119 165 119 428 l0 185 -212 0 c-184 -1 -220 -3 -267 -20z" />
      <path d="M4432 2348 l3 -224 33 -66 c38 -77 92 -130 171 -167 48 -22 70 -26 146 -26 82 0 97 3 157 33 77 38 130 92 167 171 22 47 26 70 26 146 0 76 -4 99 -26 146 -37 79 -90 133 -167 171 l-66 33 -224 3 -223 3 3 -223z" />
    </g>
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
  'builtin:antigravity': AntigravityIcon,
  'builtin:opencode': OpenCodeIcon,
  'builtin:commandcode': CommandCodeIcon,
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

const PROVIDER_ICON_KEYWORDS: Array<{ icon: BuiltinProviderIconKey; keywords: string[] }> = [
  { icon: 'builtin:claude', keywords: ['claude', 'anthropic'] },
  { icon: 'builtin:chatgpt', keywords: ['chatgpt', 'openai', 'gpt'] },
  { icon: 'builtin:antigravity', keywords: ['antigravity', 'google'] },
  { icon: 'builtin:opencode', keywords: ['opencode'] },
  { icon: 'builtin:commandcode', keywords: ['commandcode', 'command'] },
  { icon: 'builtin:bailian', keywords: ['bailian', '百炼', '阿里百炼'] },
  { icon: 'builtin:tencent', keywords: ['tencent', '腾讯', 'hunyuan', '混元'] },
  { icon: 'builtin:baidu', keywords: ['baidu', '百度', 'qianfan', '千帆', 'wenxin', '文心'] },
  { icon: 'builtin:volcengine', keywords: ['volcengine', '火山引擎'] },
  { icon: 'builtin:doubao', keywords: ['doubao', '豆包'] },
  { icon: 'builtin:deepseek', keywords: ['deepseek'] },
  { icon: 'builtin:zhipu', keywords: ['zhipu', '智谱', 'glm'] },
  { icon: 'builtin:kimi', keywords: ['kimi', 'moonshot'] },
  { icon: 'builtin:minimax', keywords: ['minimax'] },
  { icon: 'builtin:stepfun', keywords: ['stepfun', '阶跃星辰', 'step'] },
  { icon: 'builtin:xfyun', keywords: ['xfyun', '讯飞', 'spark', '星火'] },
  { icon: 'builtin:sensenova', keywords: ['sensenova', '商汤', '日日新'] },
  { icon: 'builtin:lingyi', keywords: ['lingyi', '零一万物', 'yi-'] },
];

function normalizeProviderIconSource(value?: string | null): string {
  return String(value || '').trim().toLowerCase();
}

export function resolveBuiltinProviderIcon(input: {
  icon?: string | null;
  name?: string | null;
  id?: string | null;
  tool?: string | null;
}): BuiltinProviderIconKey | null {
  const explicitIcon = normalizeProviderIconSource(input.icon);
  if (explicitIcon) {
    if (isBuiltinProviderIcon(explicitIcon)) {
      return explicitIcon;
    }
    const prefixed = `builtin:${explicitIcon}` as BuiltinProviderIconKey;
    if (isBuiltinProviderIcon(prefixed)) {
      return prefixed;
    }
    if (explicitIcon === 'openai' || explicitIcon === 'chatgpt') {
      return 'builtin:chatgpt';
    }
  }

  const candidates = [
    explicitIcon,
    normalizeProviderIconSource(input.name),
    normalizeProviderIconSource(input.id),
    normalizeProviderIconSource(input.tool),
  ].filter(Boolean);

  for (const candidate of candidates) {
    const matched = PROVIDER_ICON_KEYWORDS.find(({ keywords }) =>
      keywords.some((keyword) => candidate.includes(keyword)),
    );
    if (matched) return matched.icon;
  }

  return null;
}

const TOOL_ICON_MAP = {
  claude: ClaudeIcon,
  codex: OpenAIIcon,
  antigravity: AntigravityIcon,
  opencode: OpenCodeIcon,
} as const;

type ToolKey = keyof typeof TOOL_ICON_MAP;

interface ToolAvatarIconProps {
  tool: string;
  className?: string;
}

/**
 * 根据工具类型返回对应的 SVG 图标组件。
 * 用于 provider 列表头像（Claude profile / Codex / Antigravity / OpenCode）以及同步设备列表。
 */
export const ToolAvatarIcon = ({ tool, className }: ToolAvatarIconProps) => {
  const IconComponent = TOOL_ICON_MAP[tool as ToolKey];
  if (!IconComponent) return null;
  return <IconComponent className={className} />;
};
