import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
import {frozenXterm} from '../scripts/phase7b-frozen-xterm.mjs';
test('locked real xterm keycode namespace evaluates under frozen Object.prototype without unfreezing it',()=>{
 const source=readFileSync(new URL('../node_modules/@xterm/xterm/lib/xterm.js',import.meta.url),'utf8');
 const match=source.match(/7883:\(e,t\)=>\{([\s\S]*?)\},(?=\d+:)/);assert.ok(match,'locked real keycode module not found');
 const code=`(function(e,t){'use strict';${match[1]}})(module,module.exports);`;
 const context=()=>{const c=vm.createContext({module:{exports:{}},self:{},console,setTimeout,clearTimeout});vm.runInContext('Object.freeze(Object.prototype)',c);return c;};
 assert.throws(()=>vm.runInContext(code,context()),error=>/toString/.test(error.message));
 const normal=vm.createContext({module:{exports:{}}});vm.runInContext(code,normal);
 const c=context();vm.runInContext(frozenXterm(code),c);assert.equal(typeof c.module.exports.KeyCodeUtils.toString,'function');assert.equal(c.module.exports.KeyCodeUtils.toString(3),normal.module.exports.KeyCodeUtils.toString(3));assert.equal(vm.runInContext('Object.isFrozen(Object.prototype)',c),true);
 assert.throws(()=>frozenXterm('no vendor namespace'),/anchor changed/);
});
