import { describe, expect, it } from "vitest";
import { closeGuard } from "./closeGuard";

const at = (status: "working" | "idle" | "exited" | "starting") => ({ status, since: 0, exitCode: status === "exited" ? 0 : null });

describe("closeGuard", () => {
  it("closes at once when nothing runs and nothing is unsaved", () => {
    expect(closeGuard({}, {})).toBeNull();
    expect(closeGuard({ a: at("exited") }, { a: false })).toBeNull();
  });

  it("counts every Claude that has not exited, idle ones included", () => {
    expect(closeGuard({ a: at("working"), b: at("idle"), c: at("starting"), d: at("exited") }, {})).toEqual({
      running: 3,
      unsaved: 0,
    });
  });

  it("counts editors with unsaved edits", () => {
    expect(closeGuard({ a: at("exited") }, { a: true, b: false, c: true })).toEqual({ running: 0, unsaved: 2 });
  });
});
