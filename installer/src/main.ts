// The installer's page: welcome → folder and options → progress → done (or
// error). The Rust side (src-tauri/src/main.rs) does the work; this drives the
// pages and their motion.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open as pickFolder } from "@tauri-apps/plugin-dialog";
// Inter with its optical-size axis: the fallback for anyone without SF Pro
import "@fontsource-variable/inter/opsz.css";

type FlavorId = "tauri" | "electron" | "gpui";

interface FlavorRelease {
  id: FlavorId;
  stub: string;
  package: string;
  size: number;
}
interface Release {
  version: string;
  flavors: FlavorRelease[];
}
interface Installed {
  dir: string;
  version: string;
}

const FLAVOR_LABELS: Record<FlavorId, string> = {
  tauri: "Tauri",
  electron: "Electron",
  gpui: "GPUI",
};

type Page = "welcome" | "flavor" | "options" | "progress" | "done" | "error";
const ORDER: Page[] = ["welcome", "flavor", "options", "progress", "done", "error"];

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const win = getCurrentWindow();
const card = $("card");
const pages = document.querySelector<HTMLElement>(".pages")!;

let page: Page = "welcome";
let release: Release | null = null;
let selectedFlavor: FlavorId | null = null;
let installed: Installed | null = null;
let dir = "";
let busy = false;

const mb = (bytes: number) => (bytes / 1048576).toFixed(1);

// ── pages ──

function show(next: Page) {
  const back = ORDER.indexOf(next) < ORDER.indexOf(page);
  pages.classList.toggle("back", back);
  for (const el of pages.querySelectorAll<HTMLElement>(".page")) {
    const name = el.dataset.page as Page;
    el.classList.toggle("in", name === next);
    el.classList.toggle("out", name === page && name !== next);
  }
  card.dataset.page = next;
  page = next;
}

// Hold to cancel, like Uninstall in player's Settings.
let holdTimer = 0;
function holdStart() {
  const btn = $<HTMLButtonElement>("cancel");
  if (btn.disabled) return;
  btn.classList.add("holding");
  $("cancel-label").textContent = "Keep holding…";
  holdTimer = window.setTimeout(() => {
    holdEnd();
    $("cancel-label").textContent = "Cancelling…";
    btn.disabled = true;
    invoke("cancel");
  }, 1200);
}
function holdEnd() {
  clearTimeout(holdTimer);
  const btn = $("cancel");
  btn.classList.remove("holding");
  if (!$<HTMLButtonElement>("cancel").disabled) $("cancel-label").textContent = "Hold to cancel";
}

// GPUI is experimental: picking it opens a full-window warning. The button
// unlocks after 5 s of reading, then has to be held for 5 s.
const GATE_WAIT_MS = 5000;
const GATE_HOLD_MS = 5000;
let gpuiConfirmed = false;
let gateFrame = 0;
let gateHeldSince = 0;
const secs = (ms: number) => (Math.max(0, ms) / 1000).toFixed(2);

function openGate() {
  const btn = $<HTMLButtonElement>("gpui-hold");
  btn.disabled = true;
  gateHeldSince = 0;
  $("gpui-hold-fill").style.setProperty("--p", "0");
  $("gpui-gate").hidden = false;
  const opened = performance.now();
  const tick = () => {
    const left = GATE_WAIT_MS - (performance.now() - opened);
    if (left > 0) {
      $("gpui-hold-label").textContent = `Read this first · ${secs(left)}`;
      gateFrame = requestAnimationFrame(tick);
    } else {
      btn.disabled = false;
      $("gpui-hold-label").textContent = "Install GPUI";
    }
  };
  cancelAnimationFrame(gateFrame);
  tick();
}

function closeGate() {
  cancelAnimationFrame(gateFrame);
  gateHeldSince = 0;
  $("gpui-gate").hidden = true;
}

