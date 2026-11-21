// Builds parity/ledger.md: everything the GPUI build has to reproduce (port.md §3).
//
//   node scripts/ledger/build.mjs
//
// Deterministic: the same sources give the same bytes. Row sources:
//   - CSS         src/**/*.css via postcss: one row per declaration (selector + property)
//   - TSX         src/**/*.tsx via the TypeScript compiler API: conditional render, class/style/
//                 attribute expressions, early returns, handlers, listeners, timers, settings reads
//   - Settings    src/settings.ts DEFAULTS/KEYS and the `items` list of src/windows/Settings.tsx
//   - Window      electron/main.cjs parsed (BrowserWindow options, levels, events, IPC, shortcuts)
//                 plus the curated scripts/ledger/hand-rows.json (anchors resolved to file:line)
//
// Row ids are stable: built from file + a hash of the selector / condition text + property, never a
// line number. status / gpui / evidence of an existing row survive a rebuild (matched by id).

import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
import { fileURLToPath } from "node:url";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
const postcss = require(path.join(ROOT, "node_modules", "postcss"));
const ts = require(path.join(ROOT, "node_modules", "typescript"));

const read = (rel) => fs.readFileSync(path.join(ROOT, rel), "utf8").replace(/\r\n/g, "\n");
const h6 = (s) => crypto.createHash("sha1").update(s).digest("hex").slice(0, 6);
const norm = (s) => s.replace(/\s+/g, " ").trim();
const trunc = (s, n = 200) => (s.length > n ? s.slice(0, n - 1) + "…" : s);

// ── areas, in port.md §5 order ───────────────────────────────────────────────
const AREAS = [
  ["widget", "Widget (src/App.tsx, src/App.css, src/index.css)"],
  ["settings", "Settings window (src/windows/Settings.*)"],
  ["changelog", "Changelog + past changelog (src/windows/Changelog.*; its CSS is shared by Setup, Update, Versions)"],
  ["setup", "Setup window (src/windows/Setup.*)"],
  ["update", "Update prompt (src/windows/Update.tsx)"],
  ["versions", "Version control (src/windows/Versions.tsx)"],
  ["shell", "Window shell (electron/main.cjs, src/main.tsx)"],
];
const AREA_IDX = Object.fromEntries(AREAS.map(([a], i) => [a, i]));

