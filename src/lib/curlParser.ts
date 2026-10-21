/**
 * cURL 命令行语法解析器与多语言代码生成器
 */

export interface KeyValuePair {
  name: string;
  value: string;
  enabled: boolean;
}

export interface ParsedHttpRequest {
  method: string;
  url: string;
  baseUrl: string;
  params: KeyValuePair[];
  headers: KeyValuePair[];
  body?: string;
}

export type CodeExportLanguage =
  | "typescript-fetch"
  | "python-requests"
  | "go-nethttp"
  | "rust-reqwest"
  | "curl";

/**
 * 词法切分 cURL 命令，处理引号与反斜杠
 */
export function tokenizeCurl(cmd: string): string[] {
  const normalized = cmd.replace(/\\\r?\n/g, " ");
  const tokens: string[] = [];
  let current = "";
  let inSingleQuote = false;
  let inDoubleQuote = false;
  let isEscaping = false;

  for (let i = 0; i < normalized.length; i++) {
    const char = normalized[i];

    if (isEscaping) {
      current += char;
      isEscaping = false;
      continue;
    }

    if (char === "\\") {
      isEscaping = true;
      continue;
    }

    if (char === "'" && !inDoubleQuote) {
      inSingleQuote = !inSingleQuote;
      continue;
    }

    if (char === '"' && !inSingleQuote) {
      inDoubleQuote = !inDoubleQuote;
      continue;
    }

    if (/\s/.test(char) && !inSingleQuote && !inDoubleQuote) {
      if (current.length > 0) {
        tokens.push(current);
        current = "";
      }
      continue;
    }

    current += char;
  }

  if (current.length > 0) {
    tokens.push(current);
  }

  return tokens;
}

/**
 * 将 cURL 命令行解析为结构化 HTTP 请求模型
 */
export function parseCurlCommand(rawCmd: string): ParsedHttpRequest {
  const tokens = tokenizeCurl(rawCmd.trim());
  let method = "GET";
  let fullUrl = "";
  const headers: KeyValuePair[] = [];
  const bodyChunks: string[] = [];

  for (let i = 0; i < tokens.length; i++) {
    const token = tokens[i];

    if (token === "curl") {
      continue;
    }

    // Method
    if ((token === "-X" || token === "--request") && i + 1 < tokens.length) {
      method = tokens[++i].toUpperCase();
      continue;
    }

    // Headers
    if ((token === "-H" || token === "--header") && i + 1 < tokens.length) {
      const headerStr = tokens[++i];
      const colonIdx = headerStr.indexOf(":");
      if (colonIdx > 0) {
        const name = headerStr.slice(0, colonIdx).trim();
        const value = headerStr.slice(colonIdx + 1).trim();
        headers.push({ name, value, enabled: true });
      }
      continue;
    }

    // Body
    if (
      (token === "-d" ||
        token === "--data" ||
        token === "--data-raw" ||
        token === "--data-binary") &&
      i + 1 < tokens.length
    ) {
      bodyChunks.push(tokens[++i]);
      if (method === "GET") method = "POST";
      continue;
    }

    // User-Agent
    if ((token === "-A" || token === "--user-agent") && i + 1 < tokens.length) {
      headers.push({ name: "User-Agent", value: tokens[++i], enabled: true });
      continue;
    }

    // Cookie
    if ((token === "-b" || token === "--cookie") && i + 1 < tokens.length) {
      headers.push({ name: "Cookie", value: tokens[++i], enabled: true });
      continue;
    }

    // Basic Auth
    if ((token === "-u" || token === "--user") && i + 1 < tokens.length) {
      const userPass = tokens[++i];
      const encoded =
        typeof Buffer !== "undefined"
          ? Buffer.from(userPass).toString("base64")
          : btoa(userPass);
      headers.push({
        name: "Authorization",
        value: `Basic ${encoded}`,
        enabled: true,
      });
      continue;
    }

    // URL（以 http:// 或 https:// 开头，或非开关项）
    if (!token.startsWith("-") && !fullUrl) {
      fullUrl = token;
    }
  }

  // 拆解 URL 和 Query 参数
  let baseUrl = fullUrl;
  const params: KeyValuePair[] = [];

  if (fullUrl) {
    try {
      const parsedUrl = new URL(fullUrl.startsWith("http") ? fullUrl : `http://${fullUrl}`);
      baseUrl = `${parsedUrl.origin}${parsedUrl.pathname}`;
      parsedUrl.searchParams.forEach((value, name) => {
        params.push({ name, value, enabled: true });
      });
    } catch {
      baseUrl = fullUrl;
    }
  }

  return {
    method,
    url: fullUrl,
    baseUrl: baseUrl || fullUrl,
    params,
    headers,
    body: bodyChunks.length > 0 ? bodyChunks.join("&") : undefined,
  };
}

