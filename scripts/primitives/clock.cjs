// Virtual clock for the primitive pages (scripts/primitives/main.cjs). The same idea as
// scripts/snapshot/preload.cjs, cut down to what a page without scripts needs: nothing moves
// until the runner calls __prim.step(ms); then every CSS transition / animation / Web Animation
// is seeked by hand. Each one is paused the first time it is seen and its currentTime set to
// (virtual now − virtual time it was first seen); a finished one is finish()ed.
// performance.now and requestAnimationFrame are virtual too, so a page script sees the same clock.

const realRaf = window.requestAnimationFrame.bind(window);
let vnow = 0;
let rafQueue = [];
let nextId = 1;
Object.defineProperty(performance, "now", { configurable: true, value: () => vnow });
window.requestAnimationFrame = (cb) => {
  const id = nextId++;
  rafQueue.push({ id, cb });
  return id;
};
window.cancelAnimationFrame = (id) => {
  rafQueue = rafQueue.filter((r) => r.id !== id);
};

const animStart = new WeakMap();
function sweep() {
  for (const a of document.getAnimations()) {
    if (a.playState === "finished") continue;
    let s = animStart.get(a);
    if (s === undefined) {
      s = vnow;
      animStart.set(a, s);
      a.pause();
    }
    const t = vnow - s;
    const end = a.effect?.getComputedTiming?.().endTime;
    if (typeof end === "number" && Number.isFinite(end) && t >= end) a.finish();
    else a.currentTime = t;
  }
}

window.__prim = {
  now: () => vnow,
  step(dt) {
    vnow += dt;
    const q = rafQueue;
    rafQueue = [];
    for (const r of q) r.cb(vnow);
    sweep();
  },
  /** the real rAF, for the runner to wait for painted frames */
  realFrame: () => new Promise((r) => realRaf(() => r())),
};
