export const CHUNK_SIZE = 4096;

/** Splits text for the PTY without separating a surrogate pair. */
export function chunkText(text: string, size = CHUNK_SIZE): string[] {
  const chunks: string[] = [];
  let start = 0;
  while (start < text.length) {
    let end = Math.min(start + size, text.length);
    const last = text.charCodeAt(end - 1);
    if (end < text.length && last >= 0xd800 && last <= 0xdbff && end - start > 1) end--;
    chunks.push(text.slice(start, end));
    start = end;
  }
  return chunks;
}
