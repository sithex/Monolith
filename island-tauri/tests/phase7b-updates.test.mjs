import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { load } from './phase7b-loader.mjs';
const { UpdatesApp, versionDifference, versionBump, checkedLabel, groupUpdates, updateCommands, sourceLabel } = load('ui/island/components/UpdatesApp.tsx');
const { ControlsApp } = load('ui/island/components/ControlsApp.tsx');
const adapters = load('ui/views/app-adapters.ts');
const snapshot = patch => ({ rev: 1, backend: 'pacman', available: true, detail: 'read-only-checks', count: null, repoCount: null, aurCount: null, needsReboot: false, stale: false, error: null, items: [], ageSecs: null, nextCheckInSecs: 60, ...patch });
const pkg = patch => ({ name: 'fixture-package', oldVersion: '1.2.3', newVersion: '1.2.4', ignored: false, source: 'repo', arch: null, ...patch });
const render = patch => renderToStaticMarkup(createElement(UpdatesApp, { snapshot: snapshot(patch), onRefresh: async () => {} }));

test('Updates unknown counts never become zero or a successful empty state', () => {
  const html = render({ available: false, stale: true, error: 'repo: command failed' });
  assert.match(html, /updates-total">\?<\/span>/);
  assert.match(html, /Couldn&#x27;t check · repo: command failed/);
  assert.match(html, /Stale · showing last known results/);
  assert.doesNotMatch(html, /Everything is up to date|Up to date/);
  for (const patch of [{ count: 0, stale: true }, { count: 0, error: 'offline' }, { count: 0, available: false }]) assert.doesNotMatch(render(patch), /Everything is up to date/);
});

test('Updates healthy zero uses the friendly check state and supplied snapshot age', () => {
  const html = render({ count: 0, repoCount: 0, aurCount: 0, ageSecs: 240 });
  assert.match(html, /Everything is up to date/); assert.match(html, /✓/); assert.match(html, /Checked 4 min ago/);
  assert.equal(checkedLabel(null), 'No successful check yet'); assert.equal(checkedLabel(NaN), 'No successful check yet');
  assert.equal(checkedLabel(-1), 'No successful check yet'); assert.equal(checkedLabel(59), 'Checked just now');
  assert.equal(checkedLabel(3600), 'Checked 1 hr ago'); assert.equal(checkedLabel(172800), 'Checked 2 days ago');
});

test('Updates groups sources and keeps ignored packages distinct from actionable counts', () => {
  const items = [pkg(), pkg({ name: 'aur-editor', source: 'aur', oldVersion: '1.0', newVersion: '2.0' }), pkg({ name: 'held-kernel', ignored: true }), pkg({ name: 'future-source', source: 'custom' })];
  const html = render({ count: 3, repoCount: 1, aurCount: 1, items, needsReboot: true });
  assert.match(html, /Official repositories/); assert.match(html, /AUR/); assert.match(html, /via paru/);
  assert.match(html, /1 · 1 ignored/); assert.match(html, /Ignored/); assert.match(html, /custom/);
  assert.match(html, /A restart may be needed/); assert.equal(items.length, 4);
  assert.deepEqual(groupUpdates(items, ' aur-editor ').map(group => group.source), ['aur']);
  assert.equal(groupUpdates(items, 'missing').length, 0);
  assert.equal(groupUpdates(items, '1.2.4')[0].items.length, 2);
  assert.equal(groupUpdates(items, '1.2.4').flatMap(group => group.items).length, 3);
});

test('Updates common-prefix highlighting preserves version strings exactly', () => {
  assert.deepEqual(versionDifference('1.22.3-1', '1.22.4-2'), { common: '1.22.', changed: '4-2' });
  assert.deepEqual(versionDifference('1', '1.2'), { common: '1', changed: '.2' });
  assert.deepEqual(versionDifference('2.0', '1.0'), { common: '', changed: '1.0' });
  assert.deepEqual(versionDifference('1.0', '1.0'), { common: '1.0', changed: '' });
  assert.match(render({ count: 1, items: [pkg()] }), /1\.2\.<mark>4<\/mark>/);
});

test('Updates bump hints classify numeric increases only and never guess epochs pkgrel VCS or downgrades', () => {
  assert.equal(versionBump('1.2.3', '2.0.0'), 'Major'); assert.equal(versionBump('1.2.3', '1.3.0'), 'Minor'); assert.equal(versionBump('1.2.3', '1.2.4'), 'Patch');
  assert.equal(versionBump('1.9007199254740992', '1.9007199254740993'), 'Minor');
  for (const [oldVersion, newVersion] of [['1.2.3-1', '1.2.4-1'], ['1:1.2', '1:2.0'], ['r23.abc', 'r24.def'], ['1.2', '1.2.1'], ['1.2', '1.2'], ['2.1', '1.9'], ['1.2.3.4', '1.2.3.5'], ['v1.2', 'v2.0']]) assert.equal(versionBump(oldVersion, newVersion), null);
});

test('Updates search appears only for longer lists and upgrades require explicit terminal review', () => {
  assert.doesNotMatch(render({ items: Array.from({ length: 8 }, (_, i) => pkg({ name: `pkg-${i}` })) }), /Filter packages/);
  const html = render({ items: Array.from({ length: 9 }, (_, i) => pkg({ name: `pkg-${i}` })) });
  assert.match(html, /Filter packages/); assert.match(html, /Available package updates/);
  assert.match(html, /Update…/); assert.match(html, /Review and install in your terminal/);
  assert.doesNotMatch(html, /Open in terminal|sudo pacman|paru -Syu/);
});

const dnfPkg = patch => pkg({ source: 'dnf', arch: 'x86_64', oldVersion: '2.44-1.fc45', newVersion: '2.44-2.fc45', ...patch });
const controls = updates => renderToStaticMarkup(createElement(ControlsApp, { model: { toggles: null, tiles: null, updates }, intents: new Proxy({}, { get: () => () => {} }) }));

test('Updates dnf backend renders Fedora labels, multilib rows and only the dnf command', () => {
  const items = [dnfPkg({ name: 'glibc' }), dnfPkg({ name: 'glibc', arch: 'i686' }), dnfPkg({ name: 'LibRaw', oldVersion: '0.22.2-1.fc45', newVersion: '0.22.3-1.fc45' }), dnfPkg({ name: 'obsoleter', arch: 'noarch', oldVersion: '' })];
  const html = render({ backend: 'dnf', count: 4, repoCount: 4, items, needsReboot: true });
  assert.match(html, /aria-label="Fedora repositories · dnf"/); assert.match(html, /Fedora repositories<small>dnf<\/small>/);
  assert.match(html, /updates-group-count">4</);
  assert.match(html, /glibc\.x86_64/); assert.match(html, /glibc\.i686/);
  assert.match(html, /<strong>LibRaw<\/strong>/, 'arch only where a name repeats');
  assert.match(html, /title="Installed version unknown">\?</);
  assert.doesNotMatch(html, /AUR|paru|pacman|Official repositories/);
  assert.equal(groupUpdates(items, 'i686').flatMap(group => group.items).length, 1);
  assert.deepEqual(updateCommands('dnf'), [{ label: 'Fedora repositories · dnf', command: 'sudo dnf upgrade --refresh' }]);
  assert.deepEqual(updateCommands('pacman').map(c => c.command), ['paru -Syu', 'sudo pacman -Syu']);
  assert.deepEqual(updateCommands('unsupported'), []);
  assert.deepEqual(sourceLabel('repo'), { title: 'Official repositories', via: 'pacman', aria: 'Official repositories' });
  assert.match(controls(snapshot({ backend: 'dnf', count: 4, repoCount: 4 })), /4 Fedora repositories · dnf/);
  assert.doesNotMatch(controls(snapshot({ backend: 'dnf', count: 4, repoCount: 4 })), /AUR/);
  assert.match(controls(snapshot({ count: 3, repoCount: 2, aurCount: 1 })), /2 repo · 1 AUR/, 'pacman copy unchanged');
  assert.match(controls(snapshot({ count: 1, repoCount: 1, aurCount: null })), /1 repo · Unknown AUR/, 'no paru: repo count still shown');
});

test('Updates unsupported backend is a calm empty state without error, count, refresh or commands', () => {
  const html = render({ backend: 'unsupported', available: false, detail: 'unsupported-system', nextCheckInSecs: 0 });
  assert.match(html, /Not supported on this distro/); assert.match(html, /Update checks aren’t available here/);
  assert.doesNotMatch(html, /Couldn|updates-status-error|unsupported-system|updates-total|Check updates|Update…|Stale|sudo|paru/);
  const card = controls(snapshot({ backend: 'unsupported', available: false, detail: 'unsupported-system' }));
  assert.match(card, /Not supported on this distro/);
  assert.doesNotMatch(card, /update-badge|Unknown|unsupported-system|Check updates/);
});

test('Updates adapter accepts the dnf wire shape and still rejects unknown backends and sources', () => {
  const raw = { ...snapshot({ backend: 'dnf', count: 1, repoCount: 1, items: [dnfPkg({ name: 'glibc', arch: 'i686' })] }) };
  const model = adapters.adaptIndependentControls(null, null, raw).updates;
  assert.equal(model.backend, 'dnf'); assert.equal(model.items[0].arch, 'i686'); assert.equal(model.items[0].source, 'dnf');
  const { backend, ...legacy } = snapshot({ items: [{ name: 'linux', oldVersion: '1', newVersion: '2', ignored: false, source: 'repo' }] });
  assert.equal(backend, 'pacman');
  const old = adapters.adaptIndependentControls(null, null, legacy).updates;
  assert.equal(old.backend, 'pacman'); assert.equal(old.items[0].arch, null);
  assert.throws(() => adapters.adaptIndependentControls(null, null, snapshot({ backend: 'apt' })));
  assert.throws(() => adapters.adaptIndependentControls(null, null, snapshot({ items: [dnfPkg({ source: 'apt' })] })));
});