/**
 * 组装当前激活的完整 URL（包含启用的 Query 参数）
 */
export function buildEffectiveUrl(
  baseUrl: string,
  params: KeyValuePair[],
): string {
  const activeParams = params.filter((p) => p.enabled && p.name.trim());
  if (activeParams.length === 0) return baseUrl;

  try {
    const urlObj = new URL(baseUrl.startsWith("http") ? baseUrl : `http://${baseUrl}`);
    for (const p of activeParams) {
      urlObj.searchParams.append(p.name, p.value);
    }
    return urlObj.toString();
  } catch {
    const queryStr = activeParams
      .map(
        (p) =>
          `${encodeURIComponent(p.name)}=${encodeURIComponent(p.value)}`,
      )
      .join("&");
    return baseUrl.includes("?") ? `${baseUrl}&${queryStr}` : `${baseUrl}?${queryStr}`;
  }
}

/**
 * 多语言导出代码生成器
 */
export function generateCode(
  req: ParsedHttpRequest,
  lang: CodeExportLanguage,
): string {
  const effectiveUrl = buildEffectiveUrl(req.baseUrl, req.params);
  const activeHeaders = req.headers.filter((h) => h.enabled && h.name.trim());

  switch (lang) {
    case "typescript-fetch": {
      const headersObj = Object.fromEntries(
        activeHeaders.map((h) => [h.name, h.value]),
      );
      const options: Record<string, unknown> = {
        method: req.method,
      };
      if (activeHeaders.length > 0) {
        options.headers = headersObj;
      }
      if (req.body && req.method !== "GET" && req.method !== "HEAD") {
        options.body = req.body;
      }

      return `// TypeScript fetch
const response = await fetch('${effectiveUrl}', {
  method: '${req.method}',
${
  activeHeaders.length > 0
    ? `  headers: ${JSON.stringify(headersObj, null, 2).replace(/\n/g, "\n  ")},\n`
    : ""
}${
        req.body && req.method !== "GET" && req.method !== "HEAD"
          ? `  body: ${JSON.stringify(req.body)},\n`
          : ""
}});
const data = await response.json();
console.log(data);`;
    }

    case "python-requests": {
      const headersDict = activeHeaders.length > 0
        ? `headers = {\n${activeHeaders
            .map((h) => `    "${h.name}": "${h.value}",`)
            .join("\n")}\n}\n`
        : "";

      const bodyParam =
        req.body && req.method !== "GET"
          ? `, data='''${req.body}'''`
          : "";
      const headersParam = activeHeaders.length > 0 ? ", headers=headers" : "";

      return `import requests

url = "${effectiveUrl}"
${headersDict}response = requests.${req.method.toLowerCase()}(url${headersParam}${bodyParam})
print(response.status_code)
print(response.text)`;
    }

    case "go-nethttp": {
      return `package main

import (
\t"fmt"
\t"io"
\t"net/http"
\t"strings"
)

func main() {
\turl := "${effectiveUrl}"
\tvar body io.Reader
${
  req.body && req.method !== "GET"
    ? `\tbody = strings.NewReader(\`${req.body}\`)\n`
    : ""
}\treq, err := http.NewRequest("${req.method}", url, body)
\tif err != nil {
\t\tpanic(err)
\t}

${activeHeaders
  .map((h) => `\treq.Header.Set("${h.name}", "${h.value}")`)
  .join("\n")}

\tclient := &http.Client{}
\tresp, err := client.Do(req)
\tif err != nil {
\t\tpanic(err)
\t}
\tdefer resp.Body.Close()

\tbytes, _ := io.ReadAll(resp.Body)
\tfmt.Println(string(bytes))
}`;
    }

    case "rust-reqwest": {
      return `use reqwest::header::HeaderMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let mut headers = HeaderMap::new();
${activeHeaders
  .map(
    (h) =>
      `    headers.insert("${h.name.toLowerCase()}", "${h.value}".parse()?);`,
  )
  .join("\n")}

    let response = client
        .${req.method.toLowerCase()}("${effectiveUrl}")
        .headers(headers)
${
  req.body && req.method !== "GET"
    ? `        .body(r#"${req.body}"#)\n`
    : ""
}        .send()
        .await?;

    let text = response.text().await?;
    println!("{}", text);
    Ok(())
}`;
    }

    case "curl":
    default: {
      const parts = [`curl -X ${req.method} "${effectiveUrl}"`];
      for (const h of activeHeaders) {
        parts.push(`  -H "${h.name}: ${h.value}"`);
      }
      if (req.body && req.method !== "GET") {
        parts.push(`  --data-raw '${req.body}'`);
      }
      return parts.join(" \\\n");
    }
  }
}
