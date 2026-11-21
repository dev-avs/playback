// Generates the GPUI port's settings (port.md 2.3) from the TypeScript source:
//   src-gpui/logic/src/settings_gen.rs        Settings struct, Default, KEYS (from src/settings.ts)
//   src-gpui/logic/src/settings_items_gen.rs  the settings window's rows (from `items` in src/windows/Settings.tsx)
//   scripts/golden/fixtures/settings/items.json      the same rows as JSON, for the golden test
//
//   node scripts/gen-gpui-settings.mts
//
// Never edit the generated files by hand. Rows that cannot be read statically
// are generated as Control::Custom and listed in a comment at the top of
// settings_items_gen.rs.

import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import ts from "typescript";
import * as S from "../src/settings.ts";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const outSrc = join(root, "src-gpui/logic/src");
const fixturesOut = join(root, "scripts/golden/fixtures/settings");

// ───────────────────────────── helpers ─────────────────────────────

/** a Rust string literal with exactly the text of s */
function rs(s: string): string {
  let o = '"';
  for (const ch of s) {
    const c = ch.codePointAt(0)!;
    if (ch === "\\") o += "\\\\";
    else if (ch === '"') o += '\\"';
    else if (ch === "\n") o += "\\n";
    else if (ch === "\r") o += "\\r";
    else if (ch === "\t") o += "\\t";
    else if (c < 0x20 || c === 0x7f) o += `\\u{${c.toString(16)}}`;
    else o += ch;
  }
  return o + '"';
}
const rsOpt = (s: string | null | undefined) => (s == null ? "None" : `Some(${rs(s)})`);
/** a Rust f64 literal that parses to exactly the same number */
const num = (n: number) => {
  const t = String(n);
  return /^-?\d+$/.test(t) ? `${t}.0` : t;
};
const snake = (k: string) => k.replace(/[A-Z]/g, (c) => "_" + c.toLowerCase());
const camel = (k: string) => k.replace(/_([a-z])/g, (_, c) => c.toUpperCase());
const pascal = (k: string) => k[0].toUpperCase() + k.slice(1);
const RUST_KEYWORDS = new Set(["type", "mod", "ref", "fn", "match", "move"]);

function rustfmt(file: string) {
  execFileSync("rustfmt", ["--edition", "2021", file], { stdio: "inherit" });
}

// ───────────────────────────── settings_gen.rs ─────────────────────────────

const settingsText = readFileSync(join(root, "src/settings.ts"), "utf8");
const settingsSf = ts.createSourceFile("settings.ts", settingsText, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);

/** the declared type of DEFAULTS' object-valued entries: `{} as Record<string, X>` */
const recordTypes = new Map<string, string>();
const defaultsEntries: string[] = [];
settingsSf.forEachChild((n) => {
  if (!ts.isVariableStatement(n)) return;
  for (const d of n.declarationList.declarations) {
    if (ts.isIdentifier(d.name) && d.name.text === "DEFAULTS" && d.initializer && ts.isObjectLiteralExpression(d.initializer)) {
      for (const p of d.initializer.properties) {
        if (!ts.isPropertyAssignment(p)) throw new Error("DEFAULTS: unexpected member");
        const k = p.name.getText();
        defaultsEntries.push(k);
        if (ts.isAsExpression(p.initializer) && ts.isObjectLiteralExpression(p.initializer.expression)) {
          const t = p.initializer.type;
          if (!ts.isTypeReferenceNode(t) || t.typeName.getText() !== "Record" || t.typeArguments?.length !== 2)
            throw new Error(`DEFAULTS.${k}: unsupported object type ${t.getText()}`);
          const [kt, vt] = t.typeArguments;
          if (kt.getText() !== "string") throw new Error(`DEFAULTS.${k}: key type ${kt.getText()}`);
          // number or a string-union alias (Provider) → f64 / String
          const vtext = vt.getText();
          recordTypes.set(k, vtext === "number" ? "f64" : vtext === "boolean" ? "bool" : "String");
        }
      }
    }
  }
});

