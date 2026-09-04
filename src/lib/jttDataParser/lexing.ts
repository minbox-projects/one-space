const ASCII_WHITESPACE_CHARS = new Set([" ", "\t", "\n", "\r", "\f", "\v"]);

export function isAsciiWhitespaceChar(char: string): boolean {
  return ASCII_WHITESPACE_CHARS.has(char);
}

export function trimAsciiWhitespace(text: string): string {
  let start = 0;
  let end = text.length;
  while (start < end && isAsciiWhitespaceChar(text[start])) start += 1;
  while (end > start && isAsciiWhitespaceChar(text[end - 1])) end -= 1;
  return text.slice(start, end);
}

export function stripInlineAsciiWhitespace(text: string): string {
  let output = "";
  for (const char of text) {
    if (!isAsciiWhitespaceChar(char)) output += char;
  }
  return output;
}

export type SourceLine = { text: string; lineNumber: number };

export function splitSourceLines(input: string): SourceLine[] {
  return input.split("\n").map((text, index) => ({ text, lineNumber: index + 1 }));
}

export function isBlankLine(line: string): boolean {
  return trimAsciiWhitespace(line) === "";
}

export function nonBlankSourceLines(input: string): SourceLine[] {
  return splitSourceLines(input)
    .map(({ text, lineNumber }) => ({ text: trimAsciiWhitespace(text), lineNumber }))
    .filter(({ text }) => text !== "");
}