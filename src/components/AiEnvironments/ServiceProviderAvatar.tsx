import { ToolAvatarIcon } from './icons';

interface ServiceProviderAvatarProps {
  icon?: string;
  name: string;
  id: string;
  tool?: string;
  size?: number;
}

function getFallback(name: string): string {
  for (const ch of name) {
    if (ch.trim().length > 0) return ch;
  }
  return '?';
}

export function ServiceProviderAvatar({
  icon,
  name,
  id,
  tool,
  size = 32,
}: ServiceProviderAvatarProps) {
  const fallback = icon && icon.trim().length > 0 ? icon : getFallback(name);
  const radius = Math.max(8, Math.round(size * 0.28));

  return (
    <div
      className="inline-flex shrink-0 items-center justify-center bg-[#111827] text-white"
      style={{
        width: size,
        height: size,
        borderRadius: radius,
        userSelect: 'none',
      }}
      title={name || id}
    >
      {tool ? (
        <ToolAvatarIcon tool={tool} className="h-[55%] w-[55%]" />
      ) : (
        <span
          style={{
            fontSize: size * 0.42,
            fontWeight: 700,
            lineHeight: 1,
          }}
        >
          {fallback}
        </span>
      )}
    </div>
  );
}
