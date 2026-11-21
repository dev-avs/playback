// Pixel diff for the parity harness (port.md §4.4, plan.md Decisions #1).
//
//   node scripts/parity-diff.mjs <a.png> <b.png> [--out diff.png] [--mask x,y,w,h[,tol] ...] [--aa] [--text x,y,w,h ...]
//   node scripts/parity-diff.mjs <dirA> <dirB> [--out diffDir] [--aa]
//   node scripts/parity-diff.mjs --self-test [some-capture.png]
//
// Directory mode compares every .png under dirA with the same relative path under dirB (scene
// stills and motion/<name>/fNNN.png frames). A file missing from dirB fails. If `<name>.json`
// (written by scripts/snapshot/main.cjs) sits next to the dirA image, its `masks` apply.
// Exit code 0 = pass, 1 = fail; a JSON summary goes to stdout.
//
// A pixel's delta is the largest of |Δ alpha| and the |Δ| of each colour channel premultiplied by
// alpha (so invisible transparent pixels with different RGB do not count).
// Fail when, over the whole image:
//   - more than 0.1% of pixels have delta > 8/255 ("soft"), or
//   - any pixel has delta > 24/255 ("hard").
// Mask boxes (the sung line: spring settling differs by ~20/255) raise both limits inside the
// box: deltas up to `tol` (default 20) are ignored, then the usual rules count the rest (soft
// pixels are those over tol, the hard limit stays max(24, tol)).
//
// --aa (text antialiasing, DirectWrite vs Skia): inside text regions (`--text` boxes, or the
// whole image if none are given) both images are blurred with a 3x3 box first and the limits
// become soft 16 / hard 40. Glyph-edge noise averages out below that; a 1 px layout shift, a
// weight change (strokes move ~1 px) or a colour change leaves differences well above it and
// still fails. `--self-test` checks exactly that.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { PNG } from "pngjs";
import pixelmatch from "pixelmatch";

const SOFT = 8;
const HARD = 24;
const MAX_SOFT_FRACTION = 0.001;
const AA_SOFT = 16;
const AA_HARD = 40;

const readPng = (f) => PNG.sync.read(fs.readFileSync(f));

function premult(img) {
  const { width: w, height: h, data } = img;
  const out = new Float32Array(w * h * 4);
  for (let i = 0; i < w * h; i++) {
    const a = data[i * 4 + 3];
    out[i * 4] = (data[i * 4] * a) / 255;
    out[i * 4 + 1] = (data[i * 4 + 1] * a) / 255;
    out[i * 4 + 2] = (data[i * 4 + 2] * a) / 255;
    out[i * 4 + 3] = a;
  }
  return out;
}

function blur3(src, w, h) {
  const out = new Float32Array(src.length);
  for (let y = 0; y < h; y++)
    for (let x = 0; x < w; x++) {
      for (let c = 0; c < 4; c++) {
        let s = 0;
        let n = 0;
        for (let dy = -1; dy <= 1; dy++)
          for (let dx = -1; dx <= 1; dx++) {
            const xx = x + dx;
            const yy = y + dy;
            if (xx < 0 || yy < 0 || xx >= w || yy >= h) continue;
            s += src[(yy * w + xx) * 4 + c];
            n++;
          }
        out[(y * w + x) * 4 + c] = s / n;
      }
    }
  return out;
}

const inBox = (b, x, y) => x >= b.x && y >= b.y && x < b.x + b.w && y < b.y + b.h;

