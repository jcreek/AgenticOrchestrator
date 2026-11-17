// Tauri builds the app; hdiutil packages a visible installation guide alongside it.
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { parseArgs } from "node:util";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

if (process.platform !== "darwin") throw new Error("package:macos must run on a Mac");
const appDir = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const run = (program, args, options = {}) => execFileSync(program, args, { cwd: appDir, stdio: "inherit", ...options });
// Default to the host, but allow either macOS architecture for cross-compilation.
const { values } = parseArgs({ options: { target: { type: "string" } } });
const host = run("rustc", ["-vV"], { encoding: "utf8", stdio: "pipe" }).match(/^host: (.+)$/m)?.[1];
if (!host?.endsWith("-apple-darwin")) throw new Error("Expected a native macOS Rust toolchain");
const target = values.target ?? host;
if (!["aarch64-apple-darwin", "x86_64-apple-darwin"].includes(target)) {
  throw new Error("--target must be aarch64-apple-darwin or x86_64-apple-darwin");
}
run("pnpm", ["tauri", "build", "--bundles", "app", "--target", target, "--", "--locked"]);
const metadata = JSON.parse(run("cargo", ["metadata", "--no-deps", "--format-version", "1"], { encoding: "utf8", stdio: "pipe" }));
const bundleDir = join(metadata.target_directory, target, "release", "bundle");
const { version } = JSON.parse(readFileSync(join(appDir, "package.json"), "utf8"));
const arch = target.startsWith("aarch64-") ? "aarch64" : "x64";
const outputDir = join(bundleDir, "dmg");
mkdirSync(outputDir, { recursive: true });
const output = join(outputDir, `Agentic Orchestrator_${version}_${arch}.dmg`);
const staging = mkdtempSync(join(tmpdir(), "ao-dmg-"));
try {
  run("ditto", [join(bundleDir, "macos", "Agentic Orchestrator.app"), join(staging, "Agentic Orchestrator.app")]);
  // Reject a host-architecture executable accidentally packaged for the other target.
  run("lipo", [join(staging, "Agentic Orchestrator.app", "Contents", "MacOS", "agentic-orchestrator"), "-verify_arch", arch === "aarch64" ? "arm64" : "x86_64"]);
  // Verify the final staged bundle, including its resources, before distributing it.
  run("codesign", ["--verify", "--deep", "--strict", "--verbose=2", join(staging, "Agentic Orchestrator.app")]);
  symlinkSync("/Applications", join(staging, "Applications"));
  copyFileSync(join(appDir, "packaging", "macos", "Read Me First.txt"), join(staging, "Read Me First.txt"));
  run("hdiutil", ["create", "-ov", "-format", "UDZO", "-fs", "HFS+", "-volname", "Agentic Orchestrator", "-srcfolder", staging, output]);
  console.log(`Built macOS installer: ${output}`);
} finally {
  rmSync(staging, { recursive: true, force: true });
}
