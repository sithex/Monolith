// Compile the actual production TS/TSX graph using the already locked TypeScript.
// No jsdom, copied implementation, generated source mutation or extra dependency.
import { readFileSync, existsSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import vm from 'node:vm';
import ts from 'typescript';
const root = fileURLToPath(new URL('../', import.meta.url));
const cache = new Map();
const nativeRequire = createRequire(import.meta.url);
export function load(path) {
  const file = resolve(root, path);
  if (cache.has(file)) return cache.get(file).exports;
  const source = readFileSync(file, 'utf8');
  const result = ts.transpileModule(source, { reportDiagnostics: true, compilerOptions: {
    target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX,
  } });
  if (result.diagnostics.length) throw Error(ts.formatDiagnosticsWithColorAndContext(result.diagnostics, {
    getCanonicalFileName: f => f, getCurrentDirectory: () => root, getNewLine: () => '\n',
  }));
  const module = { exports: {} }; cache.set(file, module);
  const require = name => {
    if (!name.startsWith('.')) return nativeRequire(name);
    const base = resolve(dirname(file), name);
    const dependency = ['', '.ts', '.tsx', '.js'].map(ext => base + ext).find(existsSync);
    if (!dependency) throw Error(`Unresolved production import ${name} in ${file}`);
    return load(dependency);
  };
  vm.runInThisContext(`(function(require,module,exports){${result.outputText}\n})`, { filename: file })(require, module, module.exports);
  return module.exports;
}
