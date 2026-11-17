import { expect, it } from "vitest";
import { detectPlatform } from "./platform";

it("detects Apple platforms without treating other desktops as Macs", () => {
  expect(detectPlatform("MacIntel")).toBe("macos");
  expect(detectPlatform("Win32")).toBe("other");
  expect(detectPlatform("Linux x86_64")).toBe("other");
});
