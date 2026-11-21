// Diffs every primitive case, frame by frame: Electron (parity/out/primitives/electron) against GPUI
// (parity/out/primitives/gpui), with the parity-diff.mjs thresholds (soft 8/255 over at most 0.1% of
// pixels, hard 24/255) and the shape-edge tolerance (`edges`, parity-diff.mjs: GPUI's quad AA vs Skia's,
// user decision 2026-11-20). Prints one row per case and exits non-zero if any frame fails.
//
//   node scripts/primitives/diff.mjs [--only=a,b] [--electron=dir] [--gpui=dir] [--out=dir]
//
// Diff images of failing frames go to <out>/<case>/fNNN.png (default parity/out/primitives/diff).

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { PNG } from "pngjs";
import { compare } from "../parity-diff.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.join(here, "..", "..");
const opt = (k, d) => process.argv.slice(2).find((a) => a.startsWith(`--${k}=`))?.slice(k.length + 3) ?? d;
const base = path.join(root, "parity", "out", "primitives");
const dirE = opt("electron", path.join(base, "electron"));
const dirG = opt("gpui", path.join(base, "gpui"));
const dirD = opt("out", path.join(base, "diff"));
const only = opt("only")?.split(",");
const cases = JSON.parse(fs.readFileSync(path.join(here, "cases.json"), "utf8")).cases.filter((c) => !only || only.includes(c.name));

const read = (f) => PNG.sync.read(fs.readFileSync(f));
fs.rmSync(dirD, { recursive: true, force: true });

let failed = 0;
const rows = [];
for (const c of cases) {
  const names = c.frames ? Array.from({ length: c.frames }, (_, k) => `${c.name}/f${String(k).padStart(3, "0")}.png`) : [`${c.name}.png`];
  let maxDelta = 0;
  let worstPct = 0;
  let bad = 0;
  let edgePx = 0;
  let edgeMax = 0;
  const notes = [];
  for (const n of names) {
    const fe = path.join(dirE, n);
    const fg = path.join(dirG, n);
    if (!fs.existsSync(fe) || !fs.existsSync(fg)) {
      bad++;
      notes.push(`${path.basename(n)} missing (${fs.existsSync(fe) ? "gpui" : "electron"})`);
      continue;
    }
    const r = compare(read(fe), read(fg), { edges: true, diffOut: path.join(dirD, n) });
    if (r.reason) {
      bad++;
      notes.push(`${path.basename(n)}: ${r.reason}`);
      continue;
    }
    maxDelta = Math.max(maxDelta, r.maxDelta);
    edgePx = Math.max(edgePx, r.edgePixels);
    edgeMax = Math.max(edgeMax, r.edgeMaxDelta);
    worstPct = Math.max(worstPct, r.softPct);
    if (!r.pass) {
      bad++;
      notes.push(`${path.basename(n)} max ${r.maxDelta} soft ${r.softPct}% hard ${r.hardPixels}px`);
    }
  }
  if (bad) failed++;
  rows.push({ case: c.name, frames: names.length, maxDelta, worstSoftPct: worstPct, edgePx, edgeMax, failed: bad, result: bad ? "FAIL" : "pass", notes: notes.slice(0, 4).join("; ") });
}

const cols = ["case", "frames", "maxDelta", "worstSoftPct", "edgePx", "edgeMax", "failed", "result"];
const width = cols.map((k) => Math.max(k.length, ...rows.map((r) => String(r[k]).length)));
const line = (vals) => vals.map((v, i) => String(v).padEnd(width[i])).join("  ");
console.log(line(cols));
console.log(line(width.map((w) => "-".repeat(w))));
for (const r of rows) {
  console.log(line(cols.map((k) => r[k])));
  if (r.notes) console.log(`    ${r.notes}`);
}
console.log(`\n${rows.length - failed}/${rows.length} cases pass (max delta in /255 incl. tolerated edge pixels; soft % = share of untolerated pixels over 8/255; edgePx = most tolerated edge pixels in one frame, edgeMax = their largest delta)${failed ? `; diff images: ${dirD}` : ""}`);
process.exit(failed ? 1 : 0);
