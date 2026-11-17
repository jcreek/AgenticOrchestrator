import { describe, expect, it } from "vitest";
import { terminalFontFamily } from "./font";

describe("terminal font family", () => {
  it("uses the built-in fonts when none is set", () => {
    expect(terminalFontFamily("  ")).toBe('"Cascadia Mono", Consolas, ui-monospace, "SF Mono", Menlo, monospace');
  });

  it("puts the chosen font first, quoted once", () => {
    expect(terminalFontFamily(' "CaskaydiaCove Nerd Font" ')).toBe(
      '"CaskaydiaCove Nerd Font", "Cascadia Mono", Consolas, ui-monospace, "SF Mono", Menlo, monospace',
    );
  });
});
