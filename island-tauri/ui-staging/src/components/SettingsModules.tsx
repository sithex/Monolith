import type { ModuleSettingsModel } from '../app-models';
import type { IslandIntents } from '../models';
export function SettingsModules({ model, intents }: { model: ModuleSettingsModel; intents: Pick<IslandIntents, 'modules'> }) {
  const modules = [...model.modules].sort((a, b) => a.order - b.order || a.id.localeCompare(b.id));
  function move(id: string, delta: number) {
    const order = modules.map(m => m.id); const from = order.indexOf(id); const to = from + delta;
    if (from < 0 || to < 0 || to >= order.length) return;
    [order[from], order[to]] = [order[to], order[from]];
    intents.modules({ type: 'order', order });
  }
  return <div className="app-stack settings-modules"><p className="muted">Choose your apps · Registry v{model.version}</p><ol className="app-list">{modules.map((module, index) => {
    const locked = module.locked || module.id === 'terminal' || module.id === 'settings';
    return <li className="card module-row" key={module.id}><div className="app-toolbar"><div className="row-content"><strong>{module.label}</strong><small>{locked ? 'Always enabled · Locked' : !module.known ? 'Unknown module · Inert' : module.enabled ? 'Enabled' : 'Hidden'}</small></div><label className="module-toggle"><span className="sr-only">Enable {module.label}</span><input type="checkbox" checked={locked || module.enabled} disabled={locked || !module.known} onChange={e => { if (!locked && module.known) intents.modules({ type: 'enable', id: module.id, enabled: e.target.checked }); }}/></label></div>
      <div className="app-footer"><button aria-label={`Move ${module.label} earlier`} disabled={index === 0} onClick={() => move(module.id, -1)}>↑ Earlier</button><button aria-label={`Move ${module.label} later`} disabled={index === modules.length - 1} onClick={() => move(module.id, 1)}>↓ Later</button><span className="title-spacer"/><button disabled={!module.known} aria-label={`${module.label} settings`} onClick={() => intents.modules({ type: 'settings', id: module.id })}>Settings…</button></div>
      <small className="muted">{module.known ? 'Per-module settings adapter not connected in this workshop.' : 'Opaque configuration preserved by the registry; not displayed.'}</small>
    </li>;
  })}</ol></div>;
}
