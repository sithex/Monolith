import { act, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { CalendarApp, ScratchpadApp, DownloadsApp, VolumeMixer, Brightness, NotificationsView, ClipboardView, NowPlaying, QuickInfo, SettingsModules } from '../src/components';
import { SafeImage } from '../src/components/AppPrimitives';
import * as mocks from '../src/app-mocks';
import type { ClipboardModel, MixerModel, ModuleSettingsModel } from '../src/app-models';
import type { IslandIntents } from '../src/models';
import { setReducedMotion } from './setup';
const intents = () => ({ calendar: vi.fn(), scratchpad: vi.fn(), download: vi.fn(), mixer: vi.fn(), brightness: vi.fn(), notification: vi.fn(), clipboard: vi.fn(), media: vi.fn(), modules: vi.fn() }) satisfies Pick<IslandIntents, 'calendar' | 'scratchpad' | 'download' | 'mixer' | 'brightness' | 'notification' | 'clipboard' | 'media' | 'modules'>;
afterEach(() => vi.useRealTimers());
describe('CalendarApp', () => {
  it('agenda distinguishes all-day spans recurring and supplied local labels', () => {
    render(<CalendarApp model={mocks.calendarStories.agenda} intents={intents()}/>);
    expect(screen.getByText('All day')).toBeTruthy(); expect(screen.getByText('Oct 16–18 · 3 days')).toBeTruthy();
    expect(screen.getByText('10:30–11:00 AM · Recurring')).toBeTruthy(); expect(screen.getByText('Subscription · Read-only')).toBeTruthy();
    expect(screen.getAllByText('Local event')).toHaveLength(2);
  });
  it('month repeats multi-day event on supplied days without timezone conversion', () => {
    const { container } = render(<CalendarApp model={mocks.calendar} intents={intents()}/>);
    expect(container.querySelectorAll('.month-day')).toHaveLength(35); expect(screen.getAllByText('Design retreat')).toHaveLength(3);
    expect(container.querySelectorAll('[data-today="true"]')).toHaveLength(1);
  });
  it('unconfigured and stale last-good states retain events', () => {
    const { rerender } = render(<CalendarApp model={mocks.calendarStories.unconfigured} intents={intents()}/>);
    expect(screen.getByText(/Subscription not configured/)).toBeTruthy(); expect(screen.getByText('Design retreat')).toBeTruthy();
    rerender(<CalendarApp model={mocks.calendarStories.stale} intents={intents()}/>);
    expect(screen.getByRole('status').textContent).toBe('Fetch failed · Last good shown · Stale since 10:42 AM'); expect(screen.getByText('Island design sync')).toBeTruthy();
  });
  it('calendar never renders secret URL even on malformed extra prop', () => {
    const model = { ...mocks.calendarStories.stale, subscriptionUrl: 'https://secret.invalid/private-SENTINEL.ics' };
    const { container } = render(<CalendarApp model={model} intents={intents()}/>);
    expect(container.innerHTML).not.toContain('private-SENTINEL'); expect(container.querySelector('a')).toBeNull();
  });
  it('calendar buttons emit view navigation refresh configure and event intents', () => {
    const send = intents(); render(<CalendarApp model={mocks.calendarStories.unconfigured} intents={send}/>);
    fireEvent.click(screen.getByRole('button', { name: 'Month' })); expect(send.calendar).toHaveBeenCalledWith({ type: 'view', view: 'month' });
    fireEvent.click(screen.getByLabelText('Previous month')); fireEvent.click(screen.getByLabelText('Next month')); fireEvent.click(screen.getByText('Refresh')); fireEvent.click(screen.getByText('Set up subscription')); fireEvent.click(screen.getByText('Design retreat'));
    for (const type of ['previous', 'next', 'refresh', 'configure']) expect(send.calendar).toHaveBeenCalledWith({ type });
    expect(send.calendar).toHaveBeenCalledWith({ type: 'select', instanceKey: 'retreat-1' });
  });
  it('empty agenda is explicit', () => { render(<CalendarApp model={mocks.calendarStories.empty} intents={intents()}/>); expect(screen.getByText('No events in this window')).toBeTruthy(); });
});
describe('ScratchpadApp', () => {
  it('pinned notes first without mutating input and editor sends edits only', () => {
    const send = intents(); const original = JSON.stringify(mocks.scratchpad); render(<ScratchpadApp model={mocks.scratchpad} intents={send}/>);
    const buttons = within(screen.getByRole('navigation', { name: 'Notes' })).getAllByRole('button'); expect(buttons[0].textContent).toContain('Island design brief');
    const editor = screen.getByRole('textbox', { name: 'Note body' }); editor.focus(); expect(document.activeElement).toBe(editor);
    fireEvent.change(editor, { target: { value: 'Unsubmitted note draft' } }); expect(send.scratchpad).toHaveBeenCalledWith({ type: 'edit', id: 'note-brief', body: 'Unsubmitted note draft' });
    expect((editor as HTMLTextAreaElement).value).toBe(mocks.scratchpad.notes[1].body); expect(JSON.stringify(mocks.scratchpad)).toBe(original);
  });
  it.each(['saving', 'saved', 'error'] as const)('save indicator is props-driven %s', status => { render(<ScratchpadApp model={mocks.scratchpadStories[status]} intents={intents()}/>); expect(screen.getByRole('status').textContent).toContain(status === 'saving' ? 'Saving…' : status === 'saved' ? 'Saved' : 'Save failed'); });
  it('select create pin and delete send typed intents', () => { const send = intents(); render(<ScratchpadApp model={mocks.scratchpad} intents={send}/>); fireEvent.click(screen.getByText('Later')); fireEvent.click(screen.getByText('New note')); fireEvent.click(screen.getByText('Unpin note')); fireEvent.click(screen.getByText('Delete note')); expect(send.scratchpad.mock.calls.map(call => call[0])).toEqual([{ type: 'select', id: 'note-later' }, { type: 'create' }, { type: 'pin', id: 'note-brief', pinned: false }, { type: 'delete', id: 'note-brief' }]); });
  it('empty list and no selected editor are explicit', () => { render(<ScratchpadApp model={mocks.scratchpadStories.empty} intents={intents()}/>); expect(screen.getByText('No notes yet')).toBeTruthy(); expect(screen.queryByRole('textbox')).toBeNull(); });
});
describe('DownloadsApp', () => {
  it('part files cannot open even if malformed ready state arrives', () => {
    render(<DownloadsApp items={[{ ...mocks.downloads[2], state: 'ready' }]} intents={intents()}/>);
    expect(screen.getByText('In progress')).toBeTruthy(); expect(screen.queryByRole('button', { name: /^Open / })).toBeNull(); expect(screen.getByRole('button', { name: /Reveal/ })).toBeTruthy();
  });
  it('in-progress and settling have no open action; ready sends name only', () => {
    const send = intents(); render(<DownloadsApp items={mocks.downloads} intents={send}/>); expect(screen.queryByRole('button', { name: 'Open notes.txt' })).toBeNull();
    expect(screen.getByText('Settling…')).toBeTruthy(); expect(screen.getByText('26.8 MB · 2 minutes ago')).toBeTruthy(); expect(screen.getByLabelText('image file')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Open island-concepts.zip' })); fireEvent.click(screen.getByRole('button', { name: 'Reveal recording.webm.part' })); expect(send.download.mock.calls.map(c => c[0])).toEqual([{ type: 'open', name: 'island-concepts.zip' }, { type: 'reveal', name: 'recording.webm.part' }]);
  });
  it('sorts newest first and empty has no actions', () => { const { rerender } = render(<DownloadsApp items={[...mocks.downloads].reverse()} intents={intents()}/>); expect(screen.getAllByRole('listitem')[0].textContent).toContain('island-concepts.zip'); rerender(<DownloadsApp items={[]} intents={intents()}/>); expect(screen.getByText('No recent downloads')).toBeTruthy(); });
});
describe('VolumeMixer and Brightness', () => {
  it('volume None is unknown never zero and mute remains tri-state', () => {
    render(<VolumeMixer model={mocks.mixer} intents={intents()}/>); expect(screen.getByText('Volume unknown')).toBeTruthy(); expect(screen.queryByLabelText('Browser volume')).toBeNull();
    const mute = screen.getByRole('button', { name: 'Mute Browser' }) as HTMLButtonElement; expect(mute.disabled).toBe(true); expect(mute.getAttribute('aria-pressed')).toBe('mixed');
    expect(screen.getByRole('button', { name: 'Mute Master' }).getAttribute('aria-pressed')).toBe('false'); expect(screen.getByRole('button', { name: 'Mute Messenger' }).getAttribute('aria-pressed')).toBe('true');
  });
  it('native volume ranges focus and emit only typed volume mute output intents', () => {
    const send = intents(); render(<VolumeMixer model={mocks.mixer} intents={send}/>);
    const slider = screen.getByRole('slider', { name: 'Master volume' }); slider.focus(); expect(document.activeElement).toBe(slider); expect(slider.getAttribute('type')).toBe('range');
    fireEvent.change(slider, { target: { value: '46' } }); fireEvent.click(screen.getByLabelText('Mute Master')); fireEvent.change(screen.getByLabelText('Music player volume'), { target: { value: '55' } }); fireEvent.click(screen.getByLabelText('Mute Messenger')); fireEvent.change(screen.getByLabelText('Mixer output'), { target: { value: 'fixture-display' } });
    expect(send.mixer.mock.calls.map(c => c[0])).toEqual([{ type: 'volume', target: 'sink', index: 1, percent: 46 }, { type: 'mute', target: 'sink', index: 1, muted: true }, { type: 'volume', target: 'stream', index: 10, percent: 55 }, { type: 'mute', target: 'stream', index: 12, muted: false }, { type: 'output', name: 'fixture-display' }]);
    expect((slider as HTMLInputElement).value).toBe('45');
  });
  it('mixer unavailable reason and empty streams/master are explicit', () => {
    const { rerender } = render(<VolumeMixer model={{ available: false, reason: 'Pulse unavailable' }} intents={intents()}/>); expect(screen.getByText('Unavailable · Pulse unavailable')).toBeTruthy(); expect(screen.queryByRole('slider')).toBeNull();
    const value: MixerModel = { master: null, streams: [], outputs: [], outputName: null }; rerender(<VolumeMixer model={{ available: true, value }} intents={intents()}/>); expect(screen.getByText('No app streams')).toBeTruthy(); expect(screen.getByText('Master volume unavailable')).toBeTruthy();
  });
  it('brightness per-display keyboard release and pointer release use intents', () => {
    const send = intents(); render(<Brightness displays={mocks.brightness} intents={send}/>);
    expect(screen.getByText('Brightness unknown')).toBeTruthy(); expect(screen.queryByLabelText('Projector brightness')).toBeNull();
    const slider = screen.getByRole('slider', { name: 'Laptop display brightness' }); slider.focus(); expect(document.activeElement).toBe(slider);
    fireEvent.change(slider, { target: { value: '66' } }); fireEvent.keyUp(slider, { key: 'ArrowRight' }); fireEvent.pointerUp(slider); expect(send.brightness).toHaveBeenCalledWith({ name: 'fixture-internal', percent: 66, phase: 'change' }); expect(send.brightness).toHaveBeenCalledWith({ name: 'fixture-internal', percent: 65, phase: 'end' });
    expect(screen.getByLabelText('Studio display brightness').getAttribute('aria-busy')).toBe('true');
  });
  it('brightness unavailable and empty displays are explicit', () => { const { rerender } = render(<Brightness displays={{ available: false, reason: 'Daemon absent' }} intents={intents()}/>); expect(screen.getByText('Unavailable · Daemon absent')).toBeTruthy(); rerender(<Brightness displays={{ available: true, value: [] }} intents={intents()}/>); expect(screen.getByText('No supported displays')).toBeTruthy(); });
});
describe('NotificationsView', () => {
  it('hidden notification never renders body image or actions', () => { const { container } = render(<NotificationsView model={mocks.notifications} intents={intents()}/>); const privateCard = screen.getByText('Private message').closest('li')!; expect(privateCard.textContent).toContain('Content hidden'); expect(container.innerHTML).not.toContain('HIDDEN BODY SENTINEL'); expect(container.innerHTML).not.toContain('PRIVATE ACTION SENTINEL'); expect(privateCard.querySelector('img')).toBeNull(); });
  it('groups apps newest first and keeps DND banner', () => { render(<NotificationsView model={{ ...mocks.notifications, items: [...mocks.notifications.items].reverse() }} intents={intents()}/>); const regions = screen.getAllByRole('region'); expect(regions[0].getAttribute('aria-label')).toBe('Build system notifications'); expect(within(regions[0]).getAllByRole('listitem')[0].textContent).toContain('Gallery build complete'); expect(screen.getByRole('status').textContent).toContain('Do Not Disturb'); });
  it('dismiss clear all and actions emit intents', () => { const send = intents(); render(<NotificationsView model={mocks.notifications} intents={send}/>); fireEvent.click(screen.getByText('Review build')); fireEvent.click(screen.getByLabelText('Dismiss Typecheck complete')); fireEvent.click(screen.getByText('Clear all')); expect(send.notification.mock.calls.map(c => c[0])).toEqual([{ type: 'action', id: 'n2', key: 'review' }, { type: 'dismiss', id: 'n1' }, { type: 'clearAll' }]); });
  it('passive actions disabled and empty clear disabled', () => { const send = intents(); const { rerender } = render(<NotificationsView model={{ dnd: false, items: [{ ...mocks.notifications.items[0], canInvokeActions: false }] }} intents={send}/>); expect((screen.getByText('Review build') as HTMLButtonElement).disabled).toBe(true); rerender(<NotificationsView model={{ dnd: false, items: [] }} intents={send}/>); expect(screen.getByText('All caught up')).toBeTruthy(); expect((screen.getByText('Clear all') as HTMLButtonElement).disabled).toBe(true); });
  it('bounded images accept local PNG only no remote or file URLs', () => { const { container, rerender } = render(<SafeImage src={mocks.fixturePng} alt="Attachment"/>); expect(screen.getByAltText('Attachment').className).toContain('bounded-image'); for (const src of ['https://tracking.invalid/p.png', 'file:///private/image.png', 'data:image/svg+xml,<svg/>', 'data:image/png;base64,' + 'A'.repeat(1400000)]) { rerender(<SafeImage src={src} alt="Attachment"/>); expect(container.querySelector('img')).toBeNull(); } });
});
describe('ClipboardView', () => {
  it('concealed mock is never rendered or actionable', () => {
    const malicious = { ...mocks.clipboard.items[0], id: 'concealed-id', preview: 'CONCEALED SENTINEL', concealed: true };
    const model: ClipboardModel = { paused: false, items: [...mocks.clipboard.items, malicious] }; const { container } = render(<ClipboardView model={model} intents={intents()}/>);
    expect(container.innerHTML).not.toContain('CONCEALED SENTINEL'); expect(container.innerHTML).not.toContain('concealed-id'); expect(screen.getAllByRole('listitem')).toHaveLength(3);
  });
  it('text image files and paused banner with pin delete copy resume intents', () => { const send = intents(); render(<ClipboardView model={{ ...mocks.clipboard, paused: true }} intents={send}/>); expect(screen.getByLabelText('text clipboard entry')).toBeTruthy(); expect(screen.getByLabelText('image clipboard entry')).toBeTruthy(); expect(screen.getByLabelText('files clipboard entry')).toBeTruthy(); expect(screen.getByRole('status').textContent).toContain('Capture paused'); fireEvent.click(screen.getByText('Resume capture')); fireEvent.click(screen.getByLabelText('Unpin entry clip-text')); fireEvent.click(screen.getByLabelText('Pin entry clip-image')); fireEvent.click(screen.getByLabelText('Delete entry clip-files')); fireEvent.click(screen.getByLabelText('Copy entry clip-text')); expect(send.clipboard.mock.calls.map(c => c[0])).toEqual([{ type: 'pause', paused: false }, { type: 'pin', id: 'clip-text', pinned: false }, { type: 'pin', id: 'clip-image', pinned: true }, { type: 'delete', id: 'clip-files' }, { type: 'copy', id: 'clip-text' }]); });
  it('empty clipboard has no content actions', () => { render(<ClipboardView model={{ paused: false, items: [] }} intents={intents()}/>); expect(screen.getByText('Clipboard history is empty')).toBeTruthy(); expect(screen.queryByText('Copy')).toBeNull(); });
});
describe('NowPlaying', () => {
  it('progress and lyrics use supplied position and active line no internal timer', () => {
    vi.useFakeTimers(); const { container, rerender } = render(<NowPlaying model={mocks.music} intents={intents()}/>);
    expect(screen.getByLabelText('Track progress').getAttribute('value')).toBe('83000'); expect(container.querySelector('[aria-current="true"]')?.textContent).toBe('A little light in a quiet orbit');
    act(() => vi.advanceTimersByTime(120000)); expect(screen.getByLabelText('Track progress').getAttribute('value')).toBe('83000');
    rerender(<NowPlaying model={{ ...mocks.music, nowPlaying: { ...mocks.music.nowPlaying!, positionMs: 92000 }, lyrics: { ...mocks.music.lyrics, activeIndex: 2 } }} intents={intents()}/>); expect(screen.getByLabelText('Track progress').getAttribute('value')).toBe('92000'); expect(container.querySelector('[aria-current="true"]')?.textContent).toBe('And find the space to begin');
  });
  it('reduced equalizer stays static across changing spectrum props', () => {
    setReducedMotion(true); const { container, rerender } = render(<NowPlaying model={mocks.music} intents={intents()}/>); const heights = () => [...container.querySelectorAll<HTMLElement>('.equalizer i')].map(i => i.style.height);
    const original = heights(); expect(screen.getByRole('img', { name: /static reduced-motion/ })).toBeTruthy(); expect(new Set(original)).toEqual(new Set(['35%']));
    rerender(<NowPlaying model={{ ...mocks.music, equalizer: { active: true, bands: mocks.music.equalizer.bands.map(b => 1 - b) } }} intents={intents()}/>); expect(heights()).toEqual(original);
  });
  it('normal equalizer reflects supplied values clamps invalid bands', () => { const { container } = render(<NowPlaying model={{ ...mocks.music, equalizer: { active: true, bands: [0, .5, 1, 2, -1, Number.NaN] } }} intents={intents()}/>); expect([...container.querySelectorAll<HTMLElement>('.equalizer i')].map(i => i.style.height)).toEqual(['0%', '50%', '100%', '100%', '0%', '0%']); });
  it('play pause previous next intents and capabilities', () => { const send = intents(); const { rerender } = render(<NowPlaying model={mocks.music} intents={send}/>); expect(screen.getByText('The Signal Garden')).toBeTruthy(); expect(screen.getByAltText('A quiet orbit artwork')).toBeTruthy(); fireEvent.click(screen.getByLabelText('Pause')); fireEvent.click(screen.getByLabelText('Next track')); fireEvent.click(screen.getByLabelText('Previous track')); expect(send.media.mock.calls.map(c => c[0])).toEqual([{ type: 'playPause' }, { type: 'next' }, { type: 'previous' }]); rerender(<NowPlaying model={{ ...mocks.music, nowPlaying: { ...mocks.music.nowPlaying!, playing: false, canControl: false } }} intents={send}/>); expect((screen.getByLabelText('Play') as HTMLButtonElement).disabled).toBe(true); });
  it('no player lyrics unavailable plain instrumental loading and missing position', () => { const send = intents(); const { rerender } = render(<NowPlaying model={mocks.musicStories.noPlayer} intents={send}/>); expect(screen.getByText(/No player/)).toBeTruthy(); expect(screen.queryByLabelText('Track progress')).toBeNull(); rerender(<NowPlaying model={mocks.musicStories.unavailable} intents={send}/>); expect(screen.getByText('Lyrics unavailable')).toBeTruthy(); rerender(<NowPlaying model={mocks.musicStories.plain} intents={send}/>); expect(screen.getByText(/A little light In a quiet orbit/)).toBeTruthy(); for (const [status, label] of [['loading', 'Loading lyrics…'], ['instrumental', 'Instrumental']] as const) { rerender(<NowPlaying model={{ ...mocks.music, lyrics: { ...mocks.music.lyrics, status } }} intents={send}/>); expect(screen.getByText(label)).toBeTruthy(); } rerender(<NowPlaying model={{ ...mocks.musicStories.paused, nowPlaying: { ...mocks.music.nowPlaying!, positionMs: null, durationMs: null } }} intents={send}/>); expect(screen.getByText('Position unavailable')).toBeTruthy(); expect(screen.queryByRole('img', { name: 'Live equalizer' })).toBeNull(); });
});
describe('QuickInfo', () => {
  it('unmapped WMO code uses generic icon and unknown text', () => { render(<QuickInfo model={mocks.quickinfoStories.unknownWeather}/>); expect(screen.getByLabelText('Generic weather icon')).toBeTruthy(); expect(screen.getByText('Unknown condition · WMO 42')).toBeTruthy(); expect(screen.getByText('Unknown temperature')).toBeTruthy(); });
  it.each([[0, 'Clear sky'], [45, 'Fog'], [61, 'Slight rain'], [71, 'Slight snowfall'], [97, 'Heavy thunderstorm'], [99, 'Thunderstorm with heavy hail']] as const)('WMO %s renders %s', (code, label) => { const model = structuredClone(mocks.quickinfo); if (model.weather.available) model.weather.value.code = code; render(<QuickInfo model={model}/>); expect(screen.getByText(label)).toBeTruthy(); });
  it('charging battery bluetooth known and unknown battery', () => { render(<QuickInfo model={mocks.quickinfo}/>); expect(screen.getByText('Charging')).toBeTruthy(); expect(screen.getByText('82%')).toBeTruthy(); expect(screen.getByText('74%')).toBeTruthy(); expect(screen.getByText('Battery unknown')).toBeTruthy(); });
  it('low critical full alerts and absent unknown charge stay explicit', () => { const model = structuredClone(mocks.quickinfoStories.lowBattery); const { rerender } = render(<QuickInfo model={model}/>); expect(screen.getByRole('status').textContent).toBe('Critical battery alert'); if (model.power.available) { model.power.value.alert = 'Low'; rerender(<QuickInfo model={model}/>); expect(screen.getByRole('status').textContent).toBe('Low battery alert'); model.power.value.alert = 'Full'; model.power.value.state = 'FullyCharged'; rerender(<QuickInfo model={model}/>); expect(screen.getByText('Fully charged')).toBeTruthy(); model.power.value.alert = null; model.power.value.percentage = null; model.power.value.state = 'Unknown'; rerender(<QuickInfo model={model}/>); expect(screen.getByText('Unknown charge')).toBeTruthy(); model.power.value.present = false; rerender(<QuickInfo model={model}/>); expect(screen.getByText('No battery present')).toBeTruthy(); } });
  it('unavailable services include reasons not fabricated values', () => { render(<QuickInfo model={mocks.quickinfoStories.unavailable}/>); expect(screen.getByText('Unavailable · Location not configured (mock)')).toBeTruthy(); expect(screen.getByText('Unavailable · UPower unavailable (mock)')).toBeTruthy(); expect(screen.getByText('Unavailable · BlueZ unavailable (mock)')).toBeTruthy(); });
});
describe('SettingsModules', () => {
  it('locked home and Settings cannot be disabled even if metadata misflags them', () => {
    const model: ModuleSettingsModel = { version: 1, modules: mocks.modules.modules.map(m => ['terminal', 'settings'].includes(m.id) ? { ...m, locked: false, enabled: false } : m) }; const send = intents(); render(<SettingsModules model={model} intents={send}/>);
    for (const label of ['Hermes', 'Settings']) { const toggle = screen.getByRole('checkbox', { name: `Enable ${label}` }) as HTMLInputElement; expect(toggle.checked).toBe(true); expect(toggle.disabled).toBe(true); fireEvent.click(toggle); } expect(send.modules).not.toHaveBeenCalled();
  });
  it('enable full-permutation order and settings stubs emit intents preserving unknown', () => { const send = intents(); render(<SettingsModules model={mocks.modules} intents={send}/>); fireEvent.click(screen.getByRole('checkbox', { name: 'Enable Now playing' })); fireEvent.click(screen.getByLabelText('Move Calendar earlier')); fireEvent.click(screen.getByLabelText('Calendar settings')); expect(send.modules.mock.calls[0][0]).toEqual({ type: 'enable', id: 'music', enabled: true }); const order = send.modules.mock.calls[1][0]; expect(order.type).toBe('order'); expect(new Set(order.order)).toEqual(new Set(mocks.modules.modules.map(m => m.id))); expect(order.order.indexOf('calendar')).toBeLessThan(order.order.indexOf('terminal')); expect(send.modules).toHaveBeenCalledWith({ type: 'settings', id: 'calendar' }); expect((screen.getByRole('checkbox', { name: 'Enable Future module' }) as HTMLInputElement).disabled).toBe(true); });
});
