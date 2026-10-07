import fs from "fs";
const g = JSON.parse(fs.readFileSync("gen-icons.json", "utf8"));
let src = fs.readFileSync("src/App.tsx", "utf8");

const HEADER = `/**
 * Transport and volume glyphs, from the SF Symbols exports in icons/.
 *
 * Those arrive with a per-file viewBox many hundreds of units across and a
 * \`scale(1, -1)\` flip, and no two share a scale — dropping them in as-is would
 * render them at wildly different sizes. Each path is therefore left byte for
 * byte as exported and placed by a matrix that does the flip and normalises it:
 * measured tight bounding box, fitted into a common optical box (17 wide by 14
 * tall) preserving aspect, then centred on 12,12 of the 24-unit viewBox the rest
 * of the icons here already use.
 *
 * Height is what is held constant, so widths differ by glyph the way they
 * should — the skip icons are broader than play because they carry a bar as
 * well as a triangle. Regenerate with \`node gen-icons.mjs\` if icons/ changes.
 */
`;

const comp = (name, size, key) =>
  `const ${name} = () => (\n` +
  `  <svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="currentColor">\n` +
  `    <path transform="${g[key].matrix}" d="${g[key].d}" />\n` +
  `  </svg>\n` +
  `);\n`;

// Replace the SOFT block + the four transport icons in one go.
const start = src.indexOf("/**\n * The transport triangles are drawn");
const end = src.indexOf("const IconRepeat");
if (start < 0 || end < 0) throw new Error("transport icon block not found");
src = src.slice(0, start) + HEADER +
  comp("IconPrev", 15, "previous") +
  comp("IconPause", 15, "pause") +
  comp("IconPlay", 15, "play") +
  comp("IconNext", 15, "next") +
  src.slice(end);

// Volume
const vStart = src.indexOf("const IconVolume = () => (");
const vEnd = src.indexOf("const IconLyrics");
if (vStart < 0 || vEnd < 0) throw new Error("volume icon not found");
src = src.slice(0, vStart) + comp("IconVolume", 14, "volume") + src.slice(vEnd);

fs.writeFileSync("src/App.tsx", src);
console.log("rewrote 5 icon components");
