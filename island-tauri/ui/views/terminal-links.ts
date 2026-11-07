/** xterm's mouse tracking mode while a link is clicked. */
export type MouseTrackingMode = 'none' | 'x10' | 'vt200' | 'drag' | 'any';

/**
 * Open an OSC 8 link from xterm only when no terminal app tracks the mouse. With tracking on, the
 * app receives the click and opens the link itself (the Hermes TUI does), so a second opener here
 * would launch every link twice.
 */
export function shouldOpenTerminalLink(mode: MouseTrackingMode): boolean {
  return mode === 'none';
}
