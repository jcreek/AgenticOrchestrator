import type { AppError } from "../bindings";

export type Conflict = "changed" | "deleted";

export interface EditorDoc {
  phase: "loading" | "ready" | "error";
  version: string | null;
  dirty: boolean;
  saving: boolean;
  conflict: Conflict | null;
  /** The load failure in phase "error"; the last save failure in phase "ready". */
  error: AppError | null;
}

export type EditorAction =
  | { type: "load" }
  | { type: "loaded"; version: string }
  | { type: "loadFailed"; error: AppError }
  | { type: "edited"; dirty: boolean }
  | { type: "saveStarted" }
  | { type: "saved"; version: string; dirty: boolean }
  | { type: "saveFailed"; error: AppError }
  | { type: "conflict"; conflict: Conflict };

export const initialDoc: EditorDoc = {
  phase: "loading",
  version: null,
  dirty: false,
  saving: false,
  conflict: null,
  error: null,
};

export function editorReducer(doc: EditorDoc, action: EditorAction): EditorDoc {
  switch (action.type) {
    case "load":
      return { ...doc, phase: "loading", error: null };
    case "loaded":
      return { ...initialDoc, phase: "ready", version: action.version };
    case "loadFailed":
      return { ...initialDoc, phase: "error", error: action.error };
    case "edited":
      return doc.dirty === action.dirty ? doc : { ...doc, dirty: action.dirty };
    case "saveStarted":
      return { ...doc, saving: true, error: null };
    case "saved":
      return { ...doc, saving: false, version: action.version, dirty: action.dirty, conflict: null };
    case "saveFailed":
      if (action.error.kind === "conflict") return { ...doc, saving: false, conflict: "changed" };
      return { ...doc, saving: false, error: action.error };
    case "conflict":
      return { ...doc, conflict: action.conflict };
  }
}

export type DiskDecision = "none" | "reload" | "conflict";

/** What to do when polling finds `diskVersion` (null: the file is gone) for the open document. */
export function diskDecision(doc: EditorDoc, diskVersion: string | null): DiskDecision {
  if (doc.phase !== "ready" || doc.saving || doc.conflict) return "none";
  if (diskVersion === doc.version) return "none";
  return doc.dirty || diskVersion === null ? "conflict" : "reload";
}

export const canSave = (doc: EditorDoc) => doc.phase === "ready" && doc.dirty && !doc.saving && !doc.conflict;
