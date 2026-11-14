// Ported from ui-staging; generated-field presentation model replaces mirrored DTOs.
import type { AppIntents, ClipboardModel } from '../app-models';
import { Empty, ServiceNotice, formatSize } from './AppPrimitives';
import { Icon } from './Icon';
export function ClipboardView({ model, intents }: { model: ClipboardModel; intents: Pick<AppIntents, 'clipboard'> }) {
  // Defense in depth. Rust rejects sensitive offers before a summary exists.
  const items = model.items.filter(item => !('concealed' in item && item.concealed === true));
  return <div className="app-stack clipboard-view" data-testid="clipboard-runtime"><div className="app-toolbar"><span className="muted">Private history</span><span className="title-spacer"/><button disabled={model.status === null || model.status?.enabled === false || model.status?.available === false} aria-pressed={model.paused} onClick={() => intents.clipboard({ type: 'pause', paused: !model.paused })}>{model.paused ? 'Resume capture' : 'Pause capture'}</button></div>
    {model.query !== undefined && <input aria-label="Search clipboard history" placeholder="Search clipboard" type="search" maxLength={4096} value={model.query} onChange={e => intents.clipboard({ type: 'search', query: e.target.value })}/>}
    {model.status === null ? <p role="status">Clipboard status unavailable</p> : model.status && (!model.status.available || !model.status.enabled) && <ServiceNotice icon="clipboard" source="clipboard" reason={!model.status.enabled ? 'disabled' : model.status.detail}/>}
    {model.loading && <p role="status">Loading history…</p>}{model.error && <p role="alert">{model.error}</p>}
    {model.paused && <p role="status" className="app-banner">Capture paused · Existing history is still available</p>}
    {items.length ? <ul className="app-list">{[...items].sort((a, b) => b.copiedAt - a.copiedAt).map(item => <li className="card clip-row" key={item.id}><div className="app-toolbar"><span className="kind-icon" aria-label={`${item.kind} clipboard entry`}><Icon name={item.kind === 'image' ? 'camera' : item.kind === 'files' ? 'download' : 'clipboard'}/></span><small className="muted">{item.kind} · {formatSize(item.bytes)} · {item.ageLabel}{item.sourceApp && ` · ${item.sourceApp}`}</small></div><p className="clip-preview">{item.preview}</p><div className="app-footer"><button aria-label={`Copy entry ${item.id}`} onClick={() => intents.clipboard({ type: 'copy', id: item.id })}>Copy</button><span className="title-spacer"/><button aria-label={`${item.pinned ? 'Unpin' : 'Pin'} entry ${item.id}`} aria-pressed={item.pinned} onClick={() => intents.clipboard({ type: 'pin', id: item.id, pinned: !item.pinned })}>{item.pinned ? 'Pinned' : 'Pin'}</button><button aria-label={`Delete entry ${item.id}`} onClick={() => intents.clipboard({ type: 'delete', id: item.id })}>Delete</button></div></li>)}</ul> : <Empty icon="clipboard">Clipboard history is empty</Empty>}
  </div>;
}
