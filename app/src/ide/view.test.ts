import { describe, expect, it } from "vitest";
import { cssColorToHex, languageFor } from "./view";

describe("language detection", () => {
  it.each([
    ["src/ide/IdePane.tsx", "typescript"],
    ["package.json", "javascript"],
    ["crates/core/src/git.rs", "rust"],
    ["App.CSPROJ", "xml"],
    ["build.ps1", "powershell"],
    ["Makefile", "plaintext"],
    [".gitignore", "plaintext"],
    ["notes.unknown", "plaintext"],
  ])("%s is %s", (path, language) => expect(languageFor(path)).toBe(language));
});

describe("css colours to hex", () => {
  it("passes hex through and expands short hex", () => {
    expect(cssColorToHex(" #16181D ")).toBe("#16181d");
    expect(cssColorToHex("#fff")).toBe("#ffffff");
  });

  it("converts rgba with alpha", () => {
    expect(cssColorToHex("rgba(76, 195, 138, 0.18)")).toBe("#4cc38a2e");
    expect(cssColorToHex("rgb(255,107,107)")).toBe("#ff6b6b");
  });

  it("falls back to transparent for anything else", () => {
    expect(cssColorToHex("")).toBe("#00000000");
  });
});
