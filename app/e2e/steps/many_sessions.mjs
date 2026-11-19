import { Given, Then, When } from "@cucumber/cucumber";
import assert from "node:assert/strict";
import { mkdirSync } from "node:fs";
import { randomUUID } from "node:crypto";
import { join } from "node:path";
import { claudeSession, click, clickButton, control, drag, expectTabStatus, find, press, view, workspaceId } from "../support/ui.mjs";
import { until } from "../support/wait.mjs";

const json = JSON.stringify;
const folderName = (n) => `folder-${String(n).padStart(2, "0")}`;

/** In-page: whether `element` lies fully inside the tab strip and the window, and is the topmost element at its centre. */
const FULLY_VISIBLE = `(element) => {
  if (!element) return 'missing';
  const r = element.getBoundingClientRect();
  const strip = element.closest('.tab-strip')?.getBoundingClientRect() ?? { left: 0, right: innerWidth };
  if (r.width === 0 || r.left < Math.max(0, strip.left) - 1 || r.right > Math.min(innerWidth, strip.right) + 1 || r.top < 0 || r.bottom > innerHeight) {
    return 'outside the visible area (' + Math.round(r.left) + '..' + Math.round(r.right) + ' of ' + innerWidth + ')';
  }
  const top = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
  return element.contains(top) ? 'ok' : 'covered by ' + (top?.className || top?.tagName);
}`;

Given("{int} folders are open as sessions", async function (count) {
  const workspaces = [];
  for (let n = 1; n <= count; n += 1) {
    const path = join(this.root, folderName(n));
    mkdirSync(path, { recursive: true });
    this.places.set(folderName(n), path);
    workspaces.push({ id: randomUUID(), name: folderName(n), path, claudeSessionId: null, createdWorktree: null });
  }
  this.writeState({ workspaces });
  await this.open();
  await until(`${count} tabs`, () => find(this, `document.querySelectorAll('[data-tab-id]').length === ${count}`));
});

When("I drag the {string} tab onto the {string} tab", async function (from, onto) {
  const tab = async (name) => `$.tab(${json(await workspaceId(this, name))})`;
  await drag(this, await tab(from), await tab(onto));
});

Then(/^the tabs are in the order (.+)$/, async function (names) {
  const expected = [...names.matchAll(/"([^"]*)"/g)].map(([, name]) => name);
  await until(`the tabs to be in the order ${names}`, async () => {
    const shown = await find(this, `[...document.querySelectorAll('[data-tab-id] .tab-name')].map((e) => e.innerText)`);
    if (json(shown) === json(expected)) return true;
    throw new Error(`the tabs are ${json(shown)}`);
  });
});

When("I close and reopen the app", async function () {
  await until("the tab order to be saved", async () => {
    const shown = await find(this, `[...document.querySelectorAll('[data-tab-id]')].map((e) => e.dataset.tabId)`);
    return json(this.readState().workspaces.map((w) => w.id)) === json(shown);
  });
  await this.restart();
});

Given("the window is {int} pixels wide", async function (width) {
  await this.app.cdp.send("Emulation.setDeviceMetricsOverride", { width, height: 800, deviceScaleFactor: 0, mobile: false });
  await until(`the page to be ${width} pixels wide`, () => find(this, `innerWidth === ${width}`));
});

Then("the Overview, New session, minimise, maximise and close buttons are fully visible and not covered", async function () {
  const controls = {
    Overview: ".overview-tab",
    "New session": ".new-session",
    Minimise: ".window-controls [aria-label=Minimise]",
    Maximise: ".window-controls [aria-label=Maximise], .window-controls [aria-label=Restore]",
    Close: ".window-controls [aria-label=Close]",
  };
  await until("every window control to be fully visible", async () => {
    const found = await find(this, `(() => { const check = ${FULLY_VISIBLE}; return Object.fromEntries(Object.entries(${json(controls)}).map(([name, selector]) => [name, check(document.querySelector(selector))])); })()`);
    const problems = Object.entries(found).filter(([, state]) => state !== "ok");
    if (problems.length === 0) return true;
    throw new Error(problems.map(([name, state]) => `${name} is ${state}`).join("; "));
  });
});

When("I press the New session button", async function () {
  await click(this, "document.querySelector('.new-session')", "the New session button");
});

Given(/^Claude Code asks a question in (.+)$/, async function (names) {
  for (const [, name] of names.matchAll(/"([^"]*)"/g)) {
    await view(this, name);
    await clickButton(this, "Start fresh");
    control(this, await claudeSession(this, name), { status: "waiting", waitingFor: "question" });
    await expectTabStatus(this, name, "Waiting for input", 30_000);
  }
});

