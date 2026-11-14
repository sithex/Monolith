import type { DownloadItem, FileKind, AppViewIntents, TransferItem } from '../app-presentations';
import type { IconName } from '../models';
import { Empty, formatSize } from './AppPrimitives';
import { Icon } from './Icon';
const kindIcons: Record<FileKind, IconName> = { archive: 'download', image: 'camera', video: 'monitor', audio: 'music', document: 'note', package: 'grid', code: 'terminal', other: 'download' };
/** `org.kde.dolphin` → `Dolphin`; a plain name stays as it is. */
export function transferAppLabel(app: string) {
  const last = app.split('.').filter(Boolean).pop() ?? '';
  return last ? last.charAt(0).toUpperCase() + last.slice(1) : 'KDE';
}
function transferAmount(item: TransferItem) {
  const parts = [];
  if (item.totalBytes > 0) parts.push(`${formatSize(item.processedBytes)} of ${formatSize(item.totalBytes)}`);
  else if (item.totalFiles > 0) parts.push(`${item.processedFiles} of ${item.totalFiles} files`);
  if (item.speed > 0 && !item.suspended) parts.push(`${formatSize(item.speed)}/s`);
  if (item.suspended) parts.push('Paused');
  return parts.join(' · ');
}
/** Running KDE copies and moves (Dolphin and other KIO apps), above the downloads list.
 *  No cancel: KDE accepts a cancel only from Plasma's own job popup. */
export function TransfersCard({ items, onReveal }: { items: TransferItem[]; onReveal?: (id: string) => void }) {
  if (!items.length) return null;
  return <section className="transfers" aria-label="Transfers"><h3 className="transfers-title">Transfers</h3><ul className="app-list">{items.map(item => {
    const title = item.title || 'Copying', amount = transferAmount(item);
    return <li className="file-row card transfer-row" key={item.id}><span className="kind-icon" aria-hidden="true"><Icon name="download"/></span><div className="row-content"><strong>{title}{item.percent !== null && ` · ${Math.floor(item.percent)}%`}</strong><small>{transferAppLabel(item.app)}{item.destination && ` → ${item.destination}`}</small>{amount && <small>{amount}</small>}<div className="transfer-bar" role="progressbar" aria-label={`${title} progress`} aria-valuemin={0} aria-valuemax={100} aria-valuenow={item.percent === null ? undefined : Math.floor(item.percent)}><span style={{ width: `${item.percent === null ? 0 : Math.floor(item.percent)}%` }}/></div></div>{item.destination && onReveal && <div className="row-actions"><button aria-label={`Show ${title} in folder`} onClick={() => onReveal(item.id)}>Show in folder</button></div>}</li>;
  })}</ul></section>;
}
export function DownloadsApp({ items, intents, transfers = [] }: { items: DownloadItem[]; intents: Pick<AppViewIntents, 'download'> & Partial<Pick<AppViewIntents, 'transfer'>>; transfers?: TransferItem[] }) {
  const recent = [...items].sort((a, b) => b.mtimeMs - a.mtimeMs || a.name.localeCompare(b.name));
  return <div className="downloads-app"><TransfersCard items={transfers} onReveal={intents.transfer}/>{recent.length ? <ul className="app-list">{recent.map(item => {
    const partial = /\.(part|crdownload|download|partial|opdownload)$/i.test(item.name);
    const ready = item.state === 'ready' && !partial;
    return <li className="file-row card" key={item.name}><span className="kind-icon tone-green" aria-label={`${item.kind} file`}><Icon name={kindIcons[item.kind] ?? 'download'}/></span><div className="row-content"><strong>{item.name}</strong><small>{formatSize(item.sizeBytes)} · {item.ageLabel}</small>{!ready && <small className="warning">{partial || item.state === 'inProgress' ? 'In progress' : 'Settling…'}</small>}</div><div className="row-actions">{ready && <button aria-label={`Open ${item.name}`} onClick={() => intents.download({ type: 'open', name: item.name })}>Open</button>}<button aria-label={`Reveal ${item.name}`} onClick={() => intents.download({ type: 'reveal', name: item.name })}>Reveal</button></div></li>;
  })}</ul> : <Empty icon="download">No recent downloads</Empty>}<p className="muted app-caption">Files open only after the download service marks them ready.</p></div>;
}
