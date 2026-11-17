import { Given, Then, When } from "@cucumber/cucumber";
import assert from "node:assert/strict";
import { BANNER, newSessionDialogFor, repository, startSession } from "../support/sessions.mjs";
import { claudeSession, clickButton, expectTerminalShows, find, launchesIn, tabDescription, view, workspaceId } from "../support/ui.mjs";
import { until } from "../support/wait.mjs";

const quoted = (text) => [...text.matchAll(/"([^"]*)"/g)].map((match) => match[1]);

Given("a repository {string}", function (name) {
  repository(this, name);
});

Given("a session open in repository {string}", async function (name) {
  await startSession(this, name);
});

Given(/^sessions open in repositories (.+)$/, async function (names) {
  for (const name of quoted(names)) await startSession(this, name);
});

Given("a session open in a new worktree of {string} on the branch {string}", async function (repo, branch) {
  await startSession(this, repo, branch);
});

When("I start a new session in the repository folder of {string}", async function (name) {
  await startSession(this, name);
});

Then("a tab named {string} on the branch {string} is shown", async function (name, branch) {
  const id = await workspaceId(this, name);
  await until(`a tab named "${name}" on "${branch}"`, async () => {
    const description = await tabDescription(this, name);
    const text = await find(this, `$.tab(${JSON.stringify(id)})?.querySelector('.tab-name')?.innerText`);
    if (text === name && description.startsWith(`${name} · ${branch} · `)) return true;
    throw new Error(`the tab reads "${text}" and is described as "${description}"`);
  });
});

Then("Claude Code is running in {string}", async function (name) {
  await claudeSession(this, name);
  await view(this, name);
  await expectTerminalShows(this, BANNER);
});

When("I ask for a new session in {string}", async function (name) {
  await newSessionDialogFor(this, name);
});

Then("running in the repository folder of {string} is not offered because it is already open in a tab", async function (name) {
  await until("the repository folder choice to be refused", () =>
    find(this, `$.radio('Repo folder')?.querySelector('input').disabled === true && $.dialog().innerText.includes('is already open in a tab')`),
  );
});

When("the app is restarted", async function () {
  this.sessionBefore = await claudeSession(this, [...this.places.keys()][0]);
  await this.restart();
});

Then("the {string} session offers {string} and {string}", async function (name, first, second) {
  await view(this, name);
  for (const label of [first, second]) {
    await until(`the "${label}" button`, () => find(this, `!!$.button(${JSON.stringify(label)})`));
  }
});

When("I choose {string} in the {string} session", async function (choice, name) {
  await view(this, name);
  await clickButton(this, choice);
});

Then(/^Claude Code in "([^"]*)" is started with (the same|a new) conversation id$/, async function (name, which) {
  const launch = await until(`Claude Code to start again in "${name}"`, () => launchesIn(this, name)[1]);
  const saved = () => this.readState().workspaces.find((w) => w.path.toLowerCase() === this.place(name).toLowerCase())?.claudeSessionId;
  if (which === "the same") {
    assert.deepEqual({ mode: launch.mode, id: launch.id }, { mode: "resume", id: this.sessionBefore });
  } else {
    assert.equal(launch.mode, "new");
    assert.notEqual(launch.id, this.sessionBefore);
  }
  await until(`the saved conversation id of "${name}" to be ${launch.id}`, () => saved() === launch.id);
});
