import type { ITheme, Terminal } from '@xterm/xterm';

/**
 * The Hermes TUI paints the terminal's DEFAULT colours from its skin (OSC 10/11, reset by
 * OSC 110/111). In the island that made xterm's background opaque, so the glass vanished
 * behind the terminal whenever the TUI (re)applied its skin. The island owns those two
 * colours: set/reset requests are swallowed, queries (`?`) still get xterm's own answer.
 */
export function keepIslandColours(terminal: Pick<Terminal, 'parser'>) {
  const owned = (data: string) => !data.split(';').every(part => part.trim() === '?');
  return [terminal.parser.registerOscHandler(10, owned), terminal.parser.registerOscHandler(11, owned),
    terminal.parser.registerOscHandler(110, () => true), terminal.parser.registerOscHandler(111, () => true)];
}

const ANSI_KEYS = ['black', 'red', 'green', 'yellow', 'blue', 'magenta', 'cyan', 'white',
  'brightBlack', 'brightRed', 'brightGreen', 'brightYellow', 'brightBlue', 'brightMagenta', 'brightCyan', 'brightWhite'] as const;
type Roles = { background: string; text: string; accent: string; border: string };

/**
 * xterm's theme for a palette. The background stays transparent so the island surface (and
 * its blur/tint) shows through and a partial last row never paints a strip.
 * Without `ansi16` (the original schemes, Hermes skin, Custom, Album) this is exactly the
 * pre-Nov 6 theme: xterm's built-in ANSI colours. With a scheme's official ANSI 16 the
 * transparent background also carries the scheme's base RGB (alpha 00), so OSC 11 queries
 * (the Hermes TUI's light/dark probe) and inverse video see the real polarity.
 */
export function xtermTheme(palette: Roles, ansi16?: readonly string[]): ITheme {
  const theme: ITheme = { background: '#00000000', foreground: palette.text, cursor: palette.accent, selectionBackground: palette.border };
  if (!ansi16 || ansi16.length !== 16) return theme;
  theme.background = `${palette.background}00`;
  theme.cursorAccent = palette.background;
  ANSI_KEYS.forEach((key, i) => { theme[key] = ansi16[i]; });
  return theme;
}
