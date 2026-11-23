// Reference images of the UI in fixed states, for the Tauri and GPUI builds to match.
//
//   npm run build                      (dist/ must exist)
//   npm run snapshot [-- outDir] [options]        default outDir: parity/out/electron
//
//   --kind=core|setting|all   which scenes (default all); scenes are in scenes.core.mjs
//                             (hand-written) and scenes.settings.mjs (generated, gen-scenes.mjs)
//   --only=a,b                only scenes/motions with these names
//   --motion=<name|all>       motion capture instead of stills: a frame every 33.3 ms, see below
//   --jobs=N                  parallel Electron processes (default 4)
//   --list                    print the names and exit
//
// Loads dist/ with a fake window.player (preload.cjs): the state in fixtures/fake-state.json, a
// scene's `fake` on top. Each scene gets its own Electron process and in-memory profile, so every
// setting is at its default unless the scene sets it. Offscreen only: windows are never shown,
// input is sent with webContents.sendInputEvent into the offscreen page, never to the real mouse.
//
// ── Determinism: the virtual clock ──
// preload.cjs replaces performance.now, Date, setTimeout/setInterval, requestAnimationFrame and
// Math.random in the page, and seeks every CSS transition/animation and Web Animation by hand
// (pause once, set currentTime = virtual now − virtual start). The clock only moves when this
// runner calls __snap.step(dt), in 1/60 s steps; between batches of steps it waits for real
// painted frames (so ResizeObserver, layout and the resize IPC settle). A still scene advances
// `settle` ms (default 4000), runs its `steps` (wait = virtual time), then captures. Interaction
// ops use real input events so :hover/:active and React's pointer handling behave as for a user.
//
// ── Motion ──
// A motion is a scene plus `timeline: [{ t, op… }]` and `frames`. Frame k is captured at
// t = k × 33.33 ms of virtual time after the trigger (two 1/60 s steps per frame); timeline ops
// with t <= that time run just before the capture (t: 0 ops run before frame 0). Output:
// <outDir>/motion/<name>/f000.png … plus motion.json. `fromLoad` motions start the clock at page
// load (the app mounts with the clock frozen at 0), for window-open animations.
//
// Harness-only CSS (main.cjs, after load): hides the caret and drops `will-change`, see the comment there.
//
// Sidecar <name>.json next to each still: size, and `masks` (boxes of `.lyric-line.current`, the
// sung line, where the diff allows ~20/255 for spring settling, see parity-diff.mjs).
//
// Stability: the runner captures every scene twice (a frame apart) and fails if they differ;
// stability.mjs runs the whole set twice and diffs the runs.

const { app, BrowserWindow, ipcMain } = require("electron");
const fs = require("fs");
const os = require("os");
const path = require("path");

const ROOT = path.join(__dirname, "..", "..");
const args = process.argv.slice(2);
const flag = (n) => args.find((a) => a.startsWith(`--${n}=`))?.slice(n.length + 3);
const OUT = path.resolve(args.find((a) => !a.startsWith("-") && !a.endsWith(".cjs")) ?? flag("out") ?? path.join(ROOT, "parity", "out", "electron"));
const STAGE_PAD = 22; // electron/main.cjs
const FRAME = 1000 / 60;
const WIDTHS = { widget: 350, settings: 600, changelog: 320, "changelog-past": 320, setup: 340, update: 300, versions: 320 };
const DEFAULT_MASKS = [".lyric-line.current"];

async function loadScenes() {
  const core = await import(path.join(__dirname, "scenes.core.mjs").replace(/\\/g, "/").replace(/^/, "file:///"));
  const set = await import(path.join(__dirname, "scenes.settings.mjs").replace(/\\/g, "/").replace(/^/, "file:///"));
  return { scenes: [...core.scenes, ...set.scenes], motions: core.motions.map((m) => ({ kind: "motion", ...m })) };
}

