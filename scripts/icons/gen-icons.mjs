import fs from "fs";
import path from "path";

function bbox(d) {
  const toks = d.match(/[MLHVCZmlhvcz]|-?\d*\.?\d+/g) ?? [];
  let i = 0, x = 0, y = 0, sx = 0, sy = 0, cmd = "";
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
  const hit = (px, py) => { if (px < minX) minX = px; if (px > maxX) maxX = px; if (py < minY) minY = py; if (py > maxY) maxY = py; };
  const num = () => parseFloat(toks[i++]);
  const cubic = (x0,y0,x1,y1,x2,y2,x3,y3) => {
    for (let t = 0; t <= 1.0001; t += 1/60) {
      const u = 1 - t;
      hit(u*u*u*x0 + 3*u*u*t*x1 + 3*u*t*t*x2 + t*t*t*x3,
          u*u*u*y0 + 3*u*u*t*y1 + 3*u*t*t*y2 + t*t*t*y3);
    }
  };
  while (i < toks.length) {
    if (/[A-Za-z]/.test(toks[i])) cmd = toks[i++];
    if (cmd === "M") { x = num(); y = num(); sx = x; sy = y; hit(x, y); cmd = "L"; }
    else if (cmd === "L") { x = num(); y = num(); hit(x, y); }
    else if (cmd === "H") { x = num(); hit(x, y); }
    else if (cmd === "V") { y = num(); hit(x, y); }
    else if (cmd === "C") { const a=num(),b=num(),c=num(),dd=num(),e=num(),f=num(); cubic(x,y,a,b,c,dd,e,f); x=e; y=f; }
    else if (cmd === "Z" || cmd === "z") { x = sx; y = sy; }
    else { i++; }
  }
  return { minX, minY, maxX, maxY, w: maxX - minX, h: maxY - minY };
}

const TARGET_W = 17, TARGET_H = 14;
const want = ["volume", "previous", "next", "pause", "play", "shuffle"];
const out = {};
for (const name of want) {
  const svg = fs.readFileSync(path.join(import.meta.dirname, name + ".svg"), "utf8");
  const d = svg.match(/ d="([^"]+)"/)[1];
  const b = bbox(d);
  const cx = (b.minX + b.maxX) / 2, cvy = (-b.maxY + -b.minY) / 2;
  const s = Math.min(TARGET_W / b.w, TARGET_H / b.h);
  const r = (n) => Number(n.toFixed(4));
  out[name] = {
    d,
    matrix: `matrix(${r(s)} 0 0 ${r(-s)} ${r(12 - s * cx)} ${r(12 - s * cvy)})`,
    size: `${(b.w * s).toFixed(1)} x ${(b.h * s).toFixed(1)}`,
  };
  console.log(`${name.padEnd(10)} -> ${out[name].size} in a 24 box`);
}
fs.writeFileSync(path.join(import.meta.dirname, "gen-icons.json"), JSON.stringify(out, null, 2));
console.log("\nwrote scripts/icons/gen-icons.json");
