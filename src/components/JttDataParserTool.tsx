import { useState } from "react";
import { Binary, Clipboard, Eraser, Play, Sparkles } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useToast } from "./ToastProvider";
import type { JttParserTab } from "@/lib/navigation";
import {
  JT1078_DIRECTIONS,
  JT1078_OPERATIONS,
  JT808_MODES,
  JT809_CRYPTO_MODES,
  JT809_VERSIONS,
  analyzeJt1078,
  analyzeJt808,
  analyzeJt809,
  convertHexLines,
  recordStatusLabel,
  serializeRecords,
  type AnalysisRecord,
  type HexDirection,
  type Jt1078Direction,
  type Jt1078Operation,
  type Jt808Mode,
  type Jt809CryptoMode,
  type Jt809Version,
  type ResultNode,
} from "@/lib/jttDataParser";

type TabKey = "jt808" | "jt809" | "jt1078" | "hex";

type Jt808State = {
  input: string;
  mode: Jt808Mode;
  records: AnalysisRecord[];
};

type Jt809State = {
  input: string;
  version: Jt809Version;
  cryptoMode: Jt809CryptoMode;
  m1: string;
  ia1: string;
  ic1: string;
  records: AnalysisRecord[];
};

type Jt1078State = {
  input: string;
  operation: Jt1078Operation;
  direction: Jt1078Direction;
  records: AnalysisRecord[];
};

type HexState = {
  input: string;
  direction: HexDirection;
  output: string;
  error: string;
};

const initialJt808State = (): Jt808State => ({
  input: "",
  mode: "automatic",
  records: [],
});

const initialJt809State = (): Jt809State => ({
  input: "",
  version: "2011",
  cryptoMode: "unencrypted",
  m1: "0",
  ia1: "0",
  ic1: "0",
  records: [],
});

const initialJt1078State = (): Jt1078State => ({
  input: "",
  operation: "0x9101",
  direction: "downstream",
  records: [],
});

const initialHexState = (): HexState => ({
  input: "",
  direction: "hex-to-utf8",
  output: "",
  error: "",
});

const HEX_TO_UTF8_EXAMPLE = "48656C6C6F 20576F726C6421";
const UTF8_TO_HEX_EXAMPLE = "OneSpace 数据解析 2026";

const MODE_LABELS: Record<Jt808Mode, [string, string]> = {
  automatic: ["自动识别", "Automatic"],
  "jt1078-extension": ["JT1078 扩展", "JT1078 Extension"],
  "jiangsu-active-safety": ["江苏主动安全", "Jiangsu Active Safety"],
  "guangdong-active-safety": ["广东主动安全", "Guangdong Active Safety"],
  "force-2013": ["强制 2013", "Force 2013"],
};

const VERSION_LABELS: Record<Jt809Version, [string, string]> = {
  "2011": ["2011", "2011"],
  "2019": ["2019", "2019"],
};

const CRYPTO_LABELS: Record<Jt809CryptoMode, [string, string]> = {
  unencrypted: ["不加密", "Unencrypted"],
  encrypted: ["加密", "Encrypted"],
};

const OPERATION_LABELS: Record<Jt1078Operation, [string, string]> = {
  "0x9101": ["0x9101 实时音视频传输请求", "0x9101 Realtime Stream Request"],
  "0x9102": ["0x9102 音视频实时传输控制", "0x9102 Stream Control"],
  "0x9205": ["0x9205 查询音视频资源列表", "0x9205 Query Resource List"],
  "0x9206": ["0x9206 文件上传指令", "0x9206 File Upload Instruction"],
};

const DIRECTION_LABELS: Record<Jt1078Direction, [string, string]> = {
  upstream: ["上行", "Upstream"],
  downstream: ["下行", "Downstream"],
};