// ── parent: one Electron process per scene (a second offscreen window in a process fails to load) ──
async function parent() {
  const { spawn } = require("child_process");
  const { scenes, motions } = await loadScenes();
  const only = flag("only")?.split(",");
  const kind = flag("kind") ?? "all";
  const motionArg = flag("motion");
  let jobs = [];
  if (motionArg) {
    jobs = motions.filter((m) => motionArg === "all" || motionArg.split(",").includes(m.name)).map((m) => ({ kind: "motion", name: m.name }));
  } else {
    jobs = scenes.filter((s) => kind === "all" || s.kind === kind).map((s) => ({ kind: "scene", name: s.name }));
  }
  if (only) jobs = jobs.filter((j) => only.includes(j.name));
  if (args.includes("--list")) {
    for (const j of jobs) console.log(`${j.kind}:${j.name}`);
    process.exit(0);
  }
  fs.mkdirSync(OUT, { recursive: true });
  const n = Number(flag("jobs") ?? 4);
  let failed = 0;
  let next = 0;
  const run = (j) =>
    new Promise((res) => {
      const p = spawn(process.execPath, [__filename, `--job=${j.kind}:${j.name}`, `--out=${OUT}`], { stdio: "inherit" });
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

const jobArg = flag("job");
if (!jobArg) {
  parent();
} else {
  child(jobArg);
}

function child(job) {
  const userData = fs.mkdtempSync(path.join(os.tmpdir(), "snap-"));
  app.setPath("userData", userData);
  app.disableHardwareAcceleration();
  app.commandLine.appendSwitch("disable-smooth-scrolling");

  app.whenReady().then(async () => {
    const watchdog = setTimeout(() => {
      console.log(`  [${job}] TIMEOUT`);
      app.exit(2);
    }, 240000);
    try {
      const code = await runJob(job);
      clearTimeout(watchdog);
      app.exit(code);
    } catch (e) {
      console.log(`  [${job}] ERROR ${e && e.stack ? e.stack : e}`);
      app.exit(2);
    }
  });
}

async function runJob(job) {
  const [kind, name] = [job.slice(0, job.indexOf(":")), job.slice(job.indexOf(":") + 1)];
  const { scenes, motions } = await loadScenes();
  const scene = (kind === "motion" ? motions : scenes).find((s) => s.name === name);
  if (!scene) throw new Error(`no such ${kind}: ${name}`);
  const { KEYS } = require("./settings-meta.cjs")();

  const storage = { ...(scene.storage ?? {}) };
  for (const [k, v] of Object.entries(scene.settings ?? {})) {
    if (!(k in KEYS)) throw new Error(`unknown setting ${k}`);
    storage[KEYS[k]] = v;
  }
  const win0 = scene.window ?? "widget";
  const width = (scene.width ?? (win0 === "widget" ? (scene.settings?.widgetWidth ?? WIDTHS.widget) : WIDTHS[win0])) + STAGE_PAD * 2;
  const hash = win0 === "widget" ? "" : `#${win0}`;

  const fixtureDir = path.join(ROOT, "fixtures");
  const fake = JSON.parse(fs.readFileSync(path.join(fixtureDir, "fake-state.json"), "utf8"));
  const coverUrl = "data:image/png;base64," + fs.readFileSync(path.join(fixtureDir, fake.cover)).toString("base64");

  const win = new BrowserWindow({
    show: false,
    width,
    height: 420,
    frame: false,
    transparent: true,
    webPreferences: {
      preload: path.join(__dirname, "preload.cjs"),
      partition: "snapshot", // no "persist:" → fresh, in memory
      offscreen: true,
      backgroundThrottling: false,
      contextIsolation: false, // the preload fakes the page's clock and window.player directly
      additionalArguments: [`--snapshot-scene=${encodeURIComponent(JSON.stringify({ storage, fake: scene.fake ?? {} }))}`],
    },
  });
  win.webContents.setFrameRate(60);

  let fsActive = false;
  let lastH = 420;
  ipcMain.on("snap:fixture", (e) => (e.returnValue = { fake }));
  ipcMain.handle("snap:art", () => coverUrl);
  ipcMain.handle("snap:resize", (_e, h) => {
    lastH = Math.max(60, Math.ceil(h));
    if (!fsActive) win.setSize(width, lastH);
  });
  ipcMain.handle("snap:width", (_e, w) => {
    if (!fsActive) win.setSize(Math.round(w) + STAGE_PAD * 2, win.getSize()[1]);
  });
  ipcMain.handle("snap:fullscreen", (_e, on) => {
    fsActive = on;
    if (on) win.setSize(...(scene.size ?? [1280, 720]));
    else win.setSize(width, lastH);
  });
  // electron/main.cjs fullscreenGeometry: the widget's window sits at `fsFrom` (default: top right)
  // of a screen the size of `size`
  ipcMain.handle("snap:fsGeometry", () => {
    const [sw, sh] = scene.size ?? [1280, 720];
    const [x, y] = scene.fsFrom ?? [sw - width - 40, 40];
    return { from: { x, y, width, height: lastH }, area: { x: 0, y: 0, width: sw, height: sh } };
  });
  win.webContents.on("console-message", (e) => {
    if (e.level === "error") console.log(`  [${name}] ${e.message}`);
  });

  const ev = (code) => win.webContents.executeJavaScript(code, true);
  const frames = (n) => ev(`(async()=>{for(let i=0;i<${n};i++) await __snap.realFrame();})()`);
  const advance = (ms) => {
    const n = Math.round(ms / FRAME);
    return ev(`(async()=>{for(let i=0;i<${n};i++){__snap.step(${FRAME});if(i%6===5)await __snap.realFrame();}})()`);
  };
  const real = (ms) => new Promise((r) => setTimeout(r, ms));
  // the clock is virtual, but the page needs real time to be painted; a captured frame is the latest one
  const capture = async () => {
    await frames(2);
    await ev("__snap.step(0)");
    await frames(2);
    await real(60);
    return (await win.webContents.capturePage()).toPNG();
  };
  const locate = (op) =>
    ev(`(()=>{const o=${JSON.stringify(op)};let l=[...document.querySelectorAll(o.selector)];
      if(o.text)l=l.filter(e=>e.textContent.trim()===o.text);
      const e=l[o.nth??0];if(!e)return null;const r=e.getBoundingClientRect();
      return{x:Math.round(r.left+r.width/2),y:Math.round(r.top+r.height/2)};})()`);
  let mouse = { x: 1, y: 1 };
  const send = async (e) => {
    win.webContents.sendInputEvent(e);
    await frames(2);
  };
  async function target(op) {
    if (!op.selector) return mouse;
    const p = await locate(op);
    if (!p) throw new Error(`no element for ${JSON.stringify(op)}`);
    return p;
  }
  async function doOp(op) {
    switch (op.op) {
      case "wait":
        await advance(op.ms);
        break;
      case "move":
        mouse = await target(op);
        await send({ type: "mouseMove", ...mouse });
        break;
      case "moveOut":
        await send({ type: "mouseLeave", x: -1, y: -1 });
        break;
      case "down":
        mouse = await target(op);
        if (op.selector) await send({ type: "mouseMove", ...mouse });
        await send({ type: "mouseDown", ...mouse, button: "left", clickCount: 1 });
        break;
      case "up":
        await send({ type: "mouseUp", ...mouse, button: "left", clickCount: 1 });
        break;
      case "click":
        await doOp({ ...op, op: "move" });
        await send({ type: "mouseDown", ...mouse, button: "left", clickCount: 1 });
        await send({ type: "mouseUp", ...mouse, button: "left", clickCount: 1 });
        break;
      case "wheel":
        if (op.selector) await doOp({ ...op, op: "move" });
        await send({ type: "mouseWheel", ...mouse, deltaX: 0, deltaY: op.dy });
        break;
      case "type":
        await ev(`(()=>{const e=document.querySelector(${JSON.stringify(op.selector)});
          Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,"value").set.call(e,${JSON.stringify(op.text)});
          e.dispatchEvent(new Event("input",{bubbles:true}));})()`);
        await frames(2);
        break;
      case "emit":
        await ev(`__snap.emit(${JSON.stringify(op.event)},${JSON.stringify(op.payload ?? null)})`);
        await frames(2);
        break;
      case "state":
        await ev(`__snap.state(${JSON.stringify(op.patch)})`);
        await frames(2);
        break;
      default:
        throw new Error(`unknown op ${op.op}`);
    }
  }

  // Deliver the first state event (the preload fires it 50 ms in), then let the async work it starts
  // (lyrics, cover and next-song lookups, IPC replies) land in real time while the clock is
  // frozen, so none of it arrives at a run-dependent virtual time.
  const prime = async () => {
    await advance(FRAME * 4);
    await real(500);
    await frames(2);
    await advance(FRAME * 4);
    await real(300);
    await frames(2);
  };

  await win.loadFile(path.join(ROOT, "dist", "index.html"), { hash: hash.slice(1) || undefined });
  // Harness-only CSS. No caret (it blinks on a real-time timer). No will-change: a promoted layer is
  // rasterised once and then only moved, so where the text lands inside it (sub-pixel blur or crisp)
  // depends on which real frames the compositor happened to draw while a transform changed. That made
  // identical runs differ by ~30/255 along every lyric line; without the hint the text is drawn at its
  // final position every time, which is also how a GPUI port draws it.
  await win.webContents.insertCSS("*{caret-color:transparent!important;will-change:auto!important}");
  // every @font-face, used yet or not: a face that arrives after the lyrics are laid out moves their
  // line boxes (and the scroll target computed from them) by a run-dependent amount
  await ev(`Promise.all([...document.fonts].map(f=>f.load().catch(()=>0))).then(()=>document.fonts.ready).then(()=>Promise.all([...document.images].map(i=>i.decode().catch(()=>0))))`);
  await real(500); // IPC replies and decodes land; the clock has not moved

  if (kind === "motion") {
    await ev("__snap.step(0)");
    if (!scene.fromLoad) await prime();
    if (!scene.fromLoad) await advance(scene.settle ?? 4000);
    for (const op of scene.steps ?? []) await doOp(op);
    const dir = path.join(OUT, "motion", name);
    fs.rmSync(dir, { recursive: true, force: true });
    fs.mkdirSync(dir, { recursive: true });
    const timeline = (scene.timeline ?? []).map((o) => ({ ...o, done: false }));
    const frameMs = FRAME * 2;
    const count = scene.frames ?? 30;
    let size = null;
    for (let k = 0; k < count; k++) {
      if (k > 0) await advance(frameMs);
      const t = k * frameMs;
      for (const o of timeline) if (!o.done && o.t <= t + 1e-6) {
        o.done = true;
        await doOp(o);
      }
      const png = await capture();
      size = win.getSize();
      fs.writeFileSync(path.join(dir, `f${String(k).padStart(3, "0")}.png`), png);
    }
    fs.writeFileSync(path.join(dir, "motion.json"), JSON.stringify({ name, frames: count, frameMs, size }, null, 1));
    console.log(`motion ${name}: ${count} frames ${size.join("x")} → ${dir}`);
    return 0;
  }

  if (!scene.fromLoad) {
    await prime();
    await advance(scene.settle ?? 4000);
  }
  for (const op of scene.steps ?? []) await doOp(op);
  const a = await capture();
  await frames(1);
  await real(60);
  const b = (await win.webContents.capturePage()).toPNG();
  const stable = a.equals(b);
  const masks = await ev(`(()=>{const out=[];for(const s of ${JSON.stringify(scene.maskSelectors ?? DEFAULT_MASKS)})
    for(const e of document.querySelectorAll(s)){const r=e.getBoundingClientRect();
      out.push({x:Math.max(0,Math.floor(r.left)-4),y:Math.max(0,Math.floor(r.top)-4),w:Math.ceil(r.width)+8,h:Math.ceil(r.height)+8,tol:20});}
    return out;})()`);
  const probes = {};
  for (const s of scene.probes ?? process.env.SNAP_PROBES?.split("|") ?? []) probes[s] = await ev(`[...document.querySelectorAll(${JSON.stringify(s)})].map(e=>{const r=e.getBoundingClientRect();return[r.x,r.y,r.width,r.height]})`);
  const size = win.getSize();
  fs.writeFileSync(path.join(OUT, `${name}.png`), b);
  fs.writeFileSync(path.join(OUT, `${name}.json`), JSON.stringify({ name, size, masks, probes }, null, 1));
  console.log(`${name}: ${size.join("x")} ${stable ? "stable" : "NOT STABLE"}`);
  return stable ? 0 : 1;
}
