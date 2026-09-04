export type Jt808Mode =
  | "automatic"
  | "jt1078-extension"
  | "jiangsu-active-safety"
  | "guangdong-active-safety"
  | "force-2013";

export type Jt809Version = "2011" | "2019";
export type Jt809CryptoMode = "unencrypted" | "encrypted";

export type Jt809Uint32ParamError =
  | "missing"
  | "signed"
  | "fractional"
  | "nonnumeric"
  | "out-of-range";

export type Jt809ParamError = {
  field: "M1" | "IA1" | "IC1";
  kind: Jt809Uint32ParamError;
};

export type Jt809Uint32Params = { m1: number; ia1: number; ic1: number };

export type Jt1078Operation = "0x9101" | "0x9102" | "0x9205" | "0x9206";
export type Jt1078Direction = "upstream" | "downstream";

export type HexDirection = "hex-to-utf8" | "utf8-to-hex";

export type ResultNode = {
  label: string;
  value?: string;
  children?: ResultNode[];
};

export type AnalysisRecordKind = "success" | "unsupported" | "error";

export type AnalysisRecord = {
  kind: AnalysisRecordKind;
  line?: number;
  error?: string;
  tree: ResultNode[];
  json?: unknown;
};