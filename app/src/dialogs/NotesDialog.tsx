import { useEffect, useRef, useState } from "react";
import { type AppError, commands } from "../bindings";
import { toAppError } from "../ide/errors";
import type { Workspace } from "../state/model";
import { serialSaver } from "../state/serialSave";
import { Dialog } from "./Dialog";

interface Props {
  workspace: Workspace;
  onSaved: () => void;
  onClose: () => void;
}

export function NotesDialog({ workspace, onSaved, onClose }: Props) {
  const [notes, setNotes] = useState(workspace.notes);
  const [error, setError] = useState<AppError | null>(null);
  const textRef = useRef<HTMLTextAreaElement>(null);
  const [save] = useState(() =>
    serialSaver(
      (text: string) => commands.setWorkspaceNotes(workspace.id, text),
      (caught) => {
        setError(caught ? toAppError(caught) : null);
        onSaved();
      },
    ),
  );

  useEffect(() => {
    const text = textRef.current;
    if (!text) return;
    text.setSelectionRange(text.value.length, text.value.length);
    text.scrollTop = text.scrollHeight;
  }, []);

  const footer = (
    <>
      <span className="dialog-footer-note">
        {error ? (
          <span className="field-error" role="alert">
            Not saved: {error.message}
          </span>
        ) : (
          "Saved as you type"
        )}
      </span>
      <button type="button" className="btn btn-primary" onClick={onClose}>
        Done <span className="kbd">Esc</span>
      </button>
    </>
  );
  return (
    <Dialog title={`Notes for ${workspace.name}`} onCancel={onClose} footer={footer}>
      <textarea
        ref={textRef}
        className="textarea notes-textarea"
        aria-label="Notes"
        placeholder="What you were doing, why, and what comes next"
        rows={14}
        value={notes}
        onChange={(event) => {
          setNotes(event.target.value);
          save(event.target.value);
        }}
        data-autofocus
      />
    </Dialog>
  );
}