const DEFAULTS = S.DEFAULTS as Record<string, unknown>;
const KEYS = S.KEYS as Record<string, string>;
if (JSON.stringify(Object.keys(DEFAULTS)) !== JSON.stringify(defaultsEntries)) throw new Error("DEFAULTS order mismatch");

let fields = "";
let defaults = "";
let entries = "";
let loads = "";
for (const [k, v] of Object.entries(DEFAULTS)) {
  const f = snake(k);
  if (camel(f) !== k) throw new Error(`setting ${k}: camelCase(snake_case) differs; needs an explicit rename`);
  if (RUST_KEYWORDS.has(f)) throw new Error(`setting ${k} is a Rust keyword`);
  let ty: string;
  let dv: string;
  if (typeof v === "boolean") (ty = "bool"), (dv = String(v));
  else if (typeof v === "number") (ty = "f64"), (dv = num(v));
  else if (typeof v === "string") (ty = "String"), (dv = `${rs(v)}.to_string()`);
  else if (v && typeof v === "object" && Object.keys(v).length === 0 && recordTypes.has(k)) {
    const vt = recordTypes.get(k)!;
    (ty = `BTreeMap<String, ${vt}>`), (dv = "BTreeMap::new()");
  } else throw new Error(`setting ${k}: cannot infer a Rust type`);
  fields += `    pub ${f}: ${ty},\n`;
  defaults += `            ${f}: ${dv},\n`;
  // romanizeDefault was a boolean up to 1.5.2: romaMode() takes either, as the widget does
  loads +=
    k === "romanizeDefault"
      ? `        if let Some(v) = map.get(${rs(k)}) {
            s.${f} = crate::settings::roma_mode(v).as_str().to_string();
        }
`
      : `        if let Some(Ok(v)) = map.get(${rs(k)}).map(|v| serde_json::from_value(v.clone())) {
            s.${f} = v;
        }
`;
  entries += `            (${rs(k)}, serde_json::to_value(${ty === "bool" || ty === "f64" ? "" : "&"}self.${f}).unwrap()),\n`;
}

let keysTable = "";
for (const [k, v] of Object.entries(KEYS)) keysTable += `    (${rs(k)}, ${rs(v)}),\n`;

const settingsRs = `// @generated by scripts/gen-gpui-settings.mts from src/settings.ts. Do not edit.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Every setting, one field per \`DEFAULTS\` entry in src/settings.ts, in its order.
/// Serializes to the camelCase object that \`snapshotSettingsJson()\` writes to
/// \`%APPDATA%\\Playback\\gpui-settings.json\`; a missing key takes its default.
/// String-union settings (Highlight, RomaMode, ...) are Strings, as in JavaScript.
/// A value of the wrong type takes its default instead of failing the whole file.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
${fields}}

impl Default for Settings {
    fn default() -> Self {
        Self {
${defaults}        }
    }
}

impl<'de> Deserialize<'de> for Settings {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let map = serde_json::Map::<String, serde_json::Value>::deserialize(d)?;
        let mut s = Self::default();
${loads}        Ok(s)
    }
}

impl Settings {
    /// The settings as (key, value) pairs in \`DEFAULTS\` order (a JSON object loses
    /// that order in a \`Value\`, so the writer in settings.rs takes the pairs).
    pub fn entries(&self) -> Vec<(&'static str, serde_json::Value)> {
        vec![
${entries}        ]
    }
}

/// \`KEYS\` in src/settings.ts: each setting's localStorage key (and the
/// \`state.*\` keys the widget writes), in its order.
pub const KEYS: &[(&str, &str)] = &[
${keysTable}];
`;
writeFileSync(join(outSrc, "settings_gen.rs"), settingsRs);
rustfmt(join(outSrc, "settings_gen.rs"));

// ───────────────────────────── settings_items_gen.rs ─────────────────────────────

const tsxPath = join(root, "src/windows/Settings.tsx");
const tsxText = readFileSync(tsxPath, "utf8");
const sf = ts.createSourceFile("Settings.tsx", tsxText, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);