const HEX_DIRECTION_LABELS: Record<HexDirection, [string, string]> = {
  "hex-to-utf8": ["Hex → UTF-8", "Hex → UTF-8"],
  "utf8-to-hex": ["UTF-8 → Hex", "UTF-8 → Hex"],
};

function TreeNode({ node, depth }: { node: ResultNode; depth: number }) {
  return (
    <div>
      <div style={{ paddingLeft: `${depth * 16}px` }}>
        {node.label}
        {node.value !== undefined ? `: ${node.value}` : ""}
      </div>
      {node.children
        ? node.children.map((child, index) => (
            <TreeNode key={index} node={child} depth={depth + 1} />
          ))
        : null}
    </div>
  );
}

function ResultRecords({
  records,
  resultLabel,
}: {
  records: AnalysisRecord[];
  resultLabel: string;
}) {
  if (records.length === 0) return null;
  return (
    <div
      role="region"
      aria-label={resultLabel}
      className="space-y-3 rounded-lg border bg-muted/30 p-3"
    >
      {records.map((record, index) => (
        <div key={index} className="space-y-2">
          <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-sm">
            {record.line !== undefined ? <span>{`第 ${record.line} 行`}</span> : null}
            <span
              className={
                record.kind === "success"
                  ? "font-medium text-emerald-600"
                  : record.kind === "error"
                    ? "font-medium text-destructive"
                    : "text-muted-foreground"
              }
            >
              {`状态: ${recordStatusLabel(record.kind)}`}
            </span>
            {record.error ? (
              <span role="alert" className="text-destructive">
                {`说明: ${record.error}`}
              </span>
            ) : null}
          </div>
          {record.error ? null : (
            <div className="rounded-lg border bg-background p-3 font-mono text-sm leading-6">
              {record.json !== undefined ? (
                <pre className="whitespace-pre-wrap break-words">
                  {JSON.stringify(record.json, null, 2)}
                </pre>
              ) : (
                record.tree.map((node, nodeIndex) => (
                  <TreeNode key={nodeIndex} node={node} depth={0} />
                ))
              )}
            </div>
          )}
          {index < records.length - 1 ? <hr className="my-2 border-border" /> : null}
        </div>
      ))}
    </div>
  );
}

