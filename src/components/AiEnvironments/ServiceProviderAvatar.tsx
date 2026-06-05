// ServiceProviderAvatar component

interface ServiceProviderAvatarProps {
  icon?: string;
  name: string;
  id: string;
  size?: number;
}

// Stable color generation based on ID hash
function getIdColor(id: string): string {
  const colors = [
    '#6366f1', '#8b5cf6', '#a855f7', '#d946ef',
    '#ec4899', '#f43f5e', '#ef4444', '#f97316',
    '#eab308', '#22c55e', '#14b8a6', '#06b6d4',
    '#3b82f6', '#2563eb', '#4f46e5', '#7c3aed',
  ];
  let hash = 0;
  for (let i = 0; i < id.length; i++) {
    hash = id.charCodeAt(i) + ((hash << 5) - hash);
  }
  return colors[Math.abs(hash) % colors.length];
}

function getFallback(name: string): string {
  // Try first non-empty character (letter or CJK)
  for (const ch of name) {
    if (ch.trim().length > 0) return ch;
  }
  return '?';
}

export function ServiceProviderAvatar({ icon, name, id, size = 32 }: ServiceProviderAvatarProps) {
  const bgColor = getIdColor(id);
  const display = icon && icon.trim().length > 0 ? icon : getFallback(name);

  return (
    <div
      style={{
        width: size,
        height: size,
        borderRadius: '50%',
        backgroundColor: bgColor,
        color: '#fff',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        fontSize: size * 0.45,
        fontWeight: 600,
        flexShrink: 0,
        userSelect: 'none',
      }}
      title={name}
    >
      {display}
    </div>
  );
}
