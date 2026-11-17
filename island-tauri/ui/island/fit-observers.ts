/**
 * Observers behind the content-fit measurement (IslandSurface). A ResizeObserver watches the page's root and the
 * children of the two inner boxes whose growth the root cannot show (Settings' own scroller, a fit-width row).
 * A MutationObserver covers only what a size observer cannot see: a pane being shown or hidden, a pane's root
 * element being swapped, and children added to or removed from the scroller / the row. It never watches the
 * whole page subtree (lyrics lines, process rows and notifications used to wake it every frame).
 */
export function createFitObservers(root: Element, onChange: () => void) {
  const watched = new Set<Element>(), narrow = new Set<Element>();
  const ro = new ResizeObserver(() => onChange());
  const mo = new MutationObserver(() => onChange());
  const panes = Array.from(root.querySelectorAll?.('.view-pane > .app-pane') ?? []);
  if (panes.length) for (const pane of panes) mo.observe(pane, { childList: true, attributes: true, attributeFilter: ['hidden'] });
  else mo.observe(root.querySelector('.view-pane') ?? root, { childList: true, subtree: true, attributes: true, attributeFilter: ['hidden'] });
  return {
    /** Re-point the observers at the current page: its root, its scroller's and its row's children. */
    retarget(child: Element, scroller: Element | null, row: Element | null) {
      const want = new Set<Element>([child]);
      for (const inner of [scroller, row]) for (const c of Array.from(inner?.children ?? [])) want.add(c);
      for (const el of watched) if (!want.has(el)) { ro.unobserve(el); watched.delete(el); }
      for (const el of want) if (!watched.has(el)) { ro.observe(el); watched.add(el); }
      for (const inner of [scroller, row]) if (inner && !narrow.has(inner)) { mo.observe(inner, { childList: true }); narrow.add(inner); }
    },
    dispose() { ro.disconnect(); mo.disconnect(); watched.clear(); narrow.clear(); },
  };
}
