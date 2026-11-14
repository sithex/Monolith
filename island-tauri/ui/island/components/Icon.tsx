import type { IconName } from '../models';
const paths: Record<IconName, string> = {
  weather: 'M3 8h18M3 12h18M3 16h18M12 3v2m0 14v2',
  sun: 'M12 7a5 5 0 1 1 0 10 5 5 0 0 1 0-10M12 1v3m0 16v3M1 12h3m16 0h3M4 4l2 2m12 12 2 2M20 4l-2 2M6 18l-2 2',
  cloud: 'M6 19a5 5 0 1 1 0-10 7 7 0 0 1 13-1 5.5 5.5 0 0 1-1 11z',
  rain: 'M6 14a4 4 0 0 1 0-8 6 6 0 0 1 11 0 4 4 0 0 1 1 8M7 17l-2 4m8-4-2 4m8-4-2 4',
  snow: 'M12 2v20M3 7l18 10M3 17 21 7M9 4l3 3 3-3M9 20l3-3 3 3',
  storm: 'M6 14a4 4 0 0 1 0-8 6 6 0 0 1 11 0 4 4 0 0 1 1 8M13 13l-4 6h5l-3 5',
  bluetooth: 'm6 7 12 10-6 5V2l6 5L6 17',
  terminal: 'm5 7 5 5-5 5m8 0h6', grid: 'M3 3h7v7H3zM14 3h7v7h-7zM3 14h7v7H3zM14 14h7v7h-7z',
  timer: 'M12 8v5l3 2M9 2h6m-3 2a9 9 0 1 1-6.4 2.6', settings: 'm9 3 1-1h4l1 3 3 1 3-1 1 4-2 2v3l2 2-2 3-3-1-3 1-1 2h-4l-1-3-3-1-2 1-2-3 2-2v-3L2 8l2-3 3 1zM15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0',
  volume: 'M4 9h4l5-5v16l-5-5H4zM17 8a6 6 0 0 1 0 8m3-11a10 10 0 0 1 0 14', volumeOff: 'M4 9h4l5-5v16l-5-5H4zM16 9l6 6m0-6-6 6',
  quote: 'M4 5h16v11H9l-5 4zM8 9h8M8 12h5', music: 'M10 18V5l10-2v12M10 18a3 3 0 1 1-3-3h3m10 0a3 3 0 1 1-3-3h3',
  controls: 'M3 6h18M3 12h18M3 18h18M7 3v6m10 0v6m-8 0v6', clipboard: 'M9 4H5v17h14V4h-4M9 2h6v5H9z',
  calendar: 'M3 5h18v16H3zM7 2v6m10-6v6M3 10h18M7 14h2m4 0h2m2 3h-2m-6 0H7', bell: 'M5 17h14l-2-3V9a5 5 0 0 0-10 0v5zM10 21h4',
  download: 'M12 3v12m-5-5 5 5 5-5M4 17v4h16v-4',
  shelf: 'M3 15h18v5H3zM7 15V9h6v6M15 15V5h3v10', note: 'M4 3h16v18H4zM8 8h8M8 12h8M8 16h5',
  spark: 'm12 2 3 7 7 3-7 3-3 7-3-7-7-3 7-3z', monitor: 'M2 3h20v14H2zM8 21h8m-4-4v4M5 11l4-4 4 6 6-5',
  back: 'm14 5-7 7 7 7', more: 'M4 12h.01M12 12h.01M20 12h.01', mic: 'M9 3a3 3 0 0 1 6 0v8a3 3 0 0 1-6 0zM5 9v2a7 7 0 0 0 14 0V9M12 18v4m-4 0h8',
  moon: 'M21 14A9 9 0 0 1 10 3a9 9 0 1 0 11 11', power: 'M12 2v10M6 5a9 9 0 1 0 12 0', coffee: 'M3 7h13v9a5 5 0 0 1-10 0V7m13 1h2a3 3 0 0 1 0 6h-2M5 22h13M7 2v2m5-2v2',
  gamepad: 'M7 7h10a5 5 0 0 1 4.9 6l-.8 4a2.5 2.5 0 0 1-4.3 1.2L14.5 16h-5l-2.3 2.2A2.5 2.5 0 0 1 2.9 17l-.8-4A5 5 0 0 1 7 7M8 10v4m-2-2h4M15.5 11h.01M17.5 13h.01',
  play: 'M7 4v16l6-4V8zM16 5v14m4-14v14', next: 'M5 5v14l9-7zM18 5v14', prev: 'M19 5v14l-9-7zM6 5v14',
  minimize: 'M5 12h14', resize: 'M14 4h6v6M10 20H4v-6M20 4l-7 7M4 20l7-7',
  compact: 'M12 3v6m-3-3 3 3 3-3M12 21v-6m-3 3 3-3 3 3M4 12h16', spacious: 'M4 8V4h4M16 4h4v4M20 16v4h-4M8 20H4v-4M9 12h6', bigger: 'M3 7V3h4M17 3h4v4M21 17v4h-4M7 21H3v-4M7 7h10v10H7z',
  network: 'M3 9a15 15 0 0 1 18 0M6 13a10 10 0 0 1 12 0m-9 4a5 5 0 0 1 6 0m-3 4h.01', camera: 'M2 7h5l2-3h6l2 3h5v14H2zM16 13a4 4 0 1 1-8 0 4 4 0 0 1 8 0', picker: 'm14 3 7 7-2 2-2-2-10 10H3v-4L13 6l-1-1z',
};
export function Icon({ name }: { name: IconName }) {
  return <svg aria-hidden="true" viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" strokeWidth="1.65" strokeLinecap="round" strokeLinejoin="round"><path d={paths[name]} /></svg>;
}
