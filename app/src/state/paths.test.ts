import { describe, expect, it } from "vitest";
import { baseName, groupFolder, pathSafe, samePath, worktreeFolder } from "./paths";

describe("groupFolder", () => {
  it("turns the git common dir into the repo folder", () => {
    expect(groupFolder("\\\\?\\C:\\dev\\web-console\\.git")).toBe("C:\\dev\\web-console");
    expect(groupFolder("C:\\dev\\bare.git")).toBe("C:\\dev\\bare.git");
    expect(groupFolder("\\\\?\\UNC\\server\\share\\repo\\.git")).toBe("\\\\server\\share\\repo");
  });
});

describe("worktreeFolder", () => {
  it("defaults to a sibling .worktrees folder and makes the branch path-safe", () => {
    expect(worktreeFolder("C:\\dev\\web-console", "feat/export-csv", null)).toBe(
      "C:\\dev\\web-console.worktrees\\feat-export-csv",
    );
  });

  it("uses the configured worktree root", () => {
    expect(worktreeFolder("C:\\dev\\api\\", "fix_1.2", "D:\\trees\\")).toBe("D:\\trees\\fix_1.2");
  });
});

describe("pathSafe", () => {
  it("keeps letters, digits, dash, underscore and dot", () => {
    expect(pathSafe("feat/ünïcode 2.x")).toBe("feat-ünïcode-2.x");
  });
});

describe("samePath and baseName", () => {
  it("compares Windows paths loosely", () => {
    expect(samePath("C:\\Dev\\Repo\\", "c:/dev/repo")).toBe(true);
    expect(samePath("C:\\dev\\repo", "C:\\dev\\repo2")).toBe(false);
  });

  it("returns the last segment", () => {
    expect(baseName("C:\\dev\\web-console\\")).toBe("web-console");
  });
});
