import { Given, Then, When } from "@cucumber/cucumber";
import assert from "node:assert/strict";
import { assertStopped } from "../support/app.mjs";
import { sessionProcesses } from "../support/sessions.mjs";
import { click, clickButton, expectTerminalShows, find, press, typeIntoTerminal, view } from "../support/ui.mjs";
import { until } from "../support/wait.mjs";

Given("I type {string} into Claude's terminal in {string}", async function (text, name) {
  await view(this, name);
  await typeIntoTerminal(this, text);
  await expectTerminalShows(this, text);
});

When("the window reloads", async function () {
  await this.app.reload();
});

Then("Claude's terminal in {string} shows {string}", async function (name, text) {
  await view(this, name);
  await expectTerminalShows(this, text);
});

Given("a shell in {string} is running a long command", async function (name) {
  await view(this, name);
  await click(this, "$.button('New shell', document)", "the New shell button");
  await expectTerminalShows(this, "PS ");
  await typeIntoTerminal(this, "ping -n 120 127.0.0.1");
  await press(this, "Enter");
  await until("ping to be running under the app", () => this.app.remember().some((entry) => /^ping\.exe$/i.test(entry.exe)));
});

When("I close the {string} session and confirm", async function (name) {
  await view(this, name);
  await clickButton(this, "Close session");
  await until("the close dialog", () => find(this, `$.dialog()?.getAttribute('aria-label')?.startsWith('Close ')`));
  this.stopping = sessionProcesses(this);
  await clickButton(this, "Close session");
});

Then("Claude Code and every process started for {string} have stopped", async function (name) {
  const claude = this.stopping.filter((entry) => /^claude\.exe$/i.test(entry.exe));
  assert.ok(claude.length > 0, `Claude Code was not running in "${name}" before it was closed`);
  await assertStopped(this.stopping, `after closing "${name}"`);
});

Given("the shell is a program that exits straight away", async function () {
  this.settings = { shellProgram: "cmd.exe", shellArgs: ["/c", "exit 3"] };
  if (this.app) await this.app.invoke("update_settings", { settings: { ...this.readState().settings, ...this.settings } });
});

When("I open a new shell in {string}", async function (name) {
  await view(this, name);
  await click(this, "$.button('New shell', document)", "the New shell button");
});

Then("the shell tab shows that it exited", async function () {
  await until("the shell tab to say it exited", () =>
    find(this, `[...$.session().querySelectorAll('.term-tab')].some((tab) => /^Shell \\d+ · exited$/.test(tab.innerText.trim()))`),
  );
});

When("I press Ctrl+W in Claude's terminal in {string}", async function (name) {
  await view(this, name);
  await click(this, "$.terminal()", "the terminal");
  await press(this, "w", { ctrl: true });
});

Then("no dialog is open", async function () {
  assert.equal(await find(this, "$.dialog()?.getAttribute('aria-label') ?? null"), null);
});

When("I press Ctrl+W outside the terminal", async function () {
  await click(this, "$.session().querySelector('.session-path')", "the session's folder path");
  await press(this, "w", { ctrl: true });
});

Then("I am asked {string}", async function (title) {
  await until(`a dialog titled "${title}"`, async () => {
    const shown = await find(this, "$.dialog()?.getAttribute('aria-label') ?? null");
    if (shown === title) return true;
    throw new Error(`the open dialog is ${JSON.stringify(shown)}`);
  });
});
