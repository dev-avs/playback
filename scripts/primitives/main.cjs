// Electron side of the CSS-layer primitive harness (port.md 2.4, parity/plan.md Phase 4).
//
//   node scripts/primitives/run.mjs [--only=a,b] [--out=dir] [--jobs=N]     (spawns this with Electron)
//   node scripts/primitives/run.mjs --golden                                 easing/transition goldens
//
// One Electron process per case (a second offscreen window in one process fails to load, see
// scripts/snapshot/main.cjs). Each case is scripts/primitives/pages/<name>.html, rendered OFFSCREEN with a
// transparent background at the case's fixed size (scripts/primitives/cases.json). The page clock is
// virtual (clock.cjs): it only moves when this runner steps it, and every transition/animation is seeked
// by hand. Frame k is captured at t = k × 33.33 ms of virtual time after t = 0 (two 1/60 s steps per
// frame); timeline ops with t <= that time run just before the capture. Output:
// <out>/<name>/f000.png … (motion) or <out>/<name>.png (frames = 0). Default out: parity/out/primitives/electron.
//
// --golden writes src-gpui/src/css/golden/{easing,transition}.json: Chromium's own numbers for the Rust
// unit tests (WAAPI getComputedTiming().progress for each curve, getComputedStyle mid-transition for the
// reversal cases), all on the virtual clock.

const { app, BrowserWindow } = require("electron");
const fs = require("fs");
const os = require("os");
const path = require("path");

const ROOT = path.join(__dirname, "..", "..");
const args = process.argv.slice(2);
const flag = (n) => args.find((a) => a.startsWith(`--${n}=`))?.slice(n.length + 3);
const OUT = path.resolve(flag("out") ?? path.join(ROOT, "parity", "out", "primitives", "electron"));
const FRAME = 1000 / 60;
const CASES = JSON.parse(fs.readFileSync(path.join(__dirname, "cases.json"), "utf8")).cases;
const GOLDEN_DIR = path.join(ROOT, "src-gpui", "src", "css", "golden");

const jobArg = flag("job");
if (!jobArg) parent();
else child(jobArg);

async function parent() {
  const { spawn } = require("child_process");
  const only = flag("only")?.split(",");
  let jobs = args.includes("--golden") ? ["golden"] : CASES.map((c) => c.name);
  if (only) jobs = jobs.filter((j) => only.includes(j));
  fs.mkdirSync(OUT, { recursive: true });
  const n = Number(flag("jobs") ?? 4);
  let failed = 0;
  let next = 0;
  const run = (j) =>
    new Promise((res) => {
      const p = spawn(process.execPath, [__filename, `--job=${j}`, `--out=${OUT}`], { stdio: "inherit" });
      p.on("exit", (code) => {
        if (code !== 0) failed++;
        res();
      });
    });
  await Promise.all(
    Array.from({ length: Math.min(n, jobs.length) }, async () => {
      while (next < jobs.length) await run(jobs[next++]);
    }),
  );
  console.log(`${jobs.length - failed}/${jobs.length} ok${failed ? `, ${failed} FAILED` : ""}`);
  process.exit(failed ? 1 : 0);
}

function child(job) {
  app.setPath("userData", fs.mkdtempSync(path.join(os.tmpdir(), "prim-")));
  app.disableHardwareAcceleration();
  app.whenReady().then(async () => {
    const watchdog = setTimeout(() => {
      console.log(`  [${job}] TIMEOUT`);
      app.exit(2);
    }, 240000);
    try {
      const code = job === "golden" ? await runGolden() : await runCase(CASES.find((c) => c.name === job));
      clearTimeout(watchdog);
      app.exit(code);
    } catch (e) {
      console.log(`  [${job}] ERROR ${e && e.stack ? e.stack : e}`);
      app.exit(2);
    }
  });
}

async function openPage(file, [w, h]) {
  const win = new BrowserWindow({
    show: false,
    width: w,
    height: h,
    frame: false,
    transparent: true,
    webPreferences: {
      preload: path.join(__dirname, "clock.cjs"),
      partition: "primitives",
      offscreen: true,
      backgroundThrottling: false,
      contextIsolation: false,
    },
  });
  win.webContents.setFrameRate(60);
  await win.loadFile(file);
  const ev = (code) => win.webContents.executeJavaScript(code, true);
  const frames = (n) => ev(`(async()=>{for(let i=0;i<${n};i++) await __prim.realFrame();})()`);
  return { win, ev, frames };
}

