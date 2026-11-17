/** Friendly names for MPRIS bus-name suffixes (`org.mpris.MediaPlayer2.<id>`), keyed by the lower-case id. */
const KNOWN: Readonly<Record<string, string>> = {
  'plasma-browser-integration': 'Browser',
  firefox: 'Firefox', chromium: 'Chromium', chrome: 'Google Chrome', 'google-chrome': 'Google Chrome', brave: 'Brave', vivaldi: 'Vivaldi', edge: 'Microsoft Edge',
  spotify: 'Spotify', spotifyd: 'Spotify', vlc: 'VLC', mpv: 'mpv', elisa: 'Elisa', haruna: 'Haruna', strawberry: 'Strawberry', amberol: 'Amberol',
  lollypop: 'Lollypop', rhythmbox: 'Rhythmbox', clementine: 'Clementine', audacious: 'Audacious', kdeconnect: 'KDE Connect',
};

/**
 * The player name to show for a raw MPRIS identifier. Strips the per-instance suffix browsers and VLC add
 * (`chromium.instance1234`), maps well-known ids, takes the last segment of a reverse-DNS id, and otherwise
 * title-cases the words of the identifier so nothing raw is shown.
 */
export function friendlyPlayerName(raw: string): string {
  const id = raw.trim().replace(/\.instance\d+$/i, '');
  if (!id) return 'Player';
  const known = KNOWN[id.toLowerCase()] ?? KNOWN[(id.split('.').pop() ?? '').toLowerCase()];
  if (known) return known;
  const words = (id.includes('.') ? id.split('.').pop() ?? id : id).split(/[-_.\s]+/).filter(Boolean);
  if (!words.length) return 'Player';
  return words.map(word => word.charAt(0).toLocaleUpperCase() + word.slice(1)).join(' ');
}
