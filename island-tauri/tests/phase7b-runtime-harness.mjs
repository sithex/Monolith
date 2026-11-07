import assert from 'node:assert/strict';
import { readFileSync, existsSync, statSync } from 'node:fs';
import { createRequire } from 'node:module';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import vm from 'node:vm';
import ts from 'typescript';
import * as React from 'react';
const root=fileURLToPath(new URL('../',import.meta.url));
const native=createRequire(import.meta.url);
export const tick=()=>new Promise(resolve=>setImmediate(resolve));
export const deferred=()=>{let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b;});return {promise,resolve,reject};};
// A deterministic hook/effect driver for production handlers; not a browser/GUI claim.
export function driver(overrides={}, transform=(_path, source)=>source) {
  const slots=[],effects=[],cache=new Map();let index=0;
  const hooks={...React,
    useState(initial){const i=index++;slots[i]??={value:typeof initial==='function'?initial():initial};return [slots[i].value,next=>{slots[i].value=typeof next==='function'?next(slots[i].value):next;}];},
    useRef(initial){const i=index++;return slots[i]??=( {current:initial} );},
    useSyncExternalStore(_subscribe,get){index++;return get();},
    useEffect(effect,deps){const i=index++,previous=slots[i];if(!previous||deps.some((v,n)=>v!==previous.deps[n])){slots[i]={deps};effects.push(()=>{previous?.cleanup?.();slots[i].cleanup=effect();});}},
    useId(){index++;return 'fixture-id';},
    useMemo(factory){index++;return factory();},
  };
  hooks.useLayoutEffect=hooks.useEffect;
  function compile(path){const file=resolve(root,path),relative=file.slice(root.length);if(relative in overrides)return overrides[relative];if(cache.has(file))return cache.get(file).exports;
    if(file.endsWith('.css'))return {};
    const source=transform(relative,readFileSync(file,'utf8')),out=ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,jsx:ts.JsxEmit.ReactJSX}}).outputText;
    const module={exports:{}};cache.set(file,module);
    const require=name=>{if(name==='react')return hooks;if(name in overrides)return overrides[name];if(!name.startsWith('.'))return native(name);const base=resolve(dirname(file),name),p=['','.ts','.tsx','.js','/index.ts'].map(ext=>base+ext).find(p=>existsSync(p)&&statSync(p).isFile());assert.ok(p,`Unresolved ${name}`);return compile(p);};
    vm.runInThisContext(`(function(require,module,exports){${out}\n})`,{filename:file})(require,module,module.exports);return module.exports;
  }
  return {load:compile,render(C,props){index=0;const tree=C(props);while(effects.length)effects.shift()();return tree;},dispose(){for(const slot of slots)slot?.cleanup?.();}};
}
export function nodes(tree){if(!tree||typeof tree!=='object')return [];if(Array.isArray(tree))return tree.flatMap(nodes);return [tree,...nodes(tree.props?.children)];}
// All globals below are private DOM fixtures, restored after each synchronous test.
export function globals(values, run) {
  const before=Object.fromEntries(Object.keys(values).map(k=>[k,Object.getOwnPropertyDescriptor(globalThis,k)]));
  try {for(const [k,v] of Object.entries(values))Object.defineProperty(globalThis,k,{value:v,writable:true,configurable:true});return run();}
  finally {for(const [k,v] of Object.entries(before))if(v)Object.defineProperty(globalThis,k,v);else delete globalThis[k];}
}
export const component=(tree,name)=>nodes(tree).find(n=>n.type?.name===name);
export const labeled=(tree,label)=>nodes(tree).find(n=>n.props?.['aria-label']===label);
export const textButton=(tree,text)=>nodes(tree).find(n=>n.type==='button'&&n.props.children===text);
