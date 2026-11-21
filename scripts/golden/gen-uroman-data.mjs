// Exports UROMAN_DATA (src/vendor/uroman-data.ts) to src-gpui/logic/data/uroman.json,
// which src/uroman.rs loads with include_str!. Same shape, same key order.
//   node scripts/golden/gen-uroman-data.mjs
import { writeFileSync, copyFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { UROMAN_DATA } from "../../src/vendor/uroman-data.ts";

const here = dirname(fileURLToPath(import.meta.url));
const out = join(here, "../../src-gpui/logic/data");
writeFileSync(join(out, "uroman.json"), JSON.stringify(UROMAN_DATA) + "\n");
copyFileSync(join(here, "../../src/vendor/uroman-LICENSE.txt"), join(out, "uroman-LICENSE.txt"));
console.log("src-gpui/logic/data/uroman.json");