function gpuiHoldStart() {
  if ($<HTMLButtonElement>("gpui-hold").disabled) return;
  gateHeldSince = performance.now();
  const tick = () => {
    if (!gateHeldSince) return;
    const held = performance.now() - gateHeldSince;
    $("gpui-hold-fill").style.setProperty("--p", String(Math.min(1, held / GATE_HOLD_MS)));
    if (held < GATE_HOLD_MS) {
      $("gpui-hold-label").textContent = secs(GATE_HOLD_MS - held);
      gateFrame = requestAnimationFrame(tick);
    } else {
      gpuiConfirmed = true;
      closeGate();
      selectFlavor("gpui");
      show("options");
    }
  };
  tick();
}

function gpuiHoldEnd() {
  if (!gateHeldSince) return;
  gateHeldSince = 0;
  cancelAnimationFrame(gateFrame);
  $("gpui-hold-fill").style.setProperty("--p", "0");
  $("gpui-hold-label").textContent = "Install GPUI";
}

function fail(message: string) {
  busy = false;
  card.classList.remove("installing");
  $("error-text").textContent = message;
  show("error");
}

async function close() {
  if (busy) return;
  card.classList.add("closing");
  setTimeout(() => win.close(), 230);
}

// ── welcome ──

// "Playback", one letter at a time
$("hello").innerHTML = [..."Playback"].map((c, i) => `<span style="--l: ${i}">${c}</span>`).join("");

function currentFlavor(): FlavorRelease | null {
  if (!release || !selectedFlavor) return null;
  return release.flavors.find((f) => f.id === selectedFlavor) ?? null;
}

function selectFlavor(id: FlavorId) {
  if (!release?.flavors.some((f) => f.id === id)) return;
  selectedFlavor = id;
  for (const el of document.querySelectorAll<HTMLButtonElement>(".flavor-card")) {
    const active = el.dataset.flavor === id;
    el.classList.toggle("selected", active);
    el.setAttribute("aria-checked", String(active));
  }
  ($("flavor-next") as HTMLButtonElement).disabled = false;
  const f = currentFlavor();
  if (f && release) {
    $("progress-title").textContent = `Installing Playback ${release.version} · ${FLAVOR_LABELS[f.id]}`;
  }
}

function syncFlavorCards() {
  const available = new Map<string, FlavorRelease>((release?.flavors ?? []).map((f) => [f.id, f]));
  for (const el of document.querySelectorAll<HTMLButtonElement>(".flavor-card")) {
    const id = el.dataset.flavor as FlavorId;
    const item = available.get(id);
    const sizeEl = el.querySelector<HTMLElement>(`[data-size="${id}"]`);
    el.classList.toggle("unavailable", !item);
    el.disabled = !item;
    if (sizeEl) {
      sizeEl.textContent = item ? `${mb(item.size)} MB` : "Not in this release";
    }
  }
  const preferred: FlavorId[] = ["tauri", "electron", "gpui"];
  const first = preferred.find((id) => available.has(id)) ?? null;
  if (first) {
    selectFlavor(first);
  } else {
    selectedFlavor = null;
    ($("flavor-next") as HTMLButtonElement).disabled = true;
  }
}

async function lookUp() {
  const caption = $("release");
  caption.innerHTML = '<span class="skeleton">Looking up the newest version…</span>';
  ($("start") as HTMLButtonElement).disabled = true;
  try {
    release = await invoke<Release>("latest");
  } catch (e) {
    fail(String(e));
    return;
  }
  syncFlavorCards();
  const v = release.version;
  const count = release.flavors.length;
  const flavorNote =
    count === 1
      ? `${FLAVOR_LABELS[release.flavors[0].id]} · ${mb(release.flavors[0].size)} MB`
      : `${count} flavors available`;
  $("start-label").textContent = !installed
    ? "Install"
    : installed.version === v
      ? "Reinstall Playback"
      : `Update to ${v}`;
  caption.textContent = installed
    ? `You have ${installed.version} · newest is ${v} · ${flavorNote}`
    : `Playback ${v} · ${flavorNote}`;
  ($("start") as HTMLButtonElement).disabled = false;
}