export function JttDataParserTool({
  initialTab,
}: {
  initialTab?: JttParserTab;
}) {
  const { i18n, t } = useTranslation();
  const { pushToast } = useToast();
  const label = (zh: string, en: string) => (i18n.language === "zh" ? zh : en);

  const [activeTab, setActiveTab] = useState<TabKey>(initialTab ?? "jt808");
  const [jt808State, setJt808State] = useState<Jt808State>(initialJt808State);
  const [jt809State, setJt809State] = useState<Jt809State>(initialJt809State);
  const [jt1078State, setJt1078State] = useState<Jt1078State>(initialJt1078State);
  const [hexState, setHexState] = useState<HexState>(initialHexState);

  const analyzeJt808Tab = () => {
    const records = analyzeJt808(jt808State.input, jt808State.mode);
    setJt808State((prev) => ({ ...prev, records }));
  };

  const analyzeJt809Tab = () => {
    const record = analyzeJt809(jt809State.input, jt809State.version, jt809State.cryptoMode, {
      m1: jt809State.m1,
      ia1: jt809State.ia1,
      ic1: jt809State.ic1,
    });
    setJt809State((prev) => ({ ...prev, records: [record] }));
  };

  const analyzeJt1078Tab = () => {
    const record = analyzeJt1078(
      jt1078State.input,
      jt1078State.operation,
      jt1078State.direction,
    );
    setJt1078State((prev) => ({ ...prev, records: [record] }));
  };

  const convertHexTab = () => {
    const result = convertHexLines(hexState.input, hexState.direction);
    setHexState((prev) => ({
      ...prev,
      output: result.output ?? "",
      error: result.error ?? "",
    }));
  };

  const copyCurrentTab = async () => {
    let text: string | null = null;
    if (activeTab === "hex") {
      text = hexState.output !== "" ? hexState.output : null;
    } else {
      const records =
        activeTab === "jt808"
          ? jt808State.records
          : activeTab === "jt809"
            ? jt809State.records
            : jt1078State.records;
      const serialized = records.length > 0 ? serializeRecords(records) : "";
      text = serialized !== "" ? serialized : null;
    }
    if (text === null) {
      pushToast({ title: t("jttCopyNoResult", "Nothing to copy"), kind: "error" });
      return;
    }
    try {
      await navigator.clipboard.writeText(text);
      pushToast({ title: t("jttCopySuccess", "Result copied"), kind: "success" });
    } catch {
      pushToast({ title: t("jttCopyFailed", "Unable to copy result"), kind: "error" });
    }
  };

  const tabs: { key: TabKey; name: string }[] = [
    { key: "jt808", name: label("JT808", "JT808") },
    { key: "jt809", name: label("JT809", "JT809") },
    { key: "jt1078", name: label("JT1078", "JT1078") },
    { key: "hex", name: label("Hex", "Hex") },
  ];

  const selectClass =
    "h-10 rounded-md border bg-background px-3 text-sm outline-none";
  const buttonSecondaryClass =
    "inline-flex h-10 items-center gap-2 rounded-md border px-3 text-sm font-medium hover:bg-muted";
  const textareaClass =
    "min-h-48 w-full resize rounded-md border bg-background p-3 font-mono text-sm leading-6 outline-none ring-offset-background focus-visible:ring-2 focus-visible:ring-ring";

  return (
    <section className="space-y-5 pb-5" aria-labelledby="jtt-parser-title">
      <div className="flex items-start gap-3">
        <div className="rounded-lg bg-emerald-500/10 p-2 text-emerald-600">
          <Binary className="h-5 w-5" />
        </div>
        <div>
          <h2 id="jtt-parser-title" className="text-lg font-semibold">
            {label("JT/T 数据解析", "JT/T Data Parser")}
          </h2>
          <p className="text-sm text-muted-foreground">
            {label(
              "本地解析 JT/T 808、809、1078 报文并转换十六进制。",
              "Parse JT/T 808, 809, 1078 packets and convert hex locally.",
            )}
          </p>
        </div>
      </div>

      <div role="tablist" aria-label={label("解析工具标签页", "Parser tabs")} className="flex flex-wrap gap-1 rounded-lg border bg-muted/40 p-1">
        {tabs.map((tab) => (
          <button
            key={tab.key}
            type="button"
            role="tab"
            aria-selected={activeTab === tab.key}
            onClick={() => setActiveTab(tab.key)}
            className={`inline-flex h-9 items-center rounded-md px-4 text-sm font-medium transition-colors ${
              activeTab === tab.key
                ? "bg-background text-foreground shadow-sm"
                : "text-muted-foreground hover:text-foreground"
            }`}
          >
            {tab.name}
          </button>
        ))}
      </div>

      {activeTab === "jt808" ? (
        <div
          role="tabpanel"
          aria-label={label("JT808 报文解析", "JT808 packet parsing")}
          className="space-y-4 rounded-lg border bg-card p-4"
        >
          <div className="flex flex-wrap items-end justify-between gap-3">
            <label className="grid gap-1.5 text-sm font-medium" htmlFor="jtt-jt808-mode">
              {label("模式", "Mode")}
              <select
                id="jtt-jt808-mode"
                value={jt808State.mode}
                onChange={(event) =>
                  setJt808State((prev) => ({
                    ...prev,
                    mode: event.target.value as Jt808Mode,
                    records: [],
                  }))
                }
                className={selectClass}
              >
                {JT808_MODES.map((mode) => (
                  <option key={mode} value={mode}>
                    {label(...MODE_LABELS[mode])}
                  </option>
                ))}
              </select>
            </label>
            <div className="flex flex-wrap gap-2">
              <button
                type="button"
                onClick={analyzeJt808Tab}
                className="inline-flex h-10 items-center gap-2 rounded-md bg-primary px-4 text-sm font-medium text-primary-foreground hover:bg-primary/90"
              >
                <Play className="h-4 w-4" />
                {label("解析", "Analyze")}
              </button>
              <button
                type="button"
                onClick={() => setJt808State(initialJt808State)}
                className={buttonSecondaryClass}
              >
                <Eraser className="h-4 w-4" />
                {label("清空", "Clear")}
              </button>
              <button
                type="button"
                onClick={() => void copyCurrentTab()}
                className={buttonSecondaryClass}
              >
                <Clipboard className="h-4 w-4" />
                {label("复制结果", "Copy Result")}
              </button>
            </div>
          </div>

          <label className="sr-only" htmlFor="jtt-jt808-input">
            {label("JT808 报文输入", "JT808 packet input")}
          </label>
          <textarea
            id="jtt-jt808-input"
            value={jt808State.input}
            onChange={(event) =>
              setJt808State((prev) => ({ ...prev, input: event.target.value }))
            }
            placeholder={label(
              "粘贴 JT808 报文，每行一条，支持自动识别 2011/2013/2019...",
              "Paste JT808 packets, one per line...",
            )}
            className={textareaClass}
            spellCheck={false}
          />
          <ResultRecords
            records={jt808State.records}
            resultLabel={label("解析结果", "Result")}
          />
        </div>
      ) : null}

      {activeTab === "jt809" ? (
        <div
          role="tabpanel"
          aria-label={label("JT809 报文解析", "JT809 packet parsing")}
          className="space-y-4 rounded-lg border bg-card p-4"
        >
          <div className="flex flex-wrap items-end gap-3">
            <label className="grid gap-1.5 text-sm font-medium" htmlFor="jtt-jt809-version">
              {label("版本", "Version")}
              <select
                id="jtt-jt809-version"
                value={jt809State.version}
                onChange={(event) =>
                  setJt809State((prev) => ({
                    ...prev,
                    version: event.target.value as Jt809Version,
                    records: [],
                  }))
                }
                className={selectClass}
              >
                {JT809_VERSIONS.map((version) => (
                  <option key={version} value={version}>
                    {label(...VERSION_LABELS[version])}
                  </option>
                ))}
              </select>
            </label>
            <label className="grid gap-1.5 text-sm font-medium" htmlFor="jtt-jt809-crypto">
              {label("加密", "Encryption")}
              <select
                id="jtt-jt809-crypto"
                value={jt809State.cryptoMode}
                onChange={(event) =>
                  setJt809State((prev) => ({
                    ...prev,
                    cryptoMode: event.target.value as Jt809CryptoMode,
                    records: [],
                  }))
                }
                className={selectClass}
              >
                {JT809_CRYPTO_MODES.map((mode) => (
                  <option key={mode} value={mode}>
                    {label(...CRYPTO_LABELS[mode])}
                  </option>
                ))}
              </select>
            </label>
            {jt809State.cryptoMode === "encrypted" ? (
              <div className="flex flex-wrap items-end gap-3">
                {(["m1", "ia1", "ic1"] as const).map((field) => (
                  <label
                    key={field}
                    className="grid gap-1.5 text-sm font-medium"
                    htmlFor={`jtt-jt809-${field}`}
                  >
                    {field.toUpperCase()}
                    <input
                      id={`jtt-jt809-${field}`}
                      type="text"
                      inputMode="numeric"
                      value={jt809State[field]}
                      onChange={(event) =>
                        setJt809State((prev) => ({ ...prev, [field]: event.target.value }))
                      }
                      className="h-10 w-28 rounded-md border bg-background px-3 text-sm outline-none"
                    />
                  </label>
                ))}
              </div>
            ) : null}
            <div className="flex flex-wrap gap-2">
              <button
                type="button"
                onClick={analyzeJt809Tab}
                className="inline-flex h-10 items-center gap-2 rounded-md bg-primary px-4 text-sm font-medium text-primary-foreground hover:bg-primary/90"
              >
                <Play className="h-4 w-4" />
                {label("解析", "Analyze")}
              </button>
              <button
                type="button"
                onClick={() => setJt809State(initialJt809State)}
                className={buttonSecondaryClass}
              >
                <Eraser className="h-4 w-4" />
                {label("清空", "Clear")}
              </button>
              <button
                type="button"
                onClick={() => void copyCurrentTab()}
                className={buttonSecondaryClass}
              >
                <Clipboard className="h-4 w-4" />
                {label("复制结果", "Copy Result")}
              </button>
            </div>
          </div>

          <label className="sr-only" htmlFor="jtt-jt809-input">
            {label("JT809 报文输入", "JT809 packet input")}
          </label>
          <textarea
            id="jtt-jt809-input"
            value={jt809State.input}
            onChange={(event) =>
              setJt809State((prev) => ({ ...prev, input: event.target.value }))
            }
            placeholder={label("粘贴单条 JT809 报文...", "Paste a single JT809 packet...")}
            className={textareaClass}
            spellCheck={false}
          />
          <ResultRecords
            records={jt809State.records}
            resultLabel={label("解析结果", "Result")}
          />
        </div>
      ) : null}

      {activeTab === "jt1078" ? (
        <div
          role="tabpanel"
          aria-label={label("JT1078 报文解析", "JT1078 packet parsing")}
          className="space-y-4 rounded-lg border bg-card p-4"
        >
          <div className="flex flex-wrap items-end gap-3">
            <label className="grid gap-1.5 text-sm font-medium" htmlFor="jtt-jt1078-operation">
              {label("操作", "Operation")}
              <select
                id="jtt-jt1078-operation"
                value={jt1078State.operation}
                onChange={(event) =>
                  setJt1078State((prev) => ({
                    ...prev,
                    operation: event.target.value as Jt1078Operation,
                    records: [],
                  }))
                }
                className={selectClass}
              >
                {JT1078_OPERATIONS.map((operation) => (
                  <option key={operation} value={operation}>
                    {label(...OPERATION_LABELS[operation])}
                  </option>
                ))}
              </select>
            </label>
            <label className="grid gap-1.5 text-sm font-medium" htmlFor="jtt-jt1078-direction">
              {label("方向", "Direction")}
              <select
                id="jtt-jt1078-direction"
                value={jt1078State.direction}
                onChange={(event) =>
                  setJt1078State((prev) => ({
                    ...prev,
                    direction: event.target.value as Jt1078Direction,
                    records: [],
                  }))
                }
                className={selectClass}
              >
                {JT1078_DIRECTIONS.map((direction) => (
                  <option key={direction} value={direction}>
                    {label(...DIRECTION_LABELS[direction])}
                  </option>
                ))}
              </select>
            </label>
            <div className="flex flex-wrap gap-2">
              <button
                type="button"
                onClick={analyzeJt1078Tab}
                className="inline-flex h-10 items-center gap-2 rounded-md bg-primary px-4 text-sm font-medium text-primary-foreground hover:bg-primary/90"
              >
                <Play className="h-4 w-4" />
                {label("解析", "Analyze")}
              </button>
              <button
                type="button"
                onClick={() => setJt1078State(initialJt1078State)}
                className={buttonSecondaryClass}
              >
                <Eraser className="h-4 w-4" />
                {label("清空", "Clear")}
              </button>
              <button
                type="button"
                onClick={() => void copyCurrentTab()}
                className={buttonSecondaryClass}
              >
                <Clipboard className="h-4 w-4" />
                {label("复制结果", "Copy Result")}
              </button>
            </div>
          </div>

          <label className="sr-only" htmlFor="jtt-jt1078-input">
            {label("JT1078 报文输入", "JT1078 packet input")}
          </label>
          <textarea
            id="jtt-jt1078-input"
            value={jt1078State.input}
            onChange={(event) =>
              setJt1078State((prev) => ({ ...prev, input: event.target.value }))
            }
            placeholder={label("粘贴单条 JT1078 报文...", "Paste a single JT1078 packet...")}
            className={textareaClass}
            spellCheck={false}
          />
          <ResultRecords
            records={jt1078State.records}
            resultLabel={label("解析结果", "Result")}
          />
        </div>
      ) : null}

      {activeTab === "hex" ? (
        <div
          role="tabpanel"
          aria-label={label("Hex 转换", "Hex conversion")}
          className="space-y-4 rounded-lg border bg-card p-4"
        >
          <div className="flex flex-wrap items-end justify-between gap-3">
            <label className="grid gap-1.5 text-sm font-medium" htmlFor="jtt-hex-direction">
              {label("方向", "Direction")}
              <select
                id="jtt-hex-direction"
                value={hexState.direction}
                onChange={(event) =>
                  setHexState((prev) => ({
                    ...prev,
                    direction: event.target.value as HexDirection,
                    output: "",
                    error: "",
                  }))
                }
                className={selectClass}
              >
                {(["hex-to-utf8", "utf8-to-hex"] as HexDirection[]).map((direction) => (
                  <option key={direction} value={direction}>
                    {label(...HEX_DIRECTION_LABELS[direction])}
                  </option>
                ))}
              </select>
            </label>
            <div className="flex flex-wrap gap-2">
              <button
                type="button"
                onClick={convertHexTab}
                className="inline-flex h-10 items-center gap-2 rounded-md bg-primary px-4 text-sm font-medium text-primary-foreground hover:bg-primary/90"
              >
                <Play className="h-4 w-4" />
                {label("转换", "Convert")}
              </button>
              <button
                type="button"
                onClick={() =>
                  setHexState((prev) => ({
                    ...prev,
                    input:
                      prev.direction === "hex-to-utf8"
                        ? HEX_TO_UTF8_EXAMPLE
                        : UTF8_TO_HEX_EXAMPLE,
                    output: "",
                    error: "",
                  }))
                }
                className={buttonSecondaryClass}
              >
                <Sparkles className="h-4 w-4" />
                {label("示例", "Example")}
              </button>
              <button
                type="button"
                onClick={() => setHexState(initialHexState)}
                className={buttonSecondaryClass}
              >
                <Eraser className="h-4 w-4" />
                {label("清空", "Clear")}
              </button>
              <button
                type="button"
                onClick={() => void copyCurrentTab()}
                className={buttonSecondaryClass}
              >
                <Clipboard className="h-4 w-4" />
                {label("复制结果", "Copy Result")}
              </button>
            </div>
          </div>

          <label className="sr-only" htmlFor="jtt-hex-input">
            {label("Hex 输入", "Hex input")}
          </label>
          <textarea
            id="jtt-hex-input"
            value={hexState.input}
            onChange={(event) =>
              setHexState((prev) => ({ ...prev, input: event.target.value }))
            }
            placeholder={label("粘贴十六进制或文本...", "Paste hex or text...")}
            className={textareaClass}
            spellCheck={false}
          />

          {hexState.error ? (
            <p role="alert" className="text-sm text-destructive">
              {hexState.error}
            </p>
          ) : null}
          {hexState.output !== "" ? (
            <div
              role="region"
              aria-label={label("转换结果", "Result")}
              className="max-h-80 overflow-y-auto whitespace-pre-wrap break-words rounded-lg border bg-muted/30 p-3 font-mono text-sm leading-6"
            >
              {hexState.output}
            </div>
          ) : null}
        </div>
      ) : null}
    </section>
  );
}