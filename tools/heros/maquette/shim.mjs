// Toile minimale : fillRect, fillStyle, globalAlpha, clearRect, save/restore.
// Composition source-over en alpha droit, flottants, arrondi à l'écriture.
import zlib from 'zlib';
function parse(c) {
  if (c[0] === '#') { const n = parseInt(c.slice(1), 16); return [n >> 16 & 255, n >> 8 & 255, n & 255, 1]; }
  const m = c.match(/rgba?\(([^)]*)\)/); if (!m) throw new Error('couleur ' + c);
  const p = m[1].split(',').map(Number); return [p[0], p[1], p[2], p.length > 3 ? p[3] : 1];
}
export function toile() {
  const px = new Float64Array(64 * 64 * 4);
  const pile = [];
  const g = {
    fillStyle: '#000000', globalAlpha: 1, imageSmoothingEnabled: true,
    clearRect(x, y, w, h) { for (let j = y; j < y + h; j++) for (let i = x; i < x + w; i++) px.fill(0, (j * 64 + i) * 4, (j * 64 + i) * 4 + 4); },
    fillRect(x, y, w, h) {
      const [r, gg, b, a0] = parse(this.fillStyle); const a = a0 * this.globalAlpha;
      for (let j = Math.max(0, y); j < Math.min(64, y + h); j++) for (let i = Math.max(0, x); i < Math.min(64, x + w); i++) {
        const k = (j * 64 + i) * 4; const da = px[k + 3] / 255; const oa = a + da * (1 - a);
        if (oa <= 0) { px.fill(0, k, k + 4); continue; }
        px[k] = Math.round((r * a + px[k] * da * (1 - a)) / oa);
        px[k + 1] = Math.round((gg * a + px[k + 1] * da * (1 - a)) / oa);
        px[k + 2] = Math.round((b * a + px[k + 2] * da * (1 - a)) / oa);
        px[k + 3] = Math.round(oa * 255);
      }
    },
    save() { pile.push([this.fillStyle, this.globalAlpha]); },
    restore() { [this.fillStyle, this.globalAlpha] = pile.pop(); }
  };
  return { width: 64, height: 64, getContext: () => g, px, png: () => png(px) };
}
function crc(buf) { let c, t = []; for (let n = 0; n < 256; n++) { c = n; for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1; t[n] = c >>> 0; } let x = 0xffffffff; for (const b of buf) x = t[(x ^ b) & 255] ^ (x >>> 8); return (x ^ 0xffffffff) >>> 0; }
function chunk(type, data) { const l = Buffer.alloc(4); l.writeUInt32BE(data.length); const td = Buffer.concat([Buffer.from(type), data]); const c = Buffer.alloc(4); c.writeUInt32BE(crc(td)); return Buffer.concat([l, td, c]); }
function png(px) {
  const raw = Buffer.alloc(64 * (64 * 4 + 1));
  for (let y = 0; y < 64; y++) { raw[y * 257] = 0; for (let x = 0; x < 256; x++) raw[y * 257 + 1 + x] = px[y * 256 + x]; }
  const ihdr = Buffer.alloc(13); ihdr.writeUInt32BE(64, 0); ihdr.writeUInt32BE(64, 4); ihdr[8] = 8; ihdr[9] = 6;
  return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk('IHDR', ihdr), chunk('IDAT', zlib.deflateSync(raw)), chunk('IEND', Buffer.alloc(0))]);
}