/** the text of a node's source, whitespace-normalised */
const src = (n: ts.Node) => n.getText(sf).replace(/\s+/g, " ").trim();

// top-level consts and functions of the file (ROMA_DESC, pct, times, fmtOffset, CATEGORIES)
const topConst = new Map<string, ts.Expression>();
sf.forEachChild((n) => {
  if (ts.isVariableStatement(n))
    for (const d of n.declarationList.declarations)
      if (ts.isIdentifier(d.name) && d.initializer) topConst.set(d.name.text, d.initializer);
});

class Unextractable extends Error {}
const fail = (m: string): never => {
  throw new Unextractable(m);
};

const unwrap = (n: ts.Node): ts.Node => {
  while (ts.isParenthesizedExpression(n) || ts.isAsExpression(n) || ts.isSatisfiesExpression(n)) n = n.expression;
  return n;
};

/** a literal string/number/bool, or an identifier naming a top-level const of one */
function ev(node: ts.Node): string | number | boolean {
  const n = unwrap(node);
  if (ts.isStringLiteral(n) || ts.isNoSubstitutionTemplateLiteral(n)) return n.text;
  if (ts.isNumericLiteral(n)) return Number(n.text);
  if (n.kind === ts.SyntaxKind.TrueKeyword) return true;
  if (n.kind === ts.SyntaxKind.FalseKeyword) return false;
  if (ts.isPrefixUnaryExpression(n) && n.operator === ts.SyntaxKind.MinusToken) {
    const v = ev(n.operand);
    if (typeof v === "number") return -v;
  }
  if (ts.isIdentifier(n) && topConst.has(n.text)) return ev(topConst.get(n.text)!);
  return fail(`not a literal: ${src(n)}`);
}
const evStr = (n: ts.Node) => {
  const v = ev(n);
  if (typeof v !== "string") fail(`not a string: ${src(n)}`);
  return v as string;
};

// categories
const categories: { id: string; name: string }[] = [];
{
  const init = topConst.get("CATEGORIES");
  if (!init) throw new Error("CATEGORIES not found");
  const arr = unwrap(init) as ts.ArrayLiteralExpression;
  for (const e of arr.elements) {
    const o = e as ts.ObjectLiteralExpression;
    const get = (k: string) => evStr((o.properties.find((p) => p.name?.getText() === k) as ts.PropertyAssignment).initializer);
    categories.push({ id: get("id"), name: get("name") });
  }
}

type Opt = { value: string; label: string };
type Control =
  | { kind: "toggle"; preview: boolean }
  | { kind: "slider"; min: number; max: number; step: number; fmt: string | null; onChange: string | null }
  | { kind: "chips"; options: Opt[]; preview: boolean; peek: string | null; onChange: string | null; note: string | null }
  | { kind: "custom"; note: string };
interface Row {
  id: string;
  cat: string;
  key: string | null;
  label: string;
  desc: string;
  hint: string | null; // a fixed text
  hintExpr: string | null; // a runtime expression's source
  control: Control;
}

// slider format functions: named helpers, or an inline arrow (named after its first setting)
const fmts = new Map<string, { ts: string; js: string }>(); // variant → source
const fmtByText = new Map<string, string>();
const jsOf = (text: string) => ts.transpile(`(${text})`).replace(/^"use strict";/, "").trim().replace(/;$/, "");
function fmtVariant(node: ts.Node, key: string): string {
  const n = unwrap(node);
  if (ts.isIdentifier(n)) {
    const v = pascal(n.text);
    const init = topConst.get(n.text);
    if (!init) fail(`unknown formatter ${n.text}`);
    fmts.set(v, { ts: n.text, js: jsOf(init!.getText(sf)) });
    fmtByText.set(n.text, v);
    return v;
  }
  const text = src(n);
  let v = fmtByText.get(text);
  if (!v) {
    v = pascal(key);
    if (fmts.has(v)) throw new Error(`format variant ${v} taken`);
    fmts.set(v, { ts: text, js: jsOf(n.getText(sf)) });
    fmtByText.set(text, v);
  }
  return v;
}

