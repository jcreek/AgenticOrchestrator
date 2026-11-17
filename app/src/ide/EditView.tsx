import { useCallback, useEffect, useReducer, useRef } from "react";
import type * as Monaco from "monaco-editor/editor";
import { commands } from "../bindings";
import { canSave, diskDecision, editorReducer, initialDoc } from "./editorState";
import { describeError, toAppError } from "./errors";
import { type MonacoModule, usePoll } from "./hooks";
import { Banner, Message } from "./parts";
import { languageFor } from "./view";

const VERSION_POLL_MS = 2000;

export function EditView({ m, workspaceId, path, visible, onDirty, onCursorLine, onSaved, onOpenInVscode }: {
  m: MonacoModule;
  workspaceId: string;
  path: string;
  visible: boolean;
  onDirty: (dirty: boolean) => void;
  onCursorLine: (line: number) => void;
  onSaved: () => void;
  onOpenInVscode: () => void;
}) {
  const [doc, dispatch] = useReducer(editorReducer, initialDoc);
  const docRef = useRef(doc);
  const hostRef = useRef<HTMLDivElement>(null);
  const editorRef = useRef<Monaco.editor.IStandaloneCodeEditor | null>(null);
  const savedAlternative = useRef(0);
  const callbacks = useRef({ onCursorLine, onSaved, save: (_overwrite: boolean) => Promise.resolve() });

  useEffect(() => {
    docRef.current = doc;
  }, [doc]);

  useEffect(() => {
    onDirty(doc.dirty);
  }, [doc.dirty, onDirty]);

  useEffect(() => () => onDirty(false), [onDirty]);

  const load = useCallback(async (background = false) => {
    if (!background) dispatch({ type: "load" });
    try {
      const content = await commands.readFile(workspaceId, path);
      const editor = editorRef.current;
      const model = editor?.getModel();
      if (!editor || !model) return;
      if (background && model.getAlternativeVersionId() !== savedAlternative.current) {
        dispatch({ type: "conflict", conflict: "changed" });
        return;
      }
      if (model.getValue() !== content.text) {
        const view = editor.saveViewState();
        model.setValue(content.text);
        if (view) editor.restoreViewState(view);
      }
      savedAlternative.current = model.getAlternativeVersionId();
      dispatch({ type: "loaded", version: content.version });
    } catch (caught) {
      dispatch({ type: "loadFailed", error: toAppError(caught) });
    }
  }, [workspaceId, path]);

  const save = useCallback(
    async (overwrite: boolean) => {
      const model = editorRef.current?.getModel();
      const current = docRef.current;
      if (!model || current.saving || (!overwrite && !canSave(current))) return;
      dispatch({ type: "saveStarted" });
      const text = model.getValue();
      const alternative = model.getAlternativeVersionId();
      try {
        const expected = overwrite ? await commands.fileVersion(workspaceId, path) : current.version;
        const version = await commands.writeFile(workspaceId, path, text, expected);
        savedAlternative.current = alternative;
        dispatch({ type: "saved", version, dirty: model.getAlternativeVersionId() !== alternative });
        callbacks.current.onSaved();
      } catch (caught) {
        dispatch({ type: "saveFailed", error: toAppError(caught) });
      }
    },
    [workspaceId, path],
  );

  useEffect(() => {
    callbacks.current = { onCursorLine, onSaved, save };
  }, [onCursorLine, onSaved, save]);

  useEffect(() => {
    const { monaco, sharedOptions } = m;
    const model = monaco.editor.createModel("", languageFor(path));
    const editor = monaco.editor.create(hostRef.current!, { ...sharedOptions, model });
    editorRef.current = editor;
    const changed = model.onDidChangeContent(() =>
      dispatch({ type: "edited", dirty: model.getAlternativeVersionId() !== savedAlternative.current }),
    );
    const moved = editor.onDidChangeCursorPosition((event) => callbacks.current.onCursorLine(event.position.lineNumber));
    editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => void callbacks.current.save(false));
    void load();
    return () => {
      changed.dispose();
      moved.dispose();
      editor.dispose();
      model.dispose();
      editorRef.current = null;
    };
  }, [m, path, load]);

  const pollVersion = useCallback(async () => {
    if (docRef.current.phase !== "ready") return;
    const version = await commands.fileVersion(workspaceId, path);
    const decision = diskDecision(docRef.current, version);
    if (decision === "reload") await load(true);
    else if (decision === "conflict") dispatch({ type: "conflict", conflict: version === null ? "deleted" : "changed" });
  }, [workspaceId, path, load]);

  usePoll(pollVersion, VERSION_POLL_MS, visible);

  const failure = doc.phase === "error" ? doc.error : null;
  const vscode = failure?.kind === "tooLarge" ? [{ label: "Open in VS Code", onClick: onOpenInVscode }] : [];
  return (
    <div className="ide-editor">
      {doc.conflict && (
        <Banner
          tone="neutral"
          title={doc.conflict === "deleted" ? "Deleted on disk" : "Changed on disk"}
          detail={
            doc.conflict === "deleted"
              ? "This file was deleted while you had unsaved edits. Keep mine saves it again."
              : "This file changed outside the editor. Reload discards your edits; Keep mine overwrites the file."
          }
          actions={[
            { label: "Reload", primary: true, disabled: doc.saving, onClick: () => void load() },
            { label: "Keep mine", disabled: doc.saving, onClick: () => void save(true) },
          ]}
        />
      )}
      {doc.phase === "ready" && doc.error && (
        <Banner tone="error" title="Could not save" detail={doc.error.message} actions={[{ label: "Retry", onClick: () => void save(false) }]} />
      )}
      <div ref={hostRef} className="ide-monaco" hidden={failure !== null} />
      {failure && <Message alert {...describeError(failure)} actions={vscode} />}
    </div>
  );
}
