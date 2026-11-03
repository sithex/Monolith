import { useRef } from 'react';
import './preview-notice.css';
import { dispatch, useIslandState } from '../ipc/island-state';
import { useNotifications } from '../ipc/notifications';

/** A preview is presentation only: dismiss never removes history or touches a PTY. */
export function PreviewNotice({ fail }: { fail(error: unknown): void }) {
  const state = useIslandState(), notifications = useNotifications();
  const event = useRef<{ type: string; id: string } | null>(null);
  if (state.mode !== 'hover') event.current = null;
  else if (state.hoverCause?.type === 'notification') event.current = state.hoverCause;
  const item = event.current?.type === 'notification' ? notifications.find(n => n.id === event.current?.id) : null;
  const label = item ? item.title : state.terminalStarted ? 'Resume terminal' : 'Open terminal';
  return <div className="preview-notice" role="status">
    <button className="notice-open" data-testid="open" onClick={() => {
      void dispatch({ type: 'selectView', view: item ? (item.appName === 'Agents' ? 'agents' : 'notifications') : 'terminal' }).catch(fail);
    }}><strong>{label}</strong><span className="muted">{item ? item.appName : 'Your session, one click away'} ↗</span></button>
    <button className="notice-dismiss" aria-label="Dismiss preview" title="Dismiss preview" onClick={e => {
      e.stopPropagation(); void dispatch({ type: 'dismissPreview', rev: state.rev }).catch(fail);
    }}>×</button>
  </div>;
}
