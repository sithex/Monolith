import { expect, it } from 'vitest';
import { fixturePng } from '../src/app-mocks';
// A DOM img element does not prove WebKit can decode its PNG. Check every CRC,
// including IDAT; the original one-pixel mock had a broken IDAT checksum.
it('artwork fixture PNG chunks have valid checksums and real dimensions', () => {
  const bytes = Uint8Array.from(atob(fixturePng.split(',')[1]), c => c.charCodeAt(0));
  expect([...bytes.slice(0, 8)]).toEqual([137, 80, 78, 71, 13, 10, 26, 10]);
  const data = new DataView(bytes.buffer); let offset = 8; const chunks: string[] = [];
  while (offset < bytes.length) {
    const size = data.getUint32(offset); const start = offset + 4; const end = start + size + 4;
    const kind = String.fromCharCode(...bytes.slice(start, start + 4)); chunks.push(kind);
    let crc = 0xffffffff;
    for (const byte of bytes.slice(start, end)) {
      crc ^= byte;
      for (let n = 0; n < 8; n++) crc = (crc >>> 1) ^ ((crc & 1) ? 0xedb88320 : 0);
    }
    expect((crc ^ 0xffffffff) >>> 0, `${kind} checksum`).toBe(data.getUint32(end));
    if (kind === 'IHDR') { expect(data.getUint32(start + 4)).toBe(64); expect(data.getUint32(start + 8)).toBe(64); }
    offset = end + 4;
  }
  expect(offset).toBe(bytes.length); expect(chunks).toContain('IDAT'); expect(chunks.at(-1)).toBe('IEND');
});
