import type { NowPlayingModel } from '../app-models';
import type { IslandIntents } from '../models';
import { Empty, SafeImage } from './AppPrimitives';
import { Icon } from './Icon';
import { useReducedMotionPreference } from './AppGrid';
const duration = (ms: number) => `${Math.floor(ms / 60000)}:${String(Math.floor(ms / 1000) % 60).padStart(2, '0')}`;
export function NowPlaying({ model, intents }: { model: NowPlayingModel; intents: Pick<IslandIntents, 'media'> }) {
  const reduced = useReducedMotionPreference();
  const track = model.nowPlaying;
  if (!track) return <Empty icon="music">No player · Your music appears here</Empty>;
  // Position and activeIndex are supplied by Rust. No Date.now, intervals or
  // extrapolation here: hidden views cannot grow another clock.
  const position = track.positionMs === null ? null : Math.max(0, track.positionMs);
  const length = track.durationMs !== null && track.durationMs > 0 ? track.durationMs : null;
  return <div className="app-stack now-playing"><div className="track-header"><div className="artwork"><Icon name="music"/><SafeImage src={track.artUrl} alt={`${track.title} artwork`}/></div><div><small className="muted">{track.player}</small><h2>{track.title || 'Unknown title'}</h2><p>{track.artist || 'Unknown artist'}</p><small className="muted">{track.album}</small></div></div>
    <div className="track-progress">{position !== null && length !== null ? <progress aria-label="Track progress" max={length} value={Math.min(length, position)}/> : <p className="muted">Position unavailable</p>}<div className="app-footer muted"><span>{position === null ? 'Unknown' : duration(position)}</span><span className="title-spacer"/><span>{length === null ? 'Unknown' : duration(length)}</span></div></div>
    <div className="transport"><button aria-label="Previous track" disabled={!track.canControl || !track.canGoPrevious} onClick={() => intents.media({ type: 'previous' })}>|‹</button><button className="primary" aria-label={track.playing ? 'Pause' : 'Play'} disabled={!track.canControl} onClick={() => intents.media({ type: 'playPause' })}>{track.playing ? 'Ⅱ' : '▶'}</button><button aria-label="Next track" disabled={!track.canControl || !track.canGoNext} onClick={() => intents.media({ type: 'next' })}>›|</button></div>
    {model.equalizer.active && <div className="equalizer" role="img" aria-label={reduced ? 'Equalizer · static reduced-motion display' : 'Live equalizer'} data-static={reduced}>{model.equalizer.bands.map((band, n) => <i key={n} style={{ height: `${(reduced ? 0.35 : Number.isFinite(band) ? Math.max(0, Math.min(1, band)) : 0) * 100}%` }}/>)}</div>}
    <section className="card lyrics" aria-label="Lyrics">{model.lyrics.status === 'synced' ? <ol className="app-list">{model.lyrics.lines.map((line, index) => <li key={`${line.timeMs}-${index}`} aria-current={index === model.lyrics.activeIndex ? 'true' : undefined}>{line.text}</li>)}</ol> : model.lyrics.status === 'plain' && model.lyrics.plain ? <p>{model.lyrics.plain}</p> : <p className="muted">{model.lyrics.status === 'loading' ? 'Loading lyrics…' : model.lyrics.status === 'instrumental' ? 'Instrumental' : 'Lyrics unavailable'}</p>}</section>
  </div>;
}
