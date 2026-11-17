const VERBATIM = /^\\\\\?\\(UNC\\)?/;

function stripVerbatim(path: string): string {
  return path.replace(VERBATIM, (_match, unc: string | undefined) => (unc ? "\\\\" : ""));
}

function trimSeparators(path: string): string {
  return path.replace(/[\\/]+$/, "") || (path.startsWith("/") ? "/" : path);
}

/** The repo folder a group key (the canonical git common dir) stands for. */
export function groupFolder(key: string): string {
  const path = trimSeparators(stripVerbatim(key));
  return path.replace(/[\\/]\.git$/i, "");
}

/** Mirrors the backend's folder naming for a branch. */
export function pathSafe(branch: string): string {
  return [...branch].map((c) => (/[\p{L}\p{N}\-_.]/u.test(c) ? c : "-")).join("");
}

export function baseName(path: string): string {
  const parts = trimSeparators(path).split(/[\\/]/);
  return parts[parts.length - 1] ?? path;
}

/** Where a new worktree for `branch` will be created; mirrors the backend's default. */
export function worktreeFolder(repoRoot: string, branch: string, worktreeRoot: string | null): string {
  const repo = trimSeparators(stripVerbatim(repoRoot));
  const root = worktreeRoot ? trimSeparators(worktreeRoot) : `${repo}.worktrees`;
  const separator = repoRoot.includes("\\") ? "\\" : "/";
  return `${root.endsWith(separator) ? root : root + separator}${pathSafe(branch)}`;
}

export function samePath(a: string, b: string): boolean {
  const normal = (path: string) => trimSeparators(stripVerbatim(path)).replace(/\//g, "\\").toLowerCase();
  return normal(a) === normal(b);
}
