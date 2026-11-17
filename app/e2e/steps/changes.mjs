import { Then, When } from "@cucumber/cucumber";
import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { git, read, write } from "../support/repos.mjs";
import { click, clickButton, editorText, find, press, typeText, view } from "../support/ui.mjs";
import { until } from "../support/wait.mjs";

const json = JSON.stringify;

async function openIde(world, name) {
  await view(world, name);
  await click(world, "$.button('IDE', $.session())", "the IDE layout button");
}

When("{string} in {string} gains {int} lines", function (file, name, lines) {
  const place = this.place(name);
  write(place, file, read(place, file) + Array.from({ length: lines }, (_, n) => `added ${n + 1}\n`).join(""));
});

When(/^"([^"]*)" is created in (?:the )?"([^"]*)"(?: worktree)?$/, function (file, name) {
  write(this.place(name), file, "created by Claude\n");
});

When("I look at the changes in {string}", async function (name) {
  await openIde(this, name);
  await click(this, "$.button('Changes', $.session())", "the Changes tab");
});

Then("the changes list shows:", async function (table) {
  const expected = table.hashes().sort((a, b) => a.file.localeCompare(b.file));
  await until("the changes list to match", async () => {
    const rows = await find(this, `[...$.session().querySelectorAll('ul[aria-label="Changed files"] li[data-path]')].map((li) => ({
      file: li.dataset.path,
      change: li.querySelector('.ide-visually-hidden').innerText,
      counts: li.querySelector('.ide-counts')?.innerText ?? '',
    }))`);
    rows.sort((a, b) => a.file.localeCompare(b.file));
    if (json(rows) === json(expected)) return true;
    throw new Error(`the list shows ${json(rows)}`);
  });
});

const CHANGE_ROWS = `[...$.session().querySelectorAll('ul[aria-label="Changed files"] li')].map((li) =>
  li.dataset.path ? li.querySelector('.ide-path').innerText : 'folder:' + li.innerText.trim())`;

Then("the changes show the {string} folder with {string} inside it", async function (folder, file) {
  await until(`"${file}" under the "${folder}" folder`, async () => {
    const rows = await find(this, CHANGE_ROWS);
    if (json(rows) === json([`folder:${folder}`, file])) return true;
    throw new Error(`the changes show ${json(rows)}`);
  });
});

When("I switch the changes to a list", async function () {
  await click(this, "$.button('List', $.session())", "the List button");
});

Then("the changes show {string} on one row", async function (path) {
  await until(`"${path}" on one row`, async () => {
    const rows = await find(this, CHANGE_ROWS);
    if (json(rows) === json([path])) return true;
    throw new Error(`the changes show ${json(rows)}`);
  });
});

When("I am editing {string} in {string}", async function (file, name) {
  await openIde(this, name);
  await click(this, "$.button('Files', $.session())", "the Files tab");
  await click(this, `$.button(${json(file)}, $.session())`, `"${file}" in the file tree`);
  const content = read(this.place(name), file).trim();
  await until(`the editor to show "${file}"`, async () => (await editorText(this))?.includes(content));
});

When("I type {string} into the editor", async function (text) {
  await click(this, "$.editor()", "the editor");
  await press(this, "End", { ctrl: true });
  await typeText(this, text);
  await until(`the editor to show "${text}"`, async () => (await editorText(this))?.includes(text));
  await until("the editor to show unsaved changes", () => find(this, `!!$.session().querySelector('[aria-label="Unsaved changes"]')`));
});

When("I save with Ctrl+S", async function () {
  await press(this, "s", { ctrl: true });
});

Then("{string} in {string} contains {string}", async function (file, name, text) {
  await until(`"${file}" to contain "${text}"`, () => read(this.place(name), file).includes(text));
});

Then("the editor shows no unsaved changes", async function () {
  await until("the unsaved changes mark to go", () => find(this, `!$.session().querySelector('[aria-label="Unsaved changes"]')`));
});

When("{string} in {string} is changed on disk to {string}", function (file, name, text) {
  write(this.place(name), file, `${text}\n`);
});

Then("the editor shows {string}", async function (text) {
  await until(`the editor to show "${text}"`, async () => {
    const shown = await editorText(this);
    if (shown?.includes(text)) return true;
    throw new Error(`the editor shows ${json(shown)}`);
  });
});

Then("I am told the file changed on disk", async function () {
  await until("the changed-on-disk warning", () =>
    find(this, `[...$.session().querySelectorAll('[role=alert]')].some((e) => e.innerText.includes('Changed on disk'))`),
  );
});

When("I choose {string}", async function (choice) {
  await clickButton(this, choice);
});

When("I stage {string}", async function (file) {
  await click(this, `$.session().querySelector('[aria-label="Staged: ${file}"]')`, `the stage box of "${file}"`);
  await until(`"${file}" to be staged`, () => find(this, `$.session().querySelector('[aria-label="Staged: ${file}"]')?.getAttribute('aria-checked') === 'true'`));
});

When("I commit with the message {string}", async function (message) {
  await click(this, `$.session().querySelector('[aria-label="Commit message"]')`, "the commit message box");
  await typeText(this, message);
  await clickButton(this, "Commit");
});

Then("I am told {string}", async function (text) {
  await until(`to be told "${text}"`, () =>
    find(this, `[...document.querySelectorAll('[role=status], [role=alert]')].some((e) => e.innerText.includes(${json(text)}))`),
  );
});

Then("the last commit of {string} is {string}", function (name, message) {
  assert.equal(git(this.place(name), "log", "-1", "--format=%s"), message);
});

When("I ask to close the {string} session", async function (name) {
  await view(this, name);
  await clickButton(this, "Close session");
  await until("the close dialog", () => find(this, `$.dialog()?.getAttribute('aria-label')?.startsWith('Close ')`));
});

Then("I am told that removing the worktree loses {string}", async function (what) {
  await until(`the dialog to say what would be lost`, () => find(this, `$.dialog().innerText.includes(${json(`Removing the worktree loses ${what}`)})`));
});

When("I choose to discard the changes and remove the worktree", async function () {
  await click(this, "$.radio('Discard changes and remove the worktree')", "the discard choice");
  await clickButton(this, "Close and remove");
});

Then("the {string} worktree folder is gone", async function (name) {
  await until(`the "${name}" worktree folder to be removed`, () => !existsSync(this.place(name)));
});

Then("the branch {string} still exists in {string}", function (branch, repo) {
  assert.equal(git(this.place(repo), "branch", "--list", branch), branch);
});

Then("I am not offered to remove anything", async function () {
  await until("the close dialog to explain the folder is kept", () => find(this, `$.dialog().innerText.includes('is not changed')`));
  assert.equal(await find(this, "$.dialog().querySelectorAll('input[type=radio]').length"), 0);
});

Then("I am warned {string}", async function (text) {
  await until(`the warning "${text}"`, () => find(this, `$.dialog()?.innerText.includes(${json(text)})`));
});