// ── folder and options ──

function setDir(next: string) {
  dir = next;
  const text = $("dir");
  text.textContent = next;
  text.title = next;
  text.classList.remove("changed");
  void text.offsetWidth; // restart the flash
  text.classList.add("changed");
}

function dirError(message: string) {
  const el = $("dir-error");
  el.textContent = message;
  el.classList.remove("shake");
  void el.offsetWidth;
  if (message) el.classList.add("shake");
}

async function browse() {
  const picked = await pickFolder({ directory: true, defaultPath: dir, title: "Install Playback to" });
  if (typeof picked !== "string") return;
  try {
    setDir(await invoke<string>("check_dir", { dir: picked }));
    dirError("");
  } catch (e) {
    dirError(String(e));
  }
}

function toggle(id: string) {
  const el = $(id);
  const on = !el.classList.contains("on");
  el.classList.toggle("on", on);
  el.setAttribute("aria-checked", String(on));
}

const isOn = (id: string) => $(id).classList.contains("on");

// ── install ──

// The shown percent eases toward the real one, so the number glides.
let target = 0;
let shown = 0;
let creeping = false;
function tick() {
  if (creeping) target += (99 - target) * 0.006;
  shown += (target - shown) * 0.12;
  if (Math.abs(target - shown) < 0.05) shown = target;
  $("percent").textContent = String(Math.floor(shown));
  $("fill").style.width = `${shown}%`;
  if (busy || shown !== target) requestAnimationFrame(tick);
}

async function install() {
  const flavor = currentFlavor();
  if (!release || !flavor) return;
  try {
    dir = await invoke<string>("check_dir", { dir });
  } catch (e) {
    dirError(String(e));
    return;
  }
  dirError("");
  busy = true;
  target = shown = 0;
  $<HTMLButtonElement>("cancel").disabled = false;
  $("cancel-label").textContent = "Hold to cancel";
  $("track").classList.remove("busy");
  $("meter-text").textContent = "Starting the download…";
  card.classList.add("installing");
  show("progress");
  requestAnimationFrame(tick);

  // download speed, smoothed
  let lastAt = performance.now();
  let lastDone = 0;
  let speed = 0;
  const unProgress = await listen<{ done: number; total: number }>("progress", ({ payload }) => {
    const now = performance.now();
    const dt = (now - lastAt) / 1000;
    if (dt > 0.2) {
      const s = (payload.done - lastDone) / dt;
      speed = speed ? speed * 0.7 + s * 0.3 : s;
      lastAt = now;
      lastDone = payload.done;
    }
    // the download is the first 90%; putting it in place is the rest
    target = payload.total ? (payload.done / payload.total) * 90 : 0;
    $("meter-text").textContent =
      `Downloading · ${mb(payload.done)} of ${mb(payload.total)} MB` + (speed ? ` · ${mb(speed)} MB/s` : "");
  });
  const unPhase = await listen<string>("phase", () => {
    target = 90;
    creeping = true;
    // the stub is running: stopping it now could leave half an install
    $<HTMLButtonElement>("cancel").disabled = true;
    $("track").classList.add("busy");
    $("meter-text").textContent = "Putting it in place…";
  });

  try {
    await invoke("install", {
      version: release.version,
      flavor,
      dir,
      desktop: isOn("desktop"),
      bridge: isOn("bridge"),
    });
  } catch (e) {
    unProgress();
    unPhase();
    creeping = false;
    if (String(e) === "cancelled") {
      // back where they were, nothing installed
      busy = false;
      card.classList.remove("installing");
      dirError("");
      show("options");
      return;
    }
    fail(String(e));
    return;
  }
  unProgress();
  unPhase();
  creeping = false;
  target = 100;
  busy = false;
  card.classList.remove("installing");
  $("done-text").textContent = isOn("desktop")
    ? "Find it in the Start menu and on your desktop."
    : "Find it in the Start menu.";
  setTimeout(() => show("done"), 500);
}

