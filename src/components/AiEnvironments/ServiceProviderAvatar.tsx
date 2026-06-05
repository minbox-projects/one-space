import { BuiltinProviderIcon, isBuiltinProviderIcon, ToolAvatarIcon } from './icons';

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
  const builtinIcon = isBuiltinProviderIcon(icon) ? icon : null;
  const isTextIcon = !!icon && icon.trim().length > 0 && !builtinIcon;

  return (
    <div
      className="inline-flex shrink-0 items-center justify-center overflow-hidden border text-slate-800 shadow-sm"
      style={{
        width: size,
        height: size,
        borderRadius: radius,
        userSelect: 'none',
        borderColor: 'rgba(203, 213, 225, 0.9)',
        background: isTextIcon
          ? 'linear-gradient(180deg, #f8fafc 0%, #eef2ff 100%)'
          : 'linear-gradient(180deg, #ffffff 0%, #f8fafc 100%)',
        boxShadow: '0 8px 20px rgba(15, 23, 42, 0.06), inset 0 1px 0 rgba(255,255,255,0.95)',
      }}
      title={name || id}
    >
      {builtinIcon ? (
        <BuiltinProviderIcon icon={builtinIcon} className="h-[72%] w-[72%] object-contain" />
      ) : icon && icon.trim().length > 0 ? (
        <span
          style={{
            fontSize: size * 0.4,
            fontWeight: 700,
            lineHeight: 1,
            color: '#1f2937',
            letterSpacing: icon.length <= 2 ? '-0.03em' : '0',
          }}
        >
          {fallback}
        </span>
      ) : tool ? (
        <ToolAvatarIcon tool={tool} className="h-[60%] w-[60%]" />
      ) : (
        <span
          style={{
            fontSize: size * 0.4,
            fontWeight: 700,
            lineHeight: 1,
            color: '#1f2937',
          }}
        >
          {fallback}
        </span>
      )}
    </div>
  );
}
