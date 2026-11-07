// Standalone Claude components; existing-file integration assertions belong to the parent.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import ts from 'typescript';
import * as React from 'react';
import { createElement as el } from 'react';
import { renderToStaticMarkup as render } from 'react-dom/server';
import { load } from './claude-code-loader.mjs';
const C=load('ui/island/components/ClaudeCodeCard.tsx');
const src=p=>readFileSync(new URL(`../${p}`,import.meta.url),'utf8');
const now=Date.now();
const row=(over={})=>({id:'abc12345-def',short:'abc12345',src:'CLI',project:'Project',model:'Opus',modelId:'claude-opus-5-5',family:'opus',status:'working',statusProv:'exact',statusRaw:null,waitingFor:null,tool:'Bash',since:now-4000,ctx:{used:112000,max:1e6,pct:11.2},tokens:{input:20,output:300,cacheRead:1000,cacheWrite:50},cost:.12,costPartial:false,outEstimated:false,calls:2,hitLast:95,subagents:1,subCost:.03,compactions:0,indexing:null,error:null,...over});
const live=sessions=>({sessions,more:0,mode:'registry',idleHideMin:30,noFolder:false,unavailable:null,registry:'1 live'});
const staticReact=open=>({...React,useState:init=>[typeof init==='boolean'?open:typeof init==='function'?init():init,()=>{}]});
function parts(open=false) {
  const cache=new Map(); const react=staticReact(open);
  function custom(p) {
    if(cache.has(p))return cache.get(p);
    const code=ts.transpileModule(src(p),{fileName:p,compilerOptions:{module:ts.ModuleKind.CommonJS,jsx:ts.JsxEmit.React,esModuleInterop:true,target:ts.ScriptTarget.ES2020}}).outputText;
    const module={exports:{}};
    const req=s=>s==='react'?react:s.startsWith('./')?custom('ui/island/components/'+s.slice(2)+'.tsx'):(()=>{throw Error(s)})();
    vm.runInNewContext(code,{React:react,require:req,module,exports:module.exports,console,Date,Math,Map,Set,window:{setTimeout(){},clearTimeout(){}}},{filename:p});
    cache.set(p,module.exports);return module.exports;
  }
  return custom('ui/island/components/ClaudeCodeCard.tsx');
}
test('card missing/loading/error states distinguish no folder from no session',()=>{
  assert.match(render(el(C.ClaudeCodeCard,{live:null,now})),/Loading/);
  assert.match(render(el(C.ClaudeCodeCard,{live:null,now,error:'offline'})),/unavailable: offline/);
  assert.match(render(el(C.ClaudeCodeCard,{live:{...live([]),noFolder:true},now})),/No Claude Code folder/);
  assert.match(render(el(C.ClaudeCodeCard,{live:live([]),now})),/No live Claude Code session/);
});
test('row collapsed by default, metadata escaped, title never rendered',()=>{
  const {ClaudeCodeCard}=parts(false);
  const html=render(el(ClaudeCodeCard,{live:live([row({project:'<img src=x>',name:'SECRET_TITLE'})]),now}));
  assert.match(html,/aria-expanded="false"/); assert.match(html,/&lt;img src=x&gt;/);assert.doesNotMatch(html,/<img|SECRET_TITLE/);assert.doesNotMatch(html,/al-detail/);
});
test('expanded shows token, context and subscription estimates; no resume control',()=>{
  const {ClaudeCodeCard}=parts(true);
  const html=render(el(ClaudeCodeCard,{live:live([row()]),now}));
  assert.match(html,/aria-expanded="true"/);assert.match(html,/role="progressbar"/);assert.match(html,/112k/);assert.match(html,/subagents/);assert.match(html,/not money spent/);assert.doesNotMatch(html,/copy|resume/i);
});
test('no context means no progressbar, unknown costs remain dash, stale and uncertain statuses',()=>{
  const {ClaudeCodeCard}=parts(true);
  const html=render(el(ClaudeCodeCard,{live:live([row({ctx:null,cost:null})]),now}));
  assert.doesNotMatch(html,/progressbar/);assert.match(html,/—/);
  assert.equal(C.vsCost(row({outEstimated:true,costPartial:true})),'~$0.12+');
  assert.equal(C.vsStatusText(row({statusProv:'stale'})),'idle?');
  assert.equal(C.vsStatusText(row({status:'waiting',waitingFor:'permission',tool:null})),'waiting for you: permission');
});
test('source chips and totals show provenance, call-day semantics and progress',()=>{
  const chips=render(el(C.HistorySourceChips,{sources:[['all','All'],['cli','CLI']],value:'cli',onChange(){}}));
  assert.match(chips,/role="tablist"/);assert.match(chips,/aria-selected="true"/);
  const h={today:{cost:1,tokens:123,sessions:1,estOut:1},week:{cost:3,tokens:1000,sessions:2},yesterday:{cost:2},lastWeek:{cost:2}};
  const html=render(el(C.HistoryTotalsCard,{history:h,claudeSrc:true,progress:{pct:25,indexed:2,files:8,readErrors:1}}));
  assert.match(html,/API-equivalent/);assert.match(html,/Claude Code by call day/);assert.match(html,/indexing Claude Code/);assert.match(html,/could not be read/);
});
test('retained visible poller and shared IPC serialization match newer AgentLive infrastructure',()=>{
  const view=src('ui/views/ClaudeCodeView.tsx'),ipc=src('ui/views/claude-code-ipc.ts');
  assert.match(view,/createVisiblePoller/);assert.match(view,/while \(inFlight\) await/);
  assert.match(view,/if \(!current\(\)\) throw/);assert.match(view,/document\.visibilityState !== 'hidden'/);
  assert.match(view,/poller\.setVisible\(false\)/);assert.match(view,/poller\.dispose/);
  assert.match(view,/s\.more \? MORE_DELAY_MS/);assert.match(view,/nextDelay\(page/);
  assert.match(ipc,/readRaw\(req\)/);
  assert.match(src('ui/ipc/client.ts'),/unwrap\(commands\.claudeCodeRaw\(req\)\)/);
  assert.doesNotMatch(view,/setInterval\(/);
});
test('Claude renderer has no raw HTML or message content fields',()=>{
  const text=src('ui/island/components/ClaudeCodeCard.tsx');
  assert.doesNotMatch(text,/dangerouslySetInnerHTML|innerHTML|\.content\b|\.signature\b|s\.message\b|s\.toolInput\b/);
  assert.match(text,/Read-only Desktop, VS Code, CLI/);
});
