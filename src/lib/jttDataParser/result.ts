import type { AnalysisRecord, ResultNode } from "./types";

const KIND_STATUS_LABEL: Record<AnalysisRecord["kind"], string> = {
  success: "成功",
  unsupported: "暂不支持该协议体",
  error: "解析失败",
};

function serializeNode(lines: string[], node: ResultNode, depth: number): void {
  const indent = "  ".repeat(depth);
  let suffix = node.value !== undefined ? `: ${node.value}` : "";
  if (node.children && suffix === "") {
    suffix = ":";
  }
  lines.push(`${indent}${node.label}${suffix}`);
  if (node.children) {
    for (const child of node.children) {
      serializeNode(lines, child, depth + 1);
    }
  }
}

function serializeRecord(record: AnalysisRecord): string {
  const lines: string[] = [];
  if (record.line !== undefined) {
    lines.push(`第 ${record.line} 行`);
  }
  lines.push(`状态: ${KIND_STATUS_LABEL[record.kind]}`);
  if (record.error) {
    lines.push(`说明: ${record.error}`);
  }
  for (const node of record.tree) {
    serializeNode(lines, node, 0);
  }
  return lines.join("\n");
}

export function serializeRecords(records: AnalysisRecord[]): string {
  return records.map(serializeRecord).join("\n");
}