const runtimeObj = (name: string): any => (S as any)[name] ?? fail(`${name} is not exported by settings.ts`);

/** the options of a Chips: a literal array, or a .map over a settings.ts table */
function options(node: ts.Node): Opt[] {
  const n = unwrap(node);
  if (ts.isArrayLiteralExpression(n)) {
    return n.elements.map((e) => {
      const o = e as ts.ObjectLiteralExpression;
      const get = (k: string) => (o.properties.find((p) => p.name?.getText() === k) as ts.PropertyAssignment | undefined)?.initializer;
      return { value: evStr(get("value")!), label: evStr(get("label")!) };
    });
  }
  if (ts.isCallExpression(n) && ts.isPropertyAccessExpression(n.expression) && n.expression.name.text === "map") {
    const list = unwrap(n.expression.expression);
    let keys: string[];
    if (ts.isCallExpression(list) && list.expression.getText(sf) === "Object.keys") keys = Object.keys(runtimeObj(src(list.arguments[0])));
    else if (ts.isIdentifier(list)) keys = runtimeObj(list.text);
    else return fail(`unknown option list ${src(list)}`);
    const cb = n.arguments[0] as ts.ArrowFunction;
    const param = (cb.parameters[0].name as ts.Identifier).text;
    const body = unwrap(cb.body) as ts.ObjectLiteralExpression;
    if (!ts.isObjectLiteralExpression(body)) return fail(`option map body ${src(cb.body)}`);
    const valueP = body.properties.find((p) => p.name?.getText() === "value") as ts.PropertyAssignment;
    if (src(valueP.initializer) !== param) fail(`option value is not ${param}`);
    const labelText = src((body.properties.find((p) => p.name?.getText() === "label") as ts.PropertyAssignment).initializer);
    return keys.map((k) => {
      // PROVIDER_NAMES[p] | ROMA_LABELS[m] | (<>…{HIGHLIGHTS[h].name}</>) | GLASS_TINTS[t].name
      const direct = new RegExp(`^(\\w+)\\[${param}\\]$`).exec(labelText);
      const named = new RegExp(`(\\w+)\\[${param}\\]\\.name`).exec(labelText);
      if (direct) return { value: k, label: runtimeObj(direct[1])[k] };
      if (named) return { value: k, label: runtimeObj(named[1])[k].name };
      return fail(`option label ${labelText}`);
    });
  }
  return fail(`options: ${src(n)}`);
}

const objProps = (o: ts.ObjectLiteralExpression) => {
  const m = new Map<string, ts.Expression>();
  for (const p of o.properties) {
    if (!ts.isPropertyAssignment(p)) fail(`unsupported member ${p.getText(sf)}`);
    m.set(p.name!.getText(sf), (p as ts.PropertyAssignment).initializer);
  }
  return m;
};

// the helper builders, called as rows: toggle(), slider(), chips(), cjkChips()
const cjkMapNode = (() => {
  const fn = unwrap(topConstLocal("cjkChips")) as ts.ArrowFunction;
  return fn;
})();
/** a `const name = …` anywhere in the file (the builders live inside the component) */
function topConstLocal(name: string): ts.Expression {
  let found: ts.Expression | undefined;
  const visit = (n: ts.Node) => {
    if (ts.isVariableDeclaration(n) && ts.isIdentifier(n.name) && n.name.text === name && n.initializer && !found) found = n.initializer;
    ts.forEachChild(n, visit);
  };
  visit(sf);
  if (!found) throw new Error(`const ${name} not found`);
  return found;
}

