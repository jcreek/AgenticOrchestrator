import { describe, expect, it } from "vitest";
import { osc52Text } from "./osc52";

const base64 = (text: string) => btoa(String.fromCharCode(...new TextEncoder().encode(text)));

describe("osc52Text", () => {
  it("decodes UTF-8 text for any clipboard target", () => {
    expect(osc52Text(`c;${base64("hello")}`)).toBe("hello");
    expect(osc52Text(`;${base64("café → ✓")}`)).toBe("café → ✓");
    expect(osc52Text(`pc;${base64("line 1\nline 2")}`)).toBe("line 1\nline 2");
  });

  it("ignores queries, empty payloads and malformed data", () => {
    for (const data of ["c;?", "c;", "", "no-separator", "c;not base64!", `c;${btoa("\xff\xfe")}`]) {
      expect(osc52Text(data), data).toBeNull();
    }
  });
});
