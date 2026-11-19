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
    let depth = 0, mode = null, counts = null, records = null, wanted = null, images = null, frames = null, T_now = 0;
    const pieceMatrix = (c, o) => {
      const s = o.s ?? 1;
      let m = c.getTransform().translate(o.x || 0, o.y || 0);
      if (o.r) m = m.rotate(o.r * 180 / Math.PI);
      return m.scale(s * (o.sx ?? 1), s * (o.sy ?? 1));
    };
    const shapeBox = (c, o) => {
      const m = pieceMatrix(c, o);
      let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
      for (const [x, y] of o.shape) {
        const p = m.transformPoint(new DOMPoint(x, y));
        x0 = Math.min(x0, p.x); y0 = Math.min(y0, p.y); x1 = Math.max(x1, p.x); y1 = Math.max(y1, p.y);
      }
      // Marge : le tremblé du contour, et l'ombre portée (décalée vers le bas à droite)
      const pad = 0.06 * Math.max(x1 - x0, y1 - y0) + 8 * S + (o.shadow === false ? 0 : (o.so ?? 4) * S * 1.3);
      return [x0 - pad, y0 - pad, x1 + pad, y1 + pad];
    };
    piece = function (c, o) {
      if (depth === 0) topSeen = true;
      if (mode === 'bg' && depth === 0) return; // le fond seul : ciel, étoiles, lueurs
      if (depth > 0 || !mode || (o.s ?? 1) <= 0.001) { depth++; try { orig(c, o); } finally { depth--; } return; }
      const base = `${o.seed | 0}`;
      const k = (counts[base] = (counts[base] || 0) + 1) - 1;
      const key = `${base}_${k}`;
      const box = shapeBox(c, o);
      const alpha = (o.a == null ? 1 : Math.max(0, Math.min(1, o.a))) * c.globalAlpha;
      const m = pieceMatrix(c, o);
      records.push({ key, box, alpha, blend: o.blend || null, col: o.col || null, m: [m.a, m.b, m.c, m.d, m.e, m.f] });
      if (mode === 'frame' && wanted.has(key) && !frames[key]) {
        // Le cadre de la découpe, à l'instant où la pièce est la plus grande. Visible à la page :
        // clippé à l'écran élargi (les bandes de ciel débordent)
        const bx = Math.max(box[0], -0.25 * W * S), by = Math.max(box[1], -0.25 * H * S);
        const ex = Math.min(box[2], 1.25 * W * S), ey = Math.min(box[3], 1.25 * H * S);
        const w = Math.max(2, Math.ceil(ex - bx)), h = Math.max(2, Math.ceil(ey - by));
        frames[key] = { bx, by, w, h, scale: Math.min(1, 1024 / Math.max(w, h)), m };
      }
      if (mode === 'render' && frames[key]) {
        // La pièce dessinée dans ce cadre, comme si elle était à sa place de référence : ce qu'elle
        // montre change-t-il (ce qu'elle porte bouge, pousse, tourne) ?
        const f = frames[key];
        const cnv = document.createElement('canvas'); cnv.width = Math.ceil(f.w * f.scale); cnv.height = Math.ceil(f.h * f.scale);
        const g = cnv.getContext('2d'); g._id = 'x' + key;
        // L'étiquette de travail des textes (« TEXTE 3 · À ÉCRIRE ») ne passe pas dans le jeu
        const fillText = g.fillText;
        g.fillText = function (t, ...rest) { if (/À ÉCRIRE/.test(t)) return; return fillText.call(this, t, ...rest); };
        const fix = new DOMMatrix([f.scale, 0, 0, f.scale, -f.bx * f.scale, -f.by * f.scale]);
        g.setTransform(fix.multiply(f.m).multiply(m.inverse()).multiply(c.getTransform()));
        // L'ombre portée fait partie de la découpe (translucide dans le jeu)
        const o2 = Object.assign({}, o, { a: 1, blend: undefined });
        depth++; try { orig(g, o2); } finally { depth--; }
        const thumb = document.createElement('canvas'); thumb.width = thumb.height = 96;
        const tg = thumb.getContext('2d'); tg.drawImage(cnv, 0, 0, 96, 96);
        // Comparée à la dernière image gardée, couleur moyenne à part : un simple changement de
        // teinte (le ciel qui vire au violet) se rend par une teinte de la carte, pas par une image
        const raw = tg.getImageData(0, 0, 96, 96).data;
        const mean = [0, 0, 0], px = new Float32Array(raw.length);
        let wsum = 0;
        for (let i = 0; i < raw.length; i += 4) { const a = raw[i + 3] / 255; wsum += a; for (let k = 0; k < 3; k++) mean[k] += raw[i + k] * a; }
        for (let k = 0; k < 3; k++) mean[k] = Math.max(8, mean[k] / Math.max(wsum, 1e-6));
        for (let i = 0; i < raw.length; i += 4) { for (let k = 0; k < 3; k++) px[i + k] = raw[i + k] / mean[k] * 128; px[i + 3] = raw[i + 3]; }
        const list = (images[key] = images[key] || { box: [f.bx, f.by, f.bx + f.w, f.by + f.h], variants: [] });
        const last = list.variants[list.variants.length - 1];
        let diff = Infinity;
        if (last) { diff = 0; for (let i = 0; i < px.length; i++) diff += Math.abs(px[i] - last.px[i]); diff /= px.length; }
        if (!last || (diff > 1.5 && T_now - last.T >= 0.49 && list.variants.length < 40)) list.variants.push({ T: T_now, px, col: typeof o.col === 'string' ? o.col : null, data: cnv.toDataURL('image/png') });
      }
    };
    // Voiles translucides posés sur toute l'image (la nuit qui tombe, un éclair blanc), avec le
    // nombre de pièces dessinées dessous : notés à
    // part (des fondus dans le jeu), et retirés du fond
    let veils = [], clipped = 0, clipStack = [], covered = {};
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
      // Un grand aplat recouvre les lueurs posées avant lui
      if (depth === 0 && mode === 'measure' && w * h >= W * H / 4 && this.globalCompositeOperation === 'source-over' && !(veil && this.globalAlpha * 1 < 1 && /rgba/.test(this.fillStyle))) {
        const set = (covered[T_now.toFixed(2)] = covered[T_now.toFixed(2)] || new Set());
        for (const r of records) if (r.glow) { r.covered = true; set.add(r.key); }
      }
      return fillRect.call(this, x, y, w, h);
    };
    // Les lueurs (dégradés ajoutés) : des cartes lumineuses dans le jeu, sauf celles des étoiles,
    // qui restent dans le fond
    const origGlow = glow, origStars = stars;
    let inStars = 0, glowCounts = null, topSeen = false;
    stars = function (...args) { inStars++; try { return origStars(...args); } finally { inStars--; } };
    glow = function (c, x, y, r, col, a = 1) {
      // Une lueur sous la première pièce est le fond : elle y reste (comme celles des étoiles)
      if (depth > 0 || inStars || clipped || !topSeen || !mode || a <= 0 || r <= 0) return origGlow(c, x, y, r, col, a);
      // Une lueur se reconnaît d'un instant à l'autre par son rang parmi les lueurs de la scène
      glowCounts.n = (glowCounts.n || 0) + 1;
      const key = `lueur${glowCounts.n}_0`;
      // Recouverte ensuite par un grand aplat (l'intérieur de la déchirure) : elle reste au fond
      if (mode === 'bg') { if (covered[T_now.toFixed(2)]?.has(key)) origGlow(c, x, y, r, col, a); return; }
      const m = c.getTransform(), p = m.transformPoint(new DOMPoint(x, y));
      records.push({ key, glow: true, x: p.x, y: p.y, r: r * Math.hypot(m.a, m.b), col, alpha: Math.max(0, Math.min(1, a)) * c.globalAlpha });
    };
    // Les traits et points fins dessinés devant les pièces (pluie, cendres, traînées des
    // météores) : relevés à part, 12 fois par seconde, pour devenir des cartes dans le jeu
    let path = [], complexPath = false, prims = null;
    const gradStops = new WeakMap();
    const addStop = CanvasGradient.prototype.addColorStop;
    CanvasGradient.prototype.addColorStop = function (o, col) { (gradStops.get(this) || gradStops.set(this, []).get(this)).push([o, col]); return addStop.call(this, o, col); };
    const [beginPath, moveTo, lineTo, stroke] = [cA.beginPath, cA.moveTo, cA.lineTo, cA.stroke];
    cA.beginPath = function () { path = []; complexPath = false; return beginPath.call(this); };
    cA.moveTo = function (x, y) { path.push(['M', this.getTransform().transformPoint(new DOMPoint(x, y))]); return moveTo.call(this, x, y); };
    cA.lineTo = function (x, y) { path.push(['L', this.getTransform().transformPoint(new DOMPoint(x, y))]); return lineTo.call(this, x, y); };
    for (const op of ['arc', 'arcTo', 'ellipse', 'rect', 'quadraticCurveTo', 'bezierCurveTo', 'closePath', 'roundRect']) {
      const f = cA[op];
      if (f) cA[op] = function (...args) { complexPath = true; return f.apply(this, args); };
    }
    const fine = () => depth === 0 && topSeen && !clipped && !inStars && (mode === 'particles' || mode === 'bg');
    const styleOf = (st) => typeof st === 'string' ? { col: st } : (gradStops.get(st) ? { col: gradStops.get(st)[0][1], grad: true } : null);
    cA.stroke = function (...args) {
      // Un trait qui rayonne (shadowBlur : les bords de la déchirure) reste dans le fond
      if (fine() && !complexPath && path.length > 1 && !(this.shadowBlur > 0)) {
        const segs = [];
        let ok = true, last = null;
        for (const [kind, p] of path) {
          if (kind === 'L' && last) { if (Math.hypot(p.x - last.x, p.y - last.y) > 90 * S) ok = false; segs.push([last.x, last.y, p.x, p.y]); }
          last = p;
        }
        const style = styleOf(this.strokeStyle);
        if (ok && segs.length && style) {
          if (mode === 'bg') return;
          const m = this.getTransform();
          const n = prims.stroke = (prims.stroke || 0) + 1;
          segs.forEach((sg, j) => prims.list.push({ id: `trait${n}_${j}`, kind: style.grad ? 'trainee' : 'trait', seg: sg, w: this.lineWidth * Math.hypot(m.a, m.b), col: style.col, alpha: this.globalAlpha, add: this.globalCompositeOperation === 'lighter' }));
        }
      }
      return stroke.apply(this, args);
    };
    const fillRectFine = cA.fillRect;
    cA.fillRect = function (x, y, w, h) {
      if (fine() && w <= 12 && h <= 12 && typeof this.fillStyle === 'string' && this.globalCompositeOperation === 'source-over') {
        if (mode === 'bg') return;
        const m = this.getTransform(), p = m.transformPoint(new DOMPoint(x + w / 2, y + h / 2)), k = Math.hypot(m.a, m.b);
        const n = prims.dot = (prims.dot || 0) + 1;
        prims.list.push({ id: `point${n}`, kind: 'point', seg: [p.x, p.y], w: w * k, h: h * k, col: this.fillStyle, alpha: this.globalAlpha });
        return;
      }
      return fillRectFine.call(this, x, y, w, h);
    };
    BOIL = 0;
    const scenes = [];
    const run = (idx, T) => {
      T_now = T;
      counts = {}; glowCounts = {}; topSeen = false; prims = { list: [] }; records = []; veils = []; clipped = 0; clipStack = [];
      prep(cA); cA.setTransform(S, 0, 0, S, 0, 0);
      SCENES[idx](cA, T);
      // Les textes de la maquette : des pièces de papier déchiré, devant la scène
      prep(cA); cA.setTransform(S, 0, 0, S, 0, 0);
      if (mode !== 'bg') textes(cA, T);
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
        if (r.glow) continue;
        const area = (r.box[2] - r.box[0]) * (r.box[3] - r.box[1]) * (r.alpha > 0.05 ? 1 : 0.001);
        if (!best[r.key] || area > best[r.key].area) best[r.key] = { area, T: s.T };
      }
      mode = 'frame'; frames = {}; images = {};
      const byT = {};
      for (const [key, v] of Object.entries(best)) (byT[v.T] = byT[v.T] || new Set()).add(key);
      for (const [T, keys] of Object.entries(byT)) { wanted = keys; run(idx, Number(T)); }
      // Puis à chaque échantillon, une image de plus quand ce que montre la pièce a changé
      mode = 'render';
      for (const smp of samples) { BOIL = 0; T_now = smp.T; run(idx, smp.T); }
      for (const v of Object.values(images)) for (const x of v.variants) delete x.px;
      mode = 'particles';
      const particles = [];
      for (let T = a + 1 / 24; T < b; T += 1 / 12) {
        BOIL = 0; run(idx, T);
        if (prims.list.length) particles.push({ T, prims: prims.list.map(q => [q.id, q.kind, ...q.seg.map(v => Math.round(v * 10) / 10), Math.round(q.w * 100) / 100, q.h ? Math.round(q.h * 100) / 100 : 0, q.col, Math.round(q.alpha * 1000) / 1000, q.add ? 1 : 0]) });
      }
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
      scenes.push({ idx, start: a, end: b, backgrounds, particles, samples: samples.map(s => ({ T: s.T, veils: s.veils, records: s.records.map(r => r.glow ? r : ({ key: r.key, box: r.box, m: r.m, alpha: r.alpha, blend: r.blend, col: typeof r.col === 'string' ? r.col : null })) })), images, order: samples.reduce((acc, s) => { s.records.forEach((r, i) => { if (!(r.key in acc)) acc[r.key] = i + s.records.length * 0; }); return acc; }, {}), cols: Object.fromEntries(samples.flatMap(s => s.records.map(r => [r.key, r.col]))) });
    }
    // Les coupes en page déchirée : la maquette entière, 12 images par seconde (sans sa vignette,
    // que le jeu pose lui-même)
    mode = null;
    cV.clearRect(0, 0, vign.width, vign.height);
    const tears = [];
    for (const cut of CUTS) {
      const frames = [];
      for (let T = cut - 0.6 + 1 / 24; T < cut + 0.6; T += 1 / 12) {
        render(T);
        const small = document.createElement('canvas'); small.width = 640; small.height = 360;
        small.getContext('2d').drawImage(cv, 0, 0, 640, 360);
        frames.push({ T, data: small.toDataURL('image/png') });
      }
      tears.push({ cut, frames });
    }
    return { S, W, H, textes: TEXTES, plans: PLANS, scenes, tears };
  });
  let n = 0;
  for (const scene of result.scenes) {
    scene.backgrounds.forEach((bg, i) => {
      const file = `genese-${scene.idx}-fond-${i}.png`;
      fs.writeFileSync(path.join(out, 'pieces', file), Buffer.from(bg.data.split(',')[1], 'base64'));
      bg.file = file; delete bg.data;
    });
    for (const [key, img] of Object.entries(scene.images)) {
      img.variants.forEach((v, i) => {
        const file = `genese-${scene.idx}-${key}-${i}.png`;
        fs.writeFileSync(path.join(out, 'pieces', file), Buffer.from(v.data.split(',')[1], 'base64'));
        v.file = file; delete v.data; n++;
      });
    }
  }
  result.tears.forEach(tear => tear.frames.forEach((f, i) => {
    const file = `genese-coupe-${tear.cut}-${i}.png`;
    fs.writeFileSync(path.join(out, 'pieces', file), Buffer.from(f.data.split(',')[1], 'base64'));
    f.file = file; delete f.data;
  }));
  fs.writeFileSync(path.join(out, 'manifeste.json'), JSON.stringify(result));
  console.log(`${n} découpes exportées dans ${out}`);
  await browser.close();
})();