// The welcome screen's little widget, singing along word by word.
const LYRICS = [
  "Lyrics that float above everything",
  "Every word lit up as it's sung",
  "From Spotify, your browser, anything",
  "Right where you can see them",
];
function sing(line = 0) {
  const el = $("demo-line");
  const words = LYRICS[line].split(" ");
  el.innerHTML = words.map((w) => `<span>${w}</span>`).join(" ");
  el.classList.remove("enter");
  void el.offsetWidth;
  el.classList.add("enter");
  $("demo-next").textContent = LYRICS[(line + 1) % LYRICS.length];
  const spans = [...el.querySelectorAll("span")];
  spans.forEach((s, i) => setTimeout(() => s.classList.add("sung"), 500 + i * 330));
  setTimeout(() => sing((line + 1) % LYRICS.length), 500 + spans.length * 330 + 1100);
}

// confetti for the finish, thrown in every direction
function makeBurst() {
  const colours = ["#e0408f", "#3f6bff", "#8a5cff", "#3ddc97", "#ffffff", "#ffd36b"];
  $("burst").innerHTML = Array.from({ length: 18 }, (_, i) => {
    const angle = (i / 18) * Math.PI * 2 + Math.random() * 0.3;
    const dist = 60 + Math.random() * 50;
    const x = Math.cos(angle) * dist;
    const y = Math.sin(angle) * dist;
    return `<i style="--x: ${x.toFixed(0)}px; --y: ${y.toFixed(0)}px; --c: ${colours[i % colours.length]}"></i>`;
  }).join("");
}

// ── wiring ──

$("minimize").addEventListener("click", () => win.minimize());
$("close").addEventListener("click", close);
$("start").addEventListener("click", () => show("flavor"));
$("flavor-back").addEventListener("click", () => show("welcome"));
$("flavor-next").addEventListener("click", () => show("options"));
for (const el of document.querySelectorAll<HTMLButtonElement>(".flavor-card")) {
  el.addEventListener("click", () => {
    const id = el.dataset.flavor as FlavorId;
    if (id === "gpui" && !gpuiConfirmed) openGate();
    else selectFlavor(id);
  });
}
$("gpui-back").addEventListener("click", closeGate);
$("gpui-hold").addEventListener("pointerdown", gpuiHoldStart);
for (const ev of ["pointerup", "pointerleave", "pointercancel"]) $("gpui-hold").addEventListener(ev, gpuiHoldEnd);
$("back").addEventListener("click", () => show("flavor"));
$("browse").addEventListener("click", browse);
$("bridge").addEventListener("click", () => toggle("bridge"));
$("desktop").addEventListener("click", () => toggle("desktop"));
$("install").addEventListener("click", install);
$("cancel").addEventListener("pointerdown", holdStart);
for (const ev of ["pointerup", "pointerleave", "pointercancel"]) $("cancel").addEventListener(ev, holdEnd);
$("finish").addEventListener("click", close);
$("quit").addEventListener("click", close);
$("retry").addEventListener("click", () => (release ? show("options") : (show("welcome"), lookUp())));
$("open").addEventListener("click", async () => {
  try {
    await invoke("launch", { dir });
  } catch (e) {
    fail(String(e));
    return;
  }
  close();
});
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") close();
});

// the colour behind the glass leans toward the pointer
card.addEventListener("pointermove", (e) => {
  const r = card.getBoundingClientRect();
  card.style.setProperty("--mx", ((e.clientX - r.left) / r.width - 0.5).toFixed(3));
  card.style.setProperty("--my", ((e.clientY - r.top) / r.height - 0.5).toFixed(3));
});

// ── start ──

(async () => {
  makeBurst();
  sing();
  const [suggested, existing] = await invoke<[string, Installed | null]>("start");
  installed = existing;
  setDir(suggested);
  $("dir").classList.remove("changed");
  await win.show();
  requestAnimationFrame(() => {
    card.classList.add("open");
    show("welcome");
  });
  lookUp();
})();
