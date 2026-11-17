import { execFileSync } from "node:child_process";
import { appendFileSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";

const LOCAL_CONFIG = "[user]\n\tname = BDD\n\temail = bdd@example.com\n[commit]\n\tgpgsign = false\n[core]\n\tautocrlf = false\n";

export function git(dir, ...args) {
  return execFileSync("git", ["-C", dir, ...args], { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
}

export function write(dir, rel, content) {
  const path = join(dir, rel);
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, content);
}

export const read = (dir, rel) => readFileSync(join(dir, rel), "utf8");

/** A throwaway repository on `main` with README.md, notes.txt and a .gitignore for *.log, committed. */
export function createRepo(path) {
  mkdirSync(path, { recursive: true });
  git(path, "init", "-q", "-b", "main");
  appendFileSync(join(path, ".git", "config"), LOCAL_CONFIG);
  write(path, "README.md", `# ${path.split("\\").pop()}\n`);
  write(path, "notes.txt", "first line\n");
  write(path, ".gitignore", "*.log\n");
  git(path, "add", "-A");
  git(path, "commit", "-q", "-m", "initial");
  return path;
}
