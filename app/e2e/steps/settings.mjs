import { Then, When } from "@cucumber/cucumber";
import { readFileSync } from "node:fs";
import { click, find } from "../support/ui.mjs";
import { until } from "../support/wait.mjs";

const appVersion = () => /^version = "([^"]+)"/m.exec(readFileSync(new URL("../../src-tauri/Cargo.toml", import.meta.url), "utf8"))[1];

When("I open Settings", async function () {
  await click(this, "$.pageButton('Settings')", "the Settings button");
});

Then("Settings shows the app version", async function () {
  const expected = `Version ${appVersion()}`;
  await until(`Settings to show "${expected}"`, async () => {
    const shown = await find(this, "$.dialog()?.querySelector('.dialog-footer-note')?.innerText ?? null");
    if (shown?.startsWith(expected)) return true;
    throw new Error(`the footer reads ${JSON.stringify(shown)}`);
  });
});
