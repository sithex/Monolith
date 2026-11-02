import './dev-console';
import { createRoot } from 'react-dom/client';
import { Island } from './island/Island';
import { loadStores, seedStores } from './stores';
import { beforeFirstRender } from './stores/topic';

await beforeFirstRender(loadStores, seedStores, () => {
  const root = document.getElementById('root');
  if (!root) throw new Error('Island root missing');
  createRoot(root).render(<Island />);
});
