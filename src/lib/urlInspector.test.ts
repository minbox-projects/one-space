import { describe, expect, it } from "vitest";
import {
  generateExportCode,
  looksLikeUrl,
  parseUrlHierarchy,
  rebuildNodeUrl,
} from "./urlInspector";

describe("urlInspector core", () => {
  it("detects whether a string looks like a URL", () => {
    expect(looksLikeUrl("https://example.com")).toBe(true);
    expect(looksLikeUrl("http://localhost:3000/api")).toBe(true);
    expect(looksLikeUrl("https%3A%2F%2Fexample.com%2Fcallback")).toBe(true);
    expect(looksLikeUrl("not-a-url")).toBe(false);
    expect(looksLikeUrl("12345")).toBe(false);
  });

  it("parses standard URL with query parameters and hash", () => {
    const raw = "https://api.example.com:8080/v1/users?page=1&limit=20&sort=desc#results";
    const result = parseUrlHierarchy(raw);

    expect(result).not.toBeNull();
    if (!result) return;

    const rootNode = result.nodes[result.rootId];
    expect(rootNode.hostname).toBe("api.example.com");
    expect(rootNode.port).toBe("8080");
    expect(rootNode.pathname).toBe("/v1/users");
    expect(rootNode.hash).toBe("#results");
    expect(rootNode.params).toHaveLength(3);
    expect(rootNode.params[0].key).toBe("page");
    expect(rootNode.params[0].value).toBe("1");
    expect(rootNode.params[1].key).toBe("limit");
    expect(rootNode.params[1].value).toBe("20");
  });

  it("handles URLs without explicit protocol gracefully", () => {
    const raw = "github.com/search?q=tauri&type=code";
    const result = parseUrlHierarchy(raw);

    expect(result).not.toBeNull();
    if (!result) return;

    const rootNode = result.nodes[result.rootId];
    expect(rootNode.hostname).toBe("github.com");
    expect(rootNode.pathname).toBe("/search");
    expect(rootNode.params).toHaveLength(2);
  });

  it("recursively parses nested OAuth redirect URLs", () => {
    const innerUrl = "https://myapp.com/auth/callback?code=abc12345&state=random_state";
    const outerUrl = `https://auth.provider.com/oauth/authorize?client_id=client_999&redirect_uri=${encodeURIComponent(
      innerUrl,
    )}&response_type=code`;

    const result = parseUrlHierarchy(outerUrl);
    expect(result).not.toBeNull();
    if (!result) return;

    const rootNode = result.nodes[result.rootId];
    expect(rootNode.params).toHaveLength(3);

    const redirectParam = rootNode.params.find((p) => p.key === "redirect_uri");
    expect(redirectParam).toBeDefined();
    expect(redirectParam?.isNestedUrl).toBe(true);
    expect(redirectParam?.nestedChildId).toBeDefined();

    if (redirectParam?.nestedChildId) {
      const childNode = result.nodes[redirectParam.nestedChildId];
      expect(childNode).toBeDefined();
      expect(childNode.hostname).toBe("myapp.com");
      expect(childNode.pathname).toBe("/auth/callback");
      expect(childNode.params).toHaveLength(2);
      expect(childNode.params[0].key).toBe("code");
      expect(childNode.params[0].value).toBe("abc12345");
      expect(childNode.parentKeyName).toBe("redirect_uri");
    }
  });

  it("rebuilds URL correctly when params are disabled or modified", () => {
    const raw = "https://example.com/test?a=1&b=2&c=3";
    const result = parseUrlHierarchy(raw);
    expect(result).not.toBeNull();
    if (!result) return;

    const rootNode = result.nodes[result.rootId];
    // Disable parameter 'b'
    rootNode.params[1].enabled = false;
    // Modify parameter 'a'
    rootNode.params[0].value = "100";

    const rebuilt = rebuildNodeUrl(result.rootId, result.nodes);
    expect(rebuilt).toBe("https://example.com/test?a=100&c=3");
  });

  it("rebuilds nested URL bottom-up when child params change", () => {
    const childUrl = "https://child.com/cb?target=dashboard&v=1";
    const parentUrl = `https://parent.com/sso?redirect=${encodeURIComponent(childUrl)}&app=onespace`;

    const result = parseUrlHierarchy(parentUrl);
    expect(result).not.toBeNull();
    if (!result) return;

    const rootNode = result.nodes[result.rootId];
    const redirectParam = rootNode.params.find((p) => p.key === "redirect");
    expect(redirectParam?.nestedChildId).toBeDefined();

    const childNode = result.nodes[redirectParam!.nestedChildId!];
    // Change child parameter target to 'settings'
    childNode.params[0].value = "settings";

    const rebuilt = rebuildNodeUrl(result.rootId, result.nodes);
    expect(rebuilt).toContain("https://parent.com/sso?");
    expect(rebuilt).toContain(encodeURIComponent("https://child.com/cb?target=settings&v=1"));
  });

  it("generates correct export codes", () => {
    const url = "https://api.com/v1?token=xyz";
    const params = [{ id: "1", key: "token", value: "xyz", enabled: true, isNestedUrl: false }];

    const curl = generateExportCode("curl", url, params);
    expect(curl).toBe('curl -X GET "https://api.com/v1?token=xyz"');

    const json = generateExportCode("json", url, params);
    expect(JSON.parse(json)).toEqual({ token: "xyz" });

    const md = generateExportCode("markdown", url, params);
    expect(md).toContain("| `token` | `xyz` |");
  });

  it("handles Chinese characters and special symbols accurately", () => {
    const raw = "https://example.com/search?keyword=%E6%B8%AC%E8%A9%A6&tag=c%2B%2B&space=hello+world";
    const result = parseUrlHierarchy(raw);
    expect(result).not.toBeNull();
    if (!result) return;

    const rootNode = result.nodes[result.rootId];
    expect(rootNode.params[0].key).toBe("keyword");
    expect(rootNode.params[0].value).toBe("測試");
    expect(rootNode.params[1].key).toBe("tag");
    expect(rootNode.params[1].value).toBe("c++");

    const rebuilt = rebuildNodeUrl(result.rootId, result.nodes);
    expect(rebuilt).toContain("keyword=%E6%B8%AC%E8%A9%A6");
  });

  it("handles 3-level deep nested redirect hierarchy", () => {
    const level3 = "https://leaf.com/welcome?source=mobile";
    const level2 = `https://gateway.com/route?target=${encodeURIComponent(level3)}&debug=true`;
    const level1 = `https://auth.com/login?redirect_uri=${encodeURIComponent(level2)}&app=onespace`;

    const result = parseUrlHierarchy(level1);
    expect(result).not.toBeNull();
    if (!result) return;

    const level1Node = result.nodes[result.rootId];
    const level1Redirect = level1Node.params.find((p) => p.key === "redirect_uri");
    expect(level1Redirect?.isNestedUrl).toBe(true);

    const level2Node = result.nodes[level1Redirect!.nestedChildId!];
    expect(level2Node.hostname).toBe("gateway.com");
    const level2Target = level2Node.params.find((p) => p.key === "target");
    expect(level2Target?.isNestedUrl).toBe(true);

    const level3Node = result.nodes[level2Target!.nestedChildId!];
    expect(level3Node.hostname).toBe("leaf.com");
    expect(level3Node.params[0].key).toBe("source");
    expect(level3Node.params[0].value).toBe("mobile");
  });
});
