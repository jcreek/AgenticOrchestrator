const LANGUAGES: Record<string, string> = {
  ts: "typescript",
  tsx: "typescript",
  mts: "typescript",
  cts: "typescript",
  js: "javascript",
  jsx: "javascript",
  mjs: "javascript",
  cjs: "javascript",
  // JSON's own language needs a dedicated worker; the JavaScript tokenizer colours it well enough.
  json: "javascript",
  jsonc: "javascript",
  css: "css",
  html: "html",
  htm: "html",
  md: "markdown",
  markdown: "markdown",
  rs: "rust",
  cs: "csharp",
  csx: "csharp",
  py: "python",
  yml: "yaml",
  yaml: "yaml",
  xml: "xml",
  xaml: "xml",
  svg: "xml",
  csproj: "xml",
  props: "xml",
  targets: "xml",
  resx: "xml",
  config: "xml",
  sql: "sql",
  ps1: "powershell",
  psm1: "powershell",
  psd1: "powershell",
  sh: "shell",
  bash: "shell",
  zsh: "shell",
};

export function languageFor(path: string): string {
  const name = path.split("/").pop() ?? "";
  const dot = name.lastIndexOf(".");
  if (dot <= 0) return "plaintext";
  return LANGUAGES[name.slice(dot + 1).toLowerCase()] ?? "plaintext";
}

const hex2 = (value: number) => Math.round(value).toString(16).padStart(2, "0");

/** Monaco theme colours must be hex; theme.css uses hex and rgba(). */
export function cssColorToHex(value: string): string {
  const color = value.trim();
  const short = /^#([0-9a-f])([0-9a-f])([0-9a-f])$/i.exec(color);
  if (short) return `#${short[1]}${short[1]}${short[2]}${short[2]}${short[3]}${short[3]}`.toLowerCase();
  if (/^#([0-9a-f]{6}|[0-9a-f]{8})$/i.test(color)) return color.toLowerCase();
  const rgb = /^rgba?\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)\s*(?:,\s*([\d.]+)\s*)?\)$/i.exec(color);
  if (!rgb) return "#00000000";
  const alpha = rgb[4] === undefined ? "" : hex2(Number(rgb[4]) * 255);
  return `#${hex2(Number(rgb[1]))}${hex2(Number(rgb[2]))}${hex2(Number(rgb[3]))}${alpha}`;
}