const real = (ms) => new Promise((r) => setTimeout(r, ms));

async function runCase(c) {
  if (!c) throw new Error("no such case");
  const { win, ev, frames } = await openPage(path.join(__dirname, "pages", `${c.name}.html`), c.size);
  await frames(4);
  await real(200);
  // the clock has not moved: every transition the first layout starts is seen at t = 0 and stays there
  await ev("__prim.step(0)");
  await frames(2);
  const advance = (ms) => ev(`(()=>{const n=${Math.round(ms / FRAME)};for(let i=0;i<n;i++)__prim.step(${FRAME});})()`);
  const capture = async () => {
    await frames(2);
    await ev("__prim.step(0)");
    await frames(2);
    await real(60);
    return (await win.webContents.capturePage()).toPNG();
  };
  const send = async (e) => {
    win.webContents.sendInputEvent(e);
    await frames(2);
  };
  async function doOp(op) {
    switch (op.op) {
      case "class":
        await ev(`(()=>{const e=document.querySelector(${JSON.stringify(op.selector)});
          ${op.add ? `e.classList.add(${JSON.stringify(op.add)});` : ""}${op.remove ? `e.classList.remove(${JSON.stringify(op.remove)});` : ""}
          __prim.step(0);})()`);
        await frames(2);
        break;
      case "move":
        await send({ type: "mouseMove", x: op.x, y: op.y });
        await ev("__prim.step(0)");
        break;
      case "out":
        await send({ type: "mouseLeave", x: -1, y: -1 });
        await ev("__prim.step(0)");
        break;
      default:
        throw new Error(`unknown op ${op.op}`);
    }
  }
  const timeline = (c.timeline ?? []).map((o) => ({ ...o, done: false }));
  const count = Math.max(1, c.frames);
  const dir = path.join(OUT, c.name);
  if (c.frames) {
    fs.rmSync(dir, { recursive: true, force: true });
    fs.mkdirSync(dir, { recursive: true });
  }
  for (let k = 0; k < count; k++) {
    if (k > 0) await advance(FRAME * 2);
    const t = k * FRAME * 2;
    for (const o of timeline)
      if (!o.done && o.t <= t + 1e-6) {
        o.done = true;
        await doOp(o);
      }
    const png = await capture();
    if (c.frames) fs.writeFileSync(path.join(dir, `f${String(k).padStart(3, "0")}.png`), png);
    else fs.writeFileSync(path.join(OUT, `${c.name}.png`), png);
  }
  const size = win.getSize();
  console.log(`${c.name}: ${count} frame(s) ${size.join("x")} → ${c.frames ? dir : path.join(OUT, `${c.name}.png`)}`);
  return 0;
}

// ── goldens ──
// Curves: CSS keywords (Blink's presets) and every cubic-bezier() the app's CSS uses.
const CURVES = {
  ease: "ease",
  "ease-in": "ease-in",
  "ease-out": "ease-out",
  "ease-in-out": "ease-in-out",
  linear: "linear",
  "0.4,0,0.2,1": "cubic-bezier(0.4, 0, 0.2, 1)",
  "0.3,1.5,0.5,1": "cubic-bezier(0.3, 1.5, 0.5, 1)",
  "0.3,1.6,0.5,1": "cubic-bezier(0.3, 1.6, 0.5, 1)",
  "0.2,0.8,0.2,1": "cubic-bezier(0.2, 0.8, 0.2, 1)",
  "0.22,1,0.36,1": "cubic-bezier(0.22, 1, 0.36, 1)",
  "0.45,0,0.25,1": "cubic-bezier(0.45, 0, 0.25, 1)",
};

// Reversal cases: a property, the two values, its transition, and when the target flips (virtual ms).
// From src/App.css:129-135 (.widget opacity 300ms ease, rest opacity .2), :268-292 (.lyrics-panel max-height
// 350ms (.4,0,.2,1)) and :2456-2462 (.next-up opacity 120ms ease).
const REVERSALS = [
  { name: "opacity-ease-300", prop: "opacity", a: "1", b: "0.2", transition: "opacity 300ms ease", flips: [100] },
  { name: "opacity-ease-300-twice", prop: "opacity", a: "1", b: "0.2", transition: "opacity 300ms ease", flips: [100, 150] },
  { name: "opacity-ease-300-late", prop: "opacity", a: "1", b: "0.2", transition: "opacity 300ms ease", flips: [250] },
  { name: "opacity-ease-120", prop: "opacity", a: "0.75", b: "1", transition: "opacity 120ms ease", flips: [66.66666666666667] },
  { name: "maxh-standard-350", prop: "max-height", a: "0px", b: "300px", transition: "max-height 350ms cubic-bezier(0.4, 0, 0.2, 1)", flips: [200] },
  // a retarget to a third value (not a reversal): a plain new transition from the current value
  { name: "opacity-retarget", prop: "opacity", a: "1", b: "0.2", c: "0.6", transition: "opacity 300ms ease", flips: [100] },
];

