import { useState } from "react";
import {
  ArrowDown,
  ArrowUp,
  Binary,
  Code2,
  ExternalLink,
  Plus,
  Trash2,
} from "lucide-react";
import type { ParsedUrlNode, QueryParamItem } from "@/lib/urlInspector";
import { generateUniqueId } from "@/lib/urlInspector";

interface QueryGridEditorProps {
  currentNode: ParsedUrlNode;
  onUpdateParams: (updatedParams: QueryParamItem[]) => void;
  onJumpToChildNode?: (childId: string) => void;
}

export function QueryGridEditor({
  currentNode,
  onUpdateParams,
  onJumpToChildNode,
}: QueryGridEditorProps) {
  const [newKey, setNewKey] = useState("");
  const [newValue, setNewValue] = useState("");

  const handleToggleParam = (id: string) => {
    const updated = currentNode.params.map((p) =>
      p.id === id ? { ...p, enabled: !p.enabled } : p,
    );
    onUpdateParams(updated);
  };

  const handleKeyChange = (id: string, key: string) => {
    const updated = currentNode.params.map((p) =>
      p.id === id ? { ...p, key } : p,
    );
    onUpdateParams(updated);
  };

  const handleValueChange = (id: string, value: string) => {
    const updated = currentNode.params.map((p) =>
      p.id === id ? { ...p, value } : p,
    );
    onUpdateParams(updated);
  };

  const handleDeleteParam = (id: string) => {
    const updated = currentNode.params.filter((p) => p.id !== id);
    onUpdateParams(updated);
  };

  const handleMoveUp = (index: number) => {
    if (index <= 0) return;
    const items = [...currentNode.params];
    const temp = items[index];
    items[index] = items[index - 1];
    items[index - 1] = temp;
    onUpdateParams(items);
  };

  const handleMoveDown = (index: number) => {
    if (index >= currentNode.params.length - 1) return;
    const items = [...currentNode.params];
    const temp = items[index];
    items[index] = items[index + 1];
    items[index + 1] = temp;
    onUpdateParams(items);
  };

  const handleDecodeToggle = (param: QueryParamItem) => {
    try {
      const decoded = decodeURIComponent(param.value);
      if (decoded !== param.value) {
        handleValueChange(param.id, decoded);
      } else {
        handleValueChange(param.id, encodeURIComponent(param.value));
      }
    } catch {
      handleValueChange(param.id, encodeURIComponent(param.value));
    }
  };

  const handleAddParam = (e: React.FormEvent) => {
    e.preventDefault();
    if (!newKey.trim()) return;

    const newParam: QueryParamItem = {
      id: generateUniqueId(),
      key: newKey.trim(),
      value: newValue.trim(),
      enabled: true,
      isNestedUrl: false,
    };

    onUpdateParams([...currentNode.params, newParam]);
    setNewKey("");
    setNewValue("");
  };

  return (
    <div className="rounded-2xl border border-slate-200/80 bg-white p-5 shadow-xs dark:border-slate-800/80 dark:bg-slate-900">
      <div className="mb-4 flex items-center justify-between">
        <div>
          <h3 className="text-sm font-semibold text-slate-800 dark:text-slate-100">
            Query 参数编辑器（双向实时同步）
          </h3>
          <p className="mt-0.5 text-xs text-slate-400">
            在此处修改或启停任意参数，顶部 URL 将实时重新计算拼装
          </p>
        </div>
      </div>

      {/* 参数表格 */}
      {currentNode.params.length === 0 ? (
        <div className="flex flex-col items-center justify-center rounded-xl border border-dashed border-slate-200 py-8 text-center dark:border-slate-800">
          <Code2 className="h-8 w-8 text-slate-300 dark:text-slate-600" />
          <p className="mt-2 text-xs font-medium text-slate-500 dark:text-slate-400">
            当前层级暂无 Query 参数
          </p>
          <p className="text-[11px] text-slate-400">
            可在下方直接新增参数
          </p>
        </div>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full text-left text-xs">
            <thead>
              <tr className="border-b border-slate-100 font-medium text-slate-400 dark:border-slate-800">
                <th className="w-10 pb-2 pl-1">启用</th>
                <th className="w-1/4 pb-2">参数名 (Key)</th>
                <th className="pb-2">参数值 (Value)</th>
                <th className="w-28 pb-2 text-right pr-1">操作</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-100 dark:divide-slate-800/60">
              {currentNode.params.map((param, index) => (
                <tr
                  key={param.id}
                  className={`group transition-colors ${
                    !param.enabled
                      ? "opacity-50"
                      : "hover:bg-slate-50/80 dark:hover:bg-slate-800/40"
                  }`}
                >
                  {/* 启用 Checkbox */}
                  <td className="py-2.5 pl-1">
                    <input
                      type="checkbox"
                      checked={param.enabled}
                      onChange={() => handleToggleParam(param.id)}
                      className="h-4 w-4 rounded border-slate-300 text-violet-600 focus:ring-violet-500 dark:border-slate-700 dark:bg-slate-800"
                    />
                  </td>

                  {/* 参数名 Key */}
                  <td className="py-2.5 pr-2">
                    <input
                      type="text"
                      value={param.key}
                      onChange={(e) => handleKeyChange(param.id, e.target.value)}
                      placeholder="key"
                      className="w-full rounded-lg border border-slate-200/80 bg-slate-50/50 px-2.5 py-1.5 font-mono text-xs text-slate-800 placeholder-slate-400 focus:border-violet-500 focus:bg-white focus:outline-none dark:border-slate-700 dark:bg-slate-800/50 dark:text-slate-200 dark:focus:border-violet-400 dark:focus:bg-slate-800"
                    />
                  </td>

                  {/* 参数值 Value */}
                  <td className="py-2.5 pr-2">
                    <div className="flex items-center gap-2">
                      <input
                        type="text"
                        value={param.value}
                        onChange={(e) => handleValueChange(param.id, e.target.value)}
                        placeholder="value"
                        className="w-full rounded-lg border border-slate-200/80 bg-slate-50/50 px-2.5 py-1.5 font-mono text-xs text-slate-800 placeholder-slate-400 focus:border-violet-500 focus:bg-white focus:outline-none dark:border-slate-700 dark:bg-slate-800/50 dark:text-slate-200 dark:focus:border-violet-400 dark:focus:bg-slate-800"
                      />

                      {/* 嵌套子 URL 跳转按钮 */}
                      {param.isNestedUrl && param.nestedChildId && onJumpToChildNode ? (
                        <button
                          type="button"
                          onClick={() => onJumpToChildNode(param.nestedChildId!)}
                          title="跳转至嵌套子 URL 进行深度编辑"
                          className="flex shrink-0 items-center gap-1 rounded-lg border border-violet-500/30 bg-violet-500/10 px-2 py-1 text-[11px] font-medium text-violet-600 hover:bg-violet-500/20 dark:border-violet-500/20 dark:text-violet-400"
                        >
                          <ExternalLink className="h-3 w-3" />
                          <span>解构子层</span>
                        </button>
                      ) : null}

                      {/* Decode/Encode 切换 */}
                      <button
                        type="button"
                        onClick={() => handleDecodeToggle(param)}
                        title="转义/反转义该参数值"
                        className="rounded-lg p-1.5 text-slate-400 hover:bg-slate-100 hover:text-slate-700 dark:hover:bg-slate-800 dark:hover:text-slate-200"
                      >
                        <Binary className="h-3.5 w-3.5" />
                      </button>
                    </div>
                  </td>

                  {/* 操作按钮组 */}
                  <td className="py-2.5 pr-1 text-right">
                    <div className="flex items-center justify-end gap-1">
                      <button
                        type="button"
                        onClick={() => handleMoveUp(index)}
                        disabled={index === 0}
                        title="上移"
                        className="rounded p-1 text-slate-400 hover:bg-slate-100 hover:text-slate-700 disabled:opacity-20 dark:hover:bg-slate-800 dark:hover:text-slate-200"
                      >
                        <ArrowUp className="h-3 w-3" />
                      </button>
                      <button
                        type="button"
                        onClick={() => handleMoveDown(index)}
                        disabled={index === currentNode.params.length - 1}
                        title="下移"
                        className="rounded p-1 text-slate-400 hover:bg-slate-100 hover:text-slate-700 disabled:opacity-20 dark:hover:bg-slate-800 dark:hover:text-slate-200"
                      >
                        <ArrowDown className="h-3 w-3" />
                      </button>
                      <button
                        type="button"
                        onClick={() => handleDeleteParam(param.id)}
                        title="删除参数"
                        className="rounded p-1 text-rose-400 hover:bg-rose-50 hover:text-rose-600 dark:hover:bg-rose-950/40 dark:hover:text-rose-400"
                      >
                        <Trash2 className="h-3 w-3" />
                      </button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {/* 底部新增参数行 */}
      <form
        onSubmit={handleAddParam}
        className="mt-4 flex items-center gap-2 border-t border-slate-100 pt-3 dark:border-slate-800"
      >
        <input
          type="text"
          value={newKey}
          onChange={(e) => setNewKey(e.target.value)}
          placeholder="新增参数名 (Key)"
          className="w-1/3 rounded-lg border border-slate-200/80 bg-slate-50/50 px-3 py-1.5 font-mono text-xs text-slate-800 placeholder-slate-400 focus:border-violet-500 focus:bg-white focus:outline-none dark:border-slate-700 dark:bg-slate-800/50 dark:text-slate-200 dark:focus:border-violet-400 dark:focus:bg-slate-800"
        />
        <input
          type="text"
          value={newValue}
          onChange={(e) => setNewValue(e.target.value)}
          placeholder="参数值 (Value)"
          className="flex-1 rounded-lg border border-slate-200/80 bg-slate-50/50 px-3 py-1.5 font-mono text-xs text-slate-800 placeholder-slate-400 focus:border-violet-500 focus:bg-white focus:outline-none dark:border-slate-700 dark:bg-slate-800/50 dark:text-slate-200 dark:focus:border-violet-400 dark:focus:bg-slate-800"
        />
        <button
          type="submit"
          disabled={!newKey.trim()}
          className="flex items-center gap-1.5 rounded-lg bg-violet-600 px-3 py-1.5 text-xs font-medium text-white shadow-xs transition-all hover:bg-violet-500 disabled:opacity-40"
        >
          <Plus className="h-3.5 w-3.5" />
          <span>添加参数</span>
        </button>
      </form>
    </div>
  );
}
