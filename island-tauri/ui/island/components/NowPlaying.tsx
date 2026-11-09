// Transport, lyric index and position samples stay host-owned. Bars are not audio capture.
import { useEffect, useRef, useState } from 'react';
import { AnimatePresence, motion } from 'framer-motion';
import { DUR, tween } from '../motion-profile';
import type { NowPlayingModel } from '../app-models';
import type { MediaCommand, MixerChanged, NowPlaying as Track } from '../../ipc/client';
import { DraftRange, Empty, SafeImage } from './AppPrimitives';
import { Icon } from './Icon';
import { SettingsLink } from '../../views/settings/SettingsLink';
import { useReducedMotionAll } from './motion-preference';
import { mediaProgress } from '../media-progress';
const duration = (ms: number) => `${Math.floor(ms / 60000)}:${String(Math.floor(ms / 1000) % 60).padStart(2, '0')}`;
// Keep SSR/test imports side-effect-free; bundler still extracts the component's CSS chunk.
if (typeof document !== 'undefined') void import('../media.css');
export function playerStream(track: Track, streams: MixerChanged['streams']): MixerChanged['streams'][number] | null {
  const byPid = track.pid == null ? [] : streams.filter(s => s.pid === track.pid);
  if (byPid.length === 1) return byPid[0]!;
  const names = [track.player.toLowerCase(), track.player.split('.')[0]?.toLowerCase()];
  const matches = streams.filter(s => names.includes(s.appName.toLowerCase()) || (s.binary !== null && names.includes(s.binary.split('/').pop()?.toLowerCase())));
  // Never guess between two unrelated streams or accidentally change the default sink.
  return matches.length === 1 ? matches[0]! : null;
}
export function NowPlaying({ model, intents, mixer = null, onStreamVolume, lyricsEnabled = model.lyrics.status !== 'off', onLyricsEnabled }: {
  model: NowPlayingModel; intents: { media(action: MediaCommand): void }; mixer?: MixerChanged | null;
  onStreamVolume?(index: number, percent: number): void; lyricsEnabled?: boolean; onLyricsEnabled?(enabled: boolean): void;
}) {
  const reduced = useReducedMotionAll();
  const track = model.nowPlaying;
  const root = useRef<HTMLDivElement>(null);
  // The lyrics stage is open by default once lyrics are enabled; the quote button folds it
  // away (Apple Music style) without turning the LRCLIB lookups off.
  const [showLyrics, setShowLyrics] = useState(lyricsEnabled);
  // The epoch sample crosses IPC; elapsed time after mounting uses a monotonic
  // clock so a wall-clock adjustment cannot send the playhead backwards.
  const [nowMs, setNowMs] = useState<number | null>(null);
  useEffect(() => {
    if (!root.current || !track?.playing || track.status !== 'playing' || track.positionMs === null) return;
    const epoch = Date.now(), monotonic = performance.now();
    let inView = typeof IntersectionObserver === 'undefined';
    let timer: ReturnType<typeof setInterval> | undefined;
    const tick = () => setNowMs(epoch + performance.now() - monotonic);
    const sync = () => {
      clearInterval(timer);
      timer = undefined;
      if (inView && document.visibilityState !== 'hidden') {
        tick();
        timer = setInterval(tick, 250);
      }
    };
    const observer = typeof IntersectionObserver === 'undefined' ? null : new IntersectionObserver(entries => {
      inView = entries.some(entry => entry.isIntersecting);
      sync();
    });
    observer?.observe(root.current);
    document.addEventListener('visibilitychange', sync);
    sync();
    return () => { clearInterval(timer); observer?.disconnect(); document.removeEventListener('visibilitychange', sync); };
  }, [track?.playing, track?.status, track?.positionMs, track?.sampledAtMs, track?.rate, track?.busName, track?.trackId, track?.title]);
  if (!track) return <Empty icon="music">No player · Your music appears here</Empty>;
  const stream = playerStream(track, mixer?.available ? mixer.streams : []);
  const mprisVolume = track.volumeWritable && track.volumePercent != null;
  const volume = mprisVolume ? track.volumePercent : stream?.volumePercent ?? null;
  const { position, length } = mediaProgress(track, nowMs);
  return <div ref={root} className="now-playing" data-lyrics={showLyrics} data-reduced={reduced}>
    {showLyrics && <div className="np-backdrop" aria-hidden="true"><SafeImage src={track.artUrl} alt="" /></div>}
    <div className="np-player app-stack">
      <div className="track-header"><div className="artwork"><Icon name="music"/><ArtworkCrossfade src={track.artUrl} alt={`${track.title} artwork`} reduced={reduced}/></div><div className="track-description"><div className="track-source"><small className="muted">{track.player}</small><span className="music-indicator" role="img" aria-label={track.playing ? 'Playing indicator' : 'Paused indicator'} data-playing={track.playing} data-static={reduced}>{[0,1,2,3,4].map(i => <i key={i}><b/></i>)}</span><span className="music-settings-links"><SettingsLink rowId="lyricsEnabled" label="Lyrics settings"/><SettingsLink rowId="equalizerEnabled" label="Equalizer settings"/></span></div><h2>{track.title || 'Unknown title'}</h2><p>{track.artist || 'Unknown artist'}</p><small className="muted">{track.album}</small></div></div>
      <div className="track-progress">{position !== null && length !== null ? <progress aria-label="Track progress" max={length} value={Math.min(length, position)}/> : <p className="muted">Position unavailable</p>}<div className="app-footer muted"><span>{position === null ? 'Unknown' : duration(position)}</span><span className="title-spacer"/><span>{length === null ? 'Unknown' : duration(length)}</span></div></div>
      <div className="transport"><button className="transport-skip" aria-label="Previous track" disabled={!track.canControl || !track.canGoPrevious} onClick={() => intents.media({ type: 'previous' })}><TransportGlyph kind="previous" /></button><button className="transport-main" aria-label={track.playing ? 'Pause' : 'Play'} data-playing={track.playing} disabled={!track.canControl} onClick={() => intents.media({ type: 'playPause' })}><TransportGlyph kind="play" /><TransportGlyph kind="pause" /></button><button className="transport-skip" aria-label="Next track" disabled={!track.canControl || !track.canGoNext} onClick={() => intents.media({ type: 'next' })}><TransportGlyph kind="next" /></button></div>
      <div className="player-volume"><span className="muted">Volume</span>{volume !== null ? <DraftRange aria-label="Player volume" min={0} max={100} step={1} value={Math.min(100, Math.max(0, volume))} onCommit={percent => mprisVolume ? intents.media({ type:'volume',percent }) : stream && onStreamVolume?.(stream.index,percent)} /> : <span className="muted">Unavailable for this player</span>}<small className="muted">{volume === null ? '' : `${Math.round(volume)}%`}</small>
        <button type="button" className="lyrics-toggle" aria-label={showLyrics ? 'Hide lyrics' : 'Show lyrics'} aria-pressed={showLyrics} title="Lyrics" onClick={() => setShowLyrics(on => !on)}><Icon name="quote" /></button></div>
    </div>
    {showLyrics && <LyricsStage model={model} track={track} reduced={reduced} enabled={lyricsEnabled} onEnabled={onLyricsEnabled}
      seek={track.canControl && track.canSeek ? positionMs => intents.media({ type: 'seek', positionMs }) : null} />}
  </div>;
}

