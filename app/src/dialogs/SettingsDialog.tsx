import { getVersion } from "@tauri-apps/api/app";
import { useEffect, useState } from "react";
import type { DiffLayout, LayoutMode, Settings_Serialize, WhereToRun } from "../bindings";
import { Dialog } from "./Dialog";

interface Props {
  settings: Settings_Serialize;
  nerdFont: string | null;
  onSave: (changes: Partial<Settings_Serialize>) => void;
  onClose: () => void;
}

const ZOOM_PERCENTS = [100, 110, 125, 150, 175, 200];

export function SettingsDialog({ settings, nerdFont, onSave, onClose }: Props) {
  const [zoomPercent, setZoomPercent] = useState(settings.zoomPercent);
  const [terminalFont, setTerminalFont] = useState(settings.terminalFont);
  const [defaultLayout, setDefaultLayout] = useState(settings.defaultLayout);
  const [defaultWhere, setDefaultWhere] = useState(settings.defaultWhere);
  const [diffLayout, setDiffLayout] = useState(settings.diffLayout);
  const [version, setVersion] = useState("");

  useEffect(() => {
    getVersion().then(setVersion, () => undefined);
  }, []);

  const save = () => {
    onSave({ zoomPercent, terminalFont: terminalFont.trim(), defaultLayout, defaultWhere, diffLayout });
    onClose();
  };

  const zoomOptions = [...new Set([...ZOOM_PERCENTS, zoomPercent])].sort((a, b) => a - b);
  const footer = (
    <>
      <span className="dialog-footer-note">{version && `Version ${version}`}</span>
      <button type="button" className="btn" onClick={onClose}>
        Cancel <span className="kbd">Esc</span>
      </button>
      <button type="button" className="btn btn-primary" onClick={save}>
        Save
      </button>
    </>
  );

  return (
    <Dialog title="Settings" onCancel={onClose} footer={footer}>
      <div className="field">
        <label htmlFor="st-zoom">Text size</label>
        <select id="st-zoom" className="select" value={zoomPercent} onChange={(event) => setZoomPercent(Number(event.target.value))} data-autofocus>
          {zoomOptions.map((percent) => (
            <option key={percent} value={percent}>
              {percent}%
            </option>
          ))}
        </select>
      </div>
      <div className="field">
        <label htmlFor="st-font">
          Terminal font <span className="field-optional">optional</span>
        </label>
        <input
          id="st-font"
          className="input"
          value={terminalFont}
          placeholder={nerdFont ? `Automatic: ${nerdFont}` : "No Nerd Font found; enter an installed font"}
          spellCheck={false}
          onChange={(event) => setTerminalFont(event.target.value)}
        />
      </div>
      <div className="field">
        <label htmlFor="st-layout">Layout for new tabs</label>
        <select id="st-layout" className="select" value={defaultLayout} onChange={(event) => setDefaultLayout(event.target.value as LayoutMode)}>
          <option value="terminal">Terminal</option>
          <option value="both">Both</option>
          <option value="ide">IDE</option>
        </select>
      </div>
      <div className="field">
        <label htmlFor="st-where">New sessions start in</label>
        <select id="st-where" className="select" value={defaultWhere} onChange={(event) => setDefaultWhere(event.target.value as WhereToRun)}>
          <option value="worktree">A new worktree</option>
          <option value="folder">The repo folder</option>
        </select>
      </div>
      <div className="field">
        <label htmlFor="st-diff">Diff view</label>
        <select id="st-diff" className="select" value={diffLayout} onChange={(event) => setDiffLayout(event.target.value as DiffLayout)}>
          <option value="sideBySide">Side by side</option>
          <option value="inline">Inline</option>
        </select>
      </div>
    </Dialog>
  );
}
