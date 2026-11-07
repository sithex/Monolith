// Claude-only model tests; loader transpiles in memory, no build output or real HOME access.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { load } from './claude-code-loader.mjs';
const M = load('ui/views/claude-code-model.ts');
const A = load('ui/views/agent-live-model.ts');
const NOW = Date.parse('2026-11-04T12:00:00Z');
const usage = (over = {}) => ({ input:100, output:50, cacheRead:1000, cacheWrite:20, cw1h:0, fast:false, ...over });
const fact = (over = {}) => ({ t:'a', at:NOW, model:'claude-opus-5-5', key:'m|r', side:false, ep:'cli', cwd:'/w/Proj', stop:'end_turn', tool:null, chars:10, sig:0, u:usage(), ...over });
const file = (key, over = {}) => ({ key, slug:'p', sessionId:key.includes('/subagents/') ? 's1' : key.split('/').pop().replace('.jsonl',''), sub:key.includes('/subagents/'), ino:'11', size:100, mtimeMs:NOW, reset:true, from:0, next:100, facts:[fact()], skippedHuge:0, badLines:0, error:null, ...over });
const raw = (files, registry = [], over = {}) => ({ now:NOW, claudeDir:'/fixture/.claude', folderMissing:false, registry, registryError:null, files, more:false, error:null, ...over });
const reg = (over = {}) => ({ file:'42.json', mtimeMs:NOW, alive:true, procStart:'100', data:{ sessionId:'s1', entrypoint:'cli', cwd:'/w/Proj', startedAt:NOW-1000, procStart:'100', status:'busy', ...over } });
test('stream replacements use final usage once, including one-hour writes and fast price', () => {
  const s = new M.TranscriptScan('p/s.jsonl', false);
  s.onFact(fact({ stop:null, chars:1000, u:usage({ output:2, cw1h:10, fast:true }) }));
  assert.equal(s.agg.output, 440); assert.equal(s.agg.estOut, 1);
  s.onFact(fact({ stop:null, chars:1000, u:usage({ output:3, cw1h:10, fast:true }) }));
  assert.equal(s.agg.output, 880); assert.equal(s.agg.calls, 1);
  s.onFact(fact({ chars:0, u:usage({ output:73, cw1h:10, fast:true }) }));
  assert.equal(s.agg.output, 73); assert.equal(s.agg.estOut, 0); assert.equal(s.agg.calls, 1);
  assert.ok(Math.abs(s.agg.cost - A.costOf('claude-opus-5-5', { input:100, output:73, cacheRead:1000, cacheWrite:10, cacheWrite1h:10 }, true)) < 1e-12);
});
test('thinking signature contributes to estimate; unknown prices stay null', () => {
  const c = M.contribOf('unknown', usage({ output:0 }), null, 0, 100);
  assert.equal(c.output, 21); assert.equal(c.est, true); assert.equal(c.cost, null);
  const s = new M.TranscriptScan('p/s.jsonl', false); s.onFact(fact({ model:'unknown' }));
  assert.equal(M.costOrNull(s.agg), null);
});
test('cross-file overlap excluded, same-file replacement accepted', () => {
  const claims = new Map(), a = new M.TranscriptScan('p/a.jsonl', true, { claims }), b = new M.TranscriptScan('p/b.jsonl', true, { claims });
  a.onFact(fact()); b.onFact(fact());
  assert.equal(a.agg.calls,1); assert.equal(b.agg.calls,0); assert.equal(b.stats.crossDup,1);
  a.onFact(fact({ u:usage({ output:55 }) })); assert.equal(a.agg.calls,1); assert.equal(a.agg.output,55);
  a.releaseClaims(); b.onFact(fact()); assert.equal(b.agg.calls,1);
});
test('compaction clears context; next assistant repopulates; sidechain cannot replace main context', () => {
  const s = new M.TranscriptScan('p/s.jsonl', false); s.onFact(fact());
  assert.equal(s.ctx.used,1120);
  s.onFact({ t:'compact', at:NOW+1 }); assert.equal(s.ctx,null); assert.equal(s.compactions,1);
  s.onFact(fact({ key:'m2', u:usage({ input:1 }) })); assert.equal(s.ctx.used,1021);
  s.onFact(fact({ key:'side', side:true, u:usage({ input:99999 }) })); assert.equal(s.ctx.used,1021); assert.equal(s.agg.calls,3);
});
test('tool names restricted and only names retained', () => {
  const s = new M.TranscriptScan('p/s.jsonl', false);
  s.onFact(fact({ stop:'tool_use', tool:'mcp__a:b.c-d' })); assert.equal(s.tool.name,'mcp__a:b.c-d');
  assert.equal(M.safeToolName('<script>'), 'tool'); assert.equal(M.safeToolName('x'.repeat(65)),'tool');
  s.onFact({ t:'user', at:NOW+1, meta:false, interrupt:true }); assert.equal(s.last.kind,'interrupt');
  s.onFact({ t:'turnEnd', at:NOW+2 }); assert.equal(s.last.kind,'turnEnd');
});
test('cursor desync rejected; reset rereads; partial tail is caught up when more false', () => {
  const s = new M.TranscriptScan('p/s1.jsonl', false);
  s.apply(file('p/s1.jsonl',{ size:120,next:100 }),false); assert.equal(s.caughtUp,true);
  assert.equal(s.apply(file('p/s1.jsonl',{ reset:false,from:99,next:120 })),false);
  assert.equal(s.apply(file('p/s1.jsonl',{ reset:false,from:100,next:120,facts:[] })),true);
  s.apply(file('p/s1.jsonl',{ ino:'12', facts:[fact({ key:'new' })] })); assert.equal(s.agg.calls,1); assert.equal(s.stats.resets,1);
});
test('registry excludes dead, reused, daemons and invalid; legacy identity remains unverified', () => {
  const r = M.readRegistry([reg(),{...reg(),alive:false},reg({procStart:'200'}),reg({kind:'daemon'}),{...reg(),file:'bad.json'}]);
  assert.equal(r.entries.length,1); assert.equal(r.skipped.dead,1); assert.equal(r.skipped.reused,1); assert.equal(r.skipped.daemon,1); assert.equal(r.skipped.invalid,1);
  assert.equal(M.pidIdentity({procStart:null},'100'),'unverified');
  assert.equal(M.parseRegEntry('42.json',{sessionId:'s1',name:'SECRET_TITLE'},NOW).name,null);
});
test('registry statuses do not guess permission state, stale busy becomes uncertain idle', () => {
  const r = M.parseRegEntry('42.json',{sessionId:'s1',status:'waiting',waitingFor:'permission'},NOW);
  assert.equal(M.liveStatus(r,null,NOW,NOW).waitingFor,'permission');
  r.status='new_future_status'; assert.equal(M.liveStatus(r,null,NOW,NOW).prov,'unknown');
  r.status='busy'; assert.equal(M.liveStatus(r,null,0,NOW+M.STALE_BUSY_MS+1).prov,'stale');
  assert.deepEqual(M.inferStatus({kind:'tool',at:NOW},NOW,NOW),{status:'working',prov:'estimate'});
  assert.equal(M.inferStatus(null,0,NOW).prov,'quiet');
});
test('fallback is recent, bounded to eight rows, and cannot resurrect known dead registry sessions', () => {
  const f = new M.ClaudeCodeFeed();
  f.ingest(raw([file('p/s1.jsonl')],[{...reg(),alive:false}])); assert.equal(f.liveView().sessions.length,0);
  f.ingest(raw([file('p/s1.jsonl')])); assert.equal(f.liveView().sessions.length,0);
  const g = new M.ClaudeCodeFeed();
  g.ingest(raw(Array.from({length:12},(_,i)=>file(`p/s${i}.jsonl`,{facts:[fact({ key:`k${i}` })]}))));
  assert.equal(g.liveView().sessions.length,8); assert.equal(g.liveView().more,4);
  assert.equal(g.liveView(NOW+31*60e3).sessions.length,0);
});
test('registry entrypoint falls back to transcript, tool hidden after reply, idle rows hidden', () => {
  const f = new M.ClaudeCodeFeed();
  f.ingest(raw([file('p/s1.jsonl',{facts:[fact({ep:'claude-vscode',stop:'tool_use',tool:'Bash'})]})],[reg({entrypoint:'future'})]));
  let row=f.liveView().sessions[0]; assert.equal(row.src,'VS Code'); assert.equal(row.tool,'Bash'); assert.equal(row.ctx.used,1120);
  f.ingest(raw([file('p/s1.jsonl',{reset:false,from:100,next:101,size:101,facts:[fact({key:'end'})]})],[reg({status:'idle'})]));
  row=f.liveView().sessions[0]; assert.equal(row.tool,null);
  assert.equal(f.liveView(NOW+31*60e3).sessions.length,0);
});
test('main plus subagent counts once; orphan subagents become history, not live main rows', () => {
  const f = new M.ClaudeCodeFeed();
  f.ingest(raw([file('p/s1.jsonl'),file('p/s1/subagents/agent-a.jsonl',{facts:[fact({key:'sub'})]})]));
  const row=f.liveView().sessions[0]; assert.equal(row.calls,2); assert.equal(row.subagents,1); assert.equal(row.ctx.used,1120);
  const sessions=f.sessions(); assert.equal(sessions.length,1); assert.equal(sessions[0].sub.calls,1);
  const orphan=new M.ClaudeCodeFeed(); orphan.ingest(raw([file('p/s1/subagents/agent-a.jsonl',{facts:[fact({key:'sub',ep:'claude-desktop'})]})]));
  assert.equal(orphan.liveView().sessions.length,0); assert.equal(orphan.sessions()[0].src,'desktop');
});
test('more yields early without ghost live rows, progress and cursors retained, removed files drop history', () => {
  const f = new M.ClaudeCodeFeed();
  f.ingest(raw([file('p/s1.jsonl',{facts:[],next:0})],[],{more:true}));
  assert.equal(f.liveView().sessions.length,0); assert.equal(f.progress().pct,0);
  assert.deepEqual(f.request().cursors,[{key:'p/s1.jsonl',ino:'11',off:0}]);
  f.ingest(raw([file('p/s1.jsonl',{reset:false,from:0})])); assert.equal(f.progress().pct,100); assert.equal(f.sessions().length,1);
  f.ingest(raw([])); assert.equal(f.sessions().length,0);
});
test('read errors are surfaced and do not erase history during top-level scan failure', () => {
  const f = new M.ClaudeCodeFeed(); f.ingest(raw([file('p/s1.jsonl')]));
  f.ingest(raw([],[],{error:'projects unreadable'})); assert.equal(f.sessions().length,1);
  f.ingest(raw([file('p/s1.jsonl',{reset:false,from:100,facts:[],error:'permission denied'})]));
  assert.equal(f.progress().readErrors,1); assert.equal(f.liveView().sessions[0].error,'permission denied');
});
test('history uses call days, sources, and null copy ids while preserving Hermes fields', () => {
  const f=new M.ClaudeCodeFeed(); f.ingest(raw([file('p/s1.jsonl',{facts:[fact({ep:'claude-desktop'})]})]));
  const cc=M.claudeHistoryRaw(f.sessions(),'desktop',NOW); assert.equal(cc.top[0].copy,null); assert.equal(cc.top[0].src,'Desktop');
  const h={days:[{d:M.localDay(NOW),sessions:1,tokens:10,cost:2,subCost:0,calls:1,unpriced:0,estOut:0}],models:[],top:[{id:'H',title:'Hermes',cost:2,segments:3}],savings:1};
  const merged=M.mergeHistoryRaw([{raw:h,src:'Hermes'},{raw:cc,src:'Desktop'}]);
  assert.equal(merged.days[0].sessions,2); assert.equal(merged.top.find(t=>t.id==='H').copy,0); assert.equal(merged.top.find(t=>t.id==='H').segments,3);
  const variants=M.buildHistoryVariants(h,f.sessions(),[],NOW); assert.ok(variants.all && variants.hermes && variants.desktop); assert.equal(variants.cli,null);
});
test('context limits match supported families; map keys cannot pollute prototypes', () => {
  assert.equal(M.contextFor('claude-haiku-4-5',1e6),200000); assert.equal(M.contextFor('claude-sonnet-4-6',1e6),1e6);
  assert.equal(M.contextFor('claude-sonnet-4[1m]',1e6),1e6); assert.equal(M.contextFor('haiku',123),123);
  const s=new M.TranscriptScan('p/s.jsonl',false); s.onFact(fact({model:'__proto__'}));
  assert.equal(s.agg.calls,1); assert.equal(s.agg.byModel.__proto__.calls,1); assert.equal({}.calls,undefined);
});