// ── scenes ───────────────────────────────────────────────────────────────────
// Names from the harness (scripts/snapshot/scenes.*) when it exists; otherwise inferred from the
// area and the row text. A name the harness does not have yet is kept with a trailing "?".
function loadScenes() {
  const dir = path.join(ROOT, "scripts", "snapshot");
  const names = new Set();
  if (!fs.existsSync(dir)) return names;
  for (const f of fs.readdirSync(dir).sort()) {
    if (!/^scenes\./.test(f)) continue;
    const t = fs.readFileSync(path.join(dir, f), "utf8");
    for (const m of t.matchAll(/\bname\s*:\s*["'`]([^"'`]+)["'`]/g)) names.add(m[1]);
  }
  return names;
}
const KNOWN_SCENES = loadScenes();
function scene(names) {
  const out = [];
  for (const n of names) {
    let r = n;
    if (KNOWN_SCENES.size && !KNOWN_SCENES.has(n)) {
      const base = n.split("-")[0];
      r = KNOWN_SCENES.has(base) ? base : n + "?";
    }
    if (!out.includes(r)) out.push(r);
  }
  return out.join(", ");
}
function scenesFor(area, text) {
  const t = text.toLowerCase();
  const s = [];
  if (area === "widget") {
    s.push("widget");
    if (/hover|visible|\bvol\b|vol-|ctrl-|scrub|seek|progress/.test(t)) s.push("widget-hover");
    if (/lyric|\.line|\.word|sweep|pan\b|gap|credit|backing|roma|karaoke|syllable|letter/.test(t)) s.push("widget-lyrics");
    if (/fullscreen|\.fs|fs[a-z]/.test(t)) s.push("widget-fullscreen");
    if (/album|art-mini|art-compact|\.art\b|art-/.test(t)) s.push("widget-album");
    if (/next/.test(t)) s.push("widget-next-label", "widget-next-popup");
  } else if (area === "settings") {
    s.push("settings");
    if (/search/.test(t)) s.push("settings-search");
    if (/reset|hold|armed|drain/.test(t)) s.push("settings-reset");
    if (/token|spicykey|spicy-key|\bkey\b/.test(t)) s.push("settings-token");
  } else if (area === "changelog") {
    s.push(/past/.test(t) ? "changelog-past" : "changelog");
  } else if (area === "setup") s.push("setup");
  else if (area === "update") s.push("update");
  else if (area === "versions") s.push("versions");
  else s.push("window-behaviour");
  return scene(s);
}

// ── rows ─────────────────────────────────────────────────────────────────────
const rows = [];
const usedIds = new Map();
function add(r) {
  let id = r.id;
  const n = (usedIds.get(id) ?? 0) + 1;
  usedIds.set(id, n);
  if (n > 1) id += "~" + n;
  rows.push({ ...r, id, group: r.group ?? r.file, scenes: r.scenes ?? scenesFor(r.area, r.what + " " + (r.hint ?? "")) });
}

// ── CSS ──────────────────────────────────────────────────────────────────────
const CSS_FILES = [
  ["src/App.css", "widget", "App"],
  ["src/index.css", "widget", "index"],
  ["src/windows/Settings.css", "settings", "Settings"],
  ["src/windows/Changelog.css", "changelog", "Changelog"],
  ["src/windows/Setup.css", "setup", "Setup"],
];
const slug = (s) => s.replace(/[^A-Za-z0-9_.:-]+/g, "_").replace(/^_+|_+$/g, "").slice(0, 28);

for (const [file, area, short] of CSS_FILES) {
  const root = postcss.parse(read(file), { from: file });
  root.walkDecls((d) => {
    const ctx = [];
    let p = d.parent;
    let keyframes = null;
    let rule = null;
    while (p && p.type !== "root") {
      if (p.type === "atrule") {
        ctx.unshift("@" + p.name + (p.params ? " " + norm(p.params) : ""));
        if (/keyframes$/.test(p.name)) keyframes = p.params;
      } else if (p.type === "rule") rule = p;
      p = p.parent;
    }
    const sel = rule ? norm(rule.selector) : "";
    const prop = d.prop;
    const isFont = ctx.some((c) => c.startsWith("@font-face"));
    let kind;
    if (keyframes) kind = "css-keyframe";
    else if (prop.startsWith("--")) kind = "css-var";
    else if (/^(transition|animation)/.test(prop)) kind = "css-motion";
    else if (isFont) kind = "css-font-face";
    else if (ctx.length) kind = "css-media";
    else if (/:(hover|active|focus|focus-visible|focus-within|disabled|checked)|\.hovered|\[disabled\]|::-webkit|::placeholder|::selection/.test(sel)) kind = "css-state";
    else kind = "css-rule";
    const ctxStr = ctx.join(" ");
    const key = ctxStr + "\u0000" + sel;
    const idSel = slug(keyframes ? keyframes + "_" + sel : sel || ctxStr);
    const value = d.value + (d.important ? " !important" : "");
    const what = `${ctxStr ? ctxStr + " " : ""}${sel ? sel + " " : ""}{ ${prop}: ${trunc(norm(value), 220)} }`;
    add({
      id: `C.${short}.${idSel}.${h6(key)}.${prop}`,
      area,
      kind,
      file,
      source: `${file}:${d.source.start.line}`,
      what,
      line: d.source.start.line,
    });
  });
}

// ── TSX ──────────────────────────────────────────────────────────────────────
const TSX_AREA = {
  "src/App.tsx": "widget",
  "src/windows/Settings.tsx": "settings",
  "src/windows/Changelog.tsx": "changelog",
  "src/windows/Setup.tsx": "setup",
  "src/windows/Update.tsx": "update",
  "src/windows/Versions.tsx": "versions",
  "src/main.tsx": "shell",
};
const isJsx = (n) => ts.isJsxElement(n) || ts.isJsxSelfClosingElement(n) || ts.isJsxFragment(n);
const strip = (n) => {
  while (n && (ts.isParenthesizedExpression(n) || ts.isAsExpression(n) || ts.isNonNullExpression(n))) n = n.expression;
  return n;
};
const isLogical = (n) =>
  ts.isBinaryExpression(n) &&
  [ts.SyntaxKind.AmpersandAmpersandToken, ts.SyntaxKind.BarBarToken, ts.SyntaxKind.QuestionQuestionToken].includes(n.operatorToken.kind);
function hasJsxOperand(n) {
  n = strip(n);
  if (isJsx(n)) return true;
  if (ts.isConditionalExpression(n)) return hasJsxOperand(n.whenTrue) || hasJsxOperand(n.whenFalse);
  if (isLogical(n)) return hasJsxOperand(n.right) || hasJsxOperand(n.left);
  return false;
}
function hasCondition(n) {
  let found = false;
  const v = (x) => {
    if (found) return;
    if (ts.isConditionalExpression(x) || isLogical(x)) found = true;
    else if (!ts.isFunctionLike(x)) ts.forEachChild(x, v);
  };
  v(n);
  return found;
}

function processTsx(file, area) {
  const text = read(file);
  const sf = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true, file.endsWith(".tsx") ? ts.ScriptKind.TSX : ts.ScriptKind.TS);
  const lineOf = (n) => sf.getLineAndCharacterOfPosition(n.getStart(sf)).line + 1;
  const T = (n, max = 200) => trunc(norm(n.getText(sf)), max);
  const short = path.basename(file, path.extname(file));

  function fnName(n) {
    for (let p = n.parent; p; p = p.parent) {
      if (ts.isFunctionDeclaration(p) && p.name) return p.name.text;
      if (ts.isMethodDeclaration(p) && p.name) return p.name.getText(sf);
      if ((ts.isArrowFunction(p) || ts.isFunctionExpression(p)) && ts.isVariableDeclaration(p.parent) && ts.isIdentifier(p.parent.name)) return p.parent.name.text;
    }
    return "module";
  }
  function tagOf(n) {
    n = strip(n);
    if (ts.isJsxFragment(n)) return "<>";
    if (!isJsx(n)) return null;
    const open = ts.isJsxElement(n) ? n.openingElement : n;
    let cls = "";
    for (const a of open.attributes.properties) {
      if (ts.isJsxAttribute(a) && a.name.getText(sf) === "className" && a.initializer) {
        const e = ts.isJsxExpression(a.initializer) ? a.initializer.expression : a.initializer;
        if (e && (ts.isStringLiteral(e) || ts.isNoSubstitutionTemplateLiteral(e))) cls = "." + e.text.trim().split(/\s+/).join(".");
        else if (e && ts.isTemplateExpression(e)) cls = "." + e.head.text.trim().split(/\s+/).filter(Boolean).join(".");
      }
    }
    return `<${open.tagName.getText(sf)}${trunc(cls, 40)}>`;
  }
  function armDesc(n) {
    n = strip(n);
    if (n.kind === ts.SyntaxKind.NullKeyword) return "nothing";
    if (ts.isIdentifier(n) && n.text === "undefined") return "nothing";
    if (n.kind === ts.SyntaxKind.FalseKeyword) return "nothing";
    return tagOf(n) ?? T(n, 60);
  }
  // flatten a conditional / && chain into "if c: <X>; else ..."
  function describeCond(n) {
    n = strip(n);
    if (ts.isConditionalExpression(n)) {
      const parts = [];
      let cur = n;
      for (;;) {
        parts.push(`if ${T(cur.condition, 140)}: ${armDesc(cur.whenTrue)}`);
        const f = strip(cur.whenFalse);
        if (ts.isConditionalExpression(f)) cur = f;
        else {
          parts.push(`else: ${armDesc(f)}`);
          break;
        }
      }
      return parts.join("; ");
    }
    if (isLogical(n)) {
      const op = n.operatorToken.getText(sf);
      const conds = [];
      let cur = n;
      while (isLogical(cur) && !hasJsxOperand(strip(cur.left)) && cur.operatorToken.getText(sf) === op) {
        conds.unshift(T(cur.left, 120));
        cur = strip(cur.right);
      }
      const tail = isLogical(cur) || ts.isConditionalExpression(cur) ? describeCond(cur) : armDesc(cur);
      const opText = op === "&&" ? "if" : op === "||" ? "unless" : "if nullish";
      return `${opText} ${conds.join(op === "&&" ? " && " : " " + op + " ")}: ${tail}`;
    }
    return T(n, 120);
  }
  const addRow = (kind, node, key, what, extra = {}) =>
    add({
      id: `T.${short}.${fnName(node)}.${kind}.${h6(key)}`,
      area,
      kind,
      file,
      source: `${file}:${lineOf(node)}`,
      what: trunc(what, 520),
      line: lineOf(node),
      ...extra,
    });

  const localFns = (node) => {
    // a named handler defined in an enclosing function body
    return (name) => {
      for (let p = node.parent; p; p = p.parent) {
        if (!p.statements && !(p.body && p.body.statements)) continue;
        const stmts = p.statements ?? p.body.statements;
        for (const s of stmts) {
          if (ts.isVariableStatement(s)) {
            for (const d of s.declarationList.declarations) if (ts.isIdentifier(d.name) && d.name.text === name && d.initializer) return d.initializer;
          } else if (ts.isFunctionDeclaration(s) && s.name?.text === name) return s;
        }
      }
      return null;
    };
  };
  const keysIn = (n) => {
    const t = n.getText(sf);
    const out = new Set();
    for (const m of t.matchAll(/\b(?:key|code)\s*===?\s*["']([^"']+)["']/g)) out.add(m[1]);
    for (const m of t.matchAll(/\bcase\s+["']([^"']+)["']/g)) out.add(m[1]);
    for (const m of t.matchAll(/\b(ctrlKey|metaKey|shiftKey|altKey)\b/g)) out.add(m[1]);
    for (const m of t.matchAll(/\b(?:button|buttons)\s*===?\s*(\d)/g)) out.add("button " + m[1]);
    return [...out];
  };

  function visit(n) {
    // conditional rendering
    if ((ts.isConditionalExpression(n) || isLogical(n)) && hasJsxOperand(n)) {
      let pp = n.parent;
      while (pp && ts.isParenthesizedExpression(pp)) pp = pp.parent;
      const top = !(pp && (ts.isConditionalExpression(pp) || isLogical(pp)));
      if (top) {
        const d = describeCond(n);
        addRow("cond-render", n, d, d);
      }
    }
    // early returns
    if (ts.isReturnStatement(n) && n.expression && (isJsx(strip(n.expression)) || strip(n.expression).kind === ts.SyntaxKind.NullKeyword)) {
      let inIf = null;
      for (let p = n.parent; p && !ts.isFunctionLike(p); p = p.parent) if (ts.isIfStatement(p)) inIf = inIf ?? p;
      const name = fnName(n);
      if (inIf && /^[A-Z]/.test(name)) {
        const cond = T(inIf.expression, 160);
        addRow("early-return", n, cond + "→" + armDesc(n.expression), `if ${cond}: return ${armDesc(n.expression)}`);
      }
    }
    // attributes
    if (ts.isJsxAttribute(n) && n.initializer && ts.isJsxExpression(n.initializer) && n.initializer.expression) {
      const name = n.name.getText(sf);
      const expr = n.initializer.expression;
      const open = n.parent.parent;
      const tag = tagOf(open) ?? `<${open.tagName?.getText(sf) ?? "?"}>`;
      if (name === "className" || name === "class") {
        if (!(ts.isStringLiteral(expr) || ts.isNoSubstitutionTemplateLiteral(expr))) {
          addRow("cond-class", n, T(expr, 300), `${tag} className = ${T(expr, 360)}`);
        }
      } else if (name === "style") {
        const e = strip(expr);
        if (ts.isObjectLiteralExpression(e)) {
          const dyn = e.properties.some((p) => ts.isPropertyAssignment(p) && !(ts.isStringLiteral(p.initializer) || ts.isNumericLiteral(p.initializer))) || e.properties.some((p) => !ts.isPropertyAssignment(p));
          addRow(dyn ? "cond-style" : "inline-style", n, T(expr, 300), `${tag} style = ${T(expr, 360)}`);
        } else addRow("cond-style", n, T(expr, 300), `${tag} style = ${T(expr, 360)}`);
      } else if (/^on[A-Z]/.test(name)) {
        let body = expr;
        if (ts.isIdentifier(expr)) body = localFns(n)(expr.text) ?? expr;
        const keys = keysIn(body);
        addRow("handler", n, name + "|" + T(expr, 300) + "|" + tag, `${tag} ${name} = ${T(expr, 150)}${keys.length ? " [keys: " + keys.join(", ") + "]" : ""}`);
      } else if (!["key", "ref", "children"].includes(name) && hasCondition(expr)) {
        addRow("cond-attr", n, name + "|" + T(expr, 300), `${tag} ${name} = ${T(expr, 360)}`);
      }
    }
    if (ts.isCallExpression(n)) {
      const callee = n.expression.getText(sf);
      if (/^(usePersisted|useSettingsStore)$/.test(callee)) {
        const arg0 = n.arguments[0] ? T(n.arguments[0], 80) : "";
        const decl = ts.isVariableDeclaration(n.parent) ? T(n.parent.name, 80) : "";
        addRow("setting-read", n, `${callee}|${decl}|${arg0}`, `${decl} ← ${callee}(${n.arguments.map((a) => T(a, 80)).join(", ")})`);
      } else if (/addEventListener$/.test(callee) && n.arguments.length >= 2) {
        const target = callee.replace(/\.addEventListener$/, "");
        const ev = T(n.arguments[0], 40);
        let h = n.arguments[1];
        let body = h;
        if (ts.isIdentifier(h)) body = localFns(n)(h.text) ?? h;
        const keys = keysIn(body);
        addRow("listener", n, `${target}|${ev}|${T(h, 100)}`, `${target}.addEventListener(${ev}, ${T(h, 90)})${keys.length ? " [keys: " + keys.join(", ") + "]" : ""}`);
      } else if (/^(setTimeout|setInterval|window\.setTimeout|window\.setInterval)$/.test(callee) && n.arguments.length >= 2) {
        const delay = strip(n.arguments[1]);
        if (ts.isNumericLiteral(delay) || (ts.isIdentifier(delay) && /^[A-Z][A-Z0-9_]+$/.test(delay.text)) || (ts.isBinaryExpression(delay) && /^[\d\s.*+/_A-Z-]+$/.test(delay.getText(sf)))) {
          addRow("timer", n, `${callee}|${T(delay)}|${T(n.arguments[0], 120)}`, `${callee}(${T(n.arguments[0], 100)}, ${T(delay)})`);
        }
      } else if (/window\.player/.test(callee)) {
        const m = callee.match(/window\.player\??\.(\w+)/);
        if (m && IPC_METHODS.test(m[1])) addRow("ipc-call", n, `${callee}|${n.arguments.map((a) => T(a, 60)).join(",")}`, `window.player.${m[1]}(${n.arguments.map((a) => T(a, 60)).join(", ")})`);
      }
    }
    if (ts.isVariableDeclaration(n) && ts.isIdentifier(n.name) && /^Icon[A-Z]/.test(n.name.text) && ts.isVariableStatement(n.parent.parent) && n.parent.parent.parent === sf) {
      addRow("icon", n, n.name.text, `${n.name.text} = ${T(n.initializer, 300)}`);
    }
    if (ts.isVariableDeclaration(n) && ts.isIdentifier(n.name) && /^[A-Z][A-Z0-9_]{2,}$/.test(n.name.text) && n.initializer && ts.isVariableStatement(n.parent.parent) && n.parent.parent.parent === sf && /^[-\d\s.*+/()_A-Z"']+$/.test(n.initializer.getText(sf)) && n.initializer.getText(sf).length < 80) {
      addRow("const", n, n.name.text, `${n.name.text} = ${T(n.initializer, 80)}`);
    }
    if (ts.isNewExpression(n) && /^(ResizeObserver|IntersectionObserver|MutationObserver)$/.test(n.expression.getText(sf))) {
      addRow("observer", n, n.expression.getText(sf) + "|" + T(n.arguments?.[0] ?? n, 100), `new ${n.expression.getText(sf)}(${n.arguments?.[0] ? T(n.arguments[0], 140) : ""})`);
    }
    ts.forEachChild(n, visit);
  }
  visit(sf);
  return sf;
}
const IPC_METHODS = /^(drag|setWidth|setHidden|setAlwaysOnTop|fullscreen|onFullscreen|resize|toggleSettings|openSettings|closeSettings|resizeSettings|settingsResize|close|minimize|openChangelog|openPastChangelog|changelog|closeChangelog|setup|openVersions|uninstall|win|update)/i;

for (const [file, area] of Object.entries(TSX_AREA)) processTsx(file, area);

// ── settings: DEFAULTS/KEYS + the Settings window's items ────────────────────
{
  const file = "src/settings.ts";
  const text = read(file);
  const sf = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true);
  const lineOf = (n) => sf.getLineAndCharacterOfPosition(n.getStart(sf)).line + 1;
  const defaults = new Map();
  const keys = new Map();
  sf.forEachChild((s) => {
    if (!ts.isVariableStatement(s)) return;
    for (const d of s.declarationList.declarations) {
      if (!ts.isIdentifier(d.name) || !d.initializer) continue;
      let init = d.initializer;
      while (ts.isAsExpression(init) || ts.isParenthesizedExpression(init)) init = init.expression;
      if (!ts.isObjectLiteralExpression(init)) continue;
      if (d.name.text === "DEFAULTS") for (const p of init.properties) if (ts.isPropertyAssignment(p)) defaults.set(p.name.getText(sf), { v: trunc(norm(p.initializer.getText(sf)), 160), line: lineOf(p) });
      if (d.name.text === "KEYS") for (const p of init.properties) if (ts.isPropertyAssignment(p)) keys.set(p.name.getText(sf), p.initializer.getText(sf).replace(/["']/g, ""));
    }
  });
  // who reads each key
  const users = new Map();
  for (const f of Object.keys(TSX_AREA)) {
    read(f)
      .split("\n")
      .forEach((l, i) => {
        for (const m of l.matchAll(/KEYS\.(\w+)/g)) {
          const arr = users.get(m[1]) ?? [];
          arr.push(`${path.basename(f)}:${i + 1}`);
          users.set(m[1], arr);
        }
      });
  }
  for (const [k, d] of defaults) {
    const u = users.get(k) ?? [];
    add({
      id: `S.${k}`,
      area: "settings",
      kind: "setting",
      file,
      group: "src/settings.ts (DEFAULTS / KEYS)",
      source: `${file}:${d.line}`,
      what: `${k} = ${d.v}; storage ${keys.get(k) ?? "?"}${u.length ? "; read at " + trunc(u.join(", "), 150) : "; not read through KEYS in a window"}`,
      line: d.line,
      scenes: scenesFor("settings", k) + (/^(fs|font|lyric|bg|corner|highlight|glass|cover|blur|width|motion|theme|tint|album)/i.test(k) ? ", appearance:" + k : ""),
    });
  }

  // Settings.tsx items
  const sfile = "src/windows/Settings.tsx";
  const st = read(sfile);
  const ssf = ts.createSourceFile(sfile, st, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
  const sLine = (n) => ssf.getLineAndCharacterOfPosition(n.getStart(ssf)).line + 1;
  const strOf = (n) => {
    if (!n) return "";
    if (ts.isStringLiteral(n) || ts.isNoSubstitutionTemplateLiteral(n)) return n.text;
    if (ts.isTemplateExpression(n)) return n.getText(ssf).slice(1, -1);
    return "⟨" + trunc(norm(n.getText(ssf)), 60) + "⟩";
  };
  let items = null;
  const findItems = (n) => {
    if (ts.isVariableDeclaration(n) && ts.isIdentifier(n.name) && n.name.text === "items" && n.initializer && ts.isArrayLiteralExpression(n.initializer)) items = n.initializer;
    else ts.forEachChild(n, findItems);
  };
  findItems(ssf);
  if (items) {
    for (const el of items.elements) {
      let cat = "", key = "", label = "", desc = "", hint = "", id = "";
      if (ts.isObjectLiteralExpression(el)) {
        for (const p of el.properties) {
          if (!ts.isPropertyAssignment(p)) continue;
          const nm = p.name.getText(ssf);
          if (nm === "id") id = strOf(p.initializer);
          else if (nm === "cat") cat = strOf(p.initializer);
          else if (nm === "key") key = strOf(p.initializer);
          else if (nm === "label") label = strOf(p.initializer);
          else if (nm === "desc") desc = strOf(p.initializer);
          else if (nm === "hint") hint = norm(p.initializer.getText(ssf));
        }
      } else if (ts.isCallExpression(el)) {
        const fn = el.expression.getText(ssf);
        const a = el.arguments;
        if (["toggle", "slider", "chips"].includes(fn)) {
          cat = strOf(a[0]);
          key = strOf(a[1]);
          label = strOf(a[2]);
          desc = strOf(a[3]);
          id = key;
          hint = fn;
        } else {
          cat = "?";
          label = fn;
          id = fn;
        }
      } else continue;
      add({
        id: `I.${id || h6(label)}`,
        area: "settings",
        kind: "setting-item",
        file: sfile,
        group: "src/windows/Settings.tsx (items list)",
        source: `${sfile}:${sLine(el)}`,
        what: `[${cat}] “${label}” — ${trunc(desc, 300)}${key ? ` (key ${key})` : ""}`,
        line: sLine(el),
        scenes: scene(["settings-" + cat, "settings"]),
      });
    }
  }
}

// ── window rows parsed from electron/main.cjs ────────────────────────────────
for (const file of ["electron/main.cjs", "electron/spicyKey.cjs"]) {
  const text = read(file);
  const sf = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true, ts.ScriptKind.JS);
  const lineOf = (n) => sf.getLineAndCharacterOfPosition(n.getStart(sf)).line + 1;
  const T = (n, m = 200) => trunc(norm(n.getText(sf)), m);
  const winName = (c) => {
    const p = c.parent;
    if (p && ts.isBinaryExpression(p) && p.operatorToken.kind === ts.SyntaxKind.EqualsToken) return p.left.getText(sf);
    if (p && ts.isVariableDeclaration(p)) return p.name.getText(sf);
    return "?";
  };
  const WIN_AREA = { keyWin: "settings", win: "widget", settingsWin: "settings", changelogWin: "changelog", pastChangelogWin: "changelog", setupWin: "setup", updateWin: "update", versionsWin: "versions" };
  const addW = (kind, n, key, what, who = "") => {
    add({ id: `W.${kind}.${h6(key)}`, area: "shell", kind, file, group: `${file} (parsed)`, source: `${file}:${lineOf(n)}`, what: trunc(what, 520), line: lineOf(n), scenes: scene(["window-behaviour"]) });
  };
  const IPC_PREFIX = /^(win|settings|changelog|setup|update|versions|fonts|app):/;
  const walk = (n) => {
    if (ts.isNewExpression(n) && n.expression.getText(sf) === "BrowserWindow") {
      const who = winName(n);
      const o = n.arguments?.[0];
      if (o && ts.isObjectLiteralExpression(o)) {
        for (const p of o.properties) {
          if (!ts.isPropertyAssignment(p) && !ts.isShorthandPropertyAssignment(p)) continue;
          const nm = p.name.getText(sf);
          if (nm === "webPreferences" && ts.isPropertyAssignment(p) && ts.isObjectLiteralExpression(p.initializer)) {
            for (const q of p.initializer.properties) {
              if (ts.isPropertyAssignment(q)) addW("window-option", q, `${who}.webPreferences.${q.name.getText(sf)}`, `${who} webPreferences.${q.name.getText(sf)} = ${T(q.initializer, 120)}`, who);
            }
            continue;
          }
          const val = ts.isPropertyAssignment(p) ? T(p.initializer, 160) : nm;
          addW("window-option", p, `${who}.${nm}`, `${who} option ${nm}: ${val}`, who);
        }
      }
    }
    if (ts.isCallExpression(n) && ts.isPropertyAccessExpression(n.expression)) {
      const obj = n.expression.expression.getText(sf);
      const m = n.expression.name.text;
      const who = obj;
      if (m === "setAlwaysOnTop") addW("window-level", n, `${T(n)}`, `${obj}.setAlwaysOnTop(${n.arguments.map((a) => T(a, 40)).join(", ")})`, WIN_AREA[obj.replace(/\?$/, "")] ? obj.replace(/\?$/, "") : "");
      else if ((m === "on" || m === "once") && WIN_AREA[obj] && n.arguments[0] && ts.isStringLiteral(n.arguments[0])) addW("window-event", n, `${obj}.${m}.${n.arguments[0].text}`, `${obj}.${m}("${n.arguments[0].text}") → ${T(n.arguments[1] ?? n, 120)}`, obj);
      else if (obj === "app" && (m === "on" || m === "once") && n.arguments[0] && ts.isStringLiteral(n.arguments[0])) addW("app-event", n, `app.${m}.${n.arguments[0].text}`, `app.${m}("${n.arguments[0].text}") → ${T(n.arguments[1] ?? n, 140)}`);
      else if (obj === "screen" && m === "on") addW("app-event", n, `screen.on.${T(n.arguments[0])}`, `screen.on(${T(n.arguments[0])}) → ${T(n.arguments[1] ?? n, 140)}`);
      else if (obj === "globalShortcut" && m === "register") addW("window-shortcut", n, `shortcut.${T(n.arguments[0])}`, `globalShortcut.register(${T(n.arguments[0])}) → ${T(n.arguments[1], 140)}`);
      else if (obj === "ipcMain" && (m === "handle" || m === "on") && n.arguments[0] && ts.isStringLiteral(n.arguments[0]) && IPC_PREFIX.test(n.arguments[0].text)) addW("ipc", n, `ipc.${n.arguments[0].text}`, `ipcMain.${m}("${n.arguments[0].text}") → ${T(n.arguments[1] ?? n, 160)}`);
      else if (obj === "app" && ["requestSingleInstanceLock", "setName", "setPath", "quit", "exit"].includes(m) && !ts.isFunctionLike(n.parent)) {
        // only the top-level / lifecycle calls, not every app.quit() inside handlers
        if (m !== "quit") addW("app-call", n, `app.${m}.${T(n)}`, `app.${m}(${n.arguments.map((a) => T(a, 60)).join(", ")})`);
      }
      void who;
    }
    if (ts.isVariableDeclaration(n) && ts.isIdentifier(n.name) && /^[A-Z][A-Z0-9_]+$/.test(n.name.text) && n.initializer && /^(STAGE_PAD|WIDGET_W|SETTINGS_W|CHANGELOG_W|UPDATE_W|SETUP_W|DRAG_INTERVAL_MS)$/.test(n.name.text)) {
      addW("window-const", n, n.name.text, `${n.name.text} = ${T(n.initializer, 80)}`);
    }
    ts.forEachChild(n, walk);
  };
  walk(sf);

  // curated hand rows, anchors resolved to file:line
  const lines = text.split("\n");
  const hand = file === "electron/main.cjs" ? JSON.parse(read("scripts/ledger/hand-rows.json")) : [];
  const seen = new Set();
  for (const r of hand) {
    if (seen.has(r.key)) throw new Error("duplicate hand row key " + r.key);
    seen.add(r.key);
    const idx = lines.findIndex((l) => l.includes(r.anchor));
    if (idx < 0) throw new Error(`hand row ${r.key}: anchor not found in ${file}: ${r.anchor}`);
    add({
      id: `H.${r.key}`,
      area: r.area ?? "shell",
      kind: r.kind,
      file,
      group: "electron/main.cjs (curated, hand-rows.json)",
      source: `${r.file ?? file}:${idx + 1}`,
      what: r.what,
      line: idx + 1,
      scenes: r.scenes ?? scene(["window-behaviour"]),
    });
  }
}

// ── order ────────────────────────────────────────────────────────────────────
const GROUP_ORDER = (g) => (g.startsWith("src/") && /\.css$/.test(g) ? 0 : g.includes("settings.ts") ? 1 : g.includes("items list") ? 2 : g.endsWith(".tsx") ? 3 : g.includes("parsed") ? 4 : 5);
rows.sort((a, b) => AREA_IDX[a.area] - AREA_IDX[b.area] || GROUP_ORDER(a.group) - GROUP_ORDER(b.group) || (a.file < b.file ? -1 : a.file > b.file ? 1 : 0) || a.line - b.line || (a.id < b.id ? -1 : 1));

// ── previous status survives ─────────────────────────────────────────────────
const prevFile = path.join(ROOT, "parity", "ledger.md");
const prev = new Map();
if (fs.existsSync(prevFile)) {
  for (const l of fs.readFileSync(prevFile, "utf8").split("\n")) {
    if (!l.startsWith("| ") || l.startsWith("| id ") || l.startsWith("| ---")) continue;
    const c = l.slice(2, -2).split(" | ");
    if (c.length === 9) prev.set(c[0], { gpui: c[6], status: c[7], evidence: c[8] });
  }
}

// ── write ────────────────────────────────────────────────────────────────────
const esc = (s) => s.replace(/\|/g, "\\|").replace(/\n/g, " ");
const code = (s) => "`" + s.replace(/`/g, "'") + "`";
const kinds = [...new Set(rows.map((r) => r.kind))].sort();
const count = {};
for (const r of rows) {
  count[r.area] ??= {};
  count[r.area][r.kind] = (count[r.area][r.kind] ?? 0) + 1;
}
const out = [];
out.push("# Parity ledger", "");
out.push("Generated by `node scripts/ledger/build.mjs`; do not edit rows by hand except `gpui`, `status` and `evidence` (kept across rebuilds, matched by id).");
out.push("Format follows port.md §3. `status`: todo / done / accepted-difference (only the user sets the last). `scenes`: harness scenes that verify the row (a trailing `?` = not in the harness yet).", "");
out.push("Id scheme: `C.<css file>.<selector slug>.<hash of @-context+selector>.<property>` (CSS, one row per declaration), `T.<tsx file>.<function>.<kind>.<hash of the expression text>` (TSX), `S.<key>` (DEFAULTS entry), `I.<id>` (Settings item), `W.<kind>.<hash>` (parsed main.cjs), `H.<key>` (curated). A repeat of an id gets `~2`, `~3`.", "");
out.push("## Summary", "");
out.push(`Total rows: **${rows.length}**`, "");
out.push("| kind | total | " + AREAS.map(([a]) => a).join(" | ") + " |");
out.push("| --- | ---: | " + AREAS.map(() => "---:").join(" | ") + " |");
for (const k of kinds) {
  const per = AREAS.map(([a]) => count[a]?.[k] ?? 0);
  out.push(`| ${k} | ${per.reduce((x, y) => x + y, 0)} | ` + per.map((n) => n || "").join(" | ") + " |");
}
out.push(`| **all** | **${rows.length}** | ` + AREAS.map(([a]) => Object.values(count[a] ?? {}).reduce((x, y) => x + y, 0)).join(" | ") + " |");
out.push("");
for (const [a, title] of AREAS) {
  const ar = rows.filter((r) => r.area === a);
  if (!ar.length) continue;
  out.push(`## ${a}: ${title}`, "");
  let g = null;
  for (const r of ar) {
    if (r.group !== g) {
      g = r.group;
      out.push("", `### ${a} / ${g}`, "");
      out.push("| id | area | kind | source | what | scenes | gpui | status | evidence |");
      out.push("| --- | --- | --- | --- | --- | --- | --- | --- | --- |");
    }
    const p = prev.get(r.id);
    out.push(`| ${r.id} | ${r.area} | ${r.kind} | ${r.source} | ${esc(code(r.what))} | ${r.scenes} | ${p?.gpui ?? ""} | ${p?.status ?? "todo"} | ${p?.evidence ?? ""} |`);
  }
  out.push("");
}
fs.mkdirSync(path.dirname(prevFile), { recursive: true });
fs.writeFileSync(prevFile, out.join("\n"));
console.log(`ledger: ${rows.length} rows -> parity/ledger.md`);
