// Runs the Electron side of the primitive harness (main.cjs) with the project's Electron, or with
// --gpui the GPUI side (src-gpui/target/debug/Playback.exe --primitive <case> <dir>, one process per case).
//
//   node scripts/primitives/run.mjs [--only=a,b] [--out=dir] [--jobs=N]
//   node scripts/primitives/run.mjs --gpui [--only=a,b] [--out=dir]   default out: parity/out/primitives/gpui
//   node scripts/primitives/run.mjs --golden        write src-gpui/src/css/golden/*.json
//   node scripts/primitives/run.mjs --stability     render everything twice (temp dirs) and require
//                                                   byte-identical PNGs
//
// Offscreen only: nothing is shown, input goes through webContents.sendInputEvent into the page.

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.join(here, "..", "..");
const electron = path.join(root, "node_modules", "electron", "dist", process.platform === "win32" ? "electron.exe" : "electron");
const args = process.argv.slice(2);

const gpuiExe = path.join(root, "src-gpui", "target", "debug", "Playback.exe");
const cases = JSON.parse(fs.readFileSync(path.join(here, "cases.json"), "utf8")).cases;

function runGpui(extra) {
  const opt = (k) => extra.find((a) => a.startsWith(`--${k}=`))?.slice(k.length + 3);
  const out = opt("out") ?? path.join(root, "parity", "out", "primitives", "gpui");
  const only = opt("only")?.split(",");
  for (const c of cases) {
    if (only && !only.includes(c.name)) continue;
    const dest = c.frames ? path.join(out, c.name) : path.join(out, `${c.name}.png`);
    fs.mkdirSync(out, { recursive: true });
    const st = spawnSync(gpuiExe, ["--primitive", c.name, dest], { stdio: "inherit" }).status ?? 1;
    if (st) return st;
  }
  return 0;
}

const gpui = args.includes("--gpui");
const run = (extra) =>
  gpui ? runGpui(extra) : (spawnSync(electron, [path.join(here, "main.cjs"), ...extra], { stdio: "inherit" }).status ?? 1);

function walk(dir, rel = "") {
  const out = [];
  for (const e of fs.readdirSync(path.join(dir, rel), { withFileTypes: true })) {
    const r = path.join(rel, e.name);
    if (e.isDirectory()) out.push(...walk(dir, r));
    else if (e.name.endsWith(".png")) out.push(r);
  }
  return out.sort();
}

if (args.includes("--stability")) {
  const rest = args.filter((a) => a !== "--stability" && a !== "--gpui" && !a.startsWith("--out="));
  const a = fs.mkdtempSync(path.join(os.tmpdir(), "prim-a-"));
  const b = fs.mkdtempSync(path.join(os.tmpdir(), "prim-b-"));
  if (run([...rest, `--out=${a}`]) || run([...rest, `--out=${b}`])) process.exit(1);
  const fa = walk(a);
  const fb = walk(b);
  let diff = fa.length === fb.length ? 0 : 1;
  for (const f of fa) if (!fs.existsSync(path.join(b, f)) || !fs.readFileSync(path.join(a, f)).equals(fs.readFileSync(path.join(b, f)))) {
    console.log(`  differs: ${f}`);
    diff++;
  }
  console.log(`stability: ${fa.length} PNGs, ${diff ? `${diff} DIFFER` : "all byte-identical"}`);
  fs.rmSync(a, { recursive: true, force: true });
  fs.rmSync(b, { recursive: true, force: true });
  process.exit(diff ? 1 : 0);
}
process.exit(run(args.filter((a) => a !== "--gpui")));
