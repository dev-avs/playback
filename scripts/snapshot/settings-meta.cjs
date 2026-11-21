// DEFAULTS / KEYS (and the option tables) from src/settings.ts, read without a
// build: the file is transpiled and run with stubs for react and localStorage.
const fs = require("fs");
const path = require("path");
const vm = require("vm");
const ts = require("typescript");

let cache;
module.exports = function settingsMeta() {
  if (cache) return cache;
  const file = path.join(__dirname, "..", "..", "src", "settings.ts");
  const js = ts.transpileModule(fs.readFileSync(file, "utf8"), {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
  }).outputText;
  const mod = { exports: {} };
  const react = { useCallback: () => {}, useEffect: () => {}, useState: () => {} };
  vm.runInNewContext(js, {
    module: mod,
    exports: mod.exports,
    require: (n) => (n === "react" ? react : {}),
    localStorage: { getItem: () => null, setItem: () => {} },
    encodeURIComponent,
  });
  return (cache = mod.exports);
};
