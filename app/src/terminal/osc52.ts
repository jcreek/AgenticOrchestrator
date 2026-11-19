export const OSC52 = 52;

/**
 * The text an OSC 52 "set clipboard" sequence (`targets;base64`) carries. Null for malformed data,
 * an empty payload, or a `?` query, which is never answered so programs cannot read the clipboard.
 */
export function osc52Text(data: string): string | null {
  const separator = data.indexOf(";");
  const payload = separator < 0 ? "" : data.slice(separator + 1);
  if (payload === "" || payload === "?") return null;
  try {
    const bytes = Uint8Array.from(atob(payload), (char) => char.charCodeAt(0));
    return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    return null;
  }
}
