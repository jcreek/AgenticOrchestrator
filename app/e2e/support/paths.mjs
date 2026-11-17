import { join, resolve } from "node:path";

export const APP_DIR = resolve(import.meta.dirname, "..", "..");
export const REPO_ROOT = resolve(APP_DIR, "..");
export const TARGET_DIR = join(REPO_ROOT, "target", "bdd");
const BUILT = join(TARGET_DIR, "debug");
export const APP_EXE = join(BUILT, "agentic-orchestrator.exe");
export const STUB_EXE = join(BUILT, "claude.exe");
export const PROC_EXE = join(BUILT, "ao-proc.exe");
export const RUN_ROOT = join(process.env.TEMP, "ao-bdd", `run-${new Date().toISOString().replace(/[:.]/g, "-")}`);
export const ARTEFACTS = join(RUN_ROOT, "artefacts");
