// Claude-specific in-memory test loader: retain real .ts/.tsx filenames for TS parsing.
// Existing phase7b-loader defaults every input to TSX, which misparses generic arrows.
import { readFileSync, existsSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import vm from 'node:vm';
import ts from 'typescript';
const root=fileURLToPath(new URL('../',import.meta.url));
const nativeRequire=createRequire(import.meta.url),cache=new Map();
export function load(path) {
  const file=resolve(root,path);
  if(cache.has(file))return cache.get(file).exports;
  const result=ts.transpileModule(readFileSync(file,'utf8'),{fileName:file,reportDiagnostics:true,compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,jsx:ts.JsxEmit.ReactJSX}});
  if(result.diagnostics.length)throw Error(ts.formatDiagnosticsWithColorAndContext(result.diagnostics,{getCanonicalFileName:f=>f,getCurrentDirectory:()=>root,getNewLine:()=>'\n'}));
  const module={exports:{}};cache.set(file,module);
  const require=name=>{
    if(!name.startsWith('.'))return nativeRequire(name);
    const base=resolve(dirname(file),name),dep=['','.ts','.tsx','.js'].map(ext=>base+ext).find(existsSync);
    if(!dep)throw Error(`Unresolved ${name} in ${file}`);
    return load(dep);
  };
  vm.runInThisContext(`(function(require,module,exports){${result.outputText}\n})`,{filename:file})(require,module,module.exports);
  return module.exports;
}