/** Keep both covers in the fixed artwork box during the crossfade. */
export function ArtworkCrossfade({ src, alt, reduced }: { src: string | null; alt: string; reduced: boolean }) {
  return <AnimatePresence initial={false}>
    <motion.div key={src ?? 'no-artwork'} className="np-artwork-layer"
      initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }}
      transition={tween(reduced ? DUR.press : DUR.base)}>
      <SafeImage src={src} alt={alt}/>
    </motion.div>
  </AnimatePresence>;
}

/**
 * Apple Music style lyrics: big bold lines, the current one bright, the rest dimmed and
 * softly blurred by distance; the stage follows the song unless you scroll it yourself,
 * and a click on a line seeks there. Instrumental breaks breathe as three dots.
 */
export function LyricsStage({ model, track, reduced, enabled, onEnabled, seek }: {
  model: NowPlayingModel; track: Track; reduced: boolean; enabled: boolean;
  onEnabled?(enabled: boolean): void; seek: ((positionMs: number) => void) | null;
}) {
  const list = useRef<HTMLDivElement>(null), userUntil = useRef(0);
  const { lyrics } = model, active = lyrics.activeIndex;
  const ready = enabled && (lyrics.status === 'synced' || !!lyrics.plain);
  // Lyrics follow playback (ambient), so typing elsewhere must not switch their motion off.
  useEffect(() => {
    const box = list.current, line = box?.querySelector<HTMLElement>('[aria-current="true"]');
    if (!box || !line || Date.now() < userUntil.current) return;
    // Keep the current line about a third of the way down, like Apple Music.
    box.scrollTo({ top: Math.max(0, line.offsetTop - box.clientHeight * 0.3), behavior: reduced ? 'instant' : 'smooth' });
  }, [active, lyrics.lines, reduced]);
  // Scrolling by hand pauses the follow for a few seconds.
  const hold = () => { userUntil.current = Date.now() + 3500; };
  let body;
  if (!enabled) body = <div className="lyrics-consent"><p>Lyrics come from LRCLIB. Title, artist, album and duration are shared.</p>
    <button type="button" className="pill-button primary" aria-label="Enable lyrics from LRCLIB" onClick={() => onEnabled?.(true)}>Show lyrics</button></div>;
  else if (lyrics.status === 'synced') body = <ol className="am-lyrics" aria-label="Synced lyrics">{lyrics.lines.map((line, index) => {
    const current = index === active, delta = active === null ? null : Math.abs(index - active);
    const distance = delta === null ? 2 : Math.min(4, delta);
    const gap = !line.text.trim();
    return <li key={`${line.timeMs}-${index}`} aria-current={current ? 'true' : undefined} data-past={active !== null && index < active} data-near={delta !== null && delta <= 4} style={{ ['--d' as string]: distance }}>
      <button type="button" disabled={!seek} tabIndex={seek ? 0 : -1} onClick={() => { userUntil.current = 0; seek?.(line.timeMs); }}>
        {gap ? <span className="am-dots" role="img" aria-label="Instrumental" data-live={current && track.playing && !reduced}><i /><i /><i /></span> : line.text}
      </button></li>;
  })}</ol>;
  else if (lyrics.plain) body = <div className="am-plain">{lyrics.plain.split('\n').map((text, i) => <p key={i}>{text || '\u00a0'}</p>)}</div>;
  else body = <p className="lyrics-note">{lyrics.status === 'loading' ? 'Loading lyrics…' : lyrics.status === 'instrumental' ? 'Instrumental' : lyrics.status === 'error' ? 'Lyrics lookup failed. Turn lyrics off and on to retry.' : 'No lyrics found for this track'}</p>;
  return <section className="np-lyrics" aria-label="Lyrics" data-reduced={reduced}>
    <div ref={list} className="np-lyrics-scroll" onWheel={hold} onPointerDown={hold}>
      <motion.div key={`${track.busName}:${track.trackId ?? track.title}:${ready}`} className="np-lyrics-content"
        initial={ready ? { opacity: 0 } : false}
        animate={{ opacity: 1 }} transition={tween(reduced ? DUR.press : DUR.slow)}>{body}</motion.div>
    </div>
    {enabled && <div className="lyrics-credit"><small>Lyrics from LRCLIB</small><button type="button" className="lyrics-off" onClick={() => onEnabled?.(false)}>Turn off</button></div>}
  </section>;
}

