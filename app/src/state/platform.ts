export type Platform = "macos" | "other";

export function detectPlatform(value: string): Platform {
  return /Mac|iPhone|iPad/i.test(value) ? "macos" : "other";
}

export const platform: Platform = detectPlatform(typeof navigator === "undefined" ? "" : navigator.platform);
