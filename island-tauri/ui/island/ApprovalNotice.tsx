import { useEffect, useRef, useState, type MouseEvent, type ReactElement } from 'react';
import './approval-notice.css';
import { dispatch, useIslandState } from '../ipc/island-state';
import { answerApproval, useApproval } from '../ipc/approvals';
import { ARM_DELAY_MS, CHOICE_HINT, HOVER_WIDTH, KIND_LABEL, PLAIN_HOVER_HEIGHT, PLAIN_HOVER_WIDTH, canPress, needsExpand, noticeFor, noticeHeight, type NoticeView } from './approval-model';

// "Show all" is local to the notice, but the hover surface must grow with it, so the flag
// lives at module level where IslandSurface can read it (useApprovalHover).
let expandedFor: string | null = null;
const listeners = new Set<() => void>();
const setExpanded = (id: string | null) => { if (expandedFor !== id) { expandedFor = id; listeners.forEach(l => l()); } };
function useExpansionTick() {
  const [, force] = useState(0);
  useEffect(() => { const l = () => force(n => n + 1); listeners.add(l); return () => { listeners.delete(l); }; }, []);
}

/** Hover surface size for the current notice (the plain preview size when there is none). */
export function useApprovalHover(): { active: boolean; width: number; height: number } {
  const change = useApproval();
  useExpansionTick();
  const view = noticeFor(change);
  if (!view) return { active: false, width: PLAIN_HOVER_WIDTH, height: PLAIN_HOVER_HEIGHT };
  const id = view.kind === 'prompt' ? view.prompt.id : null;
  return { active: true, width: HOVER_WIDTH, height: noticeHeight(view, id !== null && expandedFor === id) };
}

/** Pending Hermes approval/clarify question in the hover notice. Nothing here decides for
 *  the user: buttons are explicit pointer clicks and Rust re-checks every guard per answer. */
export function ApprovalNotice({ fail }: { fail(error: unknown): void }) {
  const state = useIslandState(), change = useApproval(), view = noticeFor(change);
  const [pending, setPending] = useState(false), [armedId, setArmedId] = useState<string | null>(null);
  const shownId = view?.kind === 'prompt' ? view.prompt.id : null;
  const armedAt = useRef(0);
  useExpansionTick();
  useEffect(() => {
    armedAt.current = performance.now() + ARM_DELAY_MS; setPending(false); setArmedId(null);
    if (shownId === null) return;
    // Mirror the guard's deadline; a replaced prompt must never inherit a bright button.
    const timer = setTimeout(() => setArmedId(shownId), Math.max(0, Math.ceil(armedAt.current - performance.now())));
    return () => clearTimeout(timer);
  }, [shownId]);
  // Esc closes the notice (presentation only, like the × button); no key ever answers.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') { e.preventDefault(); void dispatch({ type: 'dismissPreview', rev: state.rev }).catch(fail); } };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [state.rev, fail]);
  if (!view) return null;
  const dismiss = <button className="approval-dismiss" aria-label="Dismiss" title="Dismiss (Esc)" onClick={e => {
    e.stopPropagation(); void dispatch({ type: 'dismissPreview', rev: state.rev }).catch(fail);
  }}>×</button>;
  const openTerminal = <button className="approval-open" data-testid="approval-open" onClick={() => {
    void dispatch({ type: 'selectView', view: 'terminal' }).catch(fail);
  }}>Open terminal ↗</button>;
  if (view.kind === 'waiting') return <div className="approval-notice approval-waiting" role="status">
    <div className="approval-head"><strong>Hermes is waiting for you</strong>{dismiss}</div>
    <p className="approval-note">This prompt can only be answered in the terminal.</p>
    <div className="approval-actions">{openTerminal}</div>
  </div>;
  return <Prompt view={view} pending={pending} armed={armedId === shownId} armedAt={armedAt} setPending={setPending} dismiss={dismiss} openTerminal={openTerminal} fail={fail} />;
}

function Prompt({ view, pending, armed, armedAt, setPending, dismiss, openTerminal, fail }: {
  view: Extract<NoticeView, { kind: 'prompt' }>; pending: boolean; armed: boolean; armedAt: { current: number };
  setPending(v: boolean): void; dismiss: ReactElement; openTerminal: ReactElement; fail(error: unknown): void;
}) {
  const p = view.prompt, expanded = expandedFor === p.id, long = needsExpand(p.text);
  const press = (e: MouseEvent, choiceId: string, style: string) => {
    e.stopPropagation();
    if (!canPress({ style, detail: e.detail, armedAt: armedAt.current, now: performance.now(), prompt: p, pending })) return;
    setPending(true);
    answerApproval(p.id, choiceId).catch(error => { setPending(false); fail(error); });
  };
  return <div className={`approval-notice approval-${p.kind}`} role="alertdialog" aria-label={KIND_LABEL[p.kind]} data-prompt={p.id}>
    <div className="approval-head"><strong>{KIND_LABEL[p.kind]}</strong>{p.more > 0 && <span className="approval-more">+{p.more} more</span>}{dismiss}</div>
    <pre className={`approval-text${expanded ? ' expanded' : ''}`} data-testid="approval-text">{p.text}</pre>
    {(long || p.textTruncated) && <button className="approval-expand" onClick={() => setExpanded(expanded ? null : p.id)}>{expanded ? 'Show less' : 'Show all'}</button>}
    {p.description && <p className="approval-desc">{p.description}</p>}
    {p.answerable
      ? <div className={`approval-actions choices-${p.kind}`} role="group" aria-label="Answer" data-armed={armed}>
          {p.choices.map(c => <button key={c.id} className={`approval-choice style-${c.style}`} data-testid={`approval-${c.id}`} title={CHOICE_HINT[c.id] ?? c.label} disabled={pending} aria-disabled={!armed || pending}
            onClick={e => press(e, c.id, c.style)}>{c.label}</button>)}
        </div>
      : <div className="approval-actions">
          <p className="approval-note" data-testid="approval-note">{p.note ?? 'Open the terminal to answer this'}</p>
          {!p.answered && openTerminal}
        </div>}
  </div>;
}
