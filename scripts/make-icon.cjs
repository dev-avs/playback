// Rasterise public/logo.svg into build/icon.ico.
//
//   npm run icon
//
// Windows exe icons must be .ico, and nothing in the toolchain rasterises SVG.
// Electron is already a dependency and ships a full Chromium, so we use it as
// the renderer: draw the SVG into a canvas at each size and read the PNG back.
// That is deterministic, unlike capturePage() on a hidden window, which can
// come back blank on Windows.
//
// The ICO container is written by hand — since Vista it may hold PNG-compressed
// entries verbatim, so it is a 6-byte header, a 16-byte directory entry per
// image, and then the PNG bytes.

const { app, BrowserWindow } = require("electron");
const fs = require("fs");
const path = require("path");

const ROOT = path.join(__dirname, "..");
const SVG = path.join(ROOT, "public", "logo.svg");
const OUT_DIR = path.join(ROOT, "build");
const OUT = path.join(OUT_DIR, "icon.ico");

// Windows picks the closest of these per context (taskbar, alt-tab, explorer).
const SIZES = [16, 24, 32, 48, 64, 128, 256];

function buildIco(images) {
  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0); // reserved
  header.writeUInt16LE(1, 2); // type: 1 = icon
  header.writeUInt16LE(images.length, 4);

  const directory = Buffer.alloc(16 * images.length);
  // Image data begins after the header and the whole directory.
  let offset = header.length + directory.length;

  for (let i = 0; i < images.length; i++) {
    const { size, png } = images[i];
    const at = i * 16;
    // 0 means 256 — the field is a single byte.
    directory.writeUInt8(size >= 256 ? 0 : size, at + 0);
    directory.writeUInt8(size >= 256 ? 0 : size, at + 1);
    directory.writeUInt8(0, at + 2); // palette size (0 = truecolour)
    directory.writeUInt8(0, at + 3); // reserved
    directory.writeUInt16LE(1, at + 4); // colour planes
    directory.writeUInt16LE(32, at + 6); // bits per pixel
    directory.writeUInt32LE(png.length, at + 8);
    directory.writeUInt32LE(offset, at + 12);
    offset += png.length;
  }

  return Buffer.concat([header, directory, ...images.map((i) => i.png)]);
}

async function render(win, svgDataUrl, size) {
  const dataUrl = await win.webContents.executeJavaScript(`
    new Promise((resolve, reject) => {
      const img = new Image();
      img.onload = () => {
        const c = document.createElement("canvas");
        c.width = ${size};
        c.height = ${size};
        const ctx = c.getContext("2d");
        ctx.clearRect(0, 0, ${size}, ${size});
        ctx.drawImage(img, 0, 0, ${size}, ${size});
        resolve(c.toDataURL("image/png"));
      };
      img.onerror = () => reject(new Error("could not decode logo.svg"));
      img.src = ${JSON.stringify(svgDataUrl)};
    })
  `);
  return Buffer.from(dataUrl.slice(dataUrl.indexOf(",") + 1), "base64");
}

app.whenReady().then(async () => {
  let code = 0;
  try {
    const svg = fs.readFileSync(SVG, "utf8");
    const svgDataUrl =
      "data:image/svg+xml;base64," + Buffer.from(svg, "utf8").toString("base64");

    const win = new BrowserWindow({ show: false, width: 320, height: 320 });
    await win.loadURL("data:text/html,<!doctype html><meta charset=utf-8><body>");

    const images = [];
    for (const size of SIZES) {
      images.push({ size, png: await render(win, svgDataUrl, size) });
      console.log(`  rendered ${size}x${size}`);
    }

    fs.mkdirSync(OUT_DIR, { recursive: true });
    fs.writeFileSync(OUT, buildIco(images));
    console.log(`wrote ${path.relative(ROOT, OUT)} (${SIZES.length} sizes)`);
  } catch (e) {
    console.error("icon generation failed:", e.message);
    code = 1;
  }
  app.exit(code);
});
