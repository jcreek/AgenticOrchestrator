// Builds what the @ui suite runs: the debug app under its own bundle identifier, and the stub Claude.
// Both go to target\bdd so they never replace the copy `tauri dev` uses. Rebuilds are incremental.
import { execSync } from "node:child_process";
import { APP_DIR, REPO_ROOT, TARGET_DIR } from "./support/paths.mjs";

const env = { ...process.env, CARGO_TARGET_DIR: TARGET_DIR };
const run = (command, cwd) => execSync(command, { cwd, env, stdio: "inherit" });

if (process.env.AO_BDD_SKIP_BUILD !== "1") {
  run("cargo build -p claude-stub", REPO_ROOT);
  run("pnpm tauri build --debug --no-bundle --config e2e/tauri.bdd.json", APP_DIR);
}