/** Compare two decoded PNGs. opts: { masks:[{x,y,w,h,tol}], aa:boolean, text:[{x,y,w,h}], diffOut:path } */
export function compare(a, b, opts = {}) {
  if (a.width !== b.width || a.height !== b.height)
    return { pass: false, reason: `size ${a.width}x${a.height} vs ${b.width}x${b.height}` };
  const { width: w, height: h } = a;
  const masks = opts.masks ?? [];
  const pa = premult(a);
  const pb = premult(b);
  let ba;
  let bb;
  if (opts.aa) {
    ba = blur3(pa, w, h);
    bb = blur3(pb, w, h);
  }
  const text = opts.text ?? [{ x: 0, y: 0, w, h }];
  let soft = 0;
  let hard = 0;
  let max = 0;
  const diff = new PNG({ width: w, height: h });
  pixelmatch(a.data, b.data, diff.data, w, h, { threshold: 0.1, includeAA: true });
  for (let y = 0; y < h; y++)
    for (let x = 0; x < w; x++) {
      const i = (y * w + x) * 4;
      const useAA = opts.aa && text.some((t) => inBox(t, x, y));
      const A = useAA ? ba : pa;
      const B = useAA ? bb : pb;
      let d = 0;
      for (let c = 0; c < 4; c++) d = Math.max(d, Math.abs(A[i + c] - B[i + c]));
      let softT = useAA ? AA_SOFT : SOFT;
      let hardT = useAA ? AA_HARD : HARD;
      for (const m of masks) if (inBox(m, x, y)) {
        const tol = m.tol ?? 20;
        softT = Math.max(softT, tol);
        hardT = Math.max(hardT, tol);
      }
      if (d > max) max = d;
      const isHard = d > hardT;
      const isSoft = d > softT;
      if (isHard) hard++;
      if (isSoft) soft++;
      if (isHard || isSoft) {
        diff.data[i] = 255;
        diff.data[i + 1] = isHard ? 0 : 200;
        diff.data[i + 2] = 0;
        diff.data[i + 3] = 255;
      }
    }
  const softPct = (soft / (w * h)) * 100;
  const pass = hard === 0 && soft / (w * h) <= MAX_SOFT_FRACTION;
  if (opts.diffOut && !pass) {
    fs.mkdirSync(path.dirname(path.resolve(opts.diffOut)), { recursive: true });
    fs.writeFileSync(opts.diffOut, PNG.sync.write(diff));
  }
  return { pass, size: [w, h], softPixels: soft, softPct: +softPct.toFixed(4), hardPixels: hard, maxDelta: +max.toFixed(1) };
}

const parseBox = (s) => {
  const [x, y, w, h, tol] = s.split(",").map(Number);
  return { x, y, w, h, ...(tol != null && !Number.isNaN(tol) ? { tol } : {}) };
};

function walk(dir, rel = "") {
  const out = [];
  for (const e of fs.readdirSync(path.join(dir, rel), { withFileTypes: true })) {
    const r = path.join(rel, e.name);
    if (e.isDirectory()) out.push(...walk(dir, r));
    else if (e.name.endsWith(".png")) out.push(r);
  }
  return out;
}

export function compareDirs(dirA, dirB, opts = {}) {
  const files = [];
  let pass = true;
  for (const rel of walk(dirA).sort()) {
    const fb = path.join(dirB, rel);
    const entry = { file: rel.replace(/\\/g, "/") };
    if (!fs.existsSync(fb)) {
      Object.assign(entry, { pass: false, reason: "missing in B" });
    } else {
      const side = path.join(dirA, rel.replace(/\.png$/, ".json"));
      const masks = [...(opts.masks ?? []), ...(fs.existsSync(side) ? (JSON.parse(fs.readFileSync(side, "utf8")).masks ?? []) : [])];
      Object.assign(entry, compare(readPng(path.join(dirA, rel)), readPng(fb), { ...opts, masks, diffOut: opts.out ? path.join(opts.out, rel) : undefined }));
    }
    if (!entry.pass) pass = false;
    files.push(entry);
  }
  return { pass, compared: files.length, failed: files.filter((f) => !f.pass).length, files: files.filter((f) => !f.pass || opts.all) };
}

