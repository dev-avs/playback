// golden/spicy.json: src/lyrics/spicy.ts (loadSpicyLyrics, through a fake window.player) over the real payloads and the edge payloads.
import { register } from "node:module";
import type { Fixtures } from "../run.mts";

// spicy.ts imports "./lyrics" and "../vendor/objpack" without an extension, which Node's loader does not resolve
register(
  "data:text/javascript," +
    encodeURIComponent(`export async function resolve(s, c, next) {
  try { return await next(s, c); } catch (e) {
    if (s.startsWith(".") && !/\.[mc]?[jt]s$/.test(s)) return next(s + ".ts", c);
    throw e;
  }
}`),
  import.meta.url,
);

const { loadSpicyLyrics } = await import("../../../src/lyrics/spicy.ts");
const { SLObjPack } = await import("../../../src/vendor/objpack.ts");

const packer = new SLObjPack();
const req = { artist: "a", title: "t" };

async function run(res: unknown) {
  (globalThis as any).window = { player: { fetchSpicyLyrics: async () => res } };
  const quiet = console.error;
  console.error = () => {};
  try {
    return await loadSpicyLyrics(req);
  } finally {
    console.error = quiet;
  }
}

export default async (fx: Fixtures) => {
  const files = [...fx.list("spicy"), ...fx.list("spicy-edge")].filter((f) => f.endsWith(".json"));
  const payloads = files.map((f) => ({ file: f, data: fx.json(f) }));
  const out: any = { payloads: [], sources: [] };
  for (const { file, data } of payloads) {
    out.payloads.push({
      file,
      lyrics: await run({ status: "ok", lyrics: data }),
      packed: await run({ status: "ok", packed: packer.pack(data) }),
    });
  }
  // what the player bridge can answer besides a payload
  const first = payloads[0].data;
  const sources: [string, unknown][] = [
    ["null", null],
    ["no-auth", { status: "no-auth" }],
    ["error", { status: "error", lyrics: first }],
    ["ok-empty", { status: "ok" }],
    ["lyrics-wins-over-packed", { status: "ok", lyrics: first, packed: "junk" }],
    ["lyrics-null-packed", { status: "ok", lyrics: null, packed: packer.pack(first) }],
    ["packed-garbage", { status: "ok", packed: "junk" }],
    ["packed-bad-stream", { status: "ok", packed: [["a"], [9]] }],
    ["lyrics-empty-object", { status: "ok", lyrics: {} }],
    ["lyrics-empty-content", { status: "ok", lyrics: { Type: "Syllable", Content: [] } }],
    ["lyrics-content-not-array", { status: "ok", lyrics: { Type: "Line", Content: "x" } }],
    ["lyrics-zero", { status: "ok", lyrics: 0 }],
    ["lyrics-all-blank", { status: "ok", lyrics: { Type: "Line", Content: [{ Text: " " }, {}] } }],
  ];
  for (const [name, res] of sources) out.sources.push({ name, result: await run(res) });
  // the bridge missing, or without fetchSpicyLyrics
  (globalThis as any).window = {};
  const noWindow = await loadSpicyLyrics(req);
  (globalThis as any).window = { player: {} };
  const noFn = await loadSpicyLyrics(req);
  out.noBridge = { noWindow, noFn };
  return out;
};
