import type { TerminalEvent, TerminalSnapshot } from '../ipc/client';
const after = (next: number, previous: number) => ((next - previous) | 0) > 0;
export function createTerminalFlow(writer: { write(data: string, parsed: () => void): void; reset(): void },
  ack: (seq: number) => Promise<void>, fail: (error: unknown) => void,
  schedule: (callback: () => void) => unknown = callback => setTimeout(callback, 16)) {
  let seq: number | undefined, pending: number | null = null, scheduled = false, disposed = false;
  let sending = false;
  async function flush() {
    scheduled = false;
    if (disposed || sending || pending === null) return;
    const value = pending; pending = null; sending = true;
    try { await ack(value); } catch (error) { fail(error); }
    finally { sending = false; if (pending !== null && !disposed) queue(); }
  }
  function queue() { if (!scheduled) { scheduled = true; schedule(() => { void flush(); }); } }
  function write(data: string, value: number) {
    writer.write(data, () => {
      if (disposed) return;
      if (pending === null || after(value, pending)) pending = value;
      queue();
    });
  }
  function snapshot(value: TerminalSnapshot) {
    if (seq !== undefined && !after(value.seq, seq)) return;
    writer.reset(); seq = value.seq; write(value.data, value.seq);
  }
  return { snapshot, event(event: TerminalEvent) {
    if (event.type === 'snapshot') snapshot(event);
    else if (event.type === 'data' && (seq === undefined || after(event.seq, seq))) {
      seq = event.seq; write(event.data, event.seq);
    }
  }, dispose() { disposed = true; pending = null; } };
}
