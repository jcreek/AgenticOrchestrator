import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { until } from "./wait.mjs";

/** In-page finders by role, accessible name or label; `$` in the expressions passed to `find` and `click`. */
const FINDERS = String.raw`
  const shown = (e) => !!e && e.getClientRects().length > 0 && !e.closest('[hidden]');
  const nameOf = (e) => (e.getAttribute('aria-label') || e.innerText || '').replace(/\s+/g, ' ').trim();
  const named = (e, name) => nameOf(e) === name || nameOf(e).startsWith(name + ' ');
  const all = (selector, root = document) => [...root.querySelectorAll(selector)].filter(shown);
  const $ = {
    dialog: () => all('[role=dialog]')[0] ?? null,
    button: (name, root) => all('button, [role=tab], [role=menuitemradio], [role=treeitem]', root ?? $.dialog() ?? document).find((e) => named(e, name)) ?? null,
    pageButton: (name) => $.button(name, document),
    radio: (title) => all('label').find((label) => label.querySelector('input[type=radio]') && (label.querySelector('.choice-title')?.innerText ?? label.innerText).trim() === title) ?? null,
    field: (label) => { const found = all('label').find((e) => e.innerText.trim().startsWith(label)); return found ? document.getElementById(found.htmlFor) : null; },
    session: () => all('.session')[0] ?? null,
    tab: (id) => document.querySelector('[data-tab-id="' + id + '"]'),
    row: (id) => document.querySelector('.ov-row[data-workspace-id="' + id + '"]'),
    terminal: () => all('.term-slot .xterm-mount', $.session() ?? document)[0] ?? null,
    editor: () => all('.ide-monaco', $.session() ?? document)[0] ?? null,
  };
`;

const page = (expression) => `(() => { ${FINDERS} return (${expression}); })()`;

/** Evaluates `expression` in the page with the `$` finders in scope. */
export const find = (world, expression) => world.app.evaluate(page(expression));

const KEYS = {
  Enter: { code: "Enter", keyCode: 13, text: "\r" },
  Escape: { code: "Escape", keyCode: 27 },
  Tab: { code: "Tab", keyCode: 9 },
  End: { code: "End", keyCode: 35 },
};

/** Presses a key as the keyboard would, e.g. press(world, "w", { ctrl: true }). */
export async function press(world, key, { ctrl = false, shift = false } = {}) {
  const known = KEYS[key];
  const upper = key.toUpperCase();
  const code = known?.code ?? (/^\d$/.test(key) ? `Digit${key}` : `Key${upper}`);
  const keyCode = known?.keyCode ?? upper.charCodeAt(0);
  const modifiers = (ctrl ? 2 : 0) | (shift ? 8 : 0);
  const text = !ctrl && known?.text;
  const event = { key, code, windowsVirtualKeyCode: keyCode, modifiers };
  await world.app.cdp.send("Input.dispatchKeyEvent", { type: text ? "keyDown" : "rawKeyDown", ...event, ...(text ? { text } : {}) });
  await world.app.cdp.send("Input.dispatchKeyEvent", { type: "keyUp", ...event });
}

export const typeText = (world, text) => world.app.cdp.send("Input.insertText", { text });

/** Clicks the middle of the element `expression` finds, waiting for it to appear. */
export async function click(world, expression, what = expression) {
  const point = await until(`${what} to be shown`, () =>
    find(world, `(() => { const e = ${expression}; if (!e) return null; e.scrollIntoView({ block: 'nearest', inline: 'nearest' }); const r = e.getBoundingClientRect(); return r.width > 0 ? { x: r.x + r.width / 2, y: r.y + r.height / 2 } : null; })()`),
  );
  for (const type of ["mouseMoved", "mousePressed", "mouseReleased"]) {
    await world.app.cdp.send("Input.dispatchMouseEvent", { type, x: point.x, y: point.y, button: "left", clickCount: 1 });
  }
}

/** Drags from the middle of one element to the middle of another, holding the left button, in several moves. */
export async function drag(world, fromExpression, toExpression) {
  const centre = (expression) => find(world, `(() => { const r = (${expression}).getBoundingClientRect(); return { x: r.x + r.width / 2, y: r.y + r.height / 2 }; })()`);
  const [from, to] = [await centre(fromExpression), await centre(toExpression)];
  const send = (type, at, buttons) => world.app.cdp.send("Input.dispatchMouseEvent", { type, x: at.x, y: at.y, button: "left", buttons, clickCount: 1 });
  await send("mouseMoved", from, 0);
  await send("mousePressed", from, 1);
  for (let step = 1; step <= 5; step += 1) {
    await send("mouseMoved", { x: from.x + ((to.x - from.x) * step) / 5, y: from.y + ((to.y - from.y) * step) / 5 }, 1);
  }
  await send("mouseReleased", to, 0);
}

