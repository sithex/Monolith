import type { Tab } from '../settings-model';
import { SECTION_GLYPHS } from '../../island/components/icon-glyphs';
import { glyphPath } from '../../island/components/Icon';

// The settings rail uses the same Framework7 glyphs and animation layers as Icon (icon-motion.css):
// appear on mount, hover gesture. Sections that match an app icon borrow its gesture.
const GESTURE: Partial<Record<Tab, string>> = { general: 'settings', notifications: 'bell', terminal: 'terminal', apps: 'grid' };
export function SectionIcon({ section }: { section: Tab }) {
  return <svg className="icon" data-icon={GESTURE[section] ?? `section-${section}`} viewBox="0 0 56 56" width="20" height="20" fill="currentColor" aria-hidden="true">
    <g className="icon-hover"><g className="icon-fx"><g className="icon-glyph">{(SECTION_GLYPHS[section] ?? []).map(glyphPath)}</g></g></g></svg>;
}
