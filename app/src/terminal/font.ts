const BUILT_IN_FONTS = '"Cascadia Mono", Consolas, ui-monospace, "SF Mono", Menlo, monospace';

export function terminalFontFamily(custom: string): string {
  const name = custom.replace(/"/g, "").trim();
  return name ? `"${name}", ${BUILT_IN_FONTS}` : BUILT_IN_FONTS;
}
