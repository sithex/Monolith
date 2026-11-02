import type { NowPlaying } from '../ipc/client';

type ProgressSample = Pick<NowPlaying, 'positionMs' | 'durationMs' | 'sampledAtMs' | 'playing' | 'status' | 'rate'>;

/** MPRIS Position is a sample, not a continuously signalled value. Project that
 * sample, never the previous rendered value: seeks, player switches and track
 * changes therefore replace the anchor instead of accumulating stale elapsed time.
 * The caller supplies an epoch clock advanced monotonically while mounted.
 */
export function mediaProgress(sample: ProgressSample | null, nowMs: number | null) {
  const length = sample?.durationMs != null && Number.isFinite(sample.durationMs) && sample.durationMs > 0 ? sample.durationMs : null;
  if (sample?.positionMs == null || !Number.isFinite(sample.positionMs)) return { position: null, length };
  let position = Math.max(0, sample.positionMs);
  if (sample.status === 'stopped') position = 0;
  else if (sample.playing && sample.status === 'playing' && nowMs !== null && Number.isFinite(nowMs) && Number.isFinite(sample.sampledAtMs)) {
    const rate = Number.isFinite(sample.rate) && sample.rate > 0 ? sample.rate : 1;
    position += Math.max(0, nowMs - sample.sampledAtMs) * rate;
  }
  return { position: length === null ? position : Math.min(length, position), length };
}
