import type { Tab } from '../settings-model';

// One stroke weight and viewBox across the compact icon-only navigation rail.
const paths: Record<Tab, string> = {
  general: 'M4 7h16M4 17h16M8 4v6M16 14v6',
  notifications: 'M6 9a6 6 0 0 1 12 0v6l2 3H4l2-3ZM10 21h4',
  look: 'M12 3a9 9 0 1 0 9 9c0-2-3-1-5-2s-1-5-4-7ZM7 9h.01M8 15h.01M13 17h.01',
  notch: 'M3 5h18v12H3ZM9 21h6M12 17v4M8 9h.01M16 9h.01M9 12q3 3 6 0',
  terminal: 'm5 6 5 6-5 6M13 18h6',
  apps: 'M4 4h6v6H4ZM14 4h6v6h-6ZM4 14h6v6H4ZM14 14h6v6h-6Z',
  island: 'M3 7h18v13H3ZM8 7V4h8v3M12 11v6m-3-3 3 3 3-3',
};
export function SectionIcon({ section }: { section: Tab }) {
  return <svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d={paths[section]} /></svg>;
}
