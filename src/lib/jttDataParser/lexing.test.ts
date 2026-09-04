import { describe, expect, it } from "vitest";
import {
  isAsciiWhitespaceChar,
  nonBlankSourceLines,
  splitSourceLines,
  stripInlineAsciiWhitespace,
  trimAsciiWhitespace,
} from "./lexing";

describe("lexing", () => {
  it("splits LF-separated input into one-based lines and trims surrounding ASCII whitespace", () => {
    expect(splitSourceLines("AB\r\n  CD\n\nEF\n")).toEqual([
      { text: "AB\r", lineNumber: 1 },
      { text: "  CD", lineNumber: 2 },
      { text: "", lineNumber: 3 },
      { text: "EF", lineNumber: 4 },
      { text: "", lineNumber: 5 },
    ]);
  });

  it("keeps only nonblank lines with their original one-based line numbers", () => {
    expect(nonBlankSourceLines("AB\n \t\r\nCD\n\n EF \n")).toEqual([
      { text: "AB", lineNumber: 1 },
      { text: "CD", lineNumber: 3 },
      { text: "EF", lineNumber: 5 },
    ]);
  });

  it("treats empty input as having no lines", () => {
    expect(nonBlankSourceLines("")).toEqual([]);
  });

  it("recognizes each ASCII whitespace character and nothing else", () => {
    expect([" ", "\t", "\n", "\r", "\f", "\v"].every(isAsciiWhitespaceChar)).toBe(true);
    expect(isAsciiWhitespaceChar("A")).toBe(false);
    expect(isAsciiWhitespaceChar("　")).toBe(false);
  });

  it("strips only inline ASCII whitespace from a line", () => {
    expect(stripInlineAsciiWhitespace(" 7E 08 01\t40 ")).toBe("7E080140");
  });

  it("trims surrounding ASCII whitespace but keeps inner content", () => {
    expect(trimAsciiWhitespace(" \t 7E 08 01\r\n")).toBe("7E 08 01");
  });
});