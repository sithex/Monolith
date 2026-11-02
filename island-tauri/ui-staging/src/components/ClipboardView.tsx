import type { ClipboardModel } from '../app-models';
import type { IslandIntents } from '../models';
import { Empty, formatSize } from './AppPrimitives';
import { Icon } from './Icon';
export function ClipboardView({ model, intents }: { model: ClipboardModel; intents: Pick<IslandIntents, 'clipboard'> }) {
  // Defense in depth at a hand-mirrored boundary. Normal summaries cannot carry
  // concealed content: the service drops sensitive offers before ingestion.
  const items = model.items.filter(item => !('concealed' in item && item.concealed === true));
  return <div className="app-stack clipboard-view"><div className="app-toolbar"><span className="muted">Private history</span><span className="title-spacer"/><button aria-pressed={model.paused} onClick={() => intents.clipboard({ type: 'pause', paused: !model.paused })}>{model.paused ? 'Resume capture' : 'Pause capture'}</button></div>
    {model.paused && <p role="status" className="app-banner">Capture paused · Existing history is still available</p>}
    {items.length ? <ul className="app-list">{[...items].sort((a, b) => b.copiedAt - a.copiedAt).map(item => <li className="card clip-row" key={item.id}><div className="app-toolbar"><span className="kind-icon" aria-label={`${item.kind} clipboard entry`}><Icon name={item.kind === 'image' ? 'camera' : item.kind === 'files' ? 'download' : 'clipboard'}/></span><small className="muted">{item.kind} · {formatSize(item.bytes)} · {item.ageLabel}{item.sourceApp && ` · ${item.sourceApp}`}</small></div><p className="clip-preview">{item.preview}</p><div className="app-footer"><button aria-label={`Copy entry ${item.id}`} onClick={() => intents.clipboard({ type: 'copy', id: item.id })}>Copy</button><span className="title-spacer"/><button aria-label={`${item.pinned ? 'Unpin' : 'Pin'} entry ${item.id}`} aria-pressed={item.pinned} onClick={() => intents.clipboard({ type: 'pin', id: item.id, pinned: !item.pinned })}>{item.pinned ? 'Pinned' : 'Pin'}</button><button aria-label={`Delete entry ${item.id}`} onClick={() => intents.clipboard({ type: 'delete', id: item.id })}>Delete</button></div></li>)}</ul> : <Empty icon="clipboard">Clipboard history is empty</Empty>}
  </div>;
}
