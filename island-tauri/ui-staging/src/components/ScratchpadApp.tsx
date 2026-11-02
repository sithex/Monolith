import { useId } from 'react';
import type { ScratchpadModel } from '../app-models';
import type { IslandIntents } from '../models';
import { Empty } from './AppPrimitives';
export function ScratchpadApp({ model, intents }: { model: ScratchpadModel; intents: Pick<IslandIntents, 'scratchpad'> }) {
  const editorId = useId();
  const notes = [...model.notes].sort((a, b) => Number(b.pinned) - Number(a.pinned) || b.updatedMs - a.updatedMs);
  const note = notes.find(n => n.id === model.selectedId);
  return <div className="app-stack scratchpad-app">
    <div className="app-toolbar"><span className="muted">Plain text · private to the island</span><span className="title-spacer"/><button onClick={() => intents.scratchpad({ type: 'create' })}>New note</button></div>
    <div className="note-workspace"><nav aria-label="Notes" className="note-list">{notes.map(n => <button key={n.id} aria-current={n.id === model.selectedId ? 'true' : undefined} onClick={() => intents.scratchpad({ type: 'select', id: n.id })}><strong>{n.title || 'Untitled'}</strong><small>{n.pinned ? 'Pinned' : 'Note'}</small></button>)}{!notes.length && <p className="muted">No notes yet</p>}</nav>
      {note ? <section className="note-editor"><label htmlFor={editorId}>{note.title || 'Untitled'}</label><textarea id={editorId} aria-label="Note body" value={note.body} onChange={e => intents.scratchpad({ type: 'edit', id: note.id, body: e.target.value })}/><div className="app-footer"><button aria-pressed={note.pinned} onClick={() => intents.scratchpad({ type: 'pin', id: note.id, pinned: !note.pinned })}>{note.pinned ? 'Unpin note' : 'Pin note'}</button><button onClick={() => intents.scratchpad({ type: 'delete', id: note.id })}>Delete note</button></div></section> : <Empty icon="note">Select or create a note</Empty>}
    </div>
    <p role="status" className={model.save.status === 'error' ? 'warning' : 'muted'}>{model.save.status === 'saving' ? 'Saving… · Debounced save pending' : model.save.status === 'saved' ? 'Saved' : `Save failed · ${model.save.message ?? 'Try again'}`}</p>
  </div>;
}
