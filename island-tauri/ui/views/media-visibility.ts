// A dedicated bounded visibility channel, never queued behind calendar/file actions.
// A requested hide cannot be coalesced away by a subsequent rapid reopen.
export function createMediaVisibility(send: (visible: boolean) => Promise<unknown>, fail: (error: unknown) => void) {
  let desired = false, offRequired = false, busy = false, dirty = false;
  async function drain() {
    if (busy || !dirty) return;
    busy = true; dirty = false;
    const visible = offRequired ? false : desired;
    if (!visible) offRequired = false;
    try { await send(visible); }
    catch (error) { fail(error); }
    finally {
      busy = false;
      if (desired !== visible || offRequired) dirty = true;
      if (dirty) void drain();
    }
  }
  return {
    set(visible: boolean) { desired = visible; if (!visible) offRequired = true; dirty = true; void drain(); },
  };
}