export const clickButton =(world, name) => click(world, `$.button(${JSON.stringify(name)})`, `the "${name}" button`);

/** The workspace id the app gave the repository or worktree folder called `name`. */
export async function workspaceId(world, name) {
  const path = world.place(name).toLowerCase();
  return until(`a workspace for "${name}"`, () => world.readState().workspaces.find((w) => w.path.toLowerCase() === path)?.id);
}

/** A tab's full description, e.g. "alpha · main · Working · 5s". */
export async function tabDescription(world, name) {
  const id = await workspaceId(world, name);
  return find(world, `$.tab(${JSON.stringify(id)})?.getAttribute('aria-label') ?? null`);
}

export const statusIn = (description) => (description ?? "").replace(/ · \d+[smh]$/, "");

export async function expectTabStatus(world, name, label, timeoutMs) {
  await until(
    `the "${name}" tab to show "${label}"`,
    async () => {
      const description = await tabDescription(world, name);
      if (statusIn(description).endsWith(` · ${label}`)) return true;
      throw new Error(`the tab reads "${description}"`);
    },
    timeoutMs,
  );
}

/** Makes `name` the active session: by its tab when the tab is in view, else from the Overview. */
export async function view(world, name) {
  const id = JSON.stringify(await workspaceId(world, name));
  const tabInView = await find(world, `(() => { const t = $.tab(${id}); if (!t) return false; const s = t.parentElement.closest('.tab-strip') ?? t.parentElement; const a = t.getBoundingClientRect(), b = s.getBoundingClientRect(); return a.left >= b.left - 1 && a.right <= b.right + 1; })()`);
  if (tabInView) {
    await click(world, `$.tab(${id})`, `the "${name}" tab`);
  } else {
    await press(world, "0", { ctrl: true });
    await click(world, `$.row(${id})`, `the "${name}" row on the Overview`);
  }
  await until(`"${name}" to be the active session`, () => find(world, `$.tab(${id})?.getAttribute('aria-current') === 'page'`));
}

/** Text in the active session's visible terminal, read from xterm's buffer. */
export function terminalText(world) {
  return find(
    world,
    `(() => {
      const mount = $.terminal();
      if (!mount) return null;
      let fiber = mount[Object.keys(mount).find((key) => key.startsWith('__reactFiber'))];
      while (fiber && typeof fiber.type !== 'function') fiber = fiber.return;
      for (let hook = fiber?.memoizedState; hook; hook = hook.next) {
        const term = hook.memoizedState?.current;
        if (!term?.buffer?.active) continue;
        const buffer = term.buffer.active;
        const lines = [];
        for (let i = 0; i < buffer.length; i++) lines.push(buffer.getLine(i)?.translateToString(true) ?? '');
        return lines.join('\\n');
      }
      return null;
    })()`,
  );
}

export async function expectTerminalShows(world, text) {
  await until(`the terminal to show "${text}"`, async () => ((await terminalText(world)) ?? "").includes(text) || false);
}

export async function typeIntoTerminal(world, text) {
  await click(world, "$.terminal()", "the terminal");
  await typeText(world, text);
}

/** Text in the active session's editor. */
export const editorText = (world) =>
  find(world, `$.editor()?.querySelector('.view-lines')?.innerText.replace(/\\u00a0/g, ' ') ?? null`);

// Stub Claude Code control. See docs/testing.md.

function launches(world) {
  try {
    return readFileSync(join(world.stubDir, "launches.log"), "utf8")
      .split(/\r?\n/)
      .filter(Boolean)
      .map((line) => {
        const [mode, id, ...cwd] = line.split(" ");
        return { mode, id, cwd: cwd.join(" ") };
      });
  } catch {
    return [];
  }
}

/** Claude Code launches in the folder called `name`, oldest first. */
export const launchesIn = (world, name) =>
  launches(world).filter((launch) => launch.cwd.toLowerCase() === world.place(name).toLowerCase());

export async function claudeSession(world, name) {
  const found = await until(`Claude Code to start in "${name}"`, () => launchesIn(world, name).at(-1));
  return found.id;
}

export function control(world, sessionId, value) {
  mkdirSync(join(world.stubDir, "control"), { recursive: true });
  writeFileSync(join(world.stubDir, "control", `${sessionId}.json`), JSON.stringify(value));
}

export function controlAll(world, value) {
  mkdirSync(join(world.stubDir, "control"), { recursive: true });
  writeFileSync(join(world.stubDir, "control", "default.json"), JSON.stringify(value));
}

export const setAgentsMode = (world, mode) => writeFileSync(join(world.stubDir, "agents-mode"), mode);
