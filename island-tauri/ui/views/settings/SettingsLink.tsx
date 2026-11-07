import { useState } from 'react';
import { Icon } from '../../island/components/Icon';

if (typeof document !== 'undefined') void import('./settings-link.css');

let pendingJump: { rowId: string } | null = null;
let jumpGeneration = 0;

/** Consume once when Settings mounts, or from its island-setting-jump listener. */
export function takePendingSettingJump(): string | null {
  const rowId = pendingJump?.rowId ?? null;
  pendingJump = null;
  return rowId;
}

/** Keep the target before navigation: Settings may not have mounted its listener yet. */
export async function openSetting(rowId: string): Promise<void> {
  const request = { rowId }, generation = ++jumpGeneration;
  pendingJump = request;
  try {
    const { dispatch } = await import('../../ipc/island-state');
    await dispatch({ type: 'selectView', view: 'settings' });
    if (generation === jumpGeneration) {
      window.dispatchEvent(new CustomEvent<string>('island-setting-jump', { detail: rowId }));
    }
  } catch (error) {
    if (pendingJump === request) pendingJump = null;
    throw error;
  }
}

export function SettingsLink({ rowId, label, variant = 'gear' }: {
  rowId: string; label: string; variant?: 'gear' | 'text';
}) {
  const [error, setError] = useState('');
  return <>
    <button type="button" className={`settings-cross-link settings-cross-link--${variant}`}
      aria-label={label} title={label} onClick={() => {
        setError('');
        void openSetting(rowId).catch(reason => setError(reason instanceof Error ? reason.message : String(reason)));
      }}>
      {variant === 'gear' ? <Icon name="settings" /> : label}
    </button>
    {error && <span className="settings-cross-link-error" role="alert">{error}</span>}
  </>;
}
