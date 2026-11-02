import type { Terminal } from '@xterm/xterm';

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
