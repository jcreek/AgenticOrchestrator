import { join } from "node:path";
import { createRepo } from "./repos.mjs";
import { claudeSession, click, clickButton, expectTerminalShows, find, press, statusIn, tabDescription, typeText, view } from "./ui.mjs";
import { until } from "./wait.mjs";

export const BANNER = "Stub Claude Code";

export function repository(world, name) {
  if (!world.places.has(name)) world.places.set(name, createRepo(join(world.root, name)));
  return world.places.get(name);
}

/** Opens the New session dialog and enters the repository folder, waiting until its branches are read. */
export async function newSessionDialogFor(world, repoName) {
  const path = repository(world, repoName);
  await world.open();
  await press(world, "n", { ctrl: true });
  await until("the New session dialog", () => find(world, `$.dialog()?.getAttribute('aria-label') === 'New session'`));
  await click(world, "$.field('Repository')", "the Repository field");
  await press(world, "a", { ctrl: true });
  await typeText(world, path);
  await until(`the branches of "${repoName}" to be read`, () =>
    find(world, `$.radio('New worktree')?.querySelector('input').disabled === false`),
  );
  return path;
}

/** Starts a session through the New session dialog, in the repository folder or in a new worktree on `branch`. */
export async function startSession(world, repoName, branch = null) {
  const path = await newSessionDialogFor(world, repoName);
  const name = branch ?? repoName;
  if (branch) {
    await click(world, "$.field('Branch name')", "the Branch name field");
    await typeText(world, branch);
    world.places.set(branch, `${path}.worktrees\\${branch}`);
  } else {
    await click(world, "$.radio('Repo folder')", "the Repo folder choice");
  }
  await clickButton(world, "Start session");
  await until("the New session dialog to close", async () => (await find(world, "$.dialog()")) === null);
  await claudeSession(world, name);
  await view(world, name);
  await expectTerminalShows(world, BANNER);
  // The terminal shows output before the app publishes the session's status, and until then it counts as not started.
  await until(`the app to report a status for "${name}"`, async () => !statusIn(await tabDescription(world, name)).endsWith(" · Not started"));
  world.app.remember();
}

export function sessionProcesses(world) {
  return world.app.remember().filter((entry) => !/^msedgewebview2\.exe$/i.test(entry.exe));
}
