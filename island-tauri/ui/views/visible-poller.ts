// A single retained poller survives visibility changes; never two requests in flight.
export function createVisiblePoller<T>(read: () => Promise<T>, accept: (value: T, rev: number) => void,
  fail: (error: unknown) => void, schedule: (callback: () => void) => unknown = callback => setTimeout(callback, 1500),
  cancel: (id: unknown) => void = id => clearTimeout(id as ReturnType<typeof setTimeout>)) {
  let visible = false, busy = false, generation = 0, requests = 0, timer: unknown = null;
  async function poll() {
    timer = null;
    if (!visible || busy) return;
    busy = true; const epoch = generation, rev = ++requests;
    try { const value = await read(); if (visible && epoch === generation) accept(value, rev); }
    catch (error) { if (visible && epoch === generation) fail(error); }
    finally {
      busy = false;
      // A resume during an old in-flight read must refresh immediately after it drains,
      // rather than accepting its stale result or waiting a full polling interval.
      if (visible && epoch !== generation) void poll();
      else if (visible) timer = schedule(() => { void poll(); });
    }
  }
  return {
    setVisible(on: boolean) {
      if (visible === on) return;
      visible = on; generation++;
      if (timer !== null) { cancel(timer); timer = null; }
      if (visible) void poll();
    },
    diagnostics: () => ({ visible, busy, requests }),
    dispose() { visible = false; generation++; if (timer !== null) cancel(timer); },
  };
}
