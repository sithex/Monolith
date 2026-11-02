import type { Registry } from '../ipc/client';
import type { ModuleAction } from './models';
import { modulesFromRegistry } from '../views/adapters';
const locked = (id: string) => id === 'terminal' || id === 'settings';
/** Same lock rule as the Rust launcher wrapper: Hermes home fixed, Settings never hidden. */
export function registryAction(registry: Registry, action: ModuleAction): Registry {
  const before = [...registry.modules].sort((a,b)=>a.order-b.order);
  const next: Registry = { ...registry, modules: registry.modules.map(m=>({...m})) };
  if (action.type === 'order') {
    if (action.order.length !== before.length || new Set(action.order).size !== before.length || action.order.some(id=>!before.some(m=>m.id===id))) throw Error('Order must be the complete registry permutation');
    next.modules.forEach(m=>{m.order=action.order.indexOf(m.id);});
  } else {
    const m=next.modules.find(m=>m.id===action.id);
    if (!m || locked(m.id) || !modulesFromRegistry(registry).modules.find(v=>v.id===m.id)?.known) throw Error('This module is locked or unknown');
    m.enabled=action.enabled;
  }
  // Hermes home keeps its slot; Settings can't be hidden (enable action above) but floats
  // when other modules move or hide around it — same rule as the Rust launcher wrapper.
  const after=[...next.modules].sort((a,b)=>a.order-b.order);
  for(const [index,m] of before.entries()) if(m.id==='terminal' && after[index]?.id!=='terminal') throw Error('Hermes home stays the first tile');
  return next;
}
