import { describe, expect, it } from "vitest";
import {
  generateCode,
  parseCurlCommand,
} from "./curlParser";

describe("curlParser", () => {
  it("parses simple GET curl command", () => {
    const curl = 'curl "https://api.github.com/users/octocat"';
    const req = parseCurlCommand(curl);

    expect(req.method).toBe("GET");
    expect(req.url).toBe("https://api.github.com/users/octocat");
    expect(req.headers).toEqual([]);
    expect(req.body).toBeUndefined();
  });

  it("parses POST curl command with headers and json body", () => {
    const curl = `curl 'https://api.example.com/v1/users' \\
  -X POST \\
  -H 'Content-Type: application/json' \\
  -H 'Authorization: Bearer token_abc123' \\
  --data-raw '{"name":"Alice","role":"engineer"}'`;

    const req = parseCurlCommand(curl);

    expect(req.method).toBe("POST");
    expect(req.url).toBe("https://api.example.com/v1/users");
    expect(req.headers).toContainEqual({
      name: "Content-Type",
      value: "application/json",
      enabled: true,
    });
    expect(req.headers).toContainEqual({
      name: "Authorization",
      value: "Bearer token_abc123",
      enabled: true,
    });
    expect(req.body).toBe('{"name":"Alice","role":"engineer"}');
  });

  it("parses curl command with cookies and basic auth", () => {
    const curl = `curl -u admin:secret123 -b "session_id=xyz; theme=dark" https://example.com/admin`;
    const req = parseCurlCommand(curl);

    expect(req.url).toBe("https://example.com/admin");
    const authHeader = req.headers.find((h) => h.name.toLowerCase() === "authorization");
    expect(authHeader).toBeDefined();
    expect(authHeader?.value).toMatch(/^Basic /);

    const cookieHeader = req.headers.find((h) => h.name.toLowerCase() === "cookie");
    expect(cookieHeader).toBeDefined();
    expect(cookieHeader?.value).toBe("session_id=xyz; theme=dark");
  });

  it("splits url query parameters properly", () => {
    const curl = `curl "https://api.example.com/search?q=rust&limit=20&page=1"`;
    const req = parseCurlCommand(curl);

    expect(req.baseUrl).toBe("https://api.example.com/search");
    expect(req.params).toEqual([
      { name: "q", value: "rust", enabled: true },
      { name: "limit", value: "20", enabled: true },
      { name: "page", value: "1", enabled: true },
    ]);
  });

  it("generates correct code in TypeScript, Python, Go, and Rust", () => {
    const req = parseCurlCommand(
      `curl -X POST https://api.example.com/login -H "Content-Type: application/json" -d '{"user":"test"}'`,
    );

    const tsCode = generateCode(req, "typescript-fetch");
    expect(tsCode).toContain("fetch('https://api.example.com/login'");
    expect(tsCode).toContain("method: 'POST'");
    expect(tsCode).toContain('"Content-Type": "application/json"');

    const pyCode = generateCode(req, "python-requests");
    expect(pyCode).toContain("import requests");
    expect(pyCode).toContain("requests.post(");

    const goCode = generateCode(req, "go-nethttp");
    expect(goCode).toContain("http.NewRequest");

    const rustCode = generateCode(req, "rust-reqwest");
    expect(rustCode).toContain("reqwest::Client::new()");
  });
});
