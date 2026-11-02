import {test} from 'node:test';
import assert from 'node:assert/strict';
import {load} from './phase7b-loader.mjs';
const {registryAction}=load('ui/island/launcher-policy.ts');
const registry={version:1,modules:['terminal','timer','settings','mixer','downloads'].map((id,order)=>({id,order,enabled:true,config:{type:id}}))};
test('Settings rejects direct and indirect lock movement and disabling without mutating registry',()=>{
 const pristine=JSON.stringify(registry);
 for(const id of ['terminal','settings'])assert.throws(()=>registryAction(registry,{type:'enable',id,enabled:false}),/locked/);
 assert.throws(()=>registryAction(registry,{type:'order',order:['timer','terminal','settings','mixer','downloads']}),/first tile/);
 // Settings floats: reordering around it and hiding a module in front of it are legal (Nick, Nov 2).
 assert.equal(registryAction(registry,{type:'order',order:['terminal','settings','timer','mixer','downloads']}).modules.find(m=>m.id==='settings').order,1);
 assert.equal(registryAction(registry,{type:'enable',id:'timer',enabled:false}).modules.find(m=>m.id==='timer').enabled,false);
 assert.equal(JSON.stringify(registry),pristine);
 const changed=registryAction(registry,{type:'order',order:['terminal','timer','settings','downloads','mixer']});assert.equal(changed.modules.find(m=>m.id==='downloads').order,3);
 assert.equal(registryAction(registry,{type:'enable',id:'downloads',enabled:false}).modules.find(m=>m.id==='downloads').enabled,false);
});
