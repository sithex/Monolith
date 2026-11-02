// Presentation projections, not mirrored IPC DTOs. Wire fields come from the
// generated client; only labels and UI-only state are added here.
import type { ClipSummary, ClipboardStatus, NowPlaying, LyricsChanged, EqualizerChanged, MediaCommand } from '../ipc/client';
export type ClipboardItem = Omit<ClipSummary, 'rev'> & { ageLabel: string };
export interface ClipboardModel { paused: boolean; items: ClipboardItem[]; status?: ClipboardStatus | null; loading?: boolean; error?: string | null; query?: string }
export interface NowPlayingModel {
  nowPlaying: NowPlaying | null;
  lyrics: Omit<LyricsChanged, 'rev' | 'trackKey'>;
  equalizer: Omit<EqualizerChanged, 'rev'>;
}
export interface AppIntents {
  clipboard(action: { type: 'pause'; paused: boolean } | { type: 'pin'; id: string; pinned: boolean } | { type: 'delete' | 'copy'; id: string } | { type: 'search'; query: string }): void;
  media(action: Extract<MediaCommand, { type: 'playPause' | 'next' | 'previous' }>): void;
}
