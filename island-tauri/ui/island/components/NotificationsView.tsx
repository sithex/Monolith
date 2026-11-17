import type { NotificationsModel, AppViewIntents } from '../app-presentations';
import { Empty, SafeImage, ServiceNotice } from './AppPrimitives';
import { Icon } from './Icon';
import { groupNotifications } from '../notification-groups';
import { SettingsLink } from '../../views/settings/SettingsLink';
export function NotificationsView({ model, intents }: { model: NotificationsModel; intents: Pick<AppViewIntents, 'notification'> }) {
  const groups = groupNotifications(model.items);
  // A group or a folded row has no command of its own: it dismisses each stored notification, exactly as a single row does.
  const dismiss = (ids: readonly string[]) => ids.forEach(id => intents.notification({ type: 'dismiss', id }));
  return <div className="app-stack notifications-view">
    <div className="app-toolbar"><span className="muted">{model.items.length} notifications</span><span className="title-spacer"/><SettingsLink rowId="notificationsEnabled" label="Notification settings"/><button disabled={!model.items.length} onClick={() => intents.notification({ type: 'clearAll' })}>Clear all</button></div>
    {model.unavailable && <ServiceNotice icon="bell" source="notifications" reason={model.unavailable}/>}
    {model.dnd && <p className="app-banner" role="status"><Icon name="moon"/>Do Not Disturb · Notifications are quiet</p>}
    {groups.map(group => <section key={group.name} className="notification-group" aria-label={`${group.name} notifications`}>
      <div className="notification-group-head"><h3><Icon name="bell"/>{group.name}</h3><span className="notification-count muted">{group.count}</span><span className="title-spacer"/>
        {group.count > 1 && <button data-variant="quiet" aria-label={`Dismiss all ${group.name} notifications`} onClick={() => dismiss(group.rows.flatMap(row => row.ids))}>Dismiss all</button>}</div>
      <ul className="app-list notification-rows">{group.rows.map(({ key, ids, count, item }) => <li key={key} className="notification-row">
        <div className="notification-row-line">{!item.hidden && <SafeImage src={item.iconPng} alt="" className="notification-icon"/>}<strong>{item.title}</strong>{count > 1 && <span className="notification-repeat" aria-label={`${count} times`}>×{count}</span>}<span className="title-spacer"/><small className="muted">{item.ageLabel}</small><button className="icon-button" aria-label={count > 1 ? `Dismiss ${item.title} (${count})` : `Dismiss ${item.title}`} onClick={() => dismiss(ids)}>×</button></div>
        {item.hidden ? <p className="muted">Content hidden</p> : item.body && <p className="notification-body">{item.body}</p>}
        {!item.hidden && item.actions.length > 0 && <div className="app-footer">{item.actions.map(action => <button key={action.key} disabled title="Passive notification mirror">{action.label}</button>)}</div>}
      </li>)}</ul></section>)}
    {!model.items.length && <Empty icon="bell">All caught up</Empty>}
  </div>;
}
