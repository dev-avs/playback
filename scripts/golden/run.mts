// Golden outputs for the GPUI port (port.md 2.2): runs the TypeScript logic
// in src/ over scripts/golden/fixtures and writes what it returns to
// src-gpui/logic/golden/<module>.json, which the Rust tests must match
// exactly.
//
//   node scripts/golden/run.mts            every module
//   node scripts/golden/run.mts uroman     one module
//
// Each module is a case file in scripts/golden/cases/<module>.mts whose
// default export takes the fixtures helper and returns a JSON value.

import { readdirSync, readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const fixturesDir = join(here, "fixtures");
const outDir = join(here, "../../src-gpui/logic/golden");

export const fixtures = {
  /** a fixture file's text, by its path under fixtures/ */
  text: (rel: string) => readFileSync(join(fixturesDir, rel), "utf8"),
  json: (rel: string) => JSON.parse(readFileSync(join(fixturesDir, rel), "utf8")),
  /** the files in a fixtures/ folder, sorted, as paths under fixtures/ */
  list: (dir: string) =>
    readdirSync(join(fixturesDir, dir))
      .sort()
      .map((f) => `${dir}/${f}`),
};
export type Fixtures = typeof fixtures;

const only = process.argv[2];
const cases = readdirSync(join(here, "cases"))
  .filter((f) => f.endsWith(".mts"))
  .map((f) => f.replace(/\.mts$/, ""))
  .filter((m) => !only || m === only)
  .sort();
if (only && !cases.length) throw new Error(`no case file scripts/golden/cases/${only}.mts`);

mkdirSync(outDir, { recursive: true });
for (const m of cases) {
  const mod = await import(`./cases/${m}.mts`);
  const out = await mod.default(fixtures);
  writeFileSync(join(outDir, `${m}.json`), JSON.stringify(out, null, 1) + "\n");
  console.log(`golden/${m}.json`);
}
