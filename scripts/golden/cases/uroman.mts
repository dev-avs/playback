// golden/uroman.json: src/lyrics/uroman.ts over fixtures/uroman (lines.txt, parts.json) and the edge scripts.
import type { Fixtures } from "../run.mts";
import { hasUromanScript, Lattice, prepare, uromanize, uromanizeParts } from "../../../src/lyrics/uroman.ts";

export default (fx: Fixtures) => {
  const lines = [
    ...fx.text("edge/scripts.txt").split(/\r?\n/).filter(Boolean),
    ...fx.text("uroman/lines.txt").split(/\r?\n/).filter(Boolean),
    "",
    " ",
  ];
  const parts: string[][] = [...lines.filter((s) => s.includes(" ")).map((s) => s.split(" ")), ...fx.json("uroman/parts.json")];
  return {
    uromanize: lines.map((s) => ({ in: s, has: hasUromanScript(s), out: uromanize(s) })),
    prepare: lines.map((s) => ({ in: s, out: prepare(s) })),
    edges: lines.map((s) => ({ in: s, out: new Lattice(prepare(s)).romanize() })),
    uromanizeParts: parts.map((p) => ({ in: p, out: uromanizeParts(p) })),
  };
};
