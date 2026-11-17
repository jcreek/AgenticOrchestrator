import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { commands, type Branches_Serialize, type CommandOutput } from "../bindings";
import { errorMessage } from "../state/errors";
import type { Workspace } from "../state/model";
import { baseName, samePath, worktreeFolder } from "../state/paths";
import { WarningIcon } from "../shell/StatusIcon";
import { Dialog } from "./Dialog";

interface Props {
  workspaces: Workspace[];
  knownRepos: string[];
  initialRepo: string;
  worktreeRoot: string | null;
  defaultWhere: Where;
  onCreated: (workspace: Workspace, prompt: string | null) => void;
  onClose: () => void;
}

type Where = "folder" | "worktree";
type BranchMode = "new" | "existing";

const LOOKUP_DELAY_MS = 300;

function pickDefaultBase(branches: Branches_Serialize, preferred: string | null): string {
  if (preferred) return preferred;
  return ["main", "master"].find((name) => branches.local.includes(name)) ?? branches.current ?? "";
}

export function NewSessionDialog({ workspaces, knownRepos, initialRepo, worktreeRoot, defaultWhere, onCreated, onClose }: Props) {
  const [repo, setRepo] = useState(initialRepo);
  const [where, setWhere] = useState<Where>(defaultWhere);
  const [branchMode, setBranchMode] = useState<BranchMode>("new");
  const [branch, setBranch] = useState("");
  const [existing, setExisting] = useState("");
  const [base, setBase] = useState("");
  const [prompt, setPrompt] = useState("");
  const [branches, setBranches] = useState<Branches_Serialize | null>(null);
  const [defaultBase, setDefaultBase] = useState<string | null>(null);
  const [lookupError, setLookupError] = useState<string | null>(null);
  const [showErrors, setShowErrors] = useState(false);
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const [postCreate, setPostCreate] = useState<CommandOutput | null>(null);
  const branchRef = useRef<HTMLInputElement & HTMLSelectElement>(null);
  const workspacesRef = useRef(workspaces);
  workspacesRef.current = workspaces;

  const trimmedRepo = repo.trim();
  const openAlready = workspaces.find((workspace) => samePath(workspace.path, trimmedRepo));
  const isRepo = branches !== null;
  const effectiveWhere: Where = isRepo ? where : "folder";
  const pickable = branches?.local.filter((name) => name !== branches.current) ?? [];
  const branchName = branchMode === "new" ? branch.trim() : existing;
  const lookingUp = trimmedRepo !== "" && branches === null && lookupError === null;

  useEffect(() => {
    setBranches(null);
    setLookupError(null);
    setDefaultBase(null);
    if (!trimmedRepo) return;
    let cancelled = false;
    const timer = window.setTimeout(() => {
      commands.listBranches({ kind: "folder", path: trimmedRepo }).then(
        (found) => {
          if (cancelled) return;
          setBranches(found);
          setExisting((current) => current || (found.local.find((name) => name !== found.current) ?? ""));
        },
        (error: unknown) => !cancelled && setLookupError(errorMessage(error)),
      );
      const match = workspacesRef.current.find((workspace) => samePath(workspace.path, trimmedRepo));
      if (match) {
        commands.gitDefaultBase(match.id).then(
          (found) => !cancelled && setDefaultBase(found),
          () => undefined,
        );
      }
    }, LOOKUP_DELAY_MS);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [trimmedRepo]);

  useEffect(() => {
    if (branches) setBase(pickDefaultBase(branches, defaultBase));
  }, [branches, defaultBase]);

  const baseOptions = [...new Set([base, defaultBase, ...(branches?.local ?? [])].filter((name): name is string => !!name))];

  const validation = (() => {
    if (!trimmedRepo) return { field: "repo", message: "Choose a repository folder." };
    if (effectiveWhere === "folder") {
      return openAlready ? { field: "where", message: "This folder is already open in a tab." } : null;
    }
    if (branchMode === "existing") return existing ? null : { field: "branch", message: "Choose a branch." };
    if (!branchName) return { field: "branch", message: "Enter a branch name." };
    if (/\s/.test(branchName) || branchName.startsWith("-")) {
      return { field: "branch", message: "Branch names cannot contain spaces or start with a dash." };
    }
    if (branches?.local.includes(branchName)) {
      return { field: "branch", message: "A branch with this name already exists. Choose Existing branch to use it." };
    }
    return null;
  })();
  const fieldError = (field: string) =>
    showErrors && validation?.field === field ? <span className="field-error">{validation.message}</span> : null;

  const browse = async () => {
    const picked = await commands.pickFolder().catch(() => null);
    if (picked) setRepo(picked);
  };

  const submit = async () => {
    if (busy || postCreate || lookingUp) return;
    setShowErrors(true);
    if (validation) return;
    setBusy(true);
    setFailure(null);
    try {
      const startPrompt = prompt.trim() || null;
      if (effectiveWhere === "folder") {
        onCreated(await commands.addWorkspace(trimmedRepo), startPrompt);
        onClose();
        return;
      }
      const created = await commands.createWorktree(
        { kind: "folder", path: trimmedRepo },
        branchName,
        branchMode === "new" ? base || null : null,
      );
      onCreated(created.workspace, startPrompt);
      if (created.postCreate && !created.postCreate.success) setPostCreate(created.postCreate);
      else onClose();
    } catch (error) {
      setFailure(errorMessage(error));
    } finally {
      setBusy(false);
    }
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Enter" && event.ctrlKey) {
      event.preventDefault();
      void submit();
    }
  };

  const groupName = trimmedRepo ? baseName(trimmedRepo) : "";
  const footer = postCreate ? (
    <>
      <span className="dialog-footer-note">The session has started.</span>
      <button type="button" className="btn btn-primary" onClick={onClose} data-autofocus>
        Continue
      </button>
    </>
  ) : (
    <>
      <span className="dialog-footer-note">{groupName && `Opens as a new tab in the ${groupName} group`}</span>
      <button type="button" className="btn" onClick={onClose}>
        Cancel <span className="kbd">Esc</span>
      </button>
      <button type="button" className="btn btn-primary" disabled={busy || lookingUp} onClick={() => void submit()}>
        {busy ? "Starting…" : "Start session"} <span className="kbd">Ctrl+Enter</span>
      </button>
    </>
  );

  return (
    <Dialog title="New session" onCancel={onClose} footer={footer} onKeyDown={onKeyDown}>
      {postCreate && (
        <div className="dialog-alert" role="alert">
          <div className="banner banner-warning">
            <WarningIcon color="var(--status-input)" />
            <span className="banner-text">
              <span className="banner-title">The post-create command failed</span>
              <span className="banner-detail">The worktree was created. Check the output before relying on it.</span>
            </span>
          </div>
          <pre>{postCreate.output || "(no output)"}</pre>
        </div>
      )}
      {failure && (
        <div className="dialog-alert" role="alert">
          <div className="banner banner-danger">
            <WarningIcon color="var(--status-crashed)" />
            <span className="banner-text">
              <span className="banner-title">
                {effectiveWhere === "worktree" ? "Could not create the worktree" : "Could not open the folder"}
              </span>
              <span className="banner-detail">
                {effectiveWhere === "worktree" ? "Nothing was created. Fix the branch below and try again." : "Check the folder and try again."}
              </span>
            </span>
            {effectiveWhere === "worktree" && (
              <button type="button" className="btn btn-sm" onClick={() => branchRef.current?.focus()}>
                Change branch
              </button>
            )}
          </div>
          <pre>{failure}</pre>
        </div>
      )}
      <div className="field-row">
        <div className="field">
          <label htmlFor="ns-repo">Repository</label>
          <input
            id="ns-repo"
            className="input"
            list="ns-repos"
            value={repo}
            onChange={(event) => setRepo(event.target.value)}
            spellCheck={false}
            data-autofocus
          />
          <datalist id="ns-repos">
            {knownRepos.map((path) => (
              <option key={path} value={path} />
            ))}
          </datalist>
        </div>
        <button type="button" className="btn" onClick={() => void browse()}>
          Browse…
        </button>
      </div>
      {fieldError("repo")}
      {lookupError && trimmedRepo && <span className="muted">Not a git repository, so it runs in the folder itself.</span>}
      <fieldset className="choices">
        <legend>Where to run</legend>
        <label className="choice">
          <input
            type="radio"
            name="ns-where"
            checked={effectiveWhere === "folder"}
            disabled={!!openAlready}
            onChange={() => setWhere("folder")}
          />
          <span className="choice-text">
            <span className="choice-title">{isRepo ? "Repo folder" : "Folder"}</span>
            <span className="choice-detail">
              {openAlready
                ? `${trimmedRepo} is already open in a tab.`
                : `Run in ${trimmedRepo || "the folder"}${branches?.current ? ` on its current branch, ${branches.current}` : ""}.`}
            </span>
          </span>
        </label>
        <label className="choice">
          <input
            type="radio"
            name="ns-where"
            checked={effectiveWhere === "worktree"}
            disabled={!isRepo}
            onChange={() => setWhere("worktree")}
          />
          <span className="choice-text">
            <span className="choice-title">New worktree</span>
            <span className="choice-detail">
              A separate folder and branch, so this session cannot collide with the others.
            </span>
          </span>
        </label>
      </fieldset>
      {fieldError("where")}
      {effectiveWhere === "worktree" && (
        <>
          <fieldset className="choices">
            <legend>Branch</legend>
            <div className="inline-choices">
              <label>
                <input type="radio" name="ns-branch" checked={branchMode === "new"} onChange={() => setBranchMode("new")} />
                New branch
              </label>
              <label>
                <input
                  type="radio"
                  name="ns-branch"
                  checked={branchMode === "existing"}
                  disabled={pickable.length === 0}
                  onChange={() => setBranchMode("existing")}
                />
                Existing branch
              </label>
            </div>
          </fieldset>
          <div className="field-row">
            <div className="field">
              <label htmlFor="ns-name">Branch name</label>
              {branchMode === "new" ? (
                <input
                  id="ns-name"
                  ref={branchRef}
                  className="input"
                  value={branch}
                  placeholder="feat/my-change"
                  spellCheck={false}
                  onChange={(event) => setBranch(event.target.value)}
                />
              ) : (
                <select id="ns-name" ref={branchRef} className="select" value={existing} onChange={(event) => setExisting(event.target.value)}>
                  {pickable.map((name) => (
                    <option key={name} value={name}>
                      {name}
                    </option>
                  ))}
                </select>
              )}
            </div>
            {branchMode === "new" && (
              <div className="field field-narrow">
                <label htmlFor="ns-base">Based on</label>
                <select id="ns-base" className="select" value={base} onChange={(event) => setBase(event.target.value)}>
                  {baseOptions.map((name) => (
                    <option key={name} value={name}>
                      {name}
                    </option>
                  ))}
                </select>
              </div>
            )}
          </div>
          {fieldError("branch")}
          <div className="field">
            <label htmlFor="ns-folder">Worktree folder</label>
            <input
              id="ns-folder"
              className="input"
              readOnly
              value={trimmedRepo && branchName ? worktreeFolder(trimmedRepo, branchName, worktreeRoot) : ""}
            />
          </div>
        </>
      )}
      <div className="field">
        <label htmlFor="ns-prompt">
          Starting prompt <span className="field-optional">optional</span>
        </label>
        <textarea id="ns-prompt" className="textarea" rows={3} value={prompt} onChange={(event) => setPrompt(event.target.value)} />
      </div>
    </Dialog>
  );
}
