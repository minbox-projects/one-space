import type { SupportedRole } from "@/lib/aiWorkflowProfiles";

export const ROLE_BADGE_STYLES: Record<
  SupportedRole,
  { badge: string; dot: string }
> = {
  backend: {
    badge: "border-sky-500/25 bg-sky-500/10 text-sky-700 dark:text-sky-400",
    dot: "bg-sky-500",
  },
  frontend: {
    badge:
      "border-violet-500/25 bg-violet-500/10 text-violet-700 dark:text-violet-400",
    dot: "bg-violet-500",
  },
  test: {
    badge:
      "border-emerald-500/25 bg-emerald-500/10 text-emerald-700 dark:text-emerald-400",
    dot: "bg-emerald-500",
  },
  "documentation-maintainer": {
    badge:
      "border-amber-500/25 bg-amber-500/10 text-amber-700 dark:text-amber-400",
    dot: "bg-amber-500",
  },
  "file-explorer": {
    badge: "border-cyan-500/25 bg-cyan-500/10 text-cyan-700 dark:text-cyan-400",
    dot: "bg-cyan-500",
  },
  "git-operator": {
    badge: "border-rose-500/25 bg-rose-500/10 text-rose-700 dark:text-rose-400",
    dot: "bg-rose-500",
  },
  researcher: {
    badge:
      "border-indigo-500/25 bg-indigo-500/10 text-indigo-700 dark:text-indigo-400",
    dot: "bg-indigo-500",
  },
  "spec-review": {
    badge: "border-teal-500/25 bg-teal-500/10 text-teal-700 dark:text-teal-400",
    dot: "bg-teal-500",
  },
  "standards-review": {
    badge:
      "border-fuchsia-500/25 bg-fuchsia-500/10 text-fuchsia-700 dark:text-fuchsia-400",
    dot: "bg-fuchsia-500",
  },
};

export const ROLE_BADGE_FALLBACK = {
  badge: "border-border bg-muted text-muted-foreground",
  dot: "bg-muted-foreground",
};

export const getRoleStyle = (role: string) =>
  ROLE_BADGE_STYLES[role as SupportedRole] ?? ROLE_BADGE_FALLBACK;