/** Filled, optically centred transport glyphs (24 px grid); play/pause cross-fade in place. */
const GLYPHS = {
  play: 'M8.5 5.4v13.2a1 1 0 0 0 1.52.86l10.4-6.6a1 1 0 0 0 0-1.72l-10.4-6.6A1 1 0 0 0 8.5 5.4z',
  pause: 'M7.2 5h2.6c.66 0 1.2.54 1.2 1.2v11.6c0 .66-.54 1.2-1.2 1.2H7.2c-.66 0-1.2-.54-1.2-1.2V6.2C6 5.54 6.54 5 7.2 5zm7 0h2.6c.66 0 1.2.54 1.2 1.2v11.6c0 .66-.54 1.2-1.2 1.2h-2.6c-.66 0-1.2-.54-1.2-1.2V6.2c0-.66.54-1.2 1.2-1.2z',
  next: 'M2.5 6.6v10.8a.9.9 0 0 0 1.43.73L11 13.05v4.35a.9.9 0 0 0 1.43.73l7.6-5.4a.9.9 0 0 0 0-1.46l-7.6-5.4A.9.9 0 0 0 11 6.6v4.35L3.93 5.87A.9.9 0 0 0 2.5 6.6z',
} as const;
function TransportGlyph({ kind }: { kind: 'play' | 'pause' | 'next' | 'previous' }) {
  const path = GLYPHS[kind === 'previous' ? 'next' : kind];
  return <svg className={`transport-glyph glyph-${kind}`} viewBox="0 0 24 24" aria-hidden="true" fill="currentColor"
    style={kind === 'previous' ? { transform: 'scaleX(-1)' } : undefined}><path d={path} /></svg>;
}
