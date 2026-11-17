import type { ReactNode } from 'react';
import type { GroupSpec } from '../settings-model';

/**
 * Group row: the parent switch row(s), then their dependent rows indented under them. While
 * the group is off the children stay visible (search and jump links still land on them) but
 * are dimmed, and the disabled fieldset takes every control inside out of Tab order and
 * pointer reach. Nothing here writes a setting, so stored child values survive the parent
 * being off. A plain function, not a component: hook-free callers can use it directly.
 */
export function settingsGroup(group: GroupSpec, enabled: boolean, parents: ReactNode, children: ReactNode) {
  return <div key={`group-${group.id}`} className="settings-group" data-group={group.id} data-group-enabled={enabled}>
    {parents}
    <fieldset className="settings-group-children" disabled={!enabled} aria-label={`${group.label} options`}>{children}</fieldset>
  </div>;
}
