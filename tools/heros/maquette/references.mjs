// Planches de référence : chaque variante de chaque espèce par défaut, puis des tirages à graine fixe.
import fs from 'fs';
import { toile } from './shim.mjs';
let graine = 12345;
Math.random = () => { graine = (graine * 1103515245 + 12345) % 2147483648; return graine / 2147483648; };
const derniere = { c: null };
globalThis.document = { createElement: () => (derniere.c = toile()) };
const A = await import('./hero-atlas.js');
const out = process.argv[2]; fs.mkdirSync(out, { recursive: true });
const cas = [];
for (const id of A.ORDRE_ESPECES) A.ESPECES[id].variantes.forEach((v, i) => {
  const c = A.defaut(id); c.variante = i; const T = A.traitsDe(c);
  const pal = A.peauxDe(c); if (pal.indexOf(c.peau) < 0) c.peau = pal[0];
  if (T.coiffureForcee) c.coiffure = T.coiffureForcee; if (T.barbeForcee) c.barbe = T.barbeForcee;
  if (T.yeuxForces) c.yeux = T.yeuxForces; if (T.formeTeteForcee) c.formeTete = T.formeTeteForcee; if (T.boucheForcee) c.bouche = T.boucheForcee;
  cas.push(['defaut-' + id + '-' + i, c]);
});
for (let n = 0; n < 60; n++) cas.push(['tirage-' + String(n).padStart(2, '0'), A.aleatoire()]);
const index = {};
for (const [nom, cfg] of cas) { A.peindre(cfg); fs.writeFileSync(out + '/' + nom + '.png', derniere.c.png()); index[nom] = cfg; }
fs.writeFileSync(out + '/configs.json', JSON.stringify(index, null, 1));
// Les données déclaratives, telles quelles.
const donnees = {};
for (const k of ['PATRON', 'FACES', 'COIFFURES', 'YEUX', 'BOUCHES', 'MARQUES', 'BARBES', 'TENUES', 'FORMES_TETE', 'PAL_CHEVEUX', 'PAL_YEUX', 'PAL_TENUE', 'ESPECES', 'ORDRE_ESPECES']) donnees[k] = A[k];
fs.writeFileSync(out + '/atlas.json', JSON.stringify(donnees, null, 1));
console.log(cas.length, 'planches');
