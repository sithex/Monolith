import type { NotificationsModel } from '../app-models';
import type { IslandIntents } from '../models';
import { Empty, SafeImage } from './AppPrimitives';
import { Icon } from './Icon';
export function NotificationsView({ model, intents }: { model: NotificationsModel; intents: Pick<IslandIntents, 'notification'> }) {
  const groups = new Map<string, NotificationsModel['items']>();
  [...model.items].sort((a, b) => b.receivedAt - a.receivedAt).forEach(item => groups.set(item.appName, [...(groups.get(item.appName) ?? []), item]));
  return <div className="app-stack notifications-view">
    <div className="app-toolbar"><span className="muted">{model.items.length} notifications</span><span className="title-spacer"/><button disabled={!model.items.length} onClick={() => intents.notification({ type: 'clearAll' })}>Clear all</button></div>
    {model.dnd && <p className="app-banner" role="status"><Icon name="moon"/>Do Not Disturb · Notifications are quiet</p>}
    {[...groups].map(([name, items]) => <section key={name} aria-label={`${name} notifications`}><h3><Icon name="bell"/>{name}</h3><ul className="app-list">{items.map(item => <li key={item.id} className="card notification-card"><div className="app-toolbar"><SafeImage src={item.iconPng} alt="" className="notification-icon"/><strong>{item.title}</strong><span className="title-spacer"/><button aria-label={`Dismiss ${item.title}`} onClick={() => intents.notification({ type: 'dismiss', id: item.id })}>×</button></div>
      {item.hidden ? <p className="muted">Content hidden</p> : <>{item.body && <p className="notification-body">{item.body}</p>}<SafeImage src={item.imagePng} alt="Notification attachment"/></>}
      <div className="app-footer"><small className="muted">{item.ageLabel}</small><span className="title-spacer"/>{!item.hidden && item.actions.map(action => <button key={action.key} disabled={!item.canInvokeActions} onClick={() => intents.notification({ type: 'action', id: item.id, key: action.key })}>{action.label}</button>)}</div>
    </li>)}</ul></section>)}
    {!model.items.length && <Empty icon="bell">All caught up</Empty>}
  </div>;
}
