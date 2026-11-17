import { join } from "node:path";

export default {
  paths: ["../features/**/*.feature"],
  import: ["e2e/support/*.mjs", "e2e/steps/*.mjs"],
  tags: "@ui",
  format: ["progress", ["html", join(process.env.TEMP, "ao-bdd", "ui-report.html")]],
};
