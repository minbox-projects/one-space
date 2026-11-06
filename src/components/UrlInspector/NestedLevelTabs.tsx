import { ChevronRight, CornerDownRight, Globe } from "lucide-react";
import type { ParsedUrlNode } from "@/lib/urlInspector";

interface NestedLevelTabsProps {
  nodes: Record<string, ParsedUrlNode>;
  rootId: string;
  activeNodeId: string;
  onSelectNode: (nodeId: string) => void;
}

export function NestedLevelTabs({
  nodes,
  rootId,
  activeNodeId,
  onSelectNode,
}: NestedLevelTabsProps) {
  // Sort nodes by depth
  const sortedNodes = Object.values(nodes).sort((a, b) => a.depth - b.depth);

  if (sortedNodes.length <= 1) {
    return null;
  }

  return (
    <div className="rounded-2xl border border-slate-200/80 bg-white p-4 shadow-xs dark:border-slate-800/80 dark:bg-slate-900">
      <div className="mb-2.5 flex items-center gap-2">
        <span className="flex h-5 w-5 items-center justify-center rounded-full bg-violet-500/10 text-xs font-semibold text-violet-600 dark:bg-violet-500/20 dark:text-violet-400">
          🧅
        </span>
        <h3 className="text-xs font-semibold text-slate-700 dark:text-slate-300">
          检测到嵌套回调 URL（洋葱层级视图）
        </h3>
        <span className="text-[11px] text-slate-400">
          点击任意层级可单独编辑该层的参数，上层将自动编码同步
        </span>
      </div>

      <div className="flex flex-wrap items-center gap-2">
        {sortedNodes.map((node, index) => {
          const isActive = node.id === activeNodeId;
          const isRoot = node.id === rootId;

          return (
            <div key={node.id} className="flex items-center gap-2">
              {index > 0 ? (
                <ChevronRight className="h-4 w-4 text-slate-300 dark:text-slate-600" />
              ) : null}

              <button
                type="button"
                onClick={() => onSelectNode(node.id)}
                className={`group flex items-center gap-2 rounded-xl border px-3 py-2 text-left text-xs transition-all ${
                  isActive
                    ? "border-violet-500 bg-violet-50/60 text-violet-900 shadow-xs dark:border-violet-500/60 dark:bg-violet-950/40 dark:text-violet-200"
                    : "border-slate-200/80 bg-slate-50/60 text-slate-600 hover:border-slate-300 hover:bg-slate-100 dark:border-slate-800 dark:bg-slate-800/50 dark:text-slate-400 dark:hover:border-slate-700 dark:hover:bg-slate-800"
                }`}
              >
                <div
                  className={`flex h-6 w-6 shrink-0 items-center justify-center rounded-lg ${
                    isActive
                      ? "bg-violet-500 text-white"
                      : "bg-slate-200 text-slate-600 dark:bg-slate-700 dark:text-slate-300"
                  }`}
                >
                  {isRoot ? (
                    <Globe className="h-3.5 w-3.5" />
                  ) : (
                    <CornerDownRight className="h-3.5 w-3.5" />
                  )}
                </div>

                <div>
                  <div className="flex items-center gap-1.5 font-medium">
                    <span>
                      {isRoot ? "主请求 (Root)" : `嵌套层级 ${node.depth}`}
                    </span>
                    {node.parentKeyName ? (
                      <span className="rounded bg-violet-100 px-1.5 py-0.2 font-mono text-[10px] text-violet-700 dark:bg-violet-900/50 dark:text-violet-300">
                        {node.parentKeyName}
                      </span>
                    ) : null}
                  </div>
                  <div className="truncate font-mono text-[11px] text-slate-400 group-hover:text-slate-600 dark:text-slate-500 dark:group-hover:text-slate-300">
                    {node.hostname || "未知主机"}
                    {node.pathname !== "/" ? node.pathname : ""}
                  </div>
                </div>
              </button>
            </div>
          );
        })}
      </div>
    </div>
  );
}