Then("the overflow button says how many sessions are hidden and that {int} of them need me", async function (needYou) {
  await until("the overflow button to count the hidden sessions", async () => {
    const { label, hidden } = await find(this, `(() => {
      const strip = document.querySelector('.tab-strip').getBoundingClientRect();
      const hidden = [...document.querySelectorAll('[data-tab-id]')].filter((tab) => {
        const r = tab.getBoundingClientRect();
        return r.left < strip.left - 1 || r.right > strip.right + 1;
      }).length;
      return { label: document.querySelector('[aria-haspopup=menu]')?.getAttribute('aria-label') ?? null, hidden };
    })()`);
    const expected = `${hidden} more sessions, ${needYou} need you`;
    if (hidden > 0 && label === expected) return true;
    throw new Error(`the button reads ${json(label)}; expected ${json(expected)}`);
  });
});

When("I open the overflow menu", async function () {
  await click(this, "document.querySelector('[aria-haspopup=menu]')", "the overflow button");
});

Then("it lists all {int} sessions with their status", async function (count) {
  const rows = await until("the session menu", async () => {
    const found = await find(this, `[...document.querySelectorAll('[role=menu] [role=menuitemradio]')].map((row) => row.innerText.replace(/\\s+/g, ' '))`);
    return found.length > 0 ? found : null;
  });
  assert.equal(rows.length, count, rows.join("\n"));
  for (let n = 1; n <= count; n += 1) {
    const row = rows.find((text) => text.includes(folderName(n)));
    const status = n >= 30 ? "Waiting for input" : "Not started";
    assert.ok(row?.includes(status), `the menu row for ${folderName(n)} reads ${json(row)}, expected it to say "${status}"`);
  }
});

When("I choose {string} from the menu", async function (name) {
  await click(this, `[...document.querySelectorAll('[role=menuitemradio]')].find((row) => row.innerText.includes(${json(name)}))`, `"${name}" in the menu`);
});

Then("{string} is the active session", async function (name) {
  const id = await workspaceId(this, name);
  await until(`"${name}" to be active`, () => find(this, `document.querySelector('[data-tab-id=${json(id)}]')?.getAttribute('aria-current') === 'page'`));
});

When(/^I switch to the (first|last|16th) session with the keyboard$/, async function (which) {
  if (which === "last") {
    await press(this, "0", { ctrl: true });
    await press(this, "Tab", { ctrl: true, shift: true });
  } else {
    await press(this, "1", { ctrl: true });
    for (let step = 1; which === "16th" && step < 16; step += 1) await press(this, "Tab", { ctrl: true });
  }
  const expected = { first: folderName(1), last: folderName(this.places.size), "16th": folderName(16) }[which];
  const id = await workspaceId(this, expected);
  await until(`"${expected}" to be active`, () => find(this, `document.querySelector('[data-tab-id=${json(id)}]')?.getAttribute('aria-current') === 'page'`));
});

Then("the tab of the active session is fully visible", async function () {
  await until("the active tab to be fully visible", async () => {
    const state = await find(this, `(${FULLY_VISIBLE})(document.querySelector('[data-tab-id][aria-current=page]'))`);
    if (state === "ok") return true;
    throw new Error(`the active tab is ${state}`);
  });
});

Then('{string} is a single tab named {string} without a group label', async function (name, label) {
  const id = await workspaceId(this, name);
  await until(`"${name}" to be a single tab`, async () => {
    const state = await find(this, `(() => {
      const tab = document.querySelector('[data-tab-id=${json(id)}]');
      return { text: tab?.querySelector('.tab-name')?.innerText, inGroup: !!tab?.closest('[role=group]'),
        labels: [...document.querySelectorAll('.tab-group-label')].map((e) => e.innerText) };
    })()`);
    if (state.text === label && !state.inGroup && !state.labels.includes(name)) return true;
    throw new Error(json(state));
  });
});

Then("the {string} group has a label followed by tabs for {string} and {string}", async function (group, first, second) {
  await until(`the "${group}" group`, async () => {
    const found = await find(this, `(() => {
      const element = document.querySelector('[role=group][aria-label=${json(group)}]');
      if (!element) return null;
      return [...element.children].map((child) => child.classList.contains('tab-group-label') ? 'label:' + child.innerText : child.querySelector('.tab-name')?.innerText);
    })()`);
    const expected = [`label:${group}`, first, second];
    if (json(found) === json(expected)) return true;
    throw new Error(`the group shows ${json(found)}`);
  });
});

When(/^I (collapse|expand) the "([^"]*)" group$/, async function (action, group) {
  const label = `document.querySelector('[role=group][aria-label=${json(group)}] .tab-group-label')`;
  await click(this, label, `the "${group}" group label`);
  const expanded = action === "expand";
  await until(`the "${group}" group to be ${action}ed`, () => find(this, `${label}?.getAttribute('aria-expanded') === '${expanded}'`));
});

Then("the {string} group shows only its label with {int} sessions", async function (group, count) {
  await until(`the "${group}" group to be collapsed`, async () => {
    const found = await find(this, `(() => {
      const element = document.querySelector('[role=group][aria-label=${json(group)}]');
      return element && { children: element.children.length, count: element.querySelector('.tab-group-count')?.innerText };
    })()`);
    if (json(found) === json({ children: 1, count: String(count) })) return true;
    throw new Error(`the group shows ${json(found)}`);
  });
});