function helperRow(call: ts.CallExpression): Row {
  const name = (call.expression as ts.Identifier).text;
  const a = call.arguments;
  if (name === "toggle") {
    const opts = a[4] ? objProps(a[4] as ts.ObjectLiteralExpression) : new Map<string, ts.Expression>();
    const key = evStr(a[1]);
    return {
      id: key, cat: evStr(a[0]), key, label: evStr(a[2]), desc: evStr(a[3]),
      hint: opts.has("hint") ? evStr(opts.get("hint")!) : null, hintExpr: null,
      control: { kind: "toggle", preview: opts.has("preview") ? (ev(opts.get("preview")!) as boolean) : false },
    };
  }
  if (name === "slider") {
    const key = evStr(a[1]);
    const o = objProps(a[4] as ts.ObjectLiteralExpression);
    return {
      id: key, cat: evStr(a[0]), key, label: evStr(a[2]), desc: evStr(a[3]),
      hint: null, hintExpr: null,
      control: {
        kind: "slider", min: ev(o.get("min")!) as number, max: ev(o.get("max")!) as number, step: ev(o.get("step")!) as number,
        fmt: fmtVariant(o.get("fmt")!, key), onChange: null,
      },
    };
  }
  if (name === "chips") {
    const key = evStr(a[1]);
    // chips(): the default for an omitted opts is { preview: true }
    const opts = a[5] ? objProps(a[5] as ts.ObjectLiteralExpression) : new Map<string, ts.Expression>([["preview", ts.factory.createTrue()]]);
    return {
      id: key, cat: evStr(a[0]), key, label: evStr(a[2]), desc: evStr(a[3]),
      hint: opts.has("hint") ? evStr(opts.get("hint")!) : null, hintExpr: null,
      control: {
        kind: "chips", options: options(a[4]), preview: opts.has("preview") ? (ev(opts.get("preview")!) as boolean) : false,
        peek: null, onChange: null, note: null,
      },
    };
  }
  if (name === "cjkChips") {
    // cjkChips(key, lang) = chips("languages", key, CJK_FONTS[lang].name, `Typeface for ${{…}[lang]} lyrics. …`, [Default, …CJK_FONTS[lang].options])
    const key = evStr(a[0]);
    const lang = evStr(a[1]);
    const body = (cjkMapNode.body as ts.Block).statements.find((s) => ts.isReturnStatement(s)) as ts.ReturnStatement;
    const inner = body.expression as ts.CallExpression;
    const tpl = unwrap(inner.arguments[3]) as ts.TemplateExpression;
    const span = tpl.templateSpans[0];
    const langMap = objProps(unwrap((unwrap(span.expression) as ts.ElementAccessExpression).expression) as ts.ObjectLiteralExpression);
    const desc = tpl.head.text + evStr(langMap.get(lang)!) + span.literal.text;
    const font = runtimeObj("CJK_FONTS")[lang];
    if (src(inner.arguments[2]) !== "CJK_FONTS[lang].name") fail("cjkChips label changed");
    return {
      id: key, cat: evStr(inner.arguments[0]), key, label: font.name, desc, hint: null, hintExpr: null,
      control: {
        kind: "chips",
        options: [{ value: "default", label: "Default" }, ...font.options.map((f: string) => ({ value: f, label: f }))],
        preview: true, peek: null, onChange: null,
        note: "options after Default are CJK_FONTS[lang].options filtered to the installed fonts, each label drawn in its own font",
      },
    };
  }
  return fail(`unknown builder ${name}`);
}

/** every Slider / Chips / Toggle element inside a node */
function jsxControls(n: ts.Node): ts.JsxSelfClosingElement[] {
  const out: ts.JsxSelfClosingElement[] = [];
  const visit = (x: ts.Node) => {
    if (ts.isJsxSelfClosingElement(x) && ["Slider", "Chips", "Toggle"].includes(x.tagName.getText(sf))) out.push(x);
    ts.forEachChild(x, visit);
  };
  visit(n);
  return out;
}
const attrs = (el: ts.JsxSelfClosingElement) => {
  const m = new Map<string, ts.Expression>();
  for (const p of el.attributes.properties)
    if (ts.isJsxAttribute(p) && p.initializer) {
      m.set(p.name.getText(sf), ts.isJsxExpression(p.initializer) ? p.initializer.expression! : (p.initializer as ts.Expression));
    }
  return m;
};

