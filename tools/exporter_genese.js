// Exporte les découpes de la maquette HTML de la Genèse (docs/cinematiques/genese.html) pour les
// cartes de papier des cinématiques : chaque pièce de papier dessinée à la racine d'une scène
// (`piece`) devient une image PNG, avec sa place à l'écran au fil du temps. Le fond de chaque scène
// (ciel, étoiles, lueurs : tout sauf les pièces) est capturé une fois par seconde.
//
// Usage : node tools/exporter_genese.js <dossier de sortie>
// Sortie : <dossier>/pieces/*.png et <dossier>/manifeste.json, que
// tools/genese_vers_cinematique.py change en assets/cinematics/genese.json.

const path = require('path');
const fs = require('fs');
const { chromium } = require('playwright');

const out = path.resolve(process.argv[2] || 'export-genese');
fs.mkdirSync(path.join(out, 'pieces'), { recursive: true });
const page_file = path.resolve(__dirname, '../docs/cinematiques/genese.html');

(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 1600, height: 1000 } });
  await page.goto('file://' + page_file);
  await page.waitForFunction(() => typeof ready !== 'undefined' && ready, null, { timeout: 30000 });
  const result = await page.evaluate(() => {
    playing = false;
    const S = 2; // pixels par unité de la page (1600 × 900)
    K = S;
    for (const c of [cv, bufA, bufB, tmp, vign]) { c.width = W * S; c.height = H * S; }
    const orig = piece;
    let depth = 0, mode = null, counts = null, records = null, wanted = null, images = null;
    const shapeBox = (c, o) => {
      const s = o.s ?? 1;
      let m = c.getTransform().translate(o.x || 0, o.y || 0);
      if (o.r) m = m.rotate(o.r * 180 / Math.PI);
      m = m.scale(s * (o.sx ?? 1), s * (o.sy ?? 1));
      let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
      for (const [x, y] of o.shape) {
        const p = m.transformPoint(new DOMPoint(x, y));
        x0 = Math.min(x0, p.x); y0 = Math.min(y0, p.y); x1 = Math.max(x1, p.x); y1 = Math.max(y1, p.y);
      }
      const pad = 0.06 * Math.max(x1 - x0, y1 - y0) + 8 * S;
      return [x0 - pad, y0 - pad, x1 + pad, y1 + pad];
    };
    piece = function (c, o) {
      if (mode === 'bg' && depth === 0) return; // le fond seul : ciel, étoiles, lueurs
      if (depth > 0 || !mode || (o.s ?? 1) <= 0.001) { depth++; try { orig(c, o); } finally { depth--; } return; }
      const base = `${o.seed | 0}`;
      const k = (counts[base] = (counts[base] || 0) + 1) - 1;
      const key = `${base}_${k}`;
      const box = shapeBox(c, o);
      const alpha = (o.a == null ? 1 : Math.max(0, Math.min(1, o.a))) * c.globalAlpha;
      records.push({ key, box, alpha, blend: o.blend || null, col: o.col || null });
      if (mode === 'render' && wanted.has(key) && !images[key]) {
        // Visible à la page : clipper le cadre à l'écran élargi (les bandes de ciel débordent)
        const bx = Math.max(box[0], -0.25 * W * S), by = Math.max(box[1], -0.25 * H * S);
        const ex = Math.min(box[2], 1.25 * W * S), ey = Math.min(box[3], 1.25 * H * S);
        const w = Math.max(2, Math.ceil(ex - bx)), h = Math.max(2, Math.ceil(ey - by));
        const scale = Math.min(1, 1024 / Math.max(w, h));
        const cnv = document.createElement('canvas'); cnv.width = Math.ceil(w * scale); cnv.height = Math.ceil(h * scale);
        const g = cnv.getContext('2d'); g._id = 'x' + key;
        g.setTransform(new DOMMatrix([scale, 0, 0, scale, -bx * scale, -by * scale]).multiply(c.getTransform()));
        const o2 = Object.assign({}, o, { a: 1, shadow: false, blend: undefined });
        depth++; try { orig(g, o2); } finally { depth--; }
        images[key] = { data: cnv.toDataURL('image/png'), box: [bx, by, bx + w, by + h] };
      }
    };
    // Voiles translucides posés sur toute l'image (la nuit qui tombe, un éclair blanc), avec le
    // nombre de pièces dessinées dessous : notés à
    // part (des fondus dans le jeu), et retirés du fond
    let veils = [], clipped = 0, clipStack = [];
    const [save, restore, clip] = [cA.save, cA.restore, cA.clip];
    cA.save = function () { clipStack.push(clipped); return save.call(this); };
    cA.restore = function () { clipped = clipStack.pop() || 0; return restore.call(this); };
    cA.clip = function (...args) { clipped++; return clip.apply(this, args); };
    const fillRect = cA.fillRect;
    cA.fillRect = function (x, y, w, h) {
      const veil = depth === 0 && !clipped && w >= W && h >= H && this.globalCompositeOperation === 'source-over' && typeof this.fillStyle === 'string';
      if (veil) {
        const st = this.fillStyle;
        let rgba = null;
        if (st[0] === '#') rgba = [parseInt(st.slice(1, 3), 16), parseInt(st.slice(3, 5), 16), parseInt(st.slice(5, 7), 16), 1];
        else { const m = st.match(/rgba?\(([^)]+)\)/); if (m) { const v = m[1].split(',').map(Number); rgba = [v[0], v[1], v[2], v.length > 3 ? v[3] : 1]; } }
        if (rgba) { rgba[3] *= this.globalAlpha; rgba.push(records.length); veils.push(rgba); if (mode === 'bg') return; }
      }
      return fillRect.call(this, x, y, w, h);
    };
    BOIL = 0;
    const scenes = [];
    const run = (idx, T) => {
      counts = {}; records = []; veils = []; clipped = 0; clipStack = [];
      prep(cA); cA.setTransform(S, 0, 0, S, 0, 0);
      SCENES[idx](cA, T);
      return records;
    };
    // Bornes des scènes, et pas d'échantillonnage
    const bounds = [0, ...BORNES, DUREE];
    const STEP = 0.5;
    for (let idx = 0; idx < SCENES.length; idx++) {
      const a = bounds[idx], b = bounds[idx + 1];
      mode = 'measure';
      const samples = [];
      for (let T = a + 0.05; T < b; T += STEP) {
        BOIL = 0;
        samples.push({ T, records: run(idx, T), veils: veils.slice() });
      }
      // Pour chaque pièce, l'instant où elle est la plus grande : son image
      const best = {};
      for (const s of samples) for (const r of s.records) {
        const area = (r.box[2] - r.box[0]) * (r.box[3] - r.box[1]) * (r.alpha > 0.05 ? 1 : 0.001);
        if (!best[r.key] || area > best[r.key].area) best[r.key] = { area, T: s.T };
      }
      mode = 'render'; images = {};
      const byT = {};
      for (const [key, v] of Object.entries(best)) (byT[v.T] = byT[v.T] || new Set()).add(key);
      for (const [T, keys] of Object.entries(byT)) { wanted = keys; run(idx, Number(T)); }
      // Le fond de la scène (tout sauf les pièces), une image par seconde, réduite
      mode = 'bg';
      const backgrounds = [];
      for (let T = a + 0.05; T < b; T += 1.0) {
        BOIL = 0;
        run(idx, T);
        const small = document.createElement('canvas'); small.width = 640; small.height = 360;
        small.getContext('2d').drawImage(bufA, 0, 0, 640, 360);
        backgrounds.push({ T, data: small.toDataURL('image/png') });
      }
      mode = null;
      scenes.push({ idx, start: a, end: b, backgrounds, samples: samples.map(s => ({ T: s.T, veils: s.veils, records: s.records.map(r => ({ key: r.key, box: r.box, alpha: r.alpha, blend: r.blend, col: typeof r.col === 'string' ? r.col : null })) })), images, order: samples.reduce((acc, s) => { s.records.forEach((r, i) => { if (!(r.key in acc)) acc[r.key] = i + s.records.length * 0; }); return acc; }, {}), cols: Object.fromEntries(samples.flatMap(s => s.records.map(r => [r.key, r.col]))) });
    }
    return { S, W, H, textes: TEXTES, plans: PLANS, scenes };
  });
  let n = 0;
  for (const scene of result.scenes) {
    scene.backgrounds.forEach((bg, i) => {
      const file = `genese-${scene.idx}-fond-${i}.png`;
      fs.writeFileSync(path.join(out, 'pieces', file), Buffer.from(bg.data.split(',')[1], 'base64'));
      bg.file = file; delete bg.data;
    });
    for (const [key, img] of Object.entries(scene.images)) {
      const file = `genese-${scene.idx}-${key}.png`;
      fs.writeFileSync(path.join(out, 'pieces', file), Buffer.from(img.data.split(',')[1], 'base64'));
      img.file = file; delete img.data; n++;
    }
  }
  fs.writeFileSync(path.join(out, 'manifeste.json'), JSON.stringify(result));
  console.log(`${n} découpes exportées dans ${out}`);
  await browser.close();
})();