// ── self-test ──
function selfTest(srcFile) {
  const src = readPng(srcFile);
  const { width: w, height: h } = src;
  const clone = () => {
    const p = new PNG({ width: w, height: h });
    src.data.copy(p.data);
    return p;
  };
  let seed = 12345;
  const rnd = () => ((seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0) / 4294967296);
  const lum = (d, i) => d[i] * 0.3 + d[i + 1] * 0.59 + d[i + 2] * 0.11;
  const edge = (img, x, y) => {
    if (x < 1 || y < 1 || x >= w - 1 || y >= h - 1) return false;
    const i = (y * w + x) * 4;
    const l = lum(img.data, i);
    return Math.abs(lum(img.data, i + 4) - l) > 30 || Math.abs(lum(img.data, i + w * 4) - l) > 30;
  };
  const shifted = clone();
  for (let y = 0; y < h; y++)
    for (let x = 0; x < w; x++) src.data.copy(shifted.data, (y * w + x) * 4, (y * w + Math.max(0, x - 1)) * 4, (y * w + Math.max(0, x - 1)) * 4 + 4);
  // AA noise: glyph-edge pixels (bright text on dark glass) nudged ±22 towards/away from the stroke
  const noise = clone();
  let edges = 0;
  for (let y = 0; y < h; y++)
    for (let x = 0; x < w; x++)
      if (edge(src, x, y)) {
        edges++;
        const i = (y * w + x) * 4;
        const n = Math.round((rnd() - 0.5) * 44);
        for (let c = 0; c < 3; c++) noise.data[i + c] = Math.max(0, Math.min(255, noise.data[i + c] + n));
      }
  // colour change: the brightest pixels (lit text) lose their red channel
  const recolour = clone();
  for (let i = 0; i < w * h * 4; i += 4) if (lum(src.data, i) > 200) recolour.data[i] = Math.max(0, recolour.data[i] - 90);
  // weight change: bright pixels grow by one pixel to the right (bolder strokes)
  const bolder = clone();
  for (let y = 0; y < h; y++)
    for (let x = 0; x < w - 1; x++) {
      const i = (y * w + x) * 4;
      if (lum(src.data, i) > 200) for (let c = 0; c < 3; c++) bolder.data[i + 4 + c] = Math.max(bolder.data[i + 4 + c], src.data[i + c]);
    }
  const results = [];
  const check = (name, got, want) => results.push({ test: name, pass: got === want, got: got ? "pass" : "fail", want: want ? "pass" : "fail" });
  check("identical", compare(src, clone(), {}).pass, true);
  check("identical (aa)", compare(src, clone(), { aa: true }).pass, true);
  check("1px shift strict", compare(src, shifted, {}).pass, false);
  check("1px shift aa", compare(src, shifted, { aa: true }).pass, false);
  check("AA noise strict (needs --aa)", compare(src, noise, {}).pass, false);
  check("AA noise aa", compare(src, noise, { aa: true }).pass, true);
  check("colour change aa", compare(src, recolour, { aa: true }).pass, false);
  check("weight change aa", compare(src, bolder, { aa: true }).pass, false);
  const maskTest = clone();
  for (let y = 10; y < 30; y++) for (let x = 10; x < 60; x++) for (let c = 0; c < 3; c++) maskTest.data[(y * w + x) * 4 + c] = Math.max(0, maskTest.data[(y * w + x) * 4 + c] - 18);
  check("18/255 change inside mask", compare(src, maskTest, { masks: [{ x: 10, y: 10, w: 50, h: 20, tol: 20 }] }).pass, true);
  check("18/255 change outside mask", compare(src, maskTest, {}).pass, false);
  return { pass: results.every((r) => r.pass), edgePixels: edges, size: [w, h], results };
}

// ── CLI ──
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const argv = process.argv.slice(2);
  const opt = (n) => {
    const out = [];
    for (let i = 0; i < argv.length; i++) if (argv[i] === `--${n}`) out.push(argv[++i]);
    return out;
  };
  const positional = [];
  for (let i = 0; i < argv.length; i++) {
    if (["--out", "--mask", "--text"].includes(argv[i])) i++;
    else if (!argv[i].startsWith("--")) positional.push(argv[i]);
  }
  if (argv.includes("--self-test")) {
    const f = positional[0] ?? path.join(path.dirname(fileURLToPath(import.meta.url)), "..", "fixtures", "parity-selftest.png");
    const r = selfTest(f);
    console.log(JSON.stringify(r, null, 2));
    process.exit(r.pass ? 0 : 1);
  }
  if (positional.length < 2) {
    console.error("usage: parity-diff.mjs <a> <b> [--out p] [--mask x,y,w,h[,tol]] [--aa] [--text x,y,w,h] | --self-test");
    process.exit(2);
  }
  const [a, b] = positional;
  const opts = { aa: argv.includes("--aa"), masks: opt("mask").map(parseBox), text: opt("text").length ? opt("text").map(parseBox) : undefined, out: opt("out")[0], all: argv.includes("--all") };
  let res;
  if (fs.statSync(a).isDirectory()) res = compareDirs(a, b, opts);
  else res = compare(readPng(a), readPng(b), { ...opts, diffOut: opts.out });
  console.log(JSON.stringify(res, null, 2));
  process.exit(res.pass ? 0 : 1);
}
