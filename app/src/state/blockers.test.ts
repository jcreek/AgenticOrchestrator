import { describe, expect, it } from "vitest";
import { describeBlockers, hasUncommitted } from "./blockers";

describe("describeBlockers", () => {
  it("lists what removal would lose in plain words", () => {
    expect(
      describeBlockers([
        { kind: "modifiedFiles", count: 3 },
        { kind: "untrackedFiles", count: 1 },
        { kind: "unpushedCommits", count: 2 },
      ]),
    ).toBe("3 modified files, 1 untracked file and 2 unpushed commits");
    expect(describeBlockers([{ kind: "ignoredFiles", count: 1 }])).toBe("1 ignored file");
    expect(describeBlockers([])).toBe("");
  });

  it("knows which blockers a commit can clear", () => {
    expect(hasUncommitted([{ kind: "unpushedCommits", count: 1 }])).toBe(false);
    expect(hasUncommitted([{ kind: "untrackedFiles", count: 1 }])).toBe(true);
  });
});
