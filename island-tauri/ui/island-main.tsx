import './dev-console';
import { installDropGuard } from './island/drop-guard';
import { createRoot } from 'react-dom/client';
import { Island } from './island/Island';
import { loadStores, seedStores } from './stores';
import { beforeFirstRender } from './stores/topic';

// A file dropped on the page must never replace the island (see drop-guard.ts).
installDropGuard(window as unknown as Parameters<typeof installDropGuard>[0]);

await beforeFirstRender(loadStores, seedStores, () => {
  const root = document.getElementById('root');
  if (!root) throw new Error('Island root missing');
  createRoot(root).render(<Island />);
});
