import { useIslandState } from '../ipc/island-state';
import { useSettings } from '../stores';

import { gamePollingActive } from './game-polling-policy';
export function useGamePollingActive(active: boolean): boolean {
  const state = useIslandState(), settings = useSettings();
  return gamePollingActive(active, state.gameMode, settings.gameModePausePollers);
}
