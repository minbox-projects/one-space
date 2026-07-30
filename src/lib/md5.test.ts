import { describe, expect, it } from "vitest";
import { md5Hex } from "@/lib/md5";

describe("md5Hex", () => {
  it.each([
    ["", "d41d8cd98f00b204e9800998ecf8427e"],
    ["a", "0cc175b9c0f1b6a831c399e269772661"],
    ["abc", "900150983cd24fb0d6963f7d28e17f72"],
    ["中文", "a7bac2239fcdcb3a067903d8077c4a07"],
    [" ", "7215ee9c7d9dc229d2921a40e899ec5f"],
    ["\t", "5e732a1878be2342dbfeff5fe3ca5aa3"],
    ["\n", "68b329da9893e34099c7d8ad5cb9c940"],
    ["\r\n", "81051bcc2cf1bedf378224b0a93e2877"],
    [" abc ", "01c9a8945abead949b46c77cf3245b8a"],
  ])("hashes %j as its original UTF-8 bytes", (input, expected) => {
    expect(md5Hex(input)).toBe(expected);
    expect(md5Hex(input)).toMatch(/^[0-9a-f]{32}$/);
  });

  it("does not normalize Unicode input", () => {
    expect(md5Hex("é")).toBe("66ddcd97cfdeabb2f6fb8a999b4bc76f");
    expect(md5Hex("e\u0301")).toBe("5526861fbb1e71a1bda6ac364310a807");
    expect(md5Hex("é")).not.toBe(md5Hex("e\u0301"));
  });

  it("keeps LF and CRLF distinct", () => {
    expect(md5Hex("line\nend")).toBe("98143b220546868e4edba99b20f1ff97");
    expect(md5Hex("line\r\nend")).toBe("2dc8f4b282f6dec7471173938ea4bd41");
  });
});