function objectRow(o: ts.ObjectLiteralExpression): Row {
  const p = objProps(o);
  const id = evStr(p.get("id")!);
  const row: Row = {
    id, cat: evStr(p.get("cat")!), key: p.has("key") ? evStr(p.get("key")!) : null,
    label: evStr(p.get("label")!), desc: evStr(p.get("desc")!), hint: null, hintExpr: null,
    control: { kind: "custom", note: "" },
  };
  if (p.has("hint")) {
    try {
      row.hint = evStr(p.get("hint")!);
    } catch {
      row.hintExpr = src(p.get("hint")!);
    }
  }
  const where = [p.get("control"), p.get("below")].filter(Boolean) as ts.Node[];
  const els = where.flatMap(jsxControls);
  const only = els.length === 1 ? els[0] : null;
  const tag = only?.tagName.getText(sf);
  try {
    if (tag === "Slider") {
      const at = attrs(only!);
      const fm = row.hintExpr ? /^(\w+)\(s\.\w+\)$/.exec(row.hintExpr) : null;
      row.control = {
        kind: "slider", min: ev(at.get("min")!) as number, max: ev(at.get("max")!) as number, step: ev(at.get("step")!) as number,
        fmt: fm && topConst.has(fm[1]) ? fmtVariant(ts.factory.createIdentifier(fm[1]) as any, id) : null,
        onChange: src(at.get("onChange")!),
      };
    } else if (tag === "Chips") {
      const at = attrs(only!);
      row.control = {
        kind: "chips", options: options(at.get("options")!), preview: false,
        peek: at.has("peek") ? src(at.get("peek")!) : null, onChange: src(at.get("onChange")!), note: null,
      };
    } else {
      row.control = { kind: "custom", note: els.length > 1 ? "several controls" : "built from runtime state or custom markup (buttons, forms, font list)" };
    }
  } catch (e) {
    if (!(e instanceof Unextractable)) throw e;
    row.control = { kind: "custom", note: e.message };
  }
  return row;
}

// the items array
let itemsArr: ts.ArrayLiteralExpression | undefined;
{
  const visit = (n: ts.Node) => {
    if (ts.isVariableDeclaration(n) && ts.isIdentifier(n.name) && n.name.text === "items" && n.initializer && ts.isArrayLiteralExpression(n.initializer)) itemsArr = n.initializer;
    ts.forEachChild(n, visit);
  };
  visit(sf);
}
if (!itemsArr) throw new Error("items array not found in Settings.tsx");

const rows: Row[] = [];
const notes: string[] = [];
for (const e of itemsArr.elements) {
  const n = unwrap(e);
  if (ts.isCallExpression(n)) rows.push(helperRow(n));
  else if (ts.isObjectLiteralExpression(n)) rows.push(objectRow(n));
  else throw new Error(`unsupported items element: ${src(n)}`);
}
for (const r of rows) {
  if (r.control.kind === "custom") notes.push(`${r.id}: ${r.control.note}`);
  if (r.control.kind === "chips" && r.control.note) notes.push(`${r.id}: ${r.control.note}`);
  if (r.hintExpr && r.control.kind !== "slider") notes.push(`${r.id}: hint is computed at run time: ${r.hintExpr}`);
}
// sanity: every key is a real setting
for (const r of rows) if (r.key && !(r.key in DEFAULTS)) throw new Error(`row ${r.id}: key ${r.key} is not a setting`);
for (const r of rows) if (!categories.some((c) => c.id === r.cat)) throw new Error(`row ${r.id}: category ${r.cat}`);

