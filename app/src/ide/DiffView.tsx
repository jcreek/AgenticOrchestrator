import { useCallback, useEffect, useRef, useState } from "react";
import type * as Monaco from "monaco-editor/editor";
import { type AppError, commands } from "../bindings";
import { describeError, toAppError } from "./errors";
import { useSettings } from "../state/settings";
import { type MonacoModule, usePoll } from "./hooks";
import { Message } from "./parts";
import { languageFor } from "./view";

const VERSION_POLL_MS = 2000;

interface Models {
  original: Monaco.editor.ITextModel;
  modified: Monaco.editor.ITextModel;
}

const setText = (model: Monaco.editor.ITextModel, text: string) => {
  if (model.getValue() !== text) model.setValue(text);
};

export function DiffView({ m, workspaceId, path, originalPath, deleted, base, revision, visible, onOpenInVscode }: {
  m: MonacoModule;
  workspaceId: string;
  path: string;
  originalPath: string | null;
  deleted: boolean;
  base: string | null;
  /** Changes when the file's row in the changes list changes, e.g. after a commit moves HEAD. */
  revision: string;
  visible: boolean;
  onOpenInVscode: () => void;
}) {
  const hostRef = useRef<HTMLDivElement>(null);
  const editorRef = useRef<Monaco.editor.IStandaloneDiffEditor | null>(null);
  const modelsRef = useRef<Models | null>(null);
  const lastVersion = useRef<string | null | undefined>(undefined);
  const [diskTick, setDiskTick] = useState(0);
  const [error, setError] = useState<AppError | null>(null);
  const [empty, setEmpty] = useState(false);
  const sideBySide = useSettings().settings.diffLayout === "sideBySide";

  useEffect(() => {
    const { monaco, sharedOptions } = m;
    const language = languageFor(path);
    const models = { original: monaco.editor.createModel("", language), modified: monaco.editor.createModel("", language) };
    const editor = monaco.editor.createDiffEditor(hostRef.current!, {
      ...sharedOptions,
      readOnly: true,
      originalEditable: false,
      useInlineViewWhenSpaceIsLimited: false,
      hideUnchangedRegions: { enabled: true },
      renderOverviewRuler: false,
    });
    editor.setModel(models);
    editorRef.current = editor;
    modelsRef.current = models;
    return () => {
      editor.dispose();
      models.original.dispose();
      models.modified.dispose();
      editorRef.current = null;
      modelsRef.current = null;
    };
  }, [m, path]);

  useEffect(() => {
    editorRef.current?.updateOptions({ renderSideBySide: sideBySide });
  }, [sideBySide, m, path]);

  useEffect(() => {
    let live = true;
    const from = originalPath ?? path;
    const before = base ? commands.gitFileAtMergeBase(workspaceId, from, base) : commands.gitFileAtHead(workspaceId, from);
    const after = deleted
      ? Promise.resolve(null)
      : commands.readFile(workspaceId, path).then(
          (content) => content.text,
          (caught: unknown) => {
            const failure = toAppError(caught);
            if (failure.kind === "notFound") return null;
            throw failure;
          },
        );
    Promise.all([before, after]).then(
      ([original, modified]) => {
        const models = modelsRef.current;
        if (!live || !models) return;
        setText(models.original, original ?? "");
        setText(models.modified, modified ?? "");
        setEmpty(!original && !modified);
        setError(null);
      },
      (caught: unknown) => live && setError(toAppError(caught)),
    );
    return () => {
      live = false;
    };
  }, [workspaceId, path, originalPath, deleted, base, revision, diskTick]);

  const pollVersion = useCallback(async () => {
    const version = await commands.fileVersion(workspaceId, path);
    if (lastVersion.current !== undefined && version !== lastVersion.current) setDiskTick((tick) => tick + 1);
    lastVersion.current = version;
  }, [workspaceId, path]);

  usePoll(pollVersion, VERSION_POLL_MS, visible);

  const vscode = error?.kind === "tooLarge" ? [{ label: "Open in VS Code", onClick: onOpenInVscode }] : [];
  return (
    <div className="ide-editor">
      <div ref={hostRef} className="ide-monaco" hidden={error !== null || empty} />
      {error && <Message alert {...describeError(error)} actions={vscode} />}
      {empty && !error && <Message title="Empty file" detail="There is nothing to compare." />}
    </div>
  );
}
