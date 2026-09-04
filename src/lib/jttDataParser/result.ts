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

export function recordStatusLabel(kind: AnalysisRecord["kind"]): string {
  return KIND_STATUS_LABEL[kind];
}

function serializeRecordTree(record: AnalysisRecord): string {
  const lines: string[] = [];
  for (const node of record.tree) {
    serializeNode(lines, node, 0);
  }
  return lines.join("\n");
}

export function serializeRecords(records: AnalysisRecord[]): string {
  const results: string[] = [];
  for (const record of records) {
    if (record.kind !== "success") continue;
    results.push(
      record.json !== undefined
        ? JSON.stringify(record.json, null, 2)
        : serializeRecordTree(record),
    );
  }
  return results.join("\n\n");
}