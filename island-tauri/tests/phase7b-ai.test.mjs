import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import ts from 'typescript';
import * as React from 'react';
import { load } from './phase7b-loader.mjs';
const { dashboardFromUsage, insightsFromUsage } = load('ui/views/adapters.ts');
const { AiInsights } = load('ui/island/components/AiInsights.tsx');
const { AiDashboard } = load('ui/island/components/AiDashboard.tsx');
const now = Date.parse('2026-11-02T15:00:00Z');
const summary = { estimatedCostUsd: 12, totalTokens: 1000, cacheReadShare: .91, partialCost: false,
  byProvider: { claude: { costUsd: 9 }, codex: { costUsd: 3 }, other: { costUsd: null } },
  models: [{name:'Opus', family:'claude', estimatedCostUsd:9}], projects:[{name:'Ripple', family:'codex',estimatedCostUsd:12}] };
const fixture = () => ({ account:{ accounts:[
  {provider:'anthropic',plan:'Max 5×',windows:[{label:'Session',used_percent:63,resets_at:'2026-11-02T17:12:00Z'},{label:'Weekly',used_percent:42,resets_at:'2026-11-05T18:00:00Z'}]},
  {provider:'openai-codex',plan:'Plus',windows:[{label:'Session',used_percent:78,resets_at:'2026-11-02T18:39:00Z'},{label:'Weekly',used_percent:17,resets_at:'2026-11-07T16:00:00Z'}]},
]}, local:{available:true,dashboard:{periodBasis:'session_started_at',periods:{today:summary,week:{...summary,estimatedCostUsd:40},month:{...summary,estimatedCostUsd:99}},
  hours:Array.from({length:24},(_,i)=>({startS:now/1000-15*3600+i*3600,estimatedCostUsd:i===10?12:null,totalTokens:i===10?1000:null,sessions:i===10?1:0,byProvider:summary.byProvider})),
  days:Array.from({length:91},(_,i)=>({date:new Date(now-(90-i)*86400000).toISOString().slice(0,10),estimatedCostUsd:i%3?null:i,sessions:i%3?0:1,byProvider:{}})),
  active:[{sessionId:'s',project:'Ripple',model:'gpt-6.1-sol',family:'codex',startedAtS:now/1000-3960,lastActivityAtS:now/1000-10,outputTokens:14000,costUsd:2.52}],
  activity:{estimatedCostUsd:1182,activeDays:49,streak:12,busiestDay:{date:'2026-09-02',costUsd:41.96}},
}} });
const render = (usage, period='today') => renderToStaticMarkup(createElement(AiDashboard, {dashboard:dashboardFromUsage(usage,period,now),period,now}));
function walk(node, result=[]) {
  if (!node || typeof node !== 'object') return result;
  if (Array.isArray(node)) { node.forEach(n=>walk(n,result)); return result; }
  if (typeof node.type === 'function') return walk(node.type(node.props),result);
  result.push(node); walk(node.props?.children,result); return result;
}

