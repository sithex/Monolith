import type { ShelfModel, ShelfItem } from '../app-presentations';
import { Empty } from './AppPrimitives';
import { Icon } from './Icon';

export type ShelfIntent = { type: 'open' | 'reveal' | 'remove'; id: string } | { type: 'clear' };
type DragEventLike = { dataTransfer: { setData(type: string, value: string): void; effectAllowed: string; dropEffect: string } | null };

/** Drag-out payload: one item per drag (a multi-item uri-list arrives glued in WebKitGTK).
 *  The URI is the one Rust encoded; the page never builds a path itself. */
export function shelfDragStart(item: ShelfItem, event: DragEventLike) {
  if (!event.dataTransfer) return;
  event.dataTransfer.setData('text/uri-list', item.uri);
  event.dataTransfer.setData('text/plain', item.path);
  event.dataTransfer.effectAllowed = 'copy';
}
/** With "remove after drag" on, an item that something outside took leaves the shelf.
 *  Dropping it back on the island counts as "none" (the page refuses file drops). */
export function shelfDragEnd(item: ShelfItem, event: DragEventLike, removeAfterDrag: boolean, shelf: (op: ShelfIntent) => void) {
  if (removeAfterDrag && event.dataTransfer && event.dataTransfer.dropEffect !== 'none') shelf({ type: 'remove', id: item.id });
}

/** The drop shelf: files dragged onto the closed notch, parked as paths only. */
export function ShelfApp({ model, removeAfterDrag, intents, off }: { model: ShelfModel; removeAfterDrag: boolean; off?: React.ReactNode;
  intents: { shelf: (op: ShelfIntent) => void; copy: (path: string) => void } }) {
  const { items } = model;
  return <div className="shelf-app">
    {items.length ? <ul className="app-list">{items.map(item => <li className="file-row card shelf-row" key={item.id} data-present={item.present}
      draggable={item.present} title={item.present ? 'Drag to drop it somewhere else' : undefined}
      onDragStart={event => shelfDragStart(item, event)} onDragEnd={event => shelfDragEnd(item, event, removeAfterDrag, intents.shelf)}>
      <span className="kind-icon tone-green" aria-hidden="true"><Icon name={item.folder ? 'grid' : 'note'}/></span>
      <div className="row-content"><strong>{item.name}</strong><small className="shelf-path">{item.path}</small>{!item.present && <small className="warning">Moved or deleted</small>}</div>
      <div className="row-actions">
        {item.present && <button aria-label={`Open ${item.name}`} onClick={() => intents.shelf({ type: 'open', id: item.id })}>Open</button>}
        {item.present && <button aria-label={`Reveal ${item.name}`} onClick={() => intents.shelf({ type: 'reveal', id: item.id })}>Reveal</button>}
        <button aria-label={`Copy path of ${item.name}`} onClick={() => intents.copy(item.path)}>Copy path</button>
        <button aria-label={`Remove ${item.name} from the shelf`} onClick={() => intents.shelf({ type: 'remove', id: item.id })}>Remove</button>
      </div>
    </li>)}</ul> : <Empty icon="shelf">Drag files onto the closed notch to park them here.</Empty>}
    <div className="shelf-footer">
      <p className="muted app-caption">{model.enabled ? 'Only the paths are kept, never the files. Names with line breaks are refused.' : off}</p>
      {items.length > 0 && <button className="shelf-clear" onClick={() => intents.shelf({ type: 'clear' })}>Clear all</button>}
    </div>
  </div>;
}
