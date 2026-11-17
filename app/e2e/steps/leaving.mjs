import { Given, Then, When } from "@cucumber/cucumber";
import assert from "node:assert/strict";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { createRepo } from "../support/repos.mjs";
import { sessionProcesses } from "../support/sessions.mjs";
import { click, clickButton } from "../support/ui.mjs";
import { until } from "../support/wait.mjs";

Given("the app is open with no sessions", async function () {
  await this.open();
});

When("I close the window", async function () {
  this.stopping = sessionProcesses(this);
  await click(this, "document.querySelector('.window-controls [aria-label=Close]')", "the window's Close button");
});

When("I cancel", async function () {
  await clickButton(this, "Cancel");
});

Then("the app has closed", async function () {
  await until("the app to exit", () => this.app.exited);
});

Given("a saved state file that is not valid JSON", function () {
  this.stateText = '{ "workspaces": [ not json';
  this.writeState({});
  writeFileSync(this.statePath, this.stateText);
});

When("the app starts", async function () {
  await this.open();
});

When("I try to add a workspace for repository {string}", async function (name) {
  const path = createRepo(join(this.root, name));
  this.result = await this.app.invoke("add_workspace", { path });
});

Then("it is refused because the saved state cannot be read", function () {
  assert.equal(this.result.error?.kind, "stateUnreadable", JSON.stringify(this.result));
});

Then("the state file is unchanged", function () {
  assert.equal(readFileSync(this.statePath, "utf8"), this.stateText);
});
