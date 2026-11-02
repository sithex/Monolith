// Pure parse-credit adapter, injectable for node's fake-Terminal tests. The production
// installer imports exactly the legacy renderer's xterm module (xterm-ack-adapter.ts).
export type Packet = { seq: number; data: string };
type Callback = () => void;
export type Writer = { write(data: string | Uint8Array, callback?: Callback): void };
type Prototype = { write: Writer["write"] };
const after = (a: number, b: number) => ((a - b) | 0) > 0;
export const ACK_INTERVAL_MS = 16;

export function createParseAck(
  ack: (seq: number) => Promise<unknown>,
  schedule: (callback: Callback) => unknown = (callback) => setTimeout(callback, ACK_INTERVAL_MS),
) {
  let pending: Packet[] = [];
  let snapshot: Packet | null = null;
  let nextAck: number | null = null;
  let scheduled = false;
  let paused = false;
  let observed: Writer | null = null;
  let generation = 0;
  const flush = () => {
    scheduled = false;
    if (paused || nextAck === null) return;
    const seq = nextAck;
    nextAck = null;
    void ack(seq).catch((error: unknown) => console.error(error));
  };
  const parsed = (seq: number) => {
    pending = pending.filter((entry) => after(entry.seq, seq));
    if (snapshot && !after(snapshot.seq, seq)) snapshot = null;
    if (nextAck === null || after(seq, nextAck)) nextAck = seq;
    if (!scheduled) {
      scheduled = true;
      schedule(flush);
    }
  };
  return {
    record(packet: Packet) {
      if (snapshot && !after(packet.seq, snapshot.seq)) return;
      if (!pending.some((entry) => entry.seq === packet.seq)) pending.push(packet);
    },
    reset() {
      generation++;
      pending = [];
      snapshot = null;
      nextAck = null;
    },
    recordSnapshot(packet: Packet) {
      // The attach return and channel Snapshot describe the same replay. Register once.
      if (snapshot?.seq === packet.seq && snapshot.data === packet.data) return;
      snapshot = packet;
      // Snapshot is parsed before the renderer's queued Data and subsumes skipped packets.
      pending = [packet, ...pending.filter((entry) => after(entry.seq, packet.seq))];
      if (!packet.data) parsed(packet.seq); // nothing for xterm to parse
    },
    install(prototype: Prototype) {
      const original = prototype.write;
      const adapter = this;
      prototype.write = function (data, callback) {
        observed = this;
        const entry = pending[0];
        if (typeof data !== "string" || !entry || data !== entry.data) {
          original.call(this, data, callback);
          return;
        }
        // Claim the write now; parsing remains asynchronous. Never ack at delivery/write time.
        pending.shift();
        const writeGeneration = generation;
        original.call(this, data, () => {
          try { callback?.(); } finally { if (writeGeneration === generation) parsed(entry.seq); }
        });
      };
      return { restore: () => { prototype.write = original; }, adapter };
    },
    setAckPaused(value: boolean) {
      paused = value;
      if (!paused && nextAck !== null && !scheduled) { scheduled = true; schedule(flush); }
    },
    terminal() { return observed; },
  };
}
