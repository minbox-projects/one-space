import { useCallback, useMemo, useState } from "react";
import { Compass, Copy, Link2, RotateCcw, Sparkles, TableProperties } from "lucide-react";
import { useToast } from "@/components/ToastProvider";
import { useCopyToClipboard } from "@/toolbox/useCopyToClipboard";
import type { QueryParamItem, UrlHierarchyResult } from "@/lib/urlInspector";
import {
  parseUrlHierarchy,
  rebuildNodeUrl,
} from "@/lib/urlInspector";

import { ExportDeck } from "./ExportDeck";
import { HopTracerView } from "./HopTracerView";
import { NestedLevelTabs } from "./NestedLevelTabs";
import { QueryGridEditor } from "./QueryGridEditor";
import { UrlBreadcrumb } from "./UrlBreadcrumb";

const DEMO_URL =
  "https://auth.example.com/oauth/authorize?client_id=onespace_client&response_type=code&scope=user%20repo&redirect_uri=https%3A%2F%2Fapi.example.com%2Fv1%2Fcallback%3Fstate%3Dsec_999%26target%3Dhttps%253A%252F%252Fapp.example.com%252Fdashboard%253Ftab%253Dsettings#top";

type UrlInspectorTab = "trace" | "edit";

export function UrlInspectorTool() {
  const { pushToast } = useToast();
  const { copy: copyFullUrl, copied: copiedFullUrl } = useCopyToClipboard();

  const [activeTab, setActiveTab] = useState<UrlInspectorTab>("trace");
  const [rawInput, setRawInput] = useState<string>(DEMO_URL);
  const [hierarchy, setHierarchy] = useState<UrlHierarchyResult | null>(() =>
    parseUrlHierarchy(DEMO_URL),
  );
  const [activeNodeId, setActiveNodeId] = useState<string>(() =>
    hierarchy ? hierarchy.rootId : "",
  );

  // Parse whenever raw input changes from outside paste/typing
  const handleInputChange = (value: string) => {
    setRawInput(value);
    const parsed = parseUrlHierarchy(value);
    setHierarchy(parsed);
    if (parsed) {
      setActiveNodeId(parsed.rootId);
    }
  };

  const handleClear = () => {
    setRawInput("");
    setHierarchy(null);
    setActiveNodeId("");
  };

  const handleLoadDemo = () => {
    handleInputChange(DEMO_URL);
    pushToast({
      title: "已加载演示嵌套 OAuth URL",
      kind: "info",
    });
  };

  // When params in active node change, rebuild bottom-up and update rawInput
  const handleUpdateParams = useCallback(
    (updatedParams: QueryParamItem[]) => {
      if (!hierarchy || !activeNodeId) return;

      const updatedNodes = { ...hierarchy.nodes };
      const currentNode = updatedNodes[activeNodeId];
      if (!currentNode) return;

      updatedNodes[activeNodeId] = {
        ...currentNode,
        params: updatedParams,
      };

      // Rebuild root URL
      const newFullUrl = rebuildNodeUrl(hierarchy.rootId, updatedNodes);

      setHierarchy({
        rootId: hierarchy.rootId,
        nodes: updatedNodes,
      });
      setRawInput(newFullUrl);
    },
    [hierarchy, activeNodeId],
  );

  const currentNode = useMemo(() => {
    if (!hierarchy || !activeNodeId) return null;
    return hierarchy.nodes[activeNodeId] || null;
  }, [hierarchy, activeNodeId]);

  return (
    <div className="flex h-full flex-col bg-slate-50 text-slate-800 dark:bg-slate-950 dark:text-slate-100">
      {/* 顶部主工具栏 (Header) - 与 SSL/ServerMonitor 规范一致 */}
      <div className="border-b border-slate-200/80 bg-white/70 px-6 py-4 backdrop-blur-md dark:border-slate-800/80 dark:bg-slate-900/70">
        <div className="flex flex-wrap items-center justify-between gap-4">
          <div className="flex items-center gap-3">
            <div className="flex h-10 w-10 items-center justify-center rounded-xl bg-violet-500/10 text-violet-600 shadow-xs dark:bg-violet-500/20 dark:text-violet-400">
              <Link2 className="h-6 w-6" />
            </div>
            <div>
              <h1 className="text-lg font-semibold tracking-tight text-slate-900 dark:text-white">
                URL 嵌套解构与跳转监测
              </h1>
              <p className="text-xs text-slate-500 dark:text-slate-400">
                真实 HTTP 逐级跳转追踪 (Hop-by-Hop) · 嵌套回调自动解包 · 参数表格双向同步
              </p>
            </div>
          </div>

          {/* 模式切换 Tab (对齐 SSL 证书风格) */}
          <div className="flex rounded-xl bg-slate-100 p-1 dark:bg-slate-800">
            <button
              type="button"
              onClick={() => setActiveTab("trace")}
              className={`flex items-center gap-1.5 rounded-lg px-3.5 py-1.5 text-xs font-medium transition-all ${
                activeTab === "trace"
                  ? "bg-white text-violet-600 shadow-xs dark:bg-slate-700 dark:text-violet-400"
                  : "text-slate-600 hover:text-slate-900 dark:text-slate-300 dark:hover:text-white"
              }`}
            >
              <Compass className="h-3.5 w-3.5" />
              <span>真实跳转追踪 (Hop-by-Hop)</span>
            </button>
            <button
              type="button"
              onClick={() => setActiveTab("edit")}
              className={`flex items-center gap-1.5 rounded-lg px-3.5 py-1.5 text-xs font-medium transition-all ${
                activeTab === "edit"
                  ? "bg-white text-violet-600 shadow-xs dark:bg-slate-700 dark:text-violet-400"
                  : "text-slate-600 hover:text-slate-900 dark:text-slate-300 dark:hover:text-white"
              }`}
            >
              <TableProperties className="h-3.5 w-3.5" />
              <span>静态参数编辑与代码导出</span>
            </button>
          </div>
        </div>
      </div>

      {/* 主工作区 */}
      <div className="flex-1 space-y-5 overflow-y-auto p-6">
        {activeTab === "trace" ? (
          <HopTracerView />
        ) : (
          <>
            {/* 输入框卡片 */}
            <div className="rounded-2xl border border-slate-200/80 bg-white p-5 shadow-xs dark:border-slate-800/80 dark:bg-slate-900">
              <div className="flex items-center justify-between mb-2">
                <label
                  htmlFor="raw-url-input"
                  className="block text-xs font-semibold text-slate-700 dark:text-slate-300"
                >
                  完整 URL 地址（粘贴任意格式 URL，支持自动格式化与实时解析）
                </label>
                <div className="flex items-center gap-2">
                  <button
                    type="button"
                    onClick={handleLoadDemo}
                    className="flex items-center gap-1 rounded-md border border-slate-200 bg-white px-2 py-0.5 text-[11px] text-slate-600 hover:bg-slate-50 dark:border-slate-700 dark:bg-slate-800 dark:text-slate-300"
                  >
                    <Sparkles className="h-3 w-3 text-violet-500" />
                    <span>载入样例</span>
                  </button>
                  <button
                    type="button"
                    onClick={handleClear}
                    disabled={!rawInput}
                    className="flex items-center gap-1 rounded-md border border-slate-200 bg-white px-2 py-0.5 text-[11px] text-slate-600 hover:bg-rose-50 hover:text-rose-600 disabled:opacity-40 dark:border-slate-700 dark:bg-slate-800 dark:text-slate-300"
                  >
                    <RotateCcw className="h-3 w-3" />
                    <span>清空</span>
                  </button>
                  <button
                    type="button"
                    onClick={() => copyFullUrl(rawInput)}
                    disabled={!rawInput}
                    className="flex items-center gap-1 rounded-md bg-violet-600 px-2 py-0.5 text-[11px] text-white hover:bg-violet-500 disabled:opacity-40"
                  >
                    <Copy className="h-3 w-3" />
                    <span>{copiedFullUrl ? "已复制" : "复制"}</span>
                  </button>
                </div>
              </div>
              <textarea
                id="raw-url-input"
                rows={3}
                value={rawInput}
                onChange={(e) => handleInputChange(e.target.value)}
                placeholder="在此粘贴任意超长 URL (如 https://auth.com/login?redirect_uri=...)"
                className="w-full resize-y rounded-xl border border-slate-200/80 bg-slate-50/60 p-3 font-mono text-xs text-slate-800 placeholder-slate-400 selection:bg-violet-500/20 focus:border-violet-500 focus:bg-white focus:outline-none dark:border-slate-800 dark:bg-slate-800/60 dark:text-slate-100 dark:focus:border-violet-400 dark:focus:bg-slate-800"
              />

              {/* 结构胶囊条 */}
              {currentNode ? (
                <div className="mt-3 border-t border-slate-100 pt-3 dark:border-slate-800">
                  <UrlBreadcrumb node={currentNode} />
                </div>
              ) : null}
            </div>

            {/* 洋葱嵌套层级视图 */}
            {hierarchy ? (
              <NestedLevelTabs
                nodes={hierarchy.nodes}
                rootId={hierarchy.rootId}
                activeNodeId={activeNodeId}
                onSelectNode={setActiveNodeId}
              />
            ) : null}

            {/* 参数表格编辑器 */}
            {currentNode ? (
              <QueryGridEditor
                currentNode={currentNode}
                onUpdateParams={handleUpdateParams}
                onJumpToChildNode={(childId) => setActiveNodeId(childId)}
              />
            ) : null}

            {/* 多语言代码与导出 */}
            {currentNode ? (
              <ExportDeck
                fullUrl={rawInput}
                params={currentNode.params}
              />
            ) : null}
          </>
        )}
      </div>
    </div>
  );
}