// ── Rust ──
const fmtNames = [...fmts.keys()];
const optsRs = (o: Opt[]) => `&[${o.map((x) => `(${rs(x.value)}, ${rs(x.label)})`).join(", ")}]`;
function controlRs(c: Control): string {
  switch (c.kind) {
    case "toggle":
      return `Control::Toggle { preview: ${c.preview} }`;
    case "slider":
      return `Control::Slider { min: ${num(c.min)}, max: ${num(c.max)}, step: ${num(c.step)}, fmt: ${c.fmt ? `Some(Fmt::${c.fmt})` : "None"}, on_change: ${rsOpt(c.onChange)} }`;
    case "chips":
      return `Control::Chips { options: ${optsRs(c.options)}, preview: ${c.preview}, peek: ${rsOpt(c.peek)}, on_change: ${rsOpt(c.onChange)}, note: ${rsOpt(c.note)} }`;
    case "custom":
      return `Control::Custom { note: ${rs(c.note)} }`;
  }
}
const itemRs = (r: Row) =>
  `    Item { id: ${rs(r.id)}, cat: ${rs(r.cat)}, key: ${rsOpt(r.key)}, label: ${rs(r.label)}, desc: ${rs(r.desc)}, hint: ${rsOpt(r.hint)}, hint_expr: ${rsOpt(r.hintExpr)}, control: ${controlRs(r.control)} },\n`;

const itemsRs = `// @generated by scripts/gen-gpui-settings.mts from \`items\` in src/windows/Settings.tsx. Do not edit.
//
// Rows are in source order (the window stable-sorts them by CATEGORIES order).
// Text is copied byte for byte. Rows or parts that cannot be read statically
// (Control::Custom, or a hint_expr computed at run time):
${notes.map((n) => `//   - ${n}`).join("\n")}

/// \`CATEGORIES\` in Settings.tsx.
pub struct Category {
    pub id: &'static str,
    pub name: &'static str,
}

pub const CATEGORIES: &[Category] = &[
${categories.map((c) => `    Category { id: ${rs(c.id)}, name: ${rs(c.name)} },`).join("\n")}
];

/// A slider's value-to-text function (the \`fmt\` of its row, or a named helper in Settings.tsx).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fmt {
${fmtNames.map((f) => `    ${f},`).join("\n")}
}

impl Fmt {
    pub const ALL: &[Fmt] = &[${fmtNames.map((f) => `Fmt::${f}`).join(", ")}];

    pub fn name(self) -> &'static str {
        match self {
${fmtNames.map((f) => `            Fmt::${f} => ${rs(f)},`).join("\n")}
        }
    }

    /// The TypeScript source of the function.
    pub fn source(self) -> &'static str {
        match self {
${fmtNames.map((f) => `            Fmt::${f} => ${rs(fmts.get(f)!.ts)},`).join("\n")}
        }
    }
}

pub enum Control {
    Toggle { preview: bool },
    Slider { min: f64, max: f64, step: f64, fmt: Option<Fmt>, on_change: Option<&'static str> },
    Chips { options: &'static [(&'static str, &'static str)], preview: bool, peek: Option<&'static str>, on_change: Option<&'static str>, note: Option<&'static str> },
    Custom { note: &'static str },
}

pub struct Item {
    pub id: &'static str,
    pub cat: &'static str,
    pub key: Option<&'static str>,
    pub label: &'static str,
    pub desc: &'static str,
    pub hint: Option<&'static str>,
    pub hint_expr: Option<&'static str>,
    pub control: Control,
}

pub const ITEMS: &[Item] = &[
${rows.map(itemRs).join("")}];
`;
writeFileSync(join(outSrc, "settings_items_gen.rs"), itemsRs);
rustfmt(join(outSrc, "settings_items_gen.rs"));

// ── JSON for the golden test ──
mkdirSync(fixturesOut, { recursive: true });
writeFileSync(
  join(fixturesOut, "items.json"),
  JSON.stringify({ categories, fmts: Object.fromEntries(fmts), items: rows }, null, 1) + "\n",
);

const custom = rows.filter((r) => r.control.kind === "custom").length;
console.log(`settings: ${Object.keys(DEFAULTS).length} fields, ${Object.keys(KEYS).length} keys; items: ${rows.length} rows (${custom} custom), ${fmts.size} formatters`);
