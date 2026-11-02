import type { ModuleSettingsModel, ModuleAction } from '../models';
export function SettingsModules({ model, intents }: { model: ModuleSettingsModel; intents: { modules(action: ModuleAction): void } }) {
  const modules = [...model.modules].sort((a, b) => a.order - b.order || a.id.localeCompare(b.id));
  function move(id: string, delta: number) {
    const order = modules.map(m => m.id); const from = order.indexOf(id); const to = from + delta;
    if (from < 0 || to < 0 || to >= order.length) return;
    const source = order[from], target = order[to];
    if (source === undefined || target === undefined || modules[from]?.locked || modules[to]?.locked) return;
    order[from] = target; order[to] = source;
    intents.modules({ type: 'order', order });
  }
  return <div className="app-stack settings-modules"><p className="muted">Choose your apps · Registry v{model.version}</p><ol className="app-list">{modules.map((module, index) => {
    const locked = module.locked || module.id === 'terminal' || module.id === 'settings';
    return <li className="card module-row" key={module.id}><div className="app-toolbar"><div className="row-content"><strong>{module.label}</strong><small>{locked ? 'Always enabled · Locked' : !module.known ? 'Unknown module · Inert' : module.enabled ? 'Enabled' : 'Hidden'}</small></div><label className="module-toggle"><span className="sr-only">Enable {module.label}</span><input type="checkbox" checked={locked || module.enabled} disabled={locked || !module.known} onChange={e => { if (!locked && module.known) intents.modules({ type: 'enable', id: module.id, enabled: e.target.checked }); }}/></label></div>
      <div className="app-footer"><button aria-label={`Move ${module.label} earlier`} disabled={locked || index === 0 || modules[index-1]?.locked} onClick={() => move(module.id, -1)}>↑ Earlier</button><button aria-label={`Move ${module.label} later`} disabled={locked || index === modules.length - 1 || modules[index+1]?.locked} onClick={() => move(module.id, 1)}>↓ Later</button></div>
      {!module.known && <small className="muted">Opaque configuration preserved; not displayed.</small>}
    </li>;
  })}</ol></div>;
}