async function runGolden() {
  const blank = path.join(__dirname, "pages", "kf-fs-in.html");
  const { ev, frames } = await openPage(blank, [200, 120]);
  await frames(2);
  const xs = [];
  for (let i = 0; i <= 200; i++) xs.push(i / 200);
  xs.push(0.0001, 0.001, 0.333, 0.4999, 0.5001, 0.999, 0.9999);
  const easing = await ev(`(()=>{const out=[];const curves=${JSON.stringify(CURVES)};const xs=${JSON.stringify(xs)};
    for(const [name,css] of Object.entries(curves)){const d=document.createElement('div');document.body.appendChild(d);
      const a=d.animate([{opacity:0},{opacity:1}],{duration:1000,easing:css,fill:'both'});a.pause();
      for(const x of xs){a.currentTime=x*1000;out.push({curve:name,x,y:a.effect.getComputedTiming().progress});}
      a.cancel();d.remove();}
    return out;})()`);
  // the same curves as CSS transitions, read back with getComputedStyle (float precision): proves the
  // transition path uses the same solver as the WAAPI one
  const css = [];
  for (const [name, fn] of Object.entries(CURVES)) {
    await ev(`(()=>{const d=document.createElement('div');d.id='g';d.style.cssText='width:10px;height:10px;opacity:0;transition:opacity 1000ms ${fn}';document.body.appendChild(d);d.offsetWidth;d.style.opacity='1';__prim.step(0);})()`);
    for (let k = 1; k <= 9; k++) {
      await ev("__prim.step(100)");
      css.push({ curve: name, t: k * 100, opacity: Number(await ev("getComputedStyle(document.getElementById('g')).opacity")) });
    }
    await ev("document.getElementById('g').remove()");
    await frames(1);
  }
  const reversals = [];
  for (const r of REVERSALS) {
    await ev(`(()=>{const d=document.createElement('div');d.id='r';d.style.cssText='width:10px;height:10px;overflow:hidden;${r.prop}:${r.a};transition:${r.transition}';document.body.appendChild(d);d.offsetWidth;d.style.setProperty('${r.prop}','${r.b}');__prim.step(0);})()`);
    const samples = [];
    let t = 0;
    let flip = 0;
    let target = r.b;
    const flips = [...r.flips];
    while (t < 700) {
      const dt = FRAME;
      if (flips.length && t + 1e-6 >= flips[0]) {
        flips.shift();
        flip++;
        target = r.c && flip === 1 ? r.c : target === r.b ? r.a : r.b;
        await ev(`(()=>{document.getElementById('r').style.setProperty('${r.prop}','${target}');__prim.step(0);})()`);
      }
      const v = await ev(`parseFloat(getComputedStyle(document.getElementById('r')).getPropertyValue('${r.prop}'))`);
      samples.push({ t: +t.toFixed(4), v });
      await ev(`__prim.step(${dt})`);
      t += dt;
    }
    reversals.push({ ...r, samples });
    await ev("document.getElementById('r').remove()");
    await frames(1);
  }
  fs.mkdirSync(GOLDEN_DIR, { recursive: true });
  fs.writeFileSync(path.join(GOLDEN_DIR, "easing.json"), JSON.stringify({ source: `Electron ${process.versions.electron} / Chromium ${process.versions.chrome}`, curves: CURVES, waapi: easing, css }, null, 0));
  fs.writeFileSync(path.join(GOLDEN_DIR, "transition.json"), JSON.stringify({ source: `Electron ${process.versions.electron} / Chromium ${process.versions.chrome}`, frameMs: FRAME, reversals }, null, 1));
  console.log(`golden: ${easing.length} easing samples, ${css.length} css samples, ${reversals.length} reversal cases → ${GOLDEN_DIR}`);
  return 0;
}
