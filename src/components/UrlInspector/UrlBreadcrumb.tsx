import { Hash, Layers, Lock, Server } from "lucide-react";
import type { ParsedUrlNode } from "@/lib/urlInspector";

interface UrlBreadcrumbProps {
  node: ParsedUrlNode;
}

export function UrlBreadcrumb({ node }: UrlBreadcrumbProps) {
  const isHttps = node.protocol === "https:";

  return (
    <div className="flex flex-wrap items-center gap-2 text-xs">
      {/* 协议胶囊 */}
      {node.protocol ? (
        <span
          className={`inline-flex items-center gap-1.5 rounded-lg border px-2.5 py-1 font-medium shadow-xs ${
            isHttps
              ? "border-emerald-500/30 bg-emerald-500/10 text-emerald-600 dark:border-emerald-500/20 dark:text-emerald-400"
              : "border-amber-500/30 bg-amber-500/10 text-amber-600 dark:border-amber-500/20 dark:text-amber-400"
          }`}
        >
          <Lock className="h-3 w-3" />
          {node.protocol.replace(":", "").toUpperCase()}
        </span>
      ) : null}

      {/* 主机名胶囊 */}
      {node.hostname ? (
        <span className="inline-flex items-center gap-1.5 rounded-lg border border-slate-200/80 bg-white px-2.5 py-1 font-medium text-slate-700 shadow-xs dark:border-slate-800 dark:bg-slate-900 dark:text-slate-200">
          <Server className="h-3 w-3 text-slate-400" />
          <span>{node.hostname}</span>
          {node.port ? (
            <span className="font-mono text-slate-400">:{node.port}</span>
          ) : null}
        </span>
      ) : null}

      {/* 路径胶囊 */}
      {node.pathname ? (
        <span className="inline-flex items-center gap-1.5 rounded-lg border border-slate-200/80 bg-white px-2.5 py-1 font-mono text-slate-600 shadow-xs dark:border-slate-800 dark:bg-slate-900 dark:text-slate-300">
          <span className="text-slate-400">Path:</span>
          <span>{node.pathname}</span>
        </span>
      ) : null}

      {/* 参数数量胶囊 */}
      <span className="inline-flex items-center gap-1.5 rounded-lg border border-violet-500/30 bg-violet-500/10 px-2.5 py-1 font-medium text-violet-600 shadow-xs dark:border-violet-500/20 dark:text-violet-400">
        <Layers className="h-3 w-3" />
        <span>
          {node.params.filter((p) => p.enabled).length} / {node.params.length} 个参数
        </span>
      </span>

      {/* Hash 胶囊 */}
      {node.hash ? (
        <span className="inline-flex items-center gap-1.5 rounded-lg border border-slate-200/80 bg-white px-2.5 py-1 font-mono text-slate-600 shadow-xs dark:border-slate-800 dark:bg-slate-900 dark:text-slate-300">
          <Hash className="h-3 w-3 text-slate-400" />
          <span>{node.hash}</span>
        </span>
      ) : null}
    </div>
  );
}
