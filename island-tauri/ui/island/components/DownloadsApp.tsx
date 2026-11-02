import type { DownloadItem, FileKind, AppViewIntents } from '../app-presentations';
import type { IconName } from '../models';
import { Empty, formatSize } from './AppPrimitives';
import { Icon } from './Icon';
const kindIcons: Record<FileKind, IconName> = { archive: 'download', image: 'camera', video: 'monitor', audio: 'music', document: 'note', package: 'grid', code: 'terminal', other: 'download' };
export function DownloadsApp({ items, intents }: { items: DownloadItem[]; intents: Pick<AppViewIntents, 'download'> }) {
  const recent = [...items].sort((a, b) => b.mtimeMs - a.mtimeMs || a.name.localeCompare(b.name));
  return <div className="downloads-app">{recent.length ? <ul className="app-list">{recent.map(item => {
    const partial = /\.(part|crdownload|download|partial|opdownload)$/i.test(item.name);
    const ready = item.state === 'ready' && !partial;
    return <li className="file-row card" key={item.name}><span className="kind-icon tone-green" aria-label={`${item.kind} file`}><Icon name={kindIcons[item.kind] ?? 'download'}/></span><div className="row-content"><strong>{item.name}</strong><small>{formatSize(item.sizeBytes)} · {item.ageLabel}</small>{!ready && <small className="warning">{partial || item.state === 'inProgress' ? 'In progress' : 'Settling…'}</small>}</div><div className="row-actions">{ready && <button aria-label={`Open ${item.name}`} onClick={() => intents.download({ type: 'open', name: item.name })}>Open</button>}<button aria-label={`Reveal ${item.name}`} onClick={() => intents.download({ type: 'reveal', name: item.name })}>Reveal</button></div></li>;
  })}</ul> : <Empty icon="download">No recent downloads</Empty>}<p className="muted app-caption">Files open only after the download service marks them ready.</p></div>;
}
