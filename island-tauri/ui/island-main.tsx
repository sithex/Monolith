import './dev-console';
import './island/error-fallback.css';
import { installDropGuard } from './island/drop-guard';
import { createRoot, type Root } from 'react-dom/client';
import { Island } from './island/Island';
import { FallbackCard, IslandErrorBoundary, TestCrashSwitch, startIsland, takeTestFault, type FallbackIpc } from './island/error-fallback';
import { islandDispatch, onIslandStateChanged, reportGeometry } from './ipc/client';
import { loadStores, runtimeStore, seedStores } from './stores';

// A file dropped on the page must never replace the island (see drop-guard.ts).
installDropGuard(window as unknown as Parameters<typeof installDropGuard>[0]);

const fallbackIpc: FallbackIpc = {
  collapse: () => islandDispatch({ type: 'collapse' }),
  report: reportGeometry,
  onState: onIslandStateChanged,
  reload: () => window.location.reload(),
};
let root: Root | null = null;
const container = () => {
  const element = document.getElementById('root');
  if (!element) throw new Error('Island root missing');
  return element;
};
const testHooks = () => { try { return runtimeStore.getSnapshot().testHooks; } catch { return false; } };

await startIsland(loadStores, snapshot => {
  seedStores(snapshot);
  if (takeTestFault(window.sessionStorage, testHooks())) throw new Error('Test-requested bootstrap failure');
}, () => {
  root = createRoot(container());
  root.render(<IslandErrorBoundary ipc={fallbackIpc}><TestCrashSwitch enabled={testHooks()}/><Island /></IslandErrorBoundary>);
}, error => {
  // A root that already mounted is reused; otherwise the card gets a fresh one.
  try {
    root ??= createRoot(container());
    root.render(<FallbackCard error={error} ipc={fallbackIpc}/>);
  } catch (failure) {
    console.error('Island fallback could not render', failure);
  }
});
