// Gate for the harness: run every scene and motion twice and diff the two runs.
//   node scripts/snapshot/stability.mjs [outDir] [--jobs=N] [--only=a,b]
// outDir (default parity/out/stability) gets run1/ and run2/. Reports how many files are
// byte-identical, how many differ but pass parity-diff, and how many fail. Exit 1 on any fail.
import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { compareDirs } from "../parity-diff.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const electron = createRequire(import.meta.url)("electron");
const argv = process.argv.slice(2);
const out = path.resolve(argv.find((a) => !a.startsWith("--")) ?? path.join(here, "..", "..", "parity", "out", "stability"));
const pass = argv.filter((a) => a.startsWith("--jobs=") || a.startsWith("--only="));

const run = (dir, extra) => spawnSync(electron, [path.join(here, "main.cjs"), dir, ...pass, ...extra], { stdio: "inherit" });
let bad = 0;
for (const n of ["run1", "run2"]) {
  const d = path.join(out, n);
  fs.rmSync(d, { recursive: true, force: true });
  if (run(d, []).status !== 0) bad++; // stills; a scene whose own two captures differ fails here
  if (run(d, ["--motion=all"]).status !== 0) bad++;
}
const files = [];
const walk = (d, rel = "") => {
  for (const e of fs.readdirSync(path.join(d, rel), { withFileTypes: true })) {
    const r = path.join(rel, e.name);
    if (e.isDirectory()) walk(d, r);
    else if (e.name.endsWith(".png")) files.push(r);
  }
};
walk(path.join(out, "run1"));
let identical = 0;
const differing = [];
for (const f of files) {
  const b = path.join(out, "run2", f);
  if (fs.existsSync(b) && fs.readFileSync(path.join(out, "run1", f)).equals(fs.readFileSync(b))) identical++;
  else differing.push(f);
}
const res = compareDirs(path.join(out, "run1"), path.join(out, "run2"), { out: path.join(out, "diff"), all: false });
const summary = {
  files: files.length,
  byteIdentical: identical,
  differButPass: differing.length - res.failed,
  failing: res.failed,
  runnerFailures: bad,
  differing,
  failures: res.files,
};
console.log(JSON.stringify(summary, null, 2));
process.exit(res.failed || bad ? 1 : 0);
