/**
 * URL & Query Inspector core engine.
 * Supports recursive parsing of nested URLs, bidirectional synchronization,
 * RFC 3986 encoding/decoding, and multi-format export.
 */

export interface QueryParamItem {
  id: string;
  key: string;
  value: string;
  enabled: boolean;
  isNestedUrl: boolean;
  nestedChildId?: string;
}

export interface ParsedUrlNode {
  id: string;
  raw: string;
  protocol: string;
  username: string;
  password: string;
  hostname: string;
  port: string;
  pathname: string;
  hash: string;
  params: QueryParamItem[];
  depth: number;
  parentParamId?: string;
  parentKeyName?: string;
}

export interface UrlHierarchyResult {
  rootId: string;
  nodes: Record<string, ParsedUrlNode>;
}

/**
 * Generates a lightweight random ID for nodes and params.
 */
export function generateUniqueId(): string {
  return Math.random().toString(36).slice(2, 10);
}

/**
 * Checks if a string looks like an HTTP/HTTPS URL (either plain or percent-encoded).
 */
export function looksLikeUrl(candidate: string): boolean {
  if (!candidate || typeof candidate !== "string") return false;
  const trimmed = candidate.trim();
  if (trimmed.startsWith("http://") || trimmed.startsWith("https://")) {
    return true;
  }
  // Try single decode to see if it starts with http:// or https://
  try {
    const decoded = decodeURIComponent(trimmed);
    return decoded.startsWith("http://") || decoded.startsWith("https://");
  } catch {
    return false;
  }
}

/**
 * Recursively parses a raw URL into a hierarchy of ParsedUrlNodes.
 */
export function parseUrlHierarchy(
  rawInput: string,
  maxDepth = 5,
  currentDepth = 0,
  parentParamId?: string,
  parentKeyName?: string,
): UrlHierarchyResult | null {
  if (!rawInput || typeof rawInput !== "string") return null;

  let input = rawInput.trim();
  if (!input) return null;

  // If missing protocol, try prepending https:// for parsing
  let hasSynthesizedProtocol = false;
  if (!input.includes("://") && !input.startsWith("//")) {
    input = "https://" + input;
    hasSynthesizedProtocol = true;
  }

  let parsed: URL;
  try {
    parsed = new URL(input);
  } catch {
    return null;
  }

  const nodeId = generateUniqueId();
  const nodes: Record<string, ParsedUrlNode> = {};
  const params: QueryParamItem[] = [];

  // Parse search params preserving duplicate keys
  const searchEntries: Array<{ key: string; value: string }> = [];
  try {
    parsed.searchParams.forEach((value, key) => {
      searchEntries.push({ key, value });
    });
  } catch {
    // Fallback manual regex if searchParams fails on malformed input
  }

  for (const entry of searchEntries) {
    const paramId = generateUniqueId();
    let isNested = false;
    let childId: string | undefined = undefined;

    // Check if the parameter value is a nested URL
    if (currentDepth < maxDepth && looksLikeUrl(entry.value)) {
      let rawChild = entry.value;
      try {
        rawChild = decodeURIComponent(entry.value);
      } catch {
        // use raw
      }

      const childHierarchy = parseUrlHierarchy(
        rawChild,
        maxDepth,
        currentDepth + 1,
        paramId,
        entry.key,
      );

      if (childHierarchy) {
        isNested = true;
        childId = childHierarchy.rootId;
        Object.assign(nodes, childHierarchy.nodes);
      }
    }

    params.push({
      id: paramId,
      key: entry.key,
      value: entry.value,
      enabled: true,
      isNestedUrl: isNested,
      nestedChildId: childId,
    });
  }

  const node: ParsedUrlNode = {
    id: nodeId,
    raw: hasSynthesizedProtocol ? rawInput.trim() : input,
    protocol: hasSynthesizedProtocol ? "" : parsed.protocol,
    username: parsed.username,
    password: parsed.password,
    hostname: parsed.hostname,
    port: parsed.port,
    pathname: parsed.pathname,
    hash: parsed.hash,
    params,
    depth: currentDepth,
    parentParamId,
    parentKeyName,
  };

  nodes[nodeId] = node;

  return {
    rootId: nodeId,
    nodes,
  };
}

/**
 * Rebuilds the URL string for a given node, recursively folding child nodes into parent values.
 */
export function rebuildNodeUrl(
  nodeId: string,
  nodes: Record<string, ParsedUrlNode>,
): string {
  const node = nodes[nodeId];
  if (!node) return "";

  const effectiveProtocol = node.protocol || "https:";
  let baseUrl = "";

  if (node.username || node.password) {
    const auth = node.password ? `${node.username}:${node.password}` : node.username;
    baseUrl = `${effectiveProtocol}//${auth}@${node.hostname}`;
  } else if (node.hostname) {
    baseUrl = `${effectiveProtocol}//${node.hostname}`;
  } else {
    baseUrl = "";
  }

  if (node.port) {
    baseUrl += `:${node.port}`;
  }

  baseUrl += node.pathname || "/";

  // Build query string
  const activeParams: Array<{ key: string; value: string }> = [];

  for (const param of node.params) {
    if (!param.enabled) continue;

    let finalValue = param.value;
    if (param.isNestedUrl && param.nestedChildId && nodes[param.nestedChildId]) {
      // Recursively rebuild child URL as raw string; the param map will percent-encode it once.
      finalValue = rebuildNodeUrl(param.nestedChildId, nodes);
    }

    activeParams.push({ key: param.key, value: finalValue });
  }

  let queryString = "";
  if (activeParams.length > 0) {
    queryString =
      "?" +
      activeParams
        .map((p) => `${encodeURIComponent(p.key)}=${encodeURIComponent(p.value)}`)
        .join("&");
  }

  return `${baseUrl}${queryString}${node.hash || ""}`;
}

/**
 * Export helpers for multiple formats.
 */
export function generateExportCode(
  format: "curl" | "fetch" | "python" | "json" | "markdown",
  fullUrl: string,
  params: QueryParamItem[],
): string {
  const enabledParams = params.filter((p) => p.enabled);

  switch (format) {
    case "curl":
      return `curl -X GET "${fullUrl}"`;

    case "fetch":
      return `fetch("${fullUrl}", {\n  method: "GET",\n  headers: {\n    "Accept": "application/json"\n  }\n})\n  .then(res => res.json())\n  .then(data => console.log(data));`;

    case "python":
      return `import requests\n\nurl = "${fullUrl}"\nresponse = requests.get(url)\nprint(response.status_code)\nprint(response.json())`;

    case "json": {
      const obj: Record<string, string> = {};
      for (const p of enabledParams) {
        obj[p.key] = p.value;
      }
      return JSON.stringify(obj, null, 2);
    }

    case "markdown": {
      if (enabledParams.length === 0) return "*无启用参数*";
      const header = "| 参数名 (Key) | 参数值 (Value) |\n| :--- | :--- |\n";
      const rows = enabledParams
        .map((p) => `| \`${p.key}\` | \`${p.value.replace(/\|/g, "\\|")}\` |`)
        .join("\n");
      return header + rows;
    }

    default:
      return fullUrl;
  }
}
