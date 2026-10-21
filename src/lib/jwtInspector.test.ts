import { describe, expect, it } from "vitest";
import {
  decodeBase64Url,
  extractSubjectContext,
  parseJwt,
  verifyJwtSignature,
} from "./jwtInspector";

describe("jwtInspector", () => {
  // 标准 HS256 JWT 测试用例 (Header: {"alg":"HS256","typ":"JWT"}, Payload: {"sub":"1234567890","name":"John Doe","iat":1516239022})
  // 正确签名 (使用 secret: "your-256-bit-secret")
  const SAMPLE_JWT =
    "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";

  it("decodes Base64Url string with utf-8 text", () => {
    // 包含中文等 UTF-8 内容
    const text = '{"user":"张三","dept":"研发部"}';
    const b64 = Buffer.from(text, "utf-8").toString("base64url");
    expect(decodeBase64Url(b64)).toBe(text);
  });

  it("parses standard JWT structure and extracts claims", () => {
    const result = parseJwt(SAMPLE_JWT);
    expect(result.valid).toBe(true);
    expect(result.header.alg).toBe("HS256");
    expect(result.header.typ).toBe("JWT");
    expect(result.payload.sub).toBe("1234567890");
    expect(result.payload.name).toBe("John Doe");
    expect(result.rawSignature).toBe("SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c");
  });

  it("extracts comprehensive subject context and roles/permissions", () => {
    const richPayload = {
      sub: "user-9988",
      username: "alex_dev",
      email: "alex@example.com",
      tenant_id: "tenant-cloud-01",
      roles: ["admin", "developer"],
      permissions: ["user:read", "user:write", "cluster:deploy"],
      custom_dept: "Platform Engineering",
      exp: 1800000000,
      iat: 1700000000,
    };

    const header = Buffer.from(JSON.stringify({ alg: "HS256" })).toString("base64url");
    const payload = Buffer.from(JSON.stringify(richPayload)).toString("base64url");
    const fakeToken = `${header}.${payload}.fakesig`;

    const result = parseJwt(fakeToken);
    expect(result.valid).toBe(true);

    const subject = result.subjectContext;
    expect(subject.subjectId).toBe("user-9988");
    expect(subject.subjectName).toBe("alex_dev");
    expect(subject.email).toBe("alex@example.com");
    expect(subject.tenantId).toBe("tenant-cloud-01");
    expect(subject.roles).toEqual(["admin", "developer"]);
    expect(subject.permissions).toEqual(["user:read", "user:write", "cluster:deploy"]);
    expect(subject.customClaims).toHaveProperty("custom_dept", "Platform Engineering");
  });

  it("handles string-based roles and space-separated scopes", () => {
    const scopePayload = {
      uid: "u-123",
      role: "editor",
      scope: "read write email",
    };
    const subject = extractSubjectContext(scopePayload);
    expect(subject.subjectId).toBe("u-123");
    expect(subject.roles).toEqual(["editor"]);
    expect(subject.permissions).toEqual(["read", "write", "email"]);
  });

  it("accurately calculates lifecycle and expiration status", () => {
    const nowSec = 1700000500;
    // 已过期 Token
    const expiredPayload = {
      iat: 1700000000,
      exp: 1700000400, // 过期时间在当前之前
    };
    const header = Buffer.from(JSON.stringify({ alg: "HS256" })).toString("base64url");
    const payload = Buffer.from(JSON.stringify(expiredPayload)).toString("base64url");
    const token = `${header}.${payload}.sig`;

    const result = parseJwt(token, nowSec);
    expect(result.valid).toBe(true);
    expect(result.lifecycle.status).toBe("expired");
    expect(result.lifecycle.remainingSeconds).toBe(-100);

    // 有效 Token
    const validPayload = {
      iat: 1700000000,
      exp: 1700001000, // 剩余 500 秒
    };
    const validToken = `${header}.${Buffer.from(JSON.stringify(validPayload)).toString("base64url")}.sig`;
    const validResult = parseJwt(validToken, nowSec);
    expect(validResult.lifecycle.status).toBe("valid");
    expect(validResult.lifecycle.remainingSeconds).toBe(500);
    expect(validResult.lifecycle.progressPercentage).toBe(50);
  });

  it("handles invalid or malformed JWT strings gracefully", () => {
    const result = parseJwt("invalid.token");
    expect(result.valid).toBe(false);
    expect(result.error).toBeTruthy();
  });

  it("verifies HS256 signature using Web Crypto API", async () => {
    const verification = await verifyJwtSignature(
      SAMPLE_JWT,
      "your-256-bit-secret",
      "HS256",
    );
    expect(verification.verified).toBe(true);

    const wrongKey = await verifyJwtSignature(
      SAMPLE_JWT,
      "wrong-secret",
      "HS256",
    );
    expect(wrongKey.verified).toBe(false);
  });
});
