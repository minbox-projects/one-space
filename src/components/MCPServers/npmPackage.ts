import type { MCPServer } from './types';

export function parseNpmPackageSpec(token: string): { packageName: string; version?: string } | null {
  const trimmed = token.trim();
  if (!trimmed || trimmed.startsWith('-')) return null;

  if (trimmed.startsWith('@')) {
    const slashIdx = trimmed.indexOf('/');
    if (slashIdx <= 0 || slashIdx >= trimmed.length - 1) return null;
    const versionIdx = trimmed.lastIndexOf('@');
    if (versionIdx > slashIdx + 1) {
      const packageName = trimmed.slice(0, versionIdx);
      const version = trimmed.slice(versionIdx + 1).trim();
      if (!version) return null;
      return { packageName, version };
    }
    return { packageName: trimmed };
  }

  const versionIdx = trimmed.lastIndexOf('@');
  if (versionIdx > 0) {
    const packageName = trimmed.slice(0, versionIdx).trim();
    const version = trimmed.slice(versionIdx + 1).trim();
    if (!packageName || !version) return null;
    return { packageName, version };
  }

  return { packageName: trimmed };
}

export function parseNpxSpecFromServer(server: MCPServer): { packageName: string; version?: string } | null {
  if (server.transport !== 'stdio') return null;
  if ((server.command || '').trim().toLowerCase() !== 'npx') return null;
  const args = server.args || [];
  if (args.length === 0) return null;
  let idx = 0;
  for (; idx < args.length; idx += 1) {
    const arg = args[idx];
    if (arg === '--') {
      idx += 1;
      break;
    }
    if (!arg.startsWith('-')) break;
  }
  if (idx >= args.length) return null;
  return parseNpmPackageSpec(args[idx]);
}
