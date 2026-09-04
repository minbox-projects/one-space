import type { ResultNode } from "./types";

export function findNode(
  tree: ResultNode[],
  label: string,
): ResultNode | undefined {
  for (const node of tree) {
    if (node.label === label) return node;
    const found = node.children ? findNode(node.children, label) : undefined;
    if (found) return found;
  }
  return undefined;
}

export function findNodeValue(tree: ResultNode[], label: string): string | undefined {
  return findNode(tree, label)?.value;
}

export function nodeLabels(tree: ResultNode[]): string[] {
  const labels: string[] = [];
  for (const node of tree) {
    labels.push(node.label);
    if (node.children) labels.push(...nodeLabels(node.children));
  }
  return labels;
}