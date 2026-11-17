import type { Blocker, BlockerKind } from "../bindings";

const NOUN: Record<BlockerKind, [string, string]> = {
  modifiedFiles: ["modified file", "modified files"],
  untrackedFiles: ["untracked file", "untracked files"],
  ignoredFiles: ["ignored file", "ignored files"],
  unpushedCommits: ["unpushed commit", "unpushed commits"],
};

export function describeBlocker({ kind, count }: Blocker): string {
  return `${count} ${NOUN[kind][count === 1 ? 0 : 1]}`;
}

/** "3 modified files, 1 untracked file and 2 unpushed commits". */
export function describeBlockers(blockers: Blocker[]): string {
  const parts = blockers.filter((blocker) => blocker.count > 0).map(describeBlocker);
  if (parts.length <= 1) return parts.join("");
  return `${parts.slice(0, -1).join(", ")} and ${parts[parts.length - 1]}`;
}

export function hasUncommitted(blockers: Blocker[]): boolean {
  return blockers.some((blocker) => blocker.kind === "modifiedFiles" || blocker.kind === "untrackedFiles");
}
