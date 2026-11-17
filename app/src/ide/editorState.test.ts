import { describe, expect, it } from "vitest";
import { canSave, diskDecision, type EditorAction, type EditorDoc, editorReducer, initialDoc } from "./editorState";

const run = (...actions: EditorAction[]): EditorDoc => actions.reduce(editorReducer, initialDoc);

describe("editor save state machine", () => {
  it("loads clean and becomes dirty on edit", () => {
    const doc = run({ type: "loaded", version: "v1" }, { type: "edited", dirty: true });
    expect(doc).toMatchObject({ phase: "ready", version: "v1", dirty: true });
    expect(canSave(doc)).toBe(true);
  });

  it("takes the new version after a save and stays dirty if typed during it", () => {
    const doc = run(
      { type: "loaded", version: "v1" },
      { type: "edited", dirty: true },
      { type: "saveStarted" },
      { type: "saved", version: "v2", dirty: true },
    );
    expect(doc).toMatchObject({ version: "v2", dirty: true, saving: false });
  });

  it("turns a conflict on save into the banner, never a silent overwrite", () => {
    const doc = run(
      { type: "loaded", version: "v1" },
      { type: "edited", dirty: true },
      { type: "saveStarted" },
      { type: "saveFailed", error: { kind: "conflict", message: "changed" } },
    );
    expect(doc).toMatchObject({ conflict: "changed", dirty: true, version: "v1", error: null });
    expect(canSave(doc)).toBe(false);
  });

  it("keeps other save errors inline and stays editable", () => {
    const doc = run(
      { type: "loaded", version: "v1" },
      { type: "edited", dirty: true },
      { type: "saveStarted" },
      { type: "saveFailed", error: { kind: "io", message: "denied" } },
    );
    expect(doc).toMatchObject({ conflict: null, error: { kind: "io" } });
    expect(canSave(doc)).toBe(true);
  });

  it("clears the banner once a save after Keep mine succeeds", () => {
    const doc = run(
      { type: "loaded", version: "v1" },
      { type: "edited", dirty: true },
      { type: "conflict", conflict: "changed" },
      { type: "saveStarted" },
      { type: "saved", version: "v3", dirty: false },
    );
    expect(doc).toMatchObject({ conflict: null, dirty: false, version: "v3" });
  });

  it("reload resets everything", () => {
    const doc = run({ type: "loaded", version: "v1" }, { type: "conflict", conflict: "changed" }, { type: "loaded", version: "v2" });
    expect(doc).toMatchObject({ conflict: null, dirty: false, version: "v2" });
  });
});

describe("disk change decision", () => {
  const clean = run({ type: "loaded", version: "v1" });
  const dirty = editorReducer(clean, { type: "edited", dirty: true });

  it("ignores an unchanged version", () => expect(diskDecision(clean, "v1")).toBe("none"));
  it("reloads silently without edits", () => expect(diskDecision(clean, "v2")).toBe("reload"));
  it("asks when there are edits", () => expect(diskDecision(dirty, "v2")).toBe("conflict"));
  it("asks when the file was deleted", () => expect(diskDecision(clean, null)).toBe("conflict"));
  it("stays quiet while saving or already asking", () => {
    expect(diskDecision(editorReducer(dirty, { type: "saveStarted" }), "v2")).toBe("none");
    expect(diskDecision(editorReducer(dirty, { type: "conflict", conflict: "changed" }), "v3")).toBe("none");
  });
});
