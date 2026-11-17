import { describe, expect, it } from "vitest";
import { chunkText } from "./chunk";

describe("chunkText", () => {
  it("splits long text into pieces that join back to the original", () => {
    const text = "x".repeat(10_000);
    const chunks = chunkText(text, 4096);
    expect(chunks.map((chunk) => chunk.length)).toEqual([4096, 4096, 1808]);
    expect(chunks.join("")).toBe(text);
  });

  it("does not split a surrogate pair", () => {
    const chunks = chunkText("ab😀cd", 3);
    expect(chunks).toEqual(["ab", "😀c", "d"]);
  });

  it("returns nothing for empty text", () => {
    expect(chunkText("")).toEqual([]);
  });
});
