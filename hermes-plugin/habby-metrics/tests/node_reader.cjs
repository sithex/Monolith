// Reads a snapshot file the way Habby's main process would (fs.readFileSync) for argv[3] ms, optionally
// sleeping argv[4] ms between reads (0 = tight loop). Prints {reads, parseErrors, openErrors}.
// Used by test_core.test_atomic_snapshot_node_reader*.
const fs = require('fs');
const file = process.argv[2];
const until = Date.now() + Number(process.argv[3] || 3000);
const gap = Number(process.argv[4] || 0);
const sleeper = new Int32Array(new SharedArrayBuffer(4));
let reads = 0, parseErrors = 0, openErrors = 0;
while (Date.now() < until) {
  let text;
  try { text = fs.readFileSync(file, 'utf8'); } catch (e) { openErrors++; continue; }
  try { JSON.parse(text); reads++; } catch (e) { parseErrors++; }
  if (gap > 0) Atomics.wait(sleeper, 0, 0, gap);
}
console.log(JSON.stringify({ reads, parseErrors, openErrors }));
