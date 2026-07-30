const MD5_SHIFT_AMOUNTS = [
  7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5,
  9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11,
  16, 23, 4, 11, 16, 23, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15,
  21,
];

const MD5_K = Array.from(
  { length: 64 },
  (_, i) => Math.floor(Math.abs(Math.sin(i + 1)) * 2 ** 32) >>> 0,
);

const leftRotate = (value: number, amount: number) =>
  ((value << amount) | (value >>> (32 - amount))) >>> 0;

const toHexLE = (word: number) =>
  [word & 0xff, (word >>> 8) & 0xff, (word >>> 16) & 0xff, (word >>> 24) & 0xff]
    .map((value) => value.toString(16).padStart(2, "0"))
    .join("");

export function md5Hex(input: string): string {
  const bytes = Array.from(new TextEncoder().encode(input));
  const bitLen = bytes.length * 8;
  const bitLenLow = bitLen >>> 0;
  const bitLenHigh = Math.floor(bitLen / 2 ** 32) >>> 0;
  bytes.push(0x80);
  while (bytes.length % 64 !== 56) {
    bytes.push(0);
  }
  for (let i = 0; i < 4; i += 1) {
    bytes.push((bitLenLow >>> (8 * i)) & 0xff);
  }
  for (let i = 0; i < 4; i += 1) {
    bytes.push((bitLenHigh >>> (8 * i)) & 0xff);
  }

  let a0 = 0x67452301;
  let b0 = 0xefcdab89;
  let c0 = 0x98badcfe;
  let d0 = 0x10325476;

  for (let offset = 0; offset < bytes.length; offset += 64) {
    const m = new Array<number>(16).fill(0);
    for (let i = 0; i < 16; i += 1) {
      const j = offset + i * 4;
      m[i] =
        (bytes[j] as number) |
        ((bytes[j + 1] as number) << 8) |
        ((bytes[j + 2] as number) << 16) |
        ((bytes[j + 3] as number) << 24);
    }

    let a = a0;
    let b = b0;
    let c = c0;
    let d = d0;

    for (let i = 0; i < 64; i += 1) {
      let f = 0;
      let g = 0;

      if (i < 16) {
        f = (b & c) | (~b & d);
        g = i;
      } else if (i < 32) {
        f = (d & b) | (~d & c);
        g = (5 * i + 1) % 16;
      } else if (i < 48) {
        f = b ^ c ^ d;
        g = (3 * i + 5) % 16;
      } else {
        f = c ^ (b | ~d);
        g = (7 * i) % 16;
      }

      const temp = d;
      d = c;
      c = b;
      const mixed = (a + f + MD5_K[i] + m[g]) >>> 0;
      b = (b + leftRotate(mixed, MD5_SHIFT_AMOUNTS[i])) >>> 0;
      a = temp;
    }

    a0 = (a0 + a) >>> 0;
    b0 = (b0 + b) >>> 0;
    c0 = (c0 + c) >>> 0;
    d0 = (d0 + d) >>> 0;
  }

  return `${toHexLE(a0)}${toHexLE(b0)}${toHexLE(c0)}${toHexLE(d0)}`;
}
