import { Given, Then, When } from "@cucumber/cucumber";
import assert from "node:assert/strict";
import {
  claudeSession,
  control,
  controlAll,
  expectTabStatus,
  expectTerminalShows,
  find,
  press,
  setAgentsMode,
  typeIntoTerminal,
  view,
  workspaceId,
} from "../support/ui.mjs";
import { until } from "../support/wait.mjs";

const REPORTS = {
  "starts working": { status: "busy" },
  "asks for permission": { status: "waiting", waitingFor: "permission prompt" },
  "asks a question": { status: "waiting", waitingFor: "question" },
  finishes: { status: "idle" },
};

/** A state change can take a quiet poll interval (15 s) plus a query to show. */
const STATUS_WAIT_MS = 30_000;

async function claudeDoes(world, name, activity) {
  const session = await claudeSession(world, name);
  const exit = /^exits with code (\d+)$/.exec(activity);
  if (exit) return control(world, session, { exit: Number(exit[1]) });
  if (activity === "starts working and then finishes") {
    control(world, session, REPORTS["starts working"]);
    await expectTabStatus(world, name, "Working", STATUS_WAIT_MS);
    return control(world, session, REPORTS.finishes);
  }
  if (!REPORTS[activity]) throw new Error(`unknown activity "${activity}"`);
  control(world, session, REPORTS[activity]);
}

When(/^Claude in "([^"]*)" (.+)$/, async function (name, activity) {
  await claudeDoes(this, name, activity);
});

Then("the {string} tab shows {string}", async function (name, label) {
  await expectTabStatus(this, name, label, STATUS_WAIT_MS);
});

Then("after a while the {string} tab shows {string}", async function (name, label) {
  await expectTabStatus(this, name, label, STATUS_WAIT_MS);
});

Given("I am viewing the {string} session", async function (name) {
  await view(this, name);
});

When("I view the {string} session", async function (name) {
  await view(this, name);
});

Given("Claude Code does not report new sessions", function () {
  controlAll(this, { listed: false });
});

async function activeName(world) {
  const activeId = await find(world, `document.querySelector('[data-tab-id][aria-current=page]')?.dataset.tabId ?? null`);
  for (const name of world.places.keys()) if ((await workspaceId(world, name)) === activeId) return name;
  return null;
}

When("I jump to the next session needing me {int} times", async function (times) {
  this.visited = [];
  for (let jump = 0; jump < times; jump += 1) {
    const before = await activeName(this);
    await press(this, "j", { ctrl: true });
    this.visited.push(await until("the jump to land on another session", async () => {
      const now = await activeName(this);
      return now !== before ? now : null;
    }));
  }
});

Then(/^I visit (.+) in that order$/, function (names) {
  assert.deepEqual(this.visited, [...names.matchAll(/"([^"]*)"/g)].map((match) => match[1]));
});

Then("the overview shows that {int} sessions need me", async function (count) {
  await until(`the Overview button to say ${count} need you`, async () => {
    const label = await find(this, `document.querySelector('.overview-tab').getAttribute('aria-label')`);
    if (label === `Overview, ${count} need you`) return true;
    throw new Error(`it says "${label}"`);
  }, STATUS_WAIT_MS);
});

When(/^Claude Code's status report (fails|stops responding)$/, function (failure) {
  setAgentsMode(this, failure === "fails" ? "fail" : "hang");
});

When("Claude Code's status report works again", function () {
  setAgentsMode(this, "ok");
});

Then("what I type into Claude's terminal in {string} still reaches Claude", async function (name) {
  const marker = `still-here-${Date.now()}`;
  await view(this, name);
  await typeIntoTerminal(this, marker);
  await expectTerminalShows(this, marker);
});