test('AI dashboard matches provider, spending, trend, shares and thirteen-week card structure',()=>{
  const html=render(fixture());
  for(const title of ['Claude','Codex','Spending','Now','Trend','Models','Projects','Activity']) assert.match(html,new RegExp(title));
  assert.match(html,/Max 5×/); assert.match(html,/Plus/); assert.match(html,/2h 12m/);
  assert.match(html,/91% from cache/); assert.match(html,/1:06/); assert.match(html,/14K written/);
  assert.match(html,/Session-start cohorts, not per-call calendar spend/);
  assert.equal((html.match(/class="ai-bar-slot"/g)||[]).length,24);
  assert.equal((html.match(/title="[A-Z][a-z]{2} \d+ ·/g)||[]).length,91);
  assert.doesNotMatch(html,/🔥/);
});
test('AI period selection changes spend models projects and daily trend, not provider limits',()=>{
  const d=fixture();
  assert.equal(dashboardFromUsage(d,'today',now).value.cost,12);
  assert.equal(dashboardFromUsage(d,'week',now).value.cost,40);
  assert.equal(dashboardFromUsage(d,'month',now).value.cost,99);
  assert.equal(dashboardFromUsage(d,'week',now).value.buckets.length,7);
  assert.equal(dashboardFromUsage(d,'month',now).value.buckets.length,30);
  assert.deepEqual(dashboardFromUsage(d,'week',now).value.accounts,dashboardFromUsage(d,'today',now).value.accounts);
});
test('AI provider cards retain real sign-in intent and spending selector handlers',()=>{
  const selected=[],signed=[];
  const nodes=walk(AiDashboard({dashboard:dashboardFromUsage({account:{},local:{}},'today',now),period:'today',onPeriodChange:p=>selected.push(p),onConnect:p=>signed.push(p),now}));
  nodes.find(n=>n.type==='select').props.onChange({target:{value:'week'}});
  nodes.filter(n=>n.type==='button').forEach(n=>n.props.onClick());
  assert.deepEqual(selected,['week']); assert.deepEqual(signed,['anthropic','openai-codex']);
});
test('AI missing sources show unavailable prices and disconnected providers, never fabricated zeros',()=>{
  const html=render({account:{},local:{available:false}});
  assert.match(html,/Not connected/); assert.match(html,/No active sessions/);
  assert.match(html,/—/); assert.doesNotMatch(html,/\$0\.00|0%|0 tokens|0 written/);
  assert.equal(dashboardFromUsage(null).available,false);
});
test('AI stale active sessions disappear from cached data without another fetch',()=>{
  const d=fixture();
  assert.equal(dashboardFromUsage(d,'today',now).value.running.length,1);
  assert.equal(dashboardFromUsage(d,'today',now+181000).value.running.length,0);
  assert.equal(dashboardFromUsage(d,'today',now-20000).value.running.length,0);
});
test('AI unknown prices stay null and invalid numbers cannot poison charts',()=>{
  const d=fixture(); d.local.dashboard.periods.today={...summary,estimatedCostUsd:NaN,totalTokens:-1,cacheReadShare:null};
  const projected=dashboardFromUsage(d,'today',now).value;
  assert.equal(projected.cost,null); assert.equal(projected.tokens,null); assert.equal(projected.cacheShare,null);
  assert.doesNotMatch(render(d),/NaN|Infinity/);
});
test('AI visible polling is sixty seconds, starts on every activation and guards stale timer callbacks',()=>{
  const source=readFileSync(new URL('../ui/views/AiStatsView.tsx',import.meta.url),'utf8');
  assert.match(source,/useGamePollingActive\(active\)/);
  assert.match(source,/if \(!active\) return/);
  assert.match(source,/createVisiblePoller\([\s\S]*60_000/);
  assert.match(source,/poller.setVisible\(active\)/);
  assert.match(source,/poller.dispose\(\)/);
  assert.doesNotMatch(source,/active && !data/);
});
test('AI runtime polling never fetches hidden, coalesces flights and refreshes every reopen',async()=>{
  const {createTopic}=load('ui/stores/topic.ts'), store=createTopic(); store.seed(null,0);
  const slots=[],effects=[],timers=new Map(),reads=[];let index=0,nextTimer=0,game=false;
  const hooks={...React,
    useState(initial){const i=index++;slots[i]??={value:typeof initial==='function'?initial():initial};return [slots[i].value,v=>{slots[i].value=typeof v==='function'?v(slots[i].value):v;}];},
    useRef(initial){const i=index++;return slots[i]??={current:initial};},
    useCallback(fn){const i=index++;return (slots[i]??={fn}).fn;},
    useEffect(fn,deps){const i=index++,old=slots[i];if(!old||deps.some((v,n)=>old.deps[n]!==v)){slots[i]={deps};effects.push(()=>{old?.cleanup?.();slots[i].cleanup=fn();});}},
  };
  const source=readFileSync(new URL('../ui/views/AiStatsView.tsx',import.meta.url),'utf8');
  const js=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022,jsx:ts.JsxEmit.ReactJSX}}).outputText;
  const module={exports:{}};
  const fakeWindow={setInterval(fn,ms){const id=++nextTimer;timers.set(id,{fn,ms});return id;},clearInterval(id){timers.delete(id);},setTimeout(fn,ms){const id=++nextTimer;timers.set(id,{fn:()=>{timers.delete(id);fn();},ms});return id;},clearTimeout(id){timers.delete(id);}};
  const require=name=>{
    if(name==='react')return hooks;
    if(name==='react/jsx-runtime')return {jsx:React.createElement,jsxs:React.createElement};
    if(name==='../ipc/client')return {aiUsage:()=>new Promise((resolve,reject)=>reads.push({resolve,reject})),aiConnect:async()=>({launched:true})};
    if(name==='../stores')return {useAi:()=>store.getSnapshot(),aiStore:store};
    if(name==='./adapters')return {dashboardFromUsage,insightsFromUsage};
    if(name==='./visible-poller')return load('ui/views/visible-poller.ts');
    if(name==='./game-polling')return {useGamePollingActive:active=>load('ui/views/game-polling-policy.ts').gamePollingActive(active,game,true)};
    if(name.endsWith('.css'))return {};
    if(name.endsWith('/AiDashboard'))return {AiDashboard};
    if(name.endsWith('/AiInsights'))return {AiInsights};
    throw Error(name);
  };
  vm.runInNewContext(`(function(require,module,exports){${js}\n})`,{window:fakeWindow,Date})(require,module,module.exports);
  const render=active=>{index=0;module.exports.AiStatsView({active});while(effects.length)effects.shift()();};
  const tick=()=>new Promise(resolve=>setImmediate(resolve));
  try {
    render(false);assert.equal(reads.length,0);assert.equal(timers.size,0);
    render(true);assert.equal(reads.length,1);
    render(true);assert.equal(reads.length,1,'one flight');
    reads[0].resolve(fixture());await tick();assert.equal(store.getSnapshot().local.available,true);
    const firstPoll=[...timers.values()].find(t=>t.ms===60000).fn;
    firstPoll();assert.equal(reads.length,2);reads[1].resolve(fixture());await tick();
    render(false);assert.equal(timers.size,0);firstPoll();assert.equal(reads.length,2,'queued hidden callback is inert');
    render(true);assert.equal(reads.length,3,'refresh cached data on reopen');reads[2].resolve(fixture());await tick();
    game=true;render(true);assert.equal(timers.size,0,'game pause stops poll and clock');
    game=false;render(true);assert.equal(reads.length,4,'game resume immediately refreshes');reads[3].resolve(fixture());await tick();
    const finalPoll=[...timers.values()].find(t=>t.ms===60000).fn;
    for(const slot of slots)slot?.cleanup?.();finalPoll();assert.equal(reads.length,4,'unmounted callback is inert');
  } finally {for(const slot of slots)slot?.cleanup?.();}
});

const insightPeriod = { sessions: 12, usedSessions: 10, emptySessions: 2, subagentSessions: 3, pricedSessions: 4, includedSessions: 6, unpricedSessions: 0,
  costUsd: 20, avgCostUsd: 5, medianCostUsd: 4.5, maxCostUsd: 9, costPerCallUsd: 0.05, apiCalls: 400, messages: 900, toolCalls: 500,
  inputTokens: 1000, outputTokens: 2000, cacheReadTokens: 95000, cacheWriteTokens: 4000, reasoningTokens: 50, promptTokens: 100000, cacheHitRate: 0.95,
  avgDurationS: 3900, totalDurationS: 39000, sources: [{ name: 'tui', sessions: 7, costUsd: 12 }, { name: 'subagent', sessions: 3, costUsd: null }],
  topSessions: [{ title: 'Big refactor', project: 'Monolith', model: 'claude-opus-5-5', family: 'claude', source: 'tui', subagent: false,
    estimatedCostUsd: 9, promptTokens: 80000, outputTokens: 1500, apiCallCount: 120, durationS: 5400, startedAtS: now / 1000 - 6000 }] };
const withInsights = () => { const d = fixture(); d.local.insights = { periodBasis: 'session_started_at', periods: { today: insightPeriod, week: { ...insightPeriod, avgCostUsd: 7 }, month: insightPeriod } }; return d; };
test('AI sessions tab shows per-session cost, cache share, workload and priciest sessions', () => {
  const html = renderToStaticMarkup(createElement(AiInsights, { insights: insightsFromUsage(withInsights(), 'today'), period: 'today' }));
  for (const t of ['Per session', 'Prompt cache', 'Workload', 'Priciest sessions', 'Where sessions run']) assert.match(html, new RegExp(t));
  assert.match(html, /\$5\.00/); assert.match(html, /95%/); assert.match(html, /Big refactor/); assert.match(html, /Terminal UI/);
  assert.match(html, /1h 5m/); assert.match(html, /Subscription included/);
  assert.equal(insightsFromUsage(withInsights(), 'week').value.avgCost, 7);
});
test('AI sessions tab never invents numbers when insights are missing or hostile', () => {
  assert.equal(insightsFromUsage(null).available, false);
  assert.equal(insightsFromUsage(fixture()).available, false);
  const d = withInsights(); d.local.insights.periods.today = { avgCostUsd: -1, cacheHitRate: 7, topSessions: 'x', sources: null };
  const v = insightsFromUsage(d, 'today').value;
  assert.equal(v.avgCost, null); assert.equal(v.cacheHit, 1); assert.deepEqual(v.top, []); assert.deepEqual(v.sources, []);
  const html = renderToStaticMarkup(createElement(AiInsights, { insights: insightsFromUsage(d, 'today'), period: 'today' }));
  assert.match(html, /No priced sessions/); assert.doesNotMatch(html, /NaN|undefined/);
});
