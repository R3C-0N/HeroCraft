/* hero-atlas.js — patron de texture 64x64 deux couches + peinture procedurale.
   Toute la geometrie UV vient du patron fourni : aucune coordonnee inventee. */

export const PATRON = {
  image: 'engine:heros',
  taille: [64, 64],
  patron: 'Minecraft 64x64, deux couches',
  unite_en_blocs: 0.05625,
  parties: {
    tete: { os: 'tete', couche: 1, boite: { largeur: 8, hauteur: 8, profondeur: 8 }, gonflement: 0,
      faces: { dessus:{x:8,y:0,largeur:8,hauteur:8}, dessous:{x:16,y:0,largeur:8,hauteur:8}, droite:{x:0,y:8,largeur:8,hauteur:8}, devant:{x:8,y:8,largeur:8,hauteur:8}, gauche:{x:16,y:8,largeur:8,hauteur:8}, derriere:{x:24,y:8,largeur:8,hauteur:8} } },
    torse: { os: 'torse', couche: 1, boite: { largeur: 8, hauteur: 12, profondeur: 4 }, gonflement: 0,
      faces: { dessus:{x:20,y:16,largeur:8,hauteur:4}, dessous:{x:28,y:16,largeur:8,hauteur:4}, droite:{x:16,y:20,largeur:4,hauteur:12}, devant:{x:20,y:20,largeur:8,hauteur:12}, gauche:{x:28,y:20,largeur:4,hauteur:12}, derriere:{x:32,y:20,largeur:8,hauteur:12} } },
    brasD: { os: 'brasD', couche: 1, boite: { largeur: 4, hauteur: 12, profondeur: 4 }, gonflement: 0,
      faces: { dessus:{x:44,y:16,largeur:4,hauteur:4}, dessous:{x:48,y:16,largeur:4,hauteur:4}, droite:{x:40,y:20,largeur:4,hauteur:12}, devant:{x:44,y:20,largeur:4,hauteur:12}, gauche:{x:48,y:20,largeur:4,hauteur:12}, derriere:{x:52,y:20,largeur:4,hauteur:12} } },
    brasG: { os: 'brasG', couche: 1, boite: { largeur: 4, hauteur: 12, profondeur: 4 }, gonflement: 0,
      faces: { dessus:{x:36,y:48,largeur:4,hauteur:4}, dessous:{x:40,y:48,largeur:4,hauteur:4}, droite:{x:32,y:52,largeur:4,hauteur:12}, devant:{x:36,y:52,largeur:4,hauteur:12}, gauche:{x:40,y:52,largeur:4,hauteur:12}, derriere:{x:44,y:52,largeur:4,hauteur:12} } },
    jambeD: { os: 'jambeD', couche: 1, boite: { largeur: 4, hauteur: 12, profondeur: 4 }, gonflement: 0,
      faces: { dessus:{x:4,y:16,largeur:4,hauteur:4}, dessous:{x:8,y:16,largeur:4,hauteur:4}, droite:{x:0,y:20,largeur:4,hauteur:12}, devant:{x:4,y:20,largeur:4,hauteur:12}, gauche:{x:8,y:20,largeur:4,hauteur:12}, derriere:{x:12,y:20,largeur:4,hauteur:12} } },
    jambeG: { os: 'jambeG', couche: 1, boite: { largeur: 4, hauteur: 12, profondeur: 4 }, gonflement: 0,
      faces: { dessus:{x:20,y:48,largeur:4,hauteur:4}, dessous:{x:24,y:48,largeur:4,hauteur:4}, droite:{x:16,y:52,largeur:4,hauteur:12}, devant:{x:20,y:52,largeur:4,hauteur:12}, gauche:{x:24,y:52,largeur:4,hauteur:12}, derriere:{x:28,y:52,largeur:4,hauteur:12} } },
    tete2: { os: 'tete', couche: 2, boite: { largeur: 8, hauteur: 8, profondeur: 8 }, gonflement: 0.5,
      faces: { dessus:{x:40,y:0,largeur:8,hauteur:8}, dessous:{x:48,y:0,largeur:8,hauteur:8}, droite:{x:32,y:8,largeur:8,hauteur:8}, devant:{x:40,y:8,largeur:8,hauteur:8}, gauche:{x:48,y:8,largeur:8,hauteur:8}, derriere:{x:56,y:8,largeur:8,hauteur:8} } },
    torse2: { os: 'torse', couche: 2, boite: { largeur: 8, hauteur: 12, profondeur: 4 }, gonflement: 0.25,
      faces: { dessus:{x:20,y:32,largeur:8,hauteur:4}, dessous:{x:28,y:32,largeur:8,hauteur:4}, droite:{x:16,y:36,largeur:4,hauteur:12}, devant:{x:20,y:36,largeur:8,hauteur:12}, gauche:{x:28,y:36,largeur:4,hauteur:12}, derriere:{x:32,y:36,largeur:8,hauteur:12} } },
    brasD2: { os: 'brasD', couche: 2, boite: { largeur: 4, hauteur: 12, profondeur: 4 }, gonflement: 0.25,
      faces: { dessus:{x:44,y:32,largeur:4,hauteur:4}, dessous:{x:48,y:32,largeur:4,hauteur:4}, droite:{x:40,y:36,largeur:4,hauteur:12}, devant:{x:44,y:36,largeur:4,hauteur:12}, gauche:{x:48,y:36,largeur:4,hauteur:12}, derriere:{x:52,y:36,largeur:4,hauteur:12} } },
    brasG2: { os: 'brasG', couche: 2, boite: { largeur: 4, hauteur: 12, profondeur: 4 }, gonflement: 0.25,
      faces: { dessus:{x:52,y:48,largeur:4,hauteur:4}, dessous:{x:56,y:48,largeur:4,hauteur:4}, droite:{x:48,y:52,largeur:4,hauteur:12}, devant:{x:52,y:52,largeur:4,hauteur:12}, gauche:{x:56,y:52,largeur:4,hauteur:12}, derriere:{x:60,y:52,largeur:4,hauteur:12} } },
    jambeD2: { os: 'jambeD', couche: 2, boite: { largeur: 4, hauteur: 12, profondeur: 4 }, gonflement: 0.25,
      faces: { dessus:{x:4,y:32,largeur:4,hauteur:4}, dessous:{x:8,y:32,largeur:4,hauteur:4}, droite:{x:0,y:36,largeur:4,hauteur:12}, devant:{x:4,y:36,largeur:4,hauteur:12}, gauche:{x:8,y:36,largeur:4,hauteur:12}, derriere:{x:12,y:36,largeur:4,hauteur:12} } },
    jambeG2: { os: 'jambeG', couche: 2, boite: { largeur: 4, hauteur: 12, profondeur: 4 }, gonflement: 0.25,
      faces: { dessus:{x:4,y:48,largeur:4,hauteur:4}, dessous:{x:8,y:48,largeur:4,hauteur:4}, droite:{x:0,y:52,largeur:4,hauteur:12}, devant:{x:4,y:52,largeur:4,hauteur:12}, gauche:{x:8,y:52,largeur:4,hauteur:12}, derriere:{x:12,y:52,largeur:4,hauteur:12} } },
    /* Ailes : forme supplementaire logee dans la colonne libre du patron
       (x 56-63, y 16-47), la seule zone inutilisee de la planche 64x64.
       Meme decoupe pour les deux ailes : l'aile droite est retournee de 180 deg. */
    aile: { os: 'torse', couche: 2, boite: { largeur: 7, hauteur: 14, profondeur: 1 }, gonflement: 0,
      faces: { dessus:{x:56,y:30,largeur:7,hauteur:1}, dessous:{x:56,y:46,largeur:7,hauteur:1}, droite:{x:56,y:16,largeur:1,hauteur:14}, devant:{x:56,y:16,largeur:7,hauteur:14}, gauche:{x:62,y:16,largeur:1,hauteur:14}, derriere:{x:56,y:32,largeur:7,hauteur:14} } },
    /* Pilosite : deux pieces a part, chacune dans un carre libre de la premiere
       bande. Elles ne lisent qu'un rectangle (la figure extrude ses pixels),
       les tranches sont echantillonnees dans le meme carre.
       barbe   : x 0-8   recouvre le bas du visage et tombe sous le menton
       chevelure : x 24-32  tombe derriere la tete, sans toucher le torse */
    barbe: { os: 'tete', couche: 2, boite: { largeur: 8, hauteur: 8, profondeur: 1 }, gonflement: 0,
      faces: { dessus:{x:0,y:0,largeur:8,hauteur:1}, dessous:{x:0,y:7,largeur:8,hauteur:1}, droite:{x:0,y:0,largeur:1,hauteur:8}, devant:{x:0,y:0,largeur:8,hauteur:8}, gauche:{x:7,y:0,largeur:1,hauteur:8}, derriere:{x:0,y:0,largeur:8,hauteur:8} } },
    chevelure: { os: 'tete', couche: 2, boite: { largeur: 8, hauteur: 8, profondeur: 1 }, gonflement: 0,
      faces: { dessus:{x:24,y:0,largeur:8,hauteur:1}, dessous:{x:24,y:7,largeur:8,hauteur:1}, droite:{x:24,y:0,largeur:1,hauteur:8}, devant:{x:24,y:0,largeur:8,hauteur:8}, gauche:{x:31,y:0,largeur:1,hauteur:8}, derriere:{x:24,y:0,largeur:8,hauteur:8} } },
    /* Appendices : deux carres restes libres de la premiere bande.
       appendice : x 32-40 — oreilles animales ou cornes, plan vertical pose
                   sur le sommet du crane (les deux cotes dans le meme carre)
       queue     : x 56-64 — plan vertical accroche au bas du dos */
    appendice: { os: 'tete', couche: 2, boite: { largeur: 8, hauteur: 8, profondeur: 1 }, gonflement: 0,
      faces: { dessus:{x:32,y:0,largeur:8,hauteur:1}, dessous:{x:32,y:7,largeur:8,hauteur:1}, droite:{x:32,y:0,largeur:1,hauteur:8}, devant:{x:32,y:0,largeur:8,hauteur:8}, gauche:{x:39,y:0,largeur:1,hauteur:8}, derriere:{x:32,y:0,largeur:8,hauteur:8} } },
    queue: { os: 'torse', couche: 2, boite: { largeur: 8, hauteur: 8, profondeur: 1 }, gonflement: 0,
      faces: { dessus:{x:56,y:0,largeur:8,hauteur:1}, dessous:{x:56,y:7,largeur:8,hauteur:1}, droite:{x:56,y:0,largeur:1,hauteur:8}, devant:{x:56,y:0,largeur:8,hauteur:8}, gauche:{x:63,y:0,largeur:1,hauteur:8}, derriere:{x:56,y:0,largeur:8,hauteur:8} } }
  }
};

export const FACES = ['dessus', 'dessous', 'droite', 'devant', 'gauche', 'derriere'];
const MEMBRES1 = ['tete', 'torse', 'brasD', 'brasG', 'jambeD', 'jambeG'];
const BRAS = ['brasD', 'brasG'];
const JAMBES = ['jambeD', 'jambeG'];

/* ---------- couleurs ---------- */
const cl = (v, a, b) => Math.max(a, Math.min(b, v));
function rgb(h) { const n = parseInt(h.slice(1), 16); return [n >> 16 & 255, n >> 8 & 255, n & 255]; }
function hex(r) { return '#' + r.map(v => cl(Math.round(v), 0, 255).toString(16).padStart(2, '0')).join(''); }
export function shade(h, amt) { const c = rgb(h); return hex(c.map(v => amt >= 0 ? v + (255 - v) * amt : v * (1 + amt))); }
export function mix(a, b, t) { const A = rgb(a), B = rgb(b); return hex(A.map((v, i) => v + (B[i] - v) * t)); }
function bruit(x, y, s) { const v = Math.sin((x + 1) * 12.9898 + (y + 1) * 78.233 + s * 3.77) * 43758.5453; return v - Math.floor(v); }

/* ---------- options ---------- */
export const COIFFURES = [
  { id: 'rasee', nom: 'Crâne rasé' },
  { id: 'courte', nom: 'Courte', dev: [3, 2, 1, 1, 1, 1, 2, 3], cot: [2, 3, 3, 4, 5, 5, 6, 6], der: [5, 6, 6, 6, 6, 6, 6, 5] },
  { id: 'brosse', nom: 'En brosse', dev: [2, 1, 1, 1, 1, 1, 1, 2], cot: [1, 2, 2, 3, 3, 4, 4, 5], der: [4, 5, 5, 5, 5, 5, 5, 4] },
  { id: 'degrade', nom: 'Dégradé haut', dev: [2, 2, 1, 1, 1, 1, 2, 2], cot: [1, 1, 2, 2, 3, 5, 7, 7], der: [4, 5, 5, 5, 5, 5, 5, 4] },
  { id: 'frange', nom: 'Frange droite', dev: [3, 3, 3, 3, 3, 3, 3, 3], cot: [3, 3, 4, 4, 5, 6, 6, 6], der: [5, 6, 6, 6, 6, 6, 6, 5] },
  { id: 'bol', nom: 'Coupe au bol', dev: [4, 3, 3, 3, 3, 3, 3, 4], cot: [5, 5, 5, 6, 6, 6, 7, 7], der: [6, 7, 7, 7, 7, 7, 7, 6] },
  { id: 'raie', nom: 'Raie de côté', dev: [1, 2, 3, 3, 2, 1, 2, 3], cot: [2, 3, 4, 4, 5, 5, 6, 6], der: [5, 6, 6, 6, 6, 6, 6, 5], extras: [['devant', 4, 0, 1, 2]] },
  { id: 'arriere', nom: 'Tirés en arrière', dev: [1, 0, 0, 0, 0, 0, 0, 1], cot: [2, 2, 3, 4, 5, 6, 7, 8], der: [6, 7, 8, 8, 8, 8, 7, 6] },
  { id: 'herisse', nom: 'Hérissée', dev: [2, 1, 2, 1, 1, 2, 1, 2], cot: [2, 3, 3, 4, 4, 5, 5, 6], der: [4, 5, 5, 6, 6, 5, 5, 4], extras: [['dessus', 1, 1, 1, 1], ['dessus', 4, 2, 1, 1], ['dessus', 6, 5, 1, 1], ['dessus', 2, 5, 1, 1]] },
  { id: 'crete', nom: 'Crête', extras: [['dessus', 3, 0, 2, 8], ['derriere', 3, 0, 2, 4], ['devant', 3, 0, 2, 1]] },
  { id: 'tonsure', nom: 'Tonsure', dev: [1, 0, 0, 0, 0, 0, 0, 1], cot: [4, 4, 5, 5, 5, 6, 6, 6], der: [5, 6, 6, 6, 6, 6, 6, 5], extras: [['dessus', 0, 0, 8, 1], ['dessus', 0, 7, 8, 1], ['dessus', 0, 0, 1, 8], ['dessus', 7, 0, 1, 8]] },
  { id: 'chignon', nom: 'Chignon', dev: [2, 2, 2, 2, 2, 2, 2, 2], cot: [2, 3, 3, 4, 4, 5, 5, 5], der: [4, 5, 5, 5, 5, 5, 5, 4], extras: [['dessus', 3, 5, 2, 2], ['derriere', 3, 0, 2, 2]] },
  { id: 'mi', nom: 'Mi-longue', dev: [3, 2, 2, 2, 2, 2, 2, 3], cot: [4, 5, 5, 6, 7, 7, 8, 8], der: [7, 8, 8, 8, 8, 8, 8, 7], chute: 'mi' },
  { id: 'longue', nom: 'Longue', dev: [2, 2, 2, 2, 2, 2, 2, 2], cot: [6, 6, 7, 7, 8, 8, 8, 8], der: [8, 8, 8, 8, 8, 8, 8, 8], chute: 'longue' },
  { id: 'lisse', nom: 'Longue et lisse', dev: [3, 2, 2, 2, 2, 2, 2, 3], cot: [7, 7, 8, 8, 8, 8, 8, 8], der: [8, 8, 8, 8, 8, 8, 8, 8], chute: 'fleuve' },
  { id: 'afro', nom: 'Volumineuse', dev: [4, 3, 3, 3, 3, 3, 3, 4], cot: [5, 6, 6, 7, 7, 7, 8, 8], der: [7, 8, 8, 8, 8, 8, 8, 7], extras: [['dessus', 0, 0, 8, 8]] },
  { id: 'queue', nom: 'Queue', dev: [2, 1, 1, 1, 1, 1, 1, 2], cot: [2, 3, 3, 4, 4, 5, 5, 6], der: [5, 6, 6, 6, 6, 6, 6, 5], chute: 'queue' },
  { id: 'tresse', nom: 'Tresse', dev: [2, 2, 2, 2, 2, 2, 2, 2], cot: [3, 3, 4, 4, 5, 5, 6, 6], der: [5, 6, 6, 6, 6, 6, 6, 5], chute: 'tresse' },
  { id: 'nattes', nom: 'Deux nattes', dev: [3, 2, 2, 2, 2, 2, 2, 3], cot: [4, 5, 5, 6, 6, 7, 7, 7], der: [6, 7, 7, 7, 7, 7, 7, 6], chute: 'nattes' },
  { id: 'couettes', nom: 'Couettes', dev: [2, 2, 2, 2, 2, 2, 2, 2], cot: [3, 4, 4, 5, 5, 6, 6, 6], der: [5, 6, 6, 6, 6, 6, 6, 5], chute: 'couettes' },
  { id: 'mulet', nom: 'Nuque longue', dev: [2, 1, 1, 1, 1, 1, 1, 2], cot: [1, 2, 2, 3, 5, 6, 8, 8], der: [7, 8, 8, 8, 8, 8, 8, 7], chute: 'mulet' },
  { id: 'bandeau', nom: 'Bandeau', dev: [2, 1, 1, 1, 1, 1, 1, 2], cot: [3, 3, 4, 4, 5, 5, 6, 6], der: [5, 6, 6, 6, 6, 6, 6, 5], extras: [['devant', 0, 1, 8, 1], ['droite', 0, 1, 8, 1], ['gauche', 0, 1, 8, 1], ['derriere', 0, 1, 8, 1]] }
];

/* ops en demi-visage, reflechies par sym() : [x, y, l, h, teinte]
   b blanc · i iris · l paupiere · c cil */
export const YEUX = [
  { id: 'amande', nom: 'Amande', ops: [[1, 3, 1, 1, 'b'], [2, 3, 1, 1, 'i']] },
  { id: 'ronds', nom: 'Ronds', ops: [[1, 3, 2, 2, 'b'], [2, 3, 1, 2, 'i']] },
  { id: 'fendus', nom: 'Fendus', ops: [[1, 2, 2, 1, 'c'], [1, 3, 1, 1, 'l'], [2, 3, 1, 1, 'i']] },
  { id: 'grands', nom: 'Grands', ops: [[1, 3, 2, 2, 'b'], [1, 4, 2, 1, 'i'], [1, 5, 2, 1, 'l']] },
  { id: 'tombants', nom: 'Tombants', ops: [[1, 3, 1, 1, 'l'], [1, 4, 1, 1, 'b'], [2, 4, 1, 1, 'i']] },
  { id: 'releves', nom: 'Relevés', ops: [[1, 4, 1, 1, 'l'], [1, 3, 1, 1, 'b'], [2, 3, 1, 1, 'i']] },
  { id: 'miClos', nom: 'Mi-clos', ops: [[1, 3, 2, 1, 'c'], [2, 4, 1, 1, 'i']] },
  { id: 'percants', nom: 'Perçants', ops: [[0, 3, 3, 1, 'b'], [1, 3, 1, 1, 'i']] },
  { id: 'lourds', nom: 'Lourds', ops: [[1, 2, 2, 1, 'c'], [1, 3, 2, 1, 'b'], [2, 3, 1, 1, 'i'], [1, 4, 2, 1, 'l']] },
  { id: 'enfonces', nom: 'Enfoncés', ops: [[0, 3, 3, 1, 'l'], [1, 3, 1, 1, 'b'], [2, 3, 1, 1, 'i']] },
  { id: 'cernes', nom: 'Cernés', ops: [[1, 3, 1, 1, 'b'], [2, 3, 1, 1, 'i'], [1, 4, 2, 1, 'l'], [1, 5, 2, 1, 'l']] },
  { id: 'globuleux', nom: 'Globuleux', ops: [[1, 2, 2, 3, 'b'], [1, 3, 2, 1, 'i']] },
  { id: 'verticaux', nom: 'Fentes verticales', ops: [[1, 2, 1, 3, 'b'], [2, 2, 1, 3, 'i']] },
  { id: 'plisses', nom: 'Plissés', ops: [[1, 3, 2, 1, 'l'], [1, 4, 2, 1, 'c']] },
  { id: 'brillants', nom: 'Brillants', ops: [[1, 3, 2, 2, 'i'], [2, 2, 1, 1, 'b'], [1, 4, 1, 1, 'b']] },
  { id: 'vides', nom: 'Sans pupille', ops: [[1, 3, 2, 1, 'i'], [1, 4, 2, 1, 'i']] },
  { id: 'petits', nom: 'Petits', ops: [[2, 3, 1, 1, 'i']] },
  { id: 'ecartes', nom: 'Écartés', ops: [[0, 3, 1, 1, 'b'], [1, 3, 1, 1, 'i']] },
  { id: 'rapproches', nom: 'Rapprochés', ops: [[2, 3, 1, 1, 'b'], [3, 3, 1, 1, 'i']] }
];

/* ops absolues sur la face : l levre · f creux · d dent · p peau ombree */
export const BOUCHES = [
  { id: 'neutre', nom: 'Neutre', ops: [[2, 6, 4, 1, 'l']] },
  { id: 'sourire', nom: 'Sourire', ops: [[3, 6, 2, 1, 'f'], [2, 5, 1, 1, 'f'], [5, 5, 1, 1, 'f']] },
  { id: 'fine', nom: 'Fine', ops: [[3, 6, 2, 1, 'f']] },
  { id: 'serree', nom: 'Serrée', ops: [[2, 6, 4, 1, 'f'], [3, 7, 2, 1, 'l']] },
  { id: 'moue', nom: 'Moue', ops: [[3, 6, 2, 1, 'l'], [3, 7, 2, 1, 'f']] },
  { id: 'rictus', nom: 'Rictus', ops: [[2, 6, 3, 1, 'f'], [5, 5, 1, 1, 'f']] },
  { id: 'dents', nom: 'Dents serrées', ops: [[2, 6, 4, 1, 'f'], [3, 6, 2, 1, 'd']] },
  { id: 'large', nom: 'Grand sourire', ops: [[2, 6, 4, 1, 'f'], [2, 5, 1, 1, 'f'], [5, 5, 1, 1, 'f'], [3, 7, 2, 1, 'l']] },
  { id: 'grimace', nom: 'Grimace', ops: [[2, 6, 1, 1, 'f'], [3, 6, 1, 1, 'l'], [4, 6, 1, 1, 'f'], [5, 6, 1, 1, 'l']] },
  { id: 'lippue', nom: 'Lippue', ops: [[2, 6, 4, 2, 'l'], [2, 6, 4, 1, 'f']] },
  { id: 'entrouverte', nom: 'Entrouverte', ops: [[3, 6, 2, 1, 'f'], [3, 7, 2, 1, 'f']] },
  { id: 'pincee', nom: 'Pincée', ops: [[3, 6, 2, 1, 'f'], [2, 6, 1, 1, 'p'], [5, 6, 1, 1, 'p']] },
  { id: 'tordue', nom: 'Tordue', ops: [[2, 6, 2, 1, 'f'], [4, 5, 2, 1, 'f']] },
  { id: 'triste', nom: 'Triste', ops: [[3, 5, 2, 1, 'f'], [2, 6, 1, 1, 'f'], [5, 6, 1, 1, 'f']] },
  { id: 'fendue', nom: 'Fendue', ops: [[2, 6, 4, 1, 'f'], [4, 5, 1, 1, 'f'], [4, 7, 1, 1, 'f']] },
  { id: 'crocs', nom: 'Crocs visibles', ops: [[2, 6, 4, 1, 'f'], [2, 7, 1, 1, 'd'], [5, 7, 1, 1, 'd']] },
  { id: 'etiree', nom: 'Étirée', ops: [[1, 6, 6, 1, 'l']] },
  { id: 'petite', nom: 'Petite', ops: [[3, 6, 1, 1, 'f'], [4, 6, 1, 1, 'l']] },
  { id: 'coin', nom: 'Sourire en coin', ops: [[3, 6, 2, 1, 'l'], [5, 5, 1, 1, 'f']] }
];

/* ops : [piece, x, y, l, h] — piece : tete, teteD, teteG, torse, bras */
export const MARQUES = [
  { id: 'aucune', nom: 'Aucune' },
  { id: 'guerre', nom: 'Peinture de guerre', teinte: '#B03A2A', ops: [['tete', 0, 4, 2, 1], ['tete', 6, 4, 2, 1], ['tete', 2, 1, 4, 1]] },
  { id: 'runes', nom: 'Runes', teinte: '#7FD2E8', ops: [['tete', 3, 1, 1, 1], ['tete', 4, 2, 1, 1], ['bras', 1, 5, 1, 1], ['bras', 2, 6, 1, 1], ['bras', 1, 7, 1, 1]] },
  { id: 'cicatrice', nom: 'Cicatrice', teinte: 'peauF', ops: [['tete', 5, 2, 1, 1], ['tete', 5, 3, 1, 1], ['tete', 6, 4, 1, 1], ['tete', 6, 5, 1, 1]] },
  { id: 'rousseur', nom: 'Taches de rousseur', teinte: 'peauF', ops: [['tete', 1, 4, 1, 1], ['tete', 2, 5, 1, 1], ['tete', 5, 4, 1, 1], ['tete', 6, 5, 1, 1], ['tete', 2, 3, 1, 1]] },
  { id: 'tribal', nom: 'Tatouage tribal', teinte: '#2A2426', ops: [['tete', 0, 2, 1, 3], ['tete', 7, 2, 1, 3], ['tete', 1, 1, 1, 1], ['tete', 6, 1, 1, 1]] },
  { id: 'larme', nom: 'Larme noire', teinte: '#2A2426', ops: [['tete', 2, 4, 1, 2], ['tete', 5, 4, 1, 2]] },
  { id: 'bandeauGuerre', nom: 'Bandeau peint', teinte: '#B03A2A', ops: [['tete', 0, 2, 8, 1]] },
  { id: 'brulure', nom: 'Brûlure', teinte: 'peauF', ops: [['tete', 5, 5, 2, 2], ['teteG', 1, 4, 3, 2]] },
  { id: 'clan', nom: 'Marque de clan', teinte: '#D9A62C', ops: [['tete', 3, 1, 2, 1], ['torse', 3, 2, 2, 2]] },
  { id: 'points', nom: 'Points rituels', teinte: '#EDE7D6', ops: [['tete', 1, 1, 1, 1], ['tete', 3, 1, 1, 1], ['tete', 5, 1, 1, 1], ['tete', 6, 1, 1, 1]] },
  { id: 'rayures', nom: 'Rayures félines', teinte: 'peauF', ops: [['tete', 0, 3, 1, 1], ['tete', 0, 5, 1, 1], ['tete', 7, 3, 1, 1], ['tete', 7, 5, 1, 1], ['teteD', 2, 3, 2, 1], ['teteG', 2, 3, 2, 1]] },
  { id: 'croix', nom: 'Croix gravée', teinte: '#2A2426', ops: [['tete', 3, 1, 2, 1], ['tete', 4, 0, 1, 3]] },
  { id: 'spirales', nom: 'Spirales', teinte: '#7FD2E8', ops: [['bras', 1, 4, 2, 1], ['bras', 2, 5, 1, 1], ['bras', 1, 6, 2, 1], ['bras', 1, 7, 1, 1]] },
  { id: 'or', nom: 'Poussière d’or', teinte: '#D9A62C', ops: [['tete', 1, 2, 1, 1], ['tete', 6, 2, 1, 1], ['torse', 1, 1, 1, 1], ['torse', 6, 1, 1, 1], ['torse', 3, 3, 2, 1]] },
  { id: 'veinesNoires', nom: 'Veines sombres', teinte: '#2A2426', ops: [['teteD', 2, 2, 1, 4], ['teteG', 5, 2, 1, 4], ['tete', 0, 1, 1, 2], ['tete', 7, 1, 1, 2]] },
  { id: 'masque', nom: 'Masque peint', teinte: '#3E2A5C', ops: [['tete', 0, 1, 8, 2]] },
  { id: 'estafilade', nom: 'Estafilade', teinte: 'peauF', ops: [['tete', 1, 5, 3, 1], ['tete', 1, 6, 1, 1]] },
  { id: 'sceau', nom: 'Sceau runique', teinte: '#7FD2E8', ops: [['tete', 3, 1, 2, 1], ['torse', 3, 3, 2, 2], ['torse', 2, 4, 1, 1], ['torse', 5, 4, 1, 1]] }
];

export const BARBES = [
  { id: 'aucune', nom: 'Aucune' },
  { id: 'ombre', nom: 'Barbe de trois jours', rangs: [[], [], [[0, 2], [6, 2]], [[1, 6]], [[2, 4]], [], [], []] },
  { id: 'moustache', nom: 'Moustache', rangs: [[], [[2, 4]], [], [], [], [], [], []] },
  { id: 'bacchantes', nom: 'Bacchantes', rangs: [[], [[1, 6]], [], [], [], [], [], []] },
  { id: 'moustacheT', nom: 'Moustache tombante', rangs: [[], [[2, 4]], [[1, 1], [6, 1]], [[1, 1], [6, 1]], [], [], [], []] },
  { id: 'bouc', nom: 'Bouc', rangs: [[], [[2, 4]], [], [[3, 2]], [[3, 2]], [], [], []] },
  { id: 'boucLong', nom: 'Bouc long', rangs: [[], [[2, 4]], [], [[3, 2]], [[3, 2]], [[3, 2]], [[3, 2]], []] },
  { id: 'menton', nom: 'Menton seul', rangs: [[], [], [], [[3, 2]], [[3, 2]], [], [], []] },
  { id: 'imperiale', nom: 'Impériale', rangs: [[], [[2, 4]], [], [], [[3, 2]], [[3, 2]], [], []] },
  { id: 'favoris', nom: 'Favoris', rangs: [[[0, 2], [6, 2]], [[0, 2], [6, 2]], [[0, 2], [6, 2]], [[0, 1], [7, 1]], [], [], [], []] },
  { id: 'collier', nom: 'Collier', rangs: [[[0, 2], [6, 2]], [[0, 2], [6, 2]], [[0, 2], [6, 2]], [[1, 6]], [[2, 4]], [], [], []] },
  { id: 'courte', nom: 'Courte', rangs: [[[1, 6]], [[1, 6]], [[0, 2], [6, 2]], [[1, 6]], [[1, 6]], [[2, 4]], [], []] },
  { id: 'ronde', nom: 'Courte et ronde', rangs: [[[1, 6]], [[1, 6]], [[0, 2], [6, 2]], [[0, 8]], [[1, 6]], [[2, 4]], [], []] },
  { id: 'pleine', nom: 'Pleine', rangs: [[[1, 6]], [[1, 6]], [[0, 2], [6, 2]], [[1, 6]], [[1, 6]], [[1, 6]], [[2, 4]], []] },
  { id: 'longue', nom: 'Longue', rangs: [[[1, 6]], [[1, 6]], [[0, 2], [6, 2]], [[1, 6]], [[1, 6]], [[1, 6]], [[2, 4]], [[3, 2]]] },
  { id: 'fleuve', nom: 'Barbe fleuve', rangs: [[[1, 6]], [[1, 6]], [[0, 2], [6, 2]], [[1, 6]], [[0, 8]], [[0, 8]], [[1, 6]], [[1, 6]]] },
  { id: 'fourchue', nom: 'Fourchue', rangs: [[[1, 6]], [[1, 6]], [[0, 2], [6, 2]], [[1, 6]], [[1, 6]], [[1, 2], [5, 2]], [[1, 2], [5, 2]], []] },
  { id: 'tressee', nom: 'Tressée', rangs: [[[1, 6]], [[1, 6]], [[0, 2], [6, 2]], [[1, 6]], [[1, 6]], [[2, 2], [4, 2]], [[2, 2], [4, 2]], [[2, 2], [4, 2]]] },
  { id: 'deuxTresses', nom: 'Deux tresses', rangs: [[[1, 6]], [[1, 6]], [[0, 2], [6, 2]], [[1, 6]], [[2, 1], [5, 1]], [[2, 1], [5, 1]], [[2, 1], [5, 1]], [[2, 1], [5, 1]]] },
  { id: 'nattee', nom: 'Nattée large', rangs: [[[1, 6]], [[1, 6]], [[0, 2], [6, 2]], [[1, 6]], [[1, 6]], [[1, 2], [3, 2], [5, 2]], [[1, 6]], [[3, 2]]] }
];

/* ton : base · sombre · clair · plaque ; manche : rangs couverts sur 12 */
export const TENUES = [
  { id: 'tunique', nom: 'Tunique de lin', ton: 'base', manche: 5, jambes: 'pantalon', col: true, ceinture: true },
  { id: 'chemise', nom: 'Chemise ouverte', ton: 'clair', manche: 9, jambes: 'pantalon', ceinture: true, extras: [[3, 0, 2, 6, 'peau']] },
  { id: 'gilet', nom: 'Gilet de cuir', ton: 'sombre', manche: 0, jambes: 'pantalon', ceinture: true, extras: [[3, 0, 2, 8, 'clair']] },
  { id: 'cuir', nom: 'Cuir clouté', ton: 'sombre', manche: 5, jambes: 'pantalon', ceinture: true, epaulettes: true, extras: [[1, 2, 1, 1, 'fer'], [3, 2, 1, 1, 'fer'], [5, 2, 1, 1, 'fer'], [1, 6, 1, 1, 'fer'], [3, 6, 1, 1, 'fer'], [5, 6, 1, 1, 'fer']] },
  { id: 'gambison', nom: 'Gambison', ton: 'base', manche: 8, jambes: 'pantalon', ceinture: true, extras: [[0, 2, 8, 1, 'sombre'], [0, 4, 8, 1, 'sombre'], [0, 6, 8, 1, 'sombre']] },
  { id: 'pourpoint', nom: 'Pourpoint', ton: 'base', manche: 12, jambes: 'pantalon', col: true, ceinture: true, extras: [[3, 1, 1, 7, 'clair'], [4, 1, 1, 7, 'sombre']] },
  { id: 'tabard', nom: 'Tabard', ton: 'sombre', manche: 5, jambes: 'pantalon', ceinture: true, extras: [[2, 0, 4, 9, 'clair'], [3, 3, 2, 2, 'fer']] },
  { id: 'plastron', nom: 'Plastron léger', ton: 'plaque', manche: 3, jambes: 'pantalon', ceinture: true, epaulettes: true, extras: [[1, 1, 6, 1, 'fer'], [1, 3, 1, 1, 'fer'], [6, 3, 1, 1, 'fer'], [3, 3, 2, 3, 'clair']] },
  { id: 'cuirasse', nom: 'Cuirasse', ton: 'plaque', manche: 2, jambes: 'pantalon', epaulettes: true, extras: [[0, 0, 8, 2, 'fer'], [3, 2, 2, 6, 'clair'], [0, 8, 8, 1, 'fer']] },
  { id: 'ecailles', nom: 'Armure d’écailles', ton: 'plaque', manche: 6, jambes: 'pantalon', ceinture: true, epaulettes: true, extras: [[0, 1, 8, 1, 'sombre'], [0, 3, 8, 1, 'sombre'], [0, 5, 8, 1, 'sombre'], [0, 7, 8, 1, 'sombre']] },
  { id: 'mailles', nom: 'Cotte de mailles', ton: 'plaque', manche: 10, jambes: 'pantalon', ceinture: true, extras: [[1, 1, 1, 10, 'sombre'], [3, 1, 1, 10, 'sombre'], [5, 1, 1, 10, 'sombre'], [7, 1, 1, 10, 'sombre']] },
  { id: 'robe', nom: 'Robe de laine', ton: 'base', manche: 9, jambes: 'robe', col: true, ceinture: true },
  { id: 'robeMage', nom: 'Robe de mage', ton: 'sombre', manche: 12, jambes: 'robe', col: true, extras: [[2, 0, 4, 1, 'clair'], [3, 1, 2, 8, 'clair'], [3, 4, 2, 1, 'fer']] },
  { id: 'toge', nom: 'Toge', ton: 'clair', manche: 2, jambes: 'robe', extras: [[0, 1, 3, 1, 'sombre'], [1, 2, 3, 1, 'sombre'], [2, 3, 3, 1, 'sombre'], [3, 4, 3, 1, 'sombre']] },
  { id: 'tablier', nom: 'Tablier de forge', ton: 'sombre', manche: 0, jambes: 'pantalon', ceinture: true, extras: [[2, 0, 4, 10, 'clair'], [2, 0, 1, 1, 'fer'], [5, 0, 1, 1, 'fer']] },
  { id: 'jerkin', nom: 'Jerkin lacé', ton: 'base', manche: 4, jambes: 'pantalon', ceinture: true, extras: [[3, 1, 1, 1, 'sombre'], [4, 2, 1, 1, 'sombre'], [3, 3, 1, 1, 'sombre'], [4, 4, 1, 1, 'sombre'], [3, 5, 1, 1, 'sombre']] },
  { id: 'bandes', nom: 'Bandes de tissu', ton: 'clair', manche: 3, jambes: 'pantalon', extras: [[0, 0, 8, 1, 'base'], [0, 2, 8, 1, 'base'], [0, 4, 8, 1, 'base'], [0, 6, 8, 1, 'base'], [0, 8, 8, 1, 'base']] },
  { id: 'nu', nom: 'Torse nu', ton: 'base', manche: 0, jambes: 'pantalon', torseNu: true, ceinture: true },
  { id: 'harnais', nom: 'Harnais de cuir', ton: 'sombre', manche: 0, jambes: 'pantalon', torseNu: true, ceinture: true, extras: [[2, 0, 1, 9, 'sombre'], [5, 0, 1, 9, 'sombre'], [0, 4, 8, 1, 'sombre']] }
];

export const FORMES_TETE = [
  { id: 'carree', nom: 'Carrée' },
  { id: 'ovale', nom: 'Ovale' },
  { id: 'anguleuse', nom: 'Anguleuse' }
];

export const PAL_CHEVEUX = ['#241C17', '#3E2A1C', '#6B4423', '#A8702F', '#C9A227', '#D9D2C5', '#8C2F23', '#2E4A5C', '#5B3A6E'];
export const PAL_YEUX = ['#4A2E18', '#3E6B3A', '#2E5A7A', '#7A6A2E', '#8E2F2F', '#6A5A7A', '#C9C4BA', '#D9A62C'];
export const PAL_TENUE = ['#6B4A2E', '#4F5B46', '#3E4A5C', '#6E3B32', '#7A6A4A', '#403A46', '#8A7250'];

const PEAU_HUMAINE = ['#F2CBA5', '#E0AC7E', '#C68863', '#9C6644', '#6F4A32', '#4E3524'];

/* ---------- especes ---------- */
export const ESPECES = {
  humain: {
    id: 'humain', nom: 'Humain', taille: 1.75, min: 1.60, max: 1.90,
    apparence: 'Silhouette proportionnée et passe-partout, traits universels sans extravagance ni signe anatomique surnaturel.',
    attribut: 'Endurance accrue (jauge plus grande et récupération plus rapide).',
    peaux: PEAU_HUMAINE, traits: {}, mods: {},
    variantes: [
      { nom: 'Cavalier des plaines',
        apparence: 'Longues jambes de cavalier, carnation dorée piquée de taches de soleil, traits doux et réguliers.',
        peaux: ['#F2CBA5', '#E0AC7E', '#C68863', '#9C6644'],
        mods: { torse: 0.96, jambes: 1.08 },
        traits: { relief: 0.7, motif: 'taches', accent: '#B5793F', formeTeteForcee: 'ovale', yeuxForces: 'amande', boucheForcee: 'neutre' } },
      { nom: 'Montagnard du nord',
        apparence: 'Carrure épaisse taillée par le froid, pommettes et nez brûlés par le gel.',
        peaux: ['#F6DCC6', '#EAC3A6', '#D9AC8C', '#C0916E'],
        mods: { torse: 1.14, bras: 1.1, tete: 1.02, jambes: 0.98 },
        traits: { relief: 1.2, motif: 'joues', accent: '#C46A55', formeTeteForcee: 'carree', yeuxForces: 'percants', boucheForcee: 'serree' } },
      { nom: 'Nomade des sables',
        apparence: 'Silhouette sèche et fine, peau cuivrée voilée de poussière claire, yeux plissés contre le vent.',
        peaux: ['#A8713F', '#8A5A31', '#6F4A32', '#4E3524'],
        mods: { torse: 0.9, jambes: 1.06, bras: 0.94 },
        traits: { relief: 1, motif: 'poussiere', accent: '#E8D9B8', formeTeteForcee: 'anguleuse', yeuxForces: 'plisses', boucheForcee: 'fine' } },
      { nom: 'Bourgeois des cités',
        apparence: "Corps sédentaire aux épaules molles, visage buriné de rides et de plis d'expression.",
        peaux: ['#E7C39B', '#D2A87F', '#B48A64', '#8E6A4A'],
        mods: { torse: 1.06, jambes: 0.96, tete: 1.04 },
        traits: { relief: 1.5, motif: 'plisse', accent: '#6E4A34', formeTeteForcee: 'ovale', yeuxForces: 'lourds', boucheForcee: 'moue' } }
    ]
  },
  nain: {
    id: 'nain', nom: 'Nain', taille: 1.30, min: 1.15, max: 1.45,
    apparence: 'Bâti comme un roc, torse très large et jambes courtes, avec une pilosité imposante et des mains massives taillées pour la forge.',
    attribut: 'Peut miner à mains nues au palier équivalent à son armure.',
    peaux: ['#E6B48C', '#D19C74', '#B57E58', '#8C5C3E', '#6B462F'],
    traits: { relief: 1.3, mainsMassives: true, barbeMin: 2 }, mods: { torse: 1.2, jambes: 0.7, tete: 1.05, bras: 1.15 },
    variantes: [
      { nom: 'Forgeron des profondeurs',
        apparence: "Bras de martelé et mains énormes, barbe pleine, peau marquée de suie et d'escarbilles.",
        peaux: ['#E6B48C', '#D19C74', '#B57E58', '#8C5C3E'],
        mods: { torse: 1.26, bras: 1.22, jambes: 0.68, tete: 1.05 },
        traits: { barbeForcee: 'pleine', motif: 'suie', accent: '#3A3330', relief: 1.4, formeTeteForcee: 'carree', yeuxForces: 'plisses', boucheForcee: 'serree' } },
      { nom: 'Graveur de runes',
        apparence: 'Tête plus large, barbe tressée serrée, glyphes gravés au front et aux avant-bras.',
        peaux: ['#EBC49E', '#D9AE86', '#C08F66', '#9A6E4C'],
        mods: { torse: 1.16, tete: 1.12, bras: 1.08, jambes: 0.72 },
        traits: { barbeForcee: 'tressee', motif: 'runes', accent: '#7FD2E8', relief: 1.1, formeTeteForcee: 'ovale', yeuxForces: 'brillants' } },
      { nom: 'Briseur de galeries',
        apparence: 'Épaules voûtées par la roche, barbe fourchue, mâchoire lourde et regard enfoncé.',
        peaux: ['#C99A72', '#B07F58', '#956744', '#77502F'],
        mods: { torse: 1.34, bras: 1.3, jambes: 0.66, tete: 1.05, voute: 6 },
        traits: { barbeForcee: 'fourchue', motif: 'plisse', accent: '#4A3A2C', relief: 1.6, machoire: true, formeTeteForcee: 'carree', yeuxForces: 'enfonces', boucheForcee: 'dents' } },
      { nom: 'Aïeul des clans',
        apparence: 'Crâne rasé et barbe fleuve jusqu’à la ceinture, peau tavelée par les siècles.',
        peaux: ['#F0D0B0', '#DDB894', '#C39B78', '#A47E5E'],
        mods: { torse: 1.14, jambes: 0.72, tete: 1.08, bras: 1.1 },
        traits: { barbeForcee: 'fleuve', coiffureForcee: 'rasee', motif: 'taches', accent: '#8A7358', relief: 1.5, yeuxForces: 'lourds', boucheForcee: 'moue' } }
    ]
  },
  elfeBois: {
    id: 'elfeBois', nom: 'Elfe des bois', taille: 1.85, min: 1.70, max: 2.00,
    apparence: "Morphologie svelte et agile, visage fin, longues oreilles pointues effilées vers l'arrière et carnation aux teintes naturelles.",
    attribut: 'Immunité totale aux dégâts de chute.',
    peaux: ['#E6D3B0', '#D9C7A3', '#C2B48B', '#A8B08A', '#8E9C74'],
    traits: { oreilles: 3, relief: 0.6 }, mods: { torse: 0.94, jambes: 1.05 },
    variantes: [
      { nom: 'Veilleur des futaies',
        apparence: 'Très longues oreilles effilées et carnation piquée de marbrures de lichen.',
        peaux: ['#E6D3B0', '#D9C7A3', '#C2B48B', '#A8B08A'],
        mods: { torse: 0.92, jambes: 1.08 },
        traits: { oreilles: 4, motif: 'lichen', accent: '#8E9C74', relief: 0.6, formeTeteForcee: 'ovale', yeuxForces: 'amande' } },
      { nom: 'Porteur de bois',
        apparence: "Une ramure de cerf lui pousse au front ; peau d'écorce claire et regard fixe.",
        peaux: ['#D8C9A6', '#C6B48E', '#B0A079', '#948B66'],
        mods: { torse: 0.96, jambes: 1.04, tete: 1.02 },
        traits: { appendice: 'bois', oreilles: 3, motif: 'lichen', accent: '#6E5238', relief: 0.8, formeTeteForcee: 'anguleuse', yeuxForces: 'percants' } },
      { nom: "Chasseur d'ombre",
        apparence: 'Carnation vert sombre rayée de camouflage, membres longs et silencieux.',
        peaux: ['#A8B08A', '#8E9C74', '#75855F', '#5C6B4A'],
        mods: { torse: 0.9, jambes: 1.12, bras: 0.94 },
        traits: { oreilles: 3, motif: 'rayures', accent: '#3C4A32', relief: 0.7, formeTeteForcee: 'anguleuse', yeuxForces: 'fendus', boucheForcee: 'fine' } },
      { nom: 'Fileur de sève',
        apparence: 'Ossature minuscule, oreilles démesurées, veines de sève dorée sous la peau.',
        peaux: ['#EFE0C0', '#E0CDA8', '#CDB88E', '#B5A176'],
        mods: { torse: 0.86, jambes: 1.02, tete: 1.06, bras: 0.9 },
        traits: { oreilles: 4, motif: 'runes', accent: '#C9A227', relief: 0.5, yeuxForces: 'vides', boucheForcee: 'petite' } }
    ]
  },
  hautElfe: {
    id: 'hautElfe', nom: 'Haut-elfe', taille: 1.90, min: 1.75, max: 2.00,
    apparence: 'Grand, longiligne et altier, traits faciaux anguleux et regard lumineux sans pupille visible trahissant son lien avec la magie.',
    attribut: 'Courte téléportation instantanée dans la direction du regard.',
    peaux: ['#F6E6D8', '#E9D2C6', '#DCC8DD', '#CFD8E8', '#C3B6CE'],
    traits: { oreilles: 3, regard: 'lumineux', relief: 1.2 }, mods: { torse: 0.9, jambes: 1.1 },
    variantes: [
      { nom: "Héraut d'aube",
        apparence: "Peau nacrée filetée d'or, regard d'aube sans pupille.",
        peaux: ['#F6E6D8', '#E9D2C6', '#DCC8DD', '#CFD8E8'],
        mods: { torse: 0.9, jambes: 1.12 },
        traits: { lueur: '#F2DDA4', oreilles: 4, motif: 'dorure', accent: '#E8C46A', relief: 1.2 } },
      { nom: 'Mage de nuit',
        apparence: 'Carnation violine, glyphes bleus au front et aux bras, regard de nuit.',
        peaux: ['#C3B6CE', '#B0A3BE', '#9A8FA8', '#84798E'],
        mods: { torse: 0.86, jambes: 1.14, tete: 0.98 },
        traits: { lueur: '#A8C6F2', oreilles: 4, motif: 'runes', accent: '#7FD2E8', relief: 1.3, boucheForcee: 'fine' } },
      { nom: 'Gardien de cristal',
        apparence: 'Des éclats de cristal percent son crâne ; peau bleutée voilée de givre.',
        peaux: ['#E4EEF2', '#D2E0E8', '#BCCEDA', '#A6BCCB'],
        mods: { torse: 0.94, jambes: 1.08, tete: 1.02 },
        traits: { appendice: 'cristaux', lueur: '#F1F0EA', oreilles: 3, motif: 'givre', accent: '#DCEAF2', relief: 1 } },
      { nom: 'Archonte',
        apparence: "Couronne de lumière fixée au crâne, filets d'or sur tout le buste.",
        peaux: ['#F2E2C8', '#E4D0AE', '#D0B992', '#B79E76'],
        mods: { torse: 0.96, jambes: 1.1, tete: 1.04 },
        traits: { appendice: 'couronne', lueur: '#F2DDA4', oreilles: 3, motif: 'dorure', accent: '#E8C46A', relief: 1.4, boucheForcee: 'serree' } }
    ]
  },
  demiOrque: {
    id: 'demiOrque', nom: 'Demi-orque', taille: 2.00, min: 1.85, max: 2.00,
    apparence: 'Carrure colossale et voûtée, peau épaisse aux reflets gris-vert et mâchoire lourde armée de deux canines inférieures saillantes.',
    attribut: 'Survit à un coup fatal en restant à 1 PV (avec temps de recharge).',
    peaux: ['#94A48C', '#8FA07A', '#7C8F6B', '#6B7F63', '#5E7355'],
    traits: { canines: 2, relief: 1.5, machoire: true }, mods: { torse: 1.14, bras: 1.12, voute: 5 },
    variantes: [
      { nom: 'Sang-vert des marais',
        apparence: 'Peau gris-vert épaisse et grenue d’écailles, deux canines saillantes.',
        peaux: ['#94A48C', '#8FA07A', '#7C8F6B', '#6B7F63'],
        mods: { torse: 1.16, bras: 1.14, voute: 5 },
        traits: { canines: 2, motif: 'ecailles', accent: '#4A5C42', relief: 1.5 } },
      { nom: 'Briseur de crânes',
        apparence: 'Colosse voûté à mâchoire démesurée, front bas et regard enfoncé.',
        peaux: ['#8A9A7E', '#77886B', '#66785C', '#54654A'],
        mods: { torse: 1.3, bras: 1.3, jambes: 0.94, tete: 1.08, voute: 9 },
        traits: { canines: 2, machoireLourde: true, motif: 'plisse', accent: '#3A4633', relief: 1.7, yeuxForces: 'enfonces', boucheForcee: 'dents' } },
      { nom: 'Porte-cornes',
        apparence: 'Deux cornes basses percent les tempes ; canines plus courtes, crâne épais.',
        peaux: ['#A0A886', '#8D9673', '#7A8462', '#677152'],
        mods: { torse: 1.2, bras: 1.16, tete: 1.06, voute: 6 },
        traits: { appendice: 'cornesCourtes', canines: 1, motif: 'ecailles', accent: '#5A6B4A', relief: 1.5, boucheForcee: 'rictus' } },
      { nom: 'Sang-cendre',
        apparence: 'Carnation cendrée de lignée volcanique, peau saupoudrée de suie.',
        peaux: ['#8E9094', '#7B7D80', '#68696C', '#55565A'],
        mods: { torse: 1.12, bras: 1.1, jambes: 1.02, voute: 4 },
        traits: { canines: 2, motif: 'suie', accent: '#2A2426', relief: 1.6, formeTeteForcee: 'anguleuse', yeuxForces: 'brillants' } }
    ]
  },
  revenant: {
    id: 'revenant', nom: 'Revenant', taille: 1.75, min: 1.60, max: 1.90,
    apparence: 'Silhouette émaciée à la peau cendreuse, parcourue de veines noires bien visibles, avec un regard vitreux totalement dénué de clignements.',
    attribut: "Respiration aquatique infinie (ne consomme pas d'oxygène).",
    peaux: ['#C9C3BC', '#B3ADA8', '#9AA0A0', '#847E7C', '#6E6A68'],
    traits: { veines: 1, regard: 'vitreux', relief: 1.4 }, mods: { torse: 0.88, bras: 0.9 },
    variantes: [
      { nom: 'Veillant',
        apparence: 'Peau cendreuse à peine fendue, veines noires fines, regard vitreux.',
        peaux: ['#C9C3BC', '#B3ADA8', '#9AA0A0', '#847E7C'],
        mods: { torse: 0.88, bras: 0.9 },
        traits: { veines: 1, motif: 'craquelures', accent: '#2A2426', relief: 1.4 } },
      { nom: 'Desséché',
        apparence: "Corps émacié jusqu'à l'os, peau craquelée sur toute la surface.",
        peaux: ['#B8AE9C', '#A2988A', '#8B8276', '#746C62'],
        mods: { torse: 0.78, bras: 0.8, jambes: 1.04, tete: 0.96 },
        traits: { veines: 2, craquelee: true, motif: 'craquelures', accent: '#1F1A18', relief: 1.7, yeuxForces: 'enfonces', boucheForcee: 'fendue' } },
      { nom: "Couronné d'os",
        apparence: 'Des pointes d’os ont percé le crâne en couronne.',
        peaux: ['#D2CCC2', '#BAB3A9', '#A29B92', '#8A837B'],
        mods: { torse: 0.92, bras: 0.92, tete: 1.04 },
        traits: { appendice: 'couronneOs', veines: 1, motif: 'craquelures', accent: '#E6DCC4', relief: 1.5, boucheForcee: 'serree' } },
      { nom: 'Noyé',
        apparence: "Chair gonflée d'eau, marbrures d'algues, bouche restée ouverte.",
        peaux: ['#9AA8A6', '#87948F', '#74807B', '#616C68'],
        mods: { torse: 0.96, bras: 0.94, jambes: 0.96, voute: 4 },
        traits: { veines: 2, motif: 'lichen', accent: '#4A5C4A', relief: 1.3, yeuxForces: 'globuleux', boucheForcee: 'entrouverte' } }
    ]
  },
  sylphe: {
    id: 'sylphe', nom: 'Sylphe', taille: 1.65, min: 1.50, max: 1.80,
    apparence: 'Allure aérienne et presque translucide, ossature très fine et chevelure éthérée qui flotte doucement comme portée par une brise continue.',
    attribut: 'Chute ralentie automatique (plane doucement sans subir de dégâts).',
    peaux: ['#E8F0F2', '#D8E6EC', '#CFE0E8', '#E6E2F2', '#C6D8E0'],
    traits: { alpha: 0.78, relief: 0.4, chevelureEtheree: true }, mods: { torse: 0.84, bras: 0.85, flotte: true },
    variantes: [
      { nom: 'Souffle clair',
        apparence: 'Presque opaque, carnation de brume claire poudrée de givre.',
        peaux: ['#E8F0F2', '#D8E6EC', '#CFE0E8', '#C6D8E0'],
        mods: { torse: 0.84, bras: 0.85, flotte: true },
        traits: { alpha: 0.9, motif: 'givre', accent: '#FBFEFF', relief: 0.4 } },
      { nom: 'Voile de brume',
        apparence: 'Silhouette étirée et translucide, regard sans pupille.',
        peaux: ['#DCE8F0', '#CBDCE8', '#BACFDE', '#A9C2D4'],
        mods: { torse: 0.78, bras: 0.8, jambes: 1.06, flotte: true },
        traits: { alpha: 0.7, motif: 'givre', accent: '#EAF4FA', relief: 0.3, yeuxForces: 'vides', boucheForcee: 'petite' } },
      { nom: 'Éclat de cristal',
        apparence: 'Des éclats de cristal lui poussent au crâne et captent la lumière.',
        peaux: ['#E6E2F2', '#D6D2E8', '#C4C0DC', '#B2AECE'],
        mods: { torse: 0.82, bras: 0.84, tete: 1.04, flotte: true },
        traits: { appendice: 'cristaux', alpha: 0.8, motif: 'givre', accent: '#CFD8E8', relief: 0.5 } },
      { nom: 'Diaphane',
        apparence: 'À peine visible, traversé de glyphes de vent.',
        peaux: ['#F2F6F8', '#E6EEF2', '#DAE6EC', '#CEDEE6'],
        mods: { torse: 0.74, bras: 0.76, jambes: 1.02, flotte: true },
        traits: { alpha: 0.55, motif: 'runes', accent: '#B8D8EA', relief: 0.2, formeTeteForcee: 'ovale', yeuxForces: 'vides' } }
    ]
  },
  gnome: {
    id: 'gnome', nom: 'Gnome', taille: 1.00, min: 0.85, max: 1.00,
    apparence: "Corps miniature et agile, surmonté d'une tête expressive avec de très grands yeux curieux et des doigts fins d'une extrême précision.",
    attribut: "Peut ramper et se faufiler dans les ouvertures d'un seul bloc de hauteur.",
    peaux: PEAU_HUMAINE.concat(['#E7C6A0']),
    traits: { yeuxForces: 'grands', relief: 0.7 }, mods: { tete: 1.22, torse: 0.92, jambes: 0.84, bras: 0.9 },
    variantes: [
      { nom: "Bricoleur d'atelier",
        apparence: "Tête surdimensionnée, mains fines, doigts et joues noircis d'atelier.",
        peaux: ['#F2CBA5', '#E0AC7E', '#C68863', '#9C6644'],
        mods: { tete: 1.24, torse: 0.92, jambes: 0.84, bras: 0.9 },
        traits: { yeuxForces: 'grands', motif: 'suie', accent: '#3A3330', relief: 0.7, boucheForcee: 'sourire' } },
      { nom: 'Fouineur de terriers',
        apparence: 'Encore plus petit, oreilles pointues courtes et yeux globuleux de fouisseur.',
        peaux: ['#E7C6A0', '#D2AC85', '#B8906A', '#96704E'],
        mods: { tete: 1.3, torse: 0.86, jambes: 0.78, bras: 0.86 },
        traits: { yeuxForces: 'globuleux', oreilles: 2, motif: 'taches', accent: '#8A5A31', relief: 0.6, boucheForcee: 'petite' } },
      { nom: 'Alchimiste',
        apparence: 'Regard brillant de veilleur, avant-bras couverts de formules.',
        peaux: ['#E0C89A', '#CDB283', '#B89A6C', '#9C7F55'],
        mods: { tete: 1.2, torse: 0.94, jambes: 0.86, bras: 0.88 },
        traits: { yeuxForces: 'brillants', motif: 'runes', accent: '#7FD2E8', relief: 0.8, boucheForcee: 'coin' } },
      { nom: 'Ancien de la guilde',
        apparence: "Ventre rond, crâne large et visage plissé de rides d'établi.",
        peaux: ['#F0D0B0', '#DDB894', '#C39B78', '#A47E5E'],
        mods: { tete: 1.26, torse: 1, jambes: 0.8, bras: 0.92 },
        traits: { yeuxForces: 'plisses', motif: 'plisse', accent: '#6E4A34', relief: 1.2, boucheForcee: 'moue' } }
    ]
  },
  aasimar: {
    id: 'aasimar', nom: 'Aasimar', taille: 1.80, min: 1.65, max: 1.95,
    apparence: 'Posture noble et peau aux reflets métalliques légers (or ou argent), couronnée par des ailes spectrales lumineuses prêtes à se déployer.',
    attribut: 'Déploie des ailes pour effectuer de courts vols.',
    peaux: ['#F0DCC0', '#EADFCB', '#E3D6E0', '#D9C08A', '#C9A97E'],
    traits: { reflet: '#E8C46A', halo: true, relief: 0.9 }, mods: { ailes: true, torse: 0.98 },
    variantes: [
      { nom: "Ailé d'or",
        apparence: "Reflets d'or sur la peau, ailes spectrales dorées, halo dans le dos.",
        peaux: ['#F0DCC0', '#EADFCB', '#D9C08A', '#C9A97E'],
        mods: { ailes: true, torse: 0.98 },
        traits: { reflet: '#E8C46A', halo: true, motif: 'dorure', accent: '#E8C46A', relief: 0.9 } },
      { nom: "Ailé d'argent",
        apparence: "Longiligne et pâle, reflets d'argent froid, regard sans pupille.",
        peaux: ['#F2EDE4', '#E3DCD2', '#D3DAE2', '#C0C8D2'],
        mods: { ailes: true, torse: 0.94, jambes: 1.06 },
        traits: { reflet: '#D3DAE2', halo: true, motif: 'givre', accent: '#D3DAE2', relief: 1, yeuxForces: 'vides' } },
      { nom: 'Couronné',
        apparence: 'Une couronne de lumière est fixée au crâne, au-dessus du halo.',
        peaux: ['#F2E2C8', '#E4D0AE', '#D0B992', '#B79E76'],
        mods: { ailes: true, torse: 1, tete: 1.04 },
        traits: { appendice: 'couronne', reflet: '#E8C46A', halo: true, motif: 'dorure', accent: '#F2DDA4', relief: 1.1, boucheForcee: 'serree' } },
      { nom: 'Ailé de cuivre',
        apparence: 'Carrure plus lourde, peau cuivrée piquée, reflets chauds.',
        peaux: ['#E8C9A4', '#D8AE7E', '#C89460', '#A9743F'],
        mods: { ailes: true, torse: 1.06, bras: 1.08 },
        traits: { reflet: '#D08A55', halo: true, motif: 'taches', accent: '#D08A55', relief: 1.2, yeuxForces: 'percants' } }
    ]
  },
  hommeBete: {
    id: 'hommeBete', nom: 'Homme-bête', taille: 1.75, min: 1.60, max: 1.90,
    apparence: "Silhouette athlétique recouverte d'un pelage ras, pourvue d'appendices crâniens mobiles, de griffes et d'une démarche sur la pointe des pieds.",
    attribut: 'Saute deux fois plus haut que les autres races.',
    peaux: ['#C79A62', '#A97C4E', '#E0C89A', '#6E5238', '#3C3330'],
    traits: { pelage: 1, griffes: true, relief: 1 },
    mods: { pointes: true, torse: 1.02 },
    variantes: [
      { nom: 'Chasseur tigré',
        apparence: "Sous-race élancée aux membres longs : pelage fauve rayé de noir, oreilles félines basses, longue queue annelée et démarche haute sur la pointe des pieds.",
        peaux: ['#E4BA76', '#C9964C', '#B07C3A', '#8E5F2A', '#6E4720'],
        mods: { torse: 0.94, jambes: 1.1, bras: 0.96, pointes: true },
        traits: { pelage: 1, motif: 'rayures', accent: '#2E2119', appendice: 'oreillesFelin',
          queue: 'longue', yeuxForces: 'fendus', formeTeteForcee: 'anguleuse', boucheForcee: 'crocs', relief: 1.1 } },
      { nom: 'Traqueur gris',
        apparence: "Sous-race massive et voûtée : carrure d'épaules deux fois plus large, selle de poil sombre sur le dos, oreilles dressées, queue touffue et mâchoire lourde.",
        peaux: ['#9AA0A2', '#828A8E', '#6B7377', '#545B60', '#3E4346'],
        mods: { torse: 1.2, bras: 1.2, tete: 1.05, jambes: 0.98, voute: 7, pointes: true },
        traits: { pelage: 1.5, motif: 'selle', accent: '#DCE0DD', appendice: 'oreillesLupin',
          queue: 'touffue', yeuxForces: 'percants', formeTeteForcee: 'carree', boucheForcee: 'dents', machoire: true, relief: 1.4 } },
      { nom: 'Rôdeur roux',
        apparence: "Sous-race courte et fine, à grosse tête : robe rousse à contre-ombre blanche, chaussettes noires, oreilles démesurées et queue en panache à bout clair.",
        peaux: ['#DE9243', '#C4762F', '#A85F26', '#8A4A1E', '#EBC9A0'],
        mods: { torse: 0.86, jambes: 0.94, bras: 0.88, tete: 1.16, pointes: true },
        traits: { pelage: 0.8, motif: 'contreOmbre', accent: '#F2EAD9', appendice: 'oreillesVulpin',
          queue: 'panache', yeuxForces: 'grands', formeTeteForcee: 'ovale', boucheForcee: 'coin', relief: 0.8 } },
      { nom: 'Briseur cornu',
        apparence: "Sous-race bovine : torse en tonneau, jambes courtes, plaque de poil clair au front et deux cornes recourbées. Sabots au lieu de griffes, aucune queue visible.",
        peaux: ['#7A6450', '#5C4A38', '#4A3A2C', '#8A7358', '#3A2E24'],
        mods: { torse: 1.34, bras: 1.26, jambes: 0.88, tete: 1.12, voute: 10 },
        traits: { pelage: 1.2, motif: 'front', accent: '#E6DCC4', appendice: 'cornes',
          griffes: false, yeuxForces: 'lourds', formeTeteForcee: 'carree', boucheForcee: 'serree', machoireLourde: true, relief: 1.5 } }
    ]
  }
};

export const ORDRE_ESPECES = ['humain', 'nain', 'elfeBois', 'hautElfe', 'demiOrque', 'revenant', 'sylphe', 'gnome', 'aasimar', 'hommeBete'];

export function varianteDe(cfg) {
  const sp = ESPECES[cfg.espece] || ESPECES.humain;
  return sp.variantes[Math.min(cfg.variante | 0, sp.variantes.length - 1)] || sp.variantes[0];
}
/* Une variante peut porter sa propre carnation et ses propres proportions :
   c'est ce qui la rend lisible à distance, avant toute couleur de detail. */
export function peauxDe(cfg) {
  const sp = ESPECES[cfg.espece] || ESPECES.humain;
  return varianteDe(cfg).peaux || sp.peaux;
}
export function modsDe(cfg) {
  const sp = ESPECES[cfg.espece] || ESPECES.humain;
  return Object.assign({}, sp.mods, varianteDe(cfg).mods || {});
}

export function traitsDe(cfg) {
  const sp = ESPECES[cfg.espece] || ESPECES.humain;
  return Object.assign({ relief: 1, alpha: 1 }, sp.traits, varianteDe(cfg).traits || {});
}

export function defaut(especeId) {
  const sp = ESPECES[especeId] || ESPECES.humain;
  const T = Object.assign({}, sp.traits, (sp.variantes[0] || {}).traits || {});
  const pal = (sp.variantes[0] || {}).peaux || sp.peaux;
  return {
    nom: 'Sans nom', espece: sp.id, variante: 0, taille: sp.taille, corpulence: 0.5,
    peau: pal[1] || pal[0], formeTete: T.formeTeteForcee || 'carree',
    yeux: T.yeuxForces || 'amande', couleurYeux: PAL_YEUX[0], bouche: T.boucheForcee || 'neutre', marques: 'aucune',
    coiffure: T.coiffureForcee || 'courte', couleurCheveux: PAL_CHEVEUX[1],
    barbe: T.barbeForcee || 'aucune', tenue: 'tunique', couleurTenue: PAL_TENUE[0]
  };
}

const pick = a => a[Math.floor(Math.random() * a.length)];
export function aleatoire(especeId) {
  const id = especeId || pick(ORDRE_ESPECES);
  const sp = ESPECES[id];
  const c = defaut(id);
  c.variante = Math.floor(Math.random() * sp.variantes.length);
  const T = traitsDe(c);
  c.taille = Math.round((sp.min + Math.random() * (sp.max - sp.min)) * 100) / 100;
  c.corpulence = Math.round(Math.random() * 100) / 100;
  c.peau = pick(peauxDe(c));
  c.formeTete = T.formeTeteForcee || pick(FORMES_TETE).id;
  c.yeux = T.yeuxForces || pick(YEUX).id;
  c.couleurYeux = pick(PAL_YEUX);
  c.bouche = T.boucheForcee || pick(BOUCHES).id;
  c.marques = pick(MARQUES).id;
  c.coiffure = T.coiffureForcee || pick(COIFFURES).id;
  c.couleurCheveux = pick(PAL_CHEVEUX);
  c.barbe = T.barbeForcee || pick(BARBES).id;
  c.tenue = pick(TENUES).id;
  c.couleurTenue = pick(PAL_TENUE);
  return c;
}

/* ---------- pinceaux ---------- */
function F(part, face) { return PATRON.parties[part].faces[face]; }

function brosse(g, r) {
  return {
    r,
    plein(col) { g.fillStyle = col; g.fillRect(r.x, r.y, r.largeur, r.hauteur); },
    px(x, y, col, w, h) {
      w = w || 1; h = h || 1;
      if (x < 0 || y < 0 || x + w > r.largeur || y + h > r.hauteur) return;
      g.fillStyle = col; g.fillRect(r.x + x, r.y + y, w, h);
    },
    /* meme bloc, miroir horizontal de la face */
    sym(x, y, col, w, h) {
      w = w || 1; h = h || 1;
      this.px(x, y, col, w, h);
      this.px(r.largeur - x - w, y, col, w, h);
    }
  };
}

function plaque(g, r, col, seed, amp) {
  amp = amp == null ? 0.07 : amp;
  g.fillStyle = col; g.fillRect(r.x, r.y, r.largeur, r.hauteur);
  const sombre = shade(col, -amp), clair = shade(col, amp * 0.9);
  for (let y = 0; y < r.hauteur; y++) {
    for (let x = 0; x < r.largeur; x++) {
      const n = bruit(x + r.x, y + r.y, seed);
      if (n < 0.2) { g.fillStyle = sombre; g.fillRect(r.x + x, r.y + y, 1, 1); }
      else if (n > 0.88) { g.fillStyle = clair; g.fillRect(r.x + x, r.y + y, 1, 1); }
    }
  }
  /* eclairage fixe : arete haute eclairee, base assombrie */
  g.fillStyle = shade(col, amp * 1.4); g.fillRect(r.x, r.y, r.largeur, 1);
  g.fillStyle = shade(col, -amp * 1.6); g.fillRect(r.x, r.y + r.hauteur - 1, r.largeur, 1);
}

/* ---------- peinture ---------- */
export function peindre(cfg) {
  const sp = ESPECES[cfg.espece] || ESPECES.humain;
  const T = traitsDe(cfg);
  const c = document.createElement('canvas');
  c.width = 64; c.height = 64;
  const g = c.getContext('2d');
  g.imageSmoothingEnabled = false;
  g.clearRect(0, 0, 64, 64);

  const peau = cfg.peau;
  const peauF = shade(peau, -0.1);       // faces laterales
  const cheveux = cfg.couleurCheveux;
  const relief = T.relief || 1;

  g.globalAlpha = T.alpha || 1;

  /* 1 — peau */
  MEMBRES1.forEach((part, i) => {
    FACES.forEach((f, j) => {
      const base = (f === 'droite' || f === 'gauche') ? peauF : (f === 'dessous' ? shade(peau, -0.18) : peau);
      plaque(g, F(part, f), base, i * 7 + j, T.pelage ? 0.13 * T.pelage : 0.06);
    });
  });
  if (T.pelage) pelage(g, peau, T);
  if (MOTIFS[T.motif]) MOTIFS[T.motif](g, peau, T.accent || shade(peau, 0.18));
  if (T.reflet) reflets(g, T.reflet);
  if (T.veines) veines(g, T);

  /* 2 — visage */
  visage(g, cfg, T, peau, cheveux, relief);

  /* 3 — tenue, puis les marques qui se posent par-dessus */
  tenue(g, cfg, T, peau);
  marques(g, cfg, peau);

  /* 4 — pilosite : uniquement des pieces de couche 2, jamais sur la peau */
  peindreBarbe(g, T.barbeForcee || cfg.barbe, cheveux);
  peindreChevelure(g, cfg.coiffure, cheveux, T);

  /* 5 — couche 2 */
  g.globalAlpha = 1;
  couche2(g, cfg, T, cheveux, peau);
  if ((sp.mods || {}).ailes) peindreAiles(g, T.reflet || '#E8C46A');

  return c;
}

/* Aile spectrale en texels : plumes de plus en plus transparentes vers la pointe.
   L'arete interne (x = 0) est celle qui touche le dos. */
function peindreAiles(g, teinte) {
  const n = parseInt(teinte.slice(1), 16);
  const rvb = [n >> 16 & 255, n >> 8 & 255, n & 255];
  const rgba = (a, l) => 'rgba(' + rvb.map(v => Math.round(v + (255 - v) * l)).join(',') + ',' + a + ')';
  const LARG = [7, 7, 7, 6, 6, 5, 5, 4, 4, 3, 3, 2, 2, 1];
  ['devant', 'derriere'].forEach((f, k) => {
    const r = F('aile', f), b = brosse(g, r);
    for (let y = 0; y < 14; y++) {
      const w = LARG[y];
      for (let x = 0; x < w; x++) {
        const bord = x === w - 1;
        const plume = (x + y) % 3 === 0;
        const a = (0.92 - y * 0.045 - x * 0.03).toFixed(2);
        /* la face arriere est le miroir de la face avant : c'est elle que l'on
           voit sur l'aile droite, retournee de 180 deg. */
        b.px(k ? 6 - x : x, y, rgba(a, bord ? 0.42 : plume ? 0.2 : k ? 0 : 0.08));
      }
    }
    b.px(k ? 5 : 0, 0, rgba(0.95, 0.5), 2, 1);
    b.px(k ? 6 : 0, 1, rgba(0.9, 0.34), 1, 3);
  });
  /* tranches : une colonne de l'aile suffit, elles ne font qu'un texel */
}

function pelage(g, peau, T) {
  const clair = shade(peau, 0.18), sombre = shade(peau, -0.22);
  const seuil = 0.86 - 0.08 * ((T.pelage || 1) - 1);
  [['torse', 'devant'], ['torse', 'derriere'], ['brasD', 'devant'], ['brasG', 'devant'], ['jambeD', 'devant'], ['jambeG', 'devant']].forEach(([p, f], i) => {
    const r = F(p, f), b = brosse(g, r);
    for (let y = 0; y < r.hauteur; y++) for (let x = 0; x < r.largeur; x++) {
      const n = bruit(x * 3 + i, y * 2, 11);
      if (n > seuil) b.px(x, y, sombre);
      else if (n < 0.14) b.px(x, y, clair);
    }
  });
  const b = brosse(g, F('torse', 'devant'));
  for (let y = 5; y < 12; y++) b.px(3, y, clair, 2);
}

/* ---------- motifs corporels de sous-race ----------
   Chaque motif couvre le crâne et les mains, qu'aucune tenue ne recouvre :
   la sous-race reste lisible même en armure de plates. */
function rayures(g, peau, acc) {
  const c = acc;
  ['devant', 'derriere'].forEach(f => {
    const b = brosse(g, F('torse', f));
    [[1, 0, 1, 5], [4, 0, 1, 4], [6, 1, 1, 5], [2, 6, 1, 4], [5, 7, 1, 4]].forEach(([x, y, w, h]) => b.px(x, y, c, w, h));
  });
  ['droite', 'gauche'].forEach(f => {
    const b = brosse(g, F('torse', f));
    b.px(1, 1, c, 1, 4); b.px(2, 7, c, 1, 3);
  });
  [['brasD', [1, 4, 7]], ['brasG', [1, 4, 7]], ['jambeD', [0, 3, 6]], ['jambeG', [0, 3, 6]]].forEach(([p, rangs]) => {
    ['devant', 'derriere', 'droite', 'gauche'].forEach(f => {
      const b = brosse(g, F(p, f));
      rangs.forEach(y => b.px(0, y, c, 4, 1));
    });
  });
  const bt = brosse(g, F('tete', 'devant'));
  bt.sym(1, 0, c, 1, 2); bt.sym(0, 3, c, 1, 1);
  ['droite', 'gauche'].forEach(f => {
    const b = brosse(g, F('tete', f));
    b.px(2, 1, c, 1, 3); b.px(5, 2, c, 1, 3);
  });
  brosse(g, F('tete', 'dessus')).px(2, 0, c, 1, 8);
  brosse(g, F('tete', 'derriere')).px(3, 0, c, 2, 5);
}

function selle(g, peau, acc) {
  const s = shade(peau, -0.32);
  brosse(g, F('torse', 'derriere')).px(0, 0, s, 8, 6);
  ['droite', 'gauche'].forEach(f => brosse(g, F('torse', f)).px(0, 0, s, 4, 5));
  brosse(g, F('torse', 'devant')).px(2, 0, acc, 4, 4);
  ['brasD', 'brasG'].forEach(p => ['devant', 'derriere', 'droite', 'gauche'].forEach(f => brosse(g, F(p, f)).px(0, 0, s, 4, 4)));
  const bt = brosse(g, F('tete', 'devant'));
  bt.sym(0, 0, s, 3, 2); bt.px(2, 5, shade(peau, -0.18), 4, 3);
  brosse(g, F('tete', 'dessus')).px(0, 0, s, 8, 5);
  brosse(g, F('tete', 'derriere')).px(0, 0, s, 8, 5);
}

function contreOmbre(g, peau, acc) {
  const noir = shade(peau, -0.62);
  const bt = brosse(g, F('tete', 'devant'));
  bt.px(2, 5, acc, 4, 3); bt.sym(0, 4, acc, 1, 2);
  ['devant', 'derriere'].forEach(f => brosse(g, F('torse', f)).px(2, 4, acc, 4, 8));
  ['brasD', 'brasG', 'jambeD', 'jambeG'].forEach(p => {
    ['devant', 'derriere', 'droite', 'gauche'].forEach(f => brosse(g, F(p, f)).px(0, 8, noir, 4, 4));
    brosse(g, F(p, 'dessous')).plein(noir);
  });
  brosse(g, F('tete', 'dessus')).px(0, 0, shade(peau, -0.2), 8, 3);
}

function front(g, peau, acc) {
  const bt = brosse(g, F('tete', 'devant'));
  bt.px(1, 0, acc, 6, 2); bt.px(2, 5, shade(peau, 0.14), 4, 1);
  brosse(g, F('tete', 'dessus')).px(1, 3, acc, 6, 5);
  ['devant', 'derriere'].forEach(f => brosse(g, F('torse', f)).px(1, 0, shade(peau, 0.1), 6, 2));
  ['brasD', 'brasG', 'jambeD', 'jambeG'].forEach(p => {
    ['devant', 'derriere', 'droite', 'gauche'].forEach(f => brosse(g, F(p, f)).px(0, 10, shade(peau, -0.5), 4, 2));
  });
}

/* Pigment, usure, gravure : motifs qui portent tous sur le crane et les mains,
   les seules zones qu'aucune tenue ne recouvre. */
function taches(g, peau, acc) {
  const bt = brosse(g, F('tete', 'devant'));
  [[1, 4], [2, 5], [5, 4], [6, 5], [2, 3], [5, 3], [1, 6], [6, 6]].forEach(([x, y]) => bt.px(x, y, acc));
  ['brasD', 'brasG'].forEach(p => {
    const b = brosse(g, F(p, 'devant'));
    [[1, 3], [2, 5], [0, 6], [3, 8]].forEach(([x, y]) => b.px(x, y, acc));
  });
}

function joues(g, peau, acc) {
  const bt = brosse(g, F('tete', 'devant'));
  bt.sym(1, 5, acc, 2, 1);
  bt.px(3, 4, acc, 2, 1);
  ['droite', 'gauche'].forEach(f => brosse(g, F('tete', f)).px(3, 5, acc, 2, 1));
}

function poussiere(g, peau, acc) {
  const bt = brosse(g, F('tete', 'devant'));
  bt.px(1, 0, acc, 6, 1); bt.sym(0, 2, acc, 1, 1);
  brosse(g, F('tete', 'dessus')).px(1, 1, acc, 6, 3);
  ['devant', 'derriere'].forEach(f => brosse(g, F('torse', f)).px(0, 0, acc, 8, 1));
}

function plisse(g, peau, acc) {
  const bt = brosse(g, F('tete', 'devant'));
  bt.px(1, 1, acc, 6, 1); bt.sym(1, 2, acc, 2, 1);
  bt.sym(0, 5, acc, 2, 1); bt.sym(1, 6, acc, 1, 1);
}

function suie(g, peau, acc) {
  const bt = brosse(g, F('tete', 'devant'));
  bt.sym(0, 5, acc, 2, 1); bt.px(2, 2, acc); bt.px(5, 3, acc);
  ['brasD', 'brasG'].forEach(p => ['devant', 'derriere', 'droite', 'gauche'].forEach(f => brosse(g, F(p, f)).px(0, 9, acc, 4, 3)));
}

function runes(g, peau, acc) {
  const bt = brosse(g, F('tete', 'devant'));
  bt.px(3, 1, acc); bt.px(4, 2, acc); bt.sym(1, 1, acc);
  ['brasD', 'brasG'].forEach(p => {
    const b = brosse(g, F(p, 'devant'));
    [[1, 4, 2, 1], [2, 5, 1, 1], [1, 6, 2, 1], [1, 7, 1, 1]].forEach(([x, y, w, h]) => b.px(x, y, acc, w, h));
  });
  brosse(g, F('torse', 'devant')).px(3, 2, acc, 2, 1);
}

function lichen(g, peau, acc) {
  [['tete', 'devant'], ['tete', 'droite'], ['tete', 'gauche'], ['torse', 'devant'],
   ['brasD', 'devant'], ['brasG', 'devant'], ['jambeD', 'devant'], ['jambeG', 'devant']].forEach(([p, f], i) => {
    const r = F(p, f), b = brosse(g, r);
    for (let y = 0; y < r.hauteur; y++) for (let x = 0; x < r.largeur; x++)
      if (bruit(x * 4 + i * 2, y * 3, 67) > 0.88) b.px(x, y, acc);
  });
}

function dorure(g, peau, acc) {
  const bt = brosse(g, F('tete', 'devant'));
  bt.px(1, 0, acc, 6, 1); bt.sym(0, 4, acc, 1, 1); bt.sym(1, 2, acc, 1, 1);
  ['devant', 'derriere'].forEach(f => {
    const b = brosse(g, F('torse', f));
    b.px(0, 0, acc, 8, 1); b.px(3, 1, acc, 2, 1);
  });
  ['brasD', 'brasG'].forEach(p => brosse(g, F(p, 'devant')).px(0, 0, acc, 4, 1));
}

function givre(g, peau, acc) {
  const bt = brosse(g, F('tete', 'devant'));
  bt.sym(0, 1, acc, 1, 2); bt.sym(1, 0, acc, 1, 1);
  brosse(g, F('tete', 'dessus')).px(0, 0, acc, 8, 2);
  ['devant', 'derriere'].forEach(f => brosse(g, F('torse', f)).px(0, 0, acc, 8, 1));
  ['brasD', 'brasG'].forEach(p => brosse(g, F(p, 'dessus')).plein(acc));
}

function ecailles(g, peau, acc) {
  [['tete', 'devant'], ['tete', 'droite'], ['tete', 'gauche'], ['torse', 'devant'],
   ['torse', 'derriere'], ['brasD', 'devant'], ['brasG', 'devant']].forEach(([p, f], i) => {
    const r = F(p, f), b = brosse(g, r);
    for (let y = 0; y < r.hauteur; y++) for (let x = y % 2; x < r.largeur; x += 2)
      if (bruit(x + i, y, 71) > 0.45) b.px(x, y, acc);
  });
}

function craquelures(g, peau, acc) {
  [['tete', 'devant'], ['tete', 'droite'], ['tete', 'gauche'], ['torse', 'devant'],
   ['brasD', 'devant'], ['brasG', 'devant'], ['jambeD', 'devant'], ['jambeG', 'devant']].forEach(([p, f], i) => {
    const r = F(p, f), b = brosse(g, r);
    for (let y = 0; y < r.hauteur; y++) {
      const n = bruit(y * 3 + i, 0, 83);
      if (n <= 0.6) continue;
      const x = Math.floor(n * r.largeur) % r.largeur;
      b.px(x, y, acc);
      b.px((x + 1) % r.largeur, y, acc);
    }
  });
}

const MOTIFS = {
  rayures: rayures, selle: selle, contreOmbre: contreOmbre, front: front,
  taches: taches, joues: joues, poussiere: poussiere, plisse: plisse, suie: suie,
  runes: runes, lichen: lichen, dorure: dorure, givre: givre, ecailles: ecailles,
  craquelures: craquelures
};

function reflets(g, col) {
  g.save(); g.globalAlpha *= 0.32;
  [['torse', 'devant'], ['torse', 'derriere'], ['brasD', 'devant'], ['brasG', 'devant'], ['tete', 'devant']].forEach(([p, f]) => {
    const r = F(p, f), b = brosse(g, r);
    for (let x = 0; x < r.largeur; x++) b.px(x, 1, col);
    b.px(0, 2, col, r.largeur, 1);
  });
  g.restore();
}

function veines(g, T) {
  const enc = T.craquelee ? '#211B1B' : '#2A2426';
  const dense = T.veines > 1;
  const zones = [['tete', 'devant'], ['torse', 'devant'], ['brasD', 'devant'], ['brasG', 'devant']];
  if (dense) zones.push(['torse', 'derriere'], ['jambeD', 'devant'], ['jambeG', 'devant'], ['tete', 'droite'], ['tete', 'gauche']);
  zones.forEach(([p, f], i) => {
    const r = F(p, f), b = brosse(g, r);
    for (let y = 0; y < r.hauteur; y++) for (let x = 0; x < r.largeur; x++) {
      const n = bruit(x * 5 + i * 3, y * 7, 23);
      if (n > (dense ? 0.9 : 0.95)) b.px(x, y, enc);
    }
  });
}

function visage(g, cfg, T, peau, cheveux, relief) {
  const r = F('tete', 'devant'), b = brosse(g, r);
  const creux = shade(peau, -0.13 * relief), creuxF = shade(peau, -0.24 * relief);

  /* forme de tete : ou tombe l'ombre */
  const formeT = cfg.formeTete;
  if (formeT === 'ovale') {
    b.sym(0, 0, creux); b.sym(0, 7, creux); b.sym(0, 1, creux);
  } else if (formeT === 'anguleuse') {
    b.sym(0, 5, creux); b.sym(0, 6, creuxF); b.sym(1, 7, creux);
  } else {
    b.sym(0, 6, creux, 1, 2);
  }
  if (T.machoire || T.machoireLourde) { b.px(1, 7, creuxF, 6, 1); if (T.machoireLourde) b.px(0, 6, creuxF, 8, 1); }

  /* nez */
  b.px(3, 4, creux, 2, 1);
  b.px(3, 5, creuxF, 2, 1);

  /* sourcils */
  b.sym(1, 2, shade(cheveux, -0.2), 2, 1);

  /* yeux */
  const forme = cfg.yeux;
  const spec = YEUX.filter(o => o.id === forme)[0] || YEUX[0];
  const lumineux = T.regard === 'lumineux', vitreux = T.regard === 'vitreux';
  const iris = lumineux ? (T.lueur || '#F2DDA4') : vitreux ? '#D8D4CE' : cfg.couleurYeux;
  const teintes = {
    b: lumineux || vitreux ? shade(iris, 0.25) : '#F4F1EA',
    i: iris,
    l: shade(peau, -0.3),
    c: shade(cheveux, -0.3)
  };
  const ops = (lumineux || vitreux) ? (YEUX.filter(o => o.id === 'vides')[0]).ops : spec.ops;
  ops.forEach(([x, y, w, h, t]) => b.sym(x, y, teintes[t], w, h));

  /* bouche */
  const lev = mix(peau, '#8A3B33', 0.45);
  const bouche = BOUCHES.filter(o => o.id === cfg.bouche)[0] || BOUCHES[0];
  const tb = { l: lev, f: shade(lev, -0.3), d: '#EDE7D6', p: shade(peau, -0.12) };
  bouche.ops.forEach(([x, y, w, h, t]) => b.px(x, y, tb[t], w, h));

  /* canines */
  if (T.canines) {
    b.sym(2, 7, '#EDE7D6', 1, 1);
    if (T.canines > 1) b.sym(2, 6, '#EDE7D6', 1, 1);
  }

  /* touffes laterales de l'homme-bete */
  if (T.pelage) {
    ['droite', 'gauche'].forEach(f => brosse(g, F('tete', f)).px(3, 1, shade(peau, -0.2), 2, 1));
  }
}

const PIECES_MARQUE = {
  tete: ['tete', 'devant'], teteD: ['tete', 'droite'], teteG: ['tete', 'gauche'], torse: ['torse', 'devant']
};

function marques(g, cfg, peau) {
  const spec = MARQUES.filter(o => o.id === cfg.marques)[0];
  if (!spec || !spec.ops) return;
  const col = spec.teinte === 'peauF' ? shade(peau, -0.38) : spec.teinte;
  spec.ops.forEach(([piece, x, y, w, h]) => {
    if (piece === 'bras') {
      ['brasD', 'brasG'].forEach(p => brosse(g, F(p, 'devant')).px(x, y, col, w, h));
      return;
    }
    const cible = PIECES_MARQUE[piece];
    if (cible) brosse(g, F(cible[0], cible[1])).px(x, y, col, w, h);
  });
}

function tenue(g, cfg, T, peau) {
  const spec = TENUES.filter(t => t.id === cfg.tenue)[0] || TENUES[0];
  const base = cfg.couleurTenue;
  const fer = '#98A1A8', cuir = '#5A3B25', cuirF = '#3E2818', botte = '#3F2A1C';
  const corps = spec.ton === 'sombre' ? shade(base, -0.24)
    : spec.ton === 'clair' ? shade(base, 0.16)
      : spec.ton === 'plaque' ? mix(base, fer, 0.4) : base;
  const trim = shade(corps, 0.16);
  const pantalon = mix(base, '#2A2320', 0.5);
  const teintes = { fer: fer, clair: trim, sombre: shade(corps, -0.26), base: corps, cuir: cuir, peau: peau };

  if (!spec.torseNu) {
    FACES.forEach((f, j) => plaque(g, F('torse', f), corps, 40 + j, 0.07));
    if (spec.col) ['devant', 'derriere', 'droite', 'gauche'].forEach(f => {
      const b = brosse(g, F('torse', f));
      b.px(0, 0, trim, b.r.largeur, 1);
    });
  }
  if (spec.ceinture) {
    ['devant', 'derriere', 'droite', 'gauche'].forEach(f => {
      const b = brosse(g, F('torse', f));
      b.px(0, 8, cuir, b.r.largeur, 2);
      b.px(0, 9, cuirF, b.r.largeur, 1);
    });
    brosse(g, F('torse', 'devant')).px(3, 8, fer, 2, 2);
  }
  if (spec.extras) {
    const b = brosse(g, F('torse', 'devant'));
    spec.extras.forEach(([x, y, w, h, t]) => b.px(x, y, teintes[t] || corps, w, h));
  }

  /* bras : manches puis mains */
  BRAS.forEach((p, i) => {
    if (spec.manche > 0) {
      FACES.forEach((f, j) => {
        if (f === 'dessous') return;
        const r = F(p, f);
        const h = f === 'dessus' ? r.hauteur : Math.min(spec.manche, r.hauteur);
        plaque(g, { x: r.x, y: r.y, largeur: r.largeur, hauteur: h }, corps, 60 + i * 5 + j, 0.07);
      });
    }
    const main = T.mainsMassives ? 3 : 2;
    ['devant', 'derriere', 'droite', 'gauche'].forEach(f => {
      const bb = brosse(g, F(p, f));
      bb.px(0, 12 - main, shade(peau, -0.05), bb.r.largeur, main);
    });
    if (T.griffes) {
      const b = brosse(g, F(p, 'devant'));
      b.px(0, 11, '#EDE7D6', 1, 1); b.px(3, 11, '#EDE7D6', 1, 1);
    }
  });

  /* jambes */
  const robe = spec.jambes === 'robe';
  const fin = robe ? 10 : 8;
  JAMBES.forEach((p, i) => {
    FACES.forEach((f, j) => {
      if (f === 'dessous') return;
      const r = F(p, f);
      const h = f === 'dessus' ? r.hauteur : fin;
      plaque(g, { x: r.x, y: r.y, largeur: r.largeur, hauteur: h }, robe ? corps : pantalon, 80 + i * 5 + j, 0.06);
    });
    ['devant', 'derriere', 'droite', 'gauche', 'dessous'].forEach(f => {
      const r = F(p, f);
      const haut = f === 'dessous' ? 0 : fin;
      const h = f === 'dessous' ? r.hauteur : 12 - fin;
      plaque(g, { x: r.x, y: r.y + haut, largeur: r.largeur, hauteur: h }, botte, 95 + i, 0.05);
    });
    brosse(g, F(p, 'devant')).px(0, fin, shade(botte, 0.18), 4, 1);
  });
}

function couche2(g, cfg, T, cheveux, peau) {
  const co = COIFFURES.filter(c => c.id === cfg.coiffure)[0] || COIFFURES[0];
  const volume = T.chevelureEtheree ? shade(cheveux, 0.2) : cheveux;
  const ombre = shade(volume, -0.12);

  const pose = (b, x, y, col) => {
    const n = bruit(x * 3 + y, y * 5, 61);
    b.px(x, y, n > 0.86 ? shade(col, 0.1) : n < 0.2 ? shade(col, -0.16) : col);
  };
  /* la ligne de cheveux suit un profil par colonne : jamais une coupe droite.
     Les cotes sont decrits d'avant en arriere ; la face droite est inversee. */
  const colonnes = (face, profil, col, inverse, plafond) => {
    if (!profil) return;
    const b = brosse(g, F('tete2', face));
    for (let x = 0; x < 8; x++) {
      const h = Math.min(8, plafond || 8, profil[inverse ? 7 - x : x] || 0);
      for (let y = 0; y < h; y++) pose(b, x, y, col);
    }
  };
  const plafond = co.der ? Math.min.apply(null, co.der) : 8;
  if (co.dev || co.cot || co.der) {
    plaque(g, F('tete2', 'dessus'), volume, 12, 0.08);
    colonnes('devant', co.dev, volume, false);
    colonnes('gauche', co.cot, volume, false, plafond);
    colonnes('droite', co.cot, volume, true, plafond);
    colonnes('derriere', co.der, ombre, false);
  }
  if (co.extras) co.extras.forEach(([f, x, y, w, h]) => brosse(g, F('tete2', f)).px(x, y, volume, w, h));

  /* oreilles pointues */
  if (T.oreilles) {
    const L = T.oreilles;
    ['droite', 'gauche'].forEach(f => {
      const b = brosse(g, F('tete2', f));
      b.px(4, 3, peau, L, 1);
      b.px(4 + L - 1, 2, peau, 1, 1);
      b.px(4, 4, shade(peau, -0.15), Math.max(1, L - 1), 1);
    });
  }

  /* oreilles animales */
  if (T.oreillesAnimales) {
    const forme = T.oreillesAnimales;
    const b = brosse(g, F('tete2', 'dessus'));
    const interieur = mix(peau, '#8A4A4A', 0.4);
    const w = 2;
    b.px(1, 3, peau, w, 2); b.px(8 - 1 - w, 3, peau, w, 2);
    b.px(1, 4, interieur, 1, 1); b.px(8 - 2, 4, interieur, 1, 1);
    const bb = brosse(g, F('tete2', 'devant'));
    bb.px(1, 0, peau, w, forme === 'lupin' ? 2 : 1);
    bb.px(8 - 1 - w, 0, peau, w, forme === 'lupin' ? 2 : 1);
  }

  /* epaulettes des tenues qui en portent */
  const spec = TENUES.filter(t => t.id === cfg.tenue)[0];
  if (spec && spec.epaulettes) {
    const metal = spec.ton === 'plaque' ? '#98A1A8' : '#5A3B25';
    BRAS.forEach((p, i) => {
      const nom = p + '2';
      ['devant', 'derriere', 'droite', 'gauche'].forEach(f => {
        const r = F(nom, f);
        plaque(g, { x: r.x, y: r.y, largeur: r.largeur, hauteur: 3 }, metal, 120 + i, 0.08);
      });
      plaque(g, F(nom, 'dessus'), shade(metal, 0.08), 130 + i, 0.06);
    });
  }

  /* appendices craniens et queue : pieces de couche 2 extrudees par la figure */
  peindreAppendice(g, T.appendice, peau, T.accent || shade(peau, 0.2));
  peindreQueue(g, T.queue, peau, T.accent || shade(peau, 0.2));

  /* halo d'aasimar */
  if (T.halo) {
    const col = T.reflet || '#E8C46A';
    g.save(); g.globalAlpha = 0.55;
    const b = brosse(g, F('torse2', 'derriere'));
    b.px(0, 1, col, 8, 1); b.px(0, 2, shade(col, -0.2), 8, 1);
    g.restore();
  }
}

/* utilitaires exposes a l'ecran */
export function hauteurTexels(cfg) {
  const m = modsDe(cfg);
  return 8 * (m.tete || 1) + 12 + 12 * (m.jambes || 1);
}
/* Encombrement reel : les appendices craniens montent 8 texels au-dessus du
   crane. C'est cette valeur, pas la taille du corps, qui fixe l'echelle
   d'apercu — sinon oreilles et cornes sortent du cadre. */
export function hauteurTotaleTexels(cfg) {
  return hauteurTexels(cfg) + (traitsDe(cfg).appendice ? 8 : 0);
}
export function metresParTexel() { return PATRON.unite_en_blocs; }/* ---------- appendices craniens ----------
   Un seul carre du patron porte les deux cotes : la figure extrude ses pixels
   dans un plan vertical pose sur le sommet du crane. */
const APPENDICES = {
  oreillesFelin: [[], [], [], [[0, 2], [6, 2]], [[0, 2], [6, 2]], [[0, 3], [5, 3]], [[1, 2], [5, 2]], [[1, 2], [5, 2]]],
  oreillesLupin: [[[0, 2], [6, 2]], [[0, 2], [6, 2]], [[0, 2], [6, 2]], [[0, 3], [5, 3]], [[0, 3], [5, 3]], [[1, 3], [4, 3]], [[1, 2], [5, 2]], [[1, 2], [5, 2]]],
  oreillesVulpin: [[[1, 2], [5, 2]], [[0, 3], [5, 3]], [[0, 3], [5, 3]], [[0, 3], [5, 3]], [[0, 3], [5, 3]], [[1, 3], [4, 3]], [[1, 2], [5, 2]], [[2, 2], [4, 2]]],
  cornes: [[], [[0, 2], [6, 2]], [[0, 2], [6, 2]], [[1, 2], [5, 2]], [[1, 2], [5, 2]], [[1, 2], [5, 2]], [[2, 2], [4, 2]], [[2, 2], [4, 2]]],
  cornesCourtes: [[], [], [], [], [[0, 2], [6, 2]], [[1, 2], [5, 2]], [[1, 2], [5, 2]], [[2, 2], [4, 2]]],
  bois: [[[0, 1], [7, 1]], [[0, 1], [2, 1], [5, 1], [7, 1]], [[1, 1], [2, 1], [5, 1], [6, 1]], [[1, 2], [5, 2]], [[2, 1], [5, 1]], [[2, 1], [5, 1]], [[2, 2], [4, 2]], [[3, 2]]],
  couronneOs: [[], [[0, 1], [7, 1]], [[0, 1], [3, 2], [7, 1]], [[0, 1], [3, 2], [7, 1]], [[0, 2], [3, 2], [6, 2]], [[0, 2], [3, 2], [6, 2]], [[0, 8]], [[0, 8]]],
  cristaux: [[[1, 1], [6, 1]], [[1, 2], [5, 2]], [[1, 2], [3, 2], [5, 2]], [[2, 1], [3, 2], [5, 1]], [[2, 2], [4, 2]], [[3, 2]], [[3, 2]], [[2, 4]]],
  couronne: [[], [], [[0, 1], [2, 1], [5, 1], [7, 1]], [[0, 1], [2, 1], [5, 1], [7, 1]], [[0, 8]], [[0, 8]], [], []]
};
/* Matiere de l'appendice : les oreilles prennent la carnation, le reste sa
   propre matiere ; 'accent' = la couleur d'accent de la sous-race. */
const MAT_APPENDICE = {
  cornes: '#D8CAA8', cornesCourtes: '#CBBA96', bois: '#8A6A44',
  couronneOs: '#E8DFC8', cristaux: 'accent', couronne: 'accent'
};

function peindreAppendice(g, kind, peau, acc) {
  const lignes = APPENDICES[kind];
  if (!lignes) return;
  const oreille = !MAT_APPENDICE[kind];
  const base = oreille ? peau : MAT_APPENDICE[kind] === 'accent' ? acc : MAT_APPENDICE[kind];
  const sombre = shade(base, -0.3), clair = shade(base, 0.16);
  const b = brosse(g, F('appendice', 'devant'));
  lignes.forEach((bandes, y) => bandes.forEach(([x, w]) => {
    const droite = x < 4;
    for (let i = 0; i < w; i++) {
      const px = x + i;
      let col;
      if (!oreille) col = y < 2 ? clair : y % 2 ? shade(base, -0.18) : base;
      else {
        const interne = droite ? px === x + w - 1 : px === x;
        col = y === 0 ? sombre : interne && y > 1 ? acc : base;
      }
      b.px(px, y, col);
    }
  }));
}

/* ---------- queue ---------- */
const QUEUES = {
  longue: [[3, 2], [3, 2], [3, 2], [3, 2], [3, 2], [3, 2], [3, 2], [3, 2]],
  touffue: [[3, 2], [3, 3], [2, 4], [2, 4], [2, 4], [2, 4], [3, 3], [3, 2]],
  panache: [[3, 2], [2, 4], [2, 4], [1, 6], [1, 6], [2, 4], [2, 4], [3, 2]]
};

function peindreQueue(g, kind, peau, acc) {
  const lignes = QUEUES[kind];
  if (!lignes) return;
  const sombre = shade(peau, -0.24);
  const b = brosse(g, F('queue', 'devant'));
  lignes.forEach(([x, w], y) => {
    for (let i = 0; i < w; i++) {
      let col = peau;
      if (kind === 'longue' && y % 2) col = sombre;
      if (kind === 'touffue' && bruit(x + i, y, 53) > 0.6) col = sombre;
      if (kind === 'panache' && y > 5) col = acc;
      b.px(x + i, y, col);
    }
  });
}

/* Pilosite : deux pieces peintes chacune dans son carre du patron (8x8).
   La figure extrude leurs pixels ; rien n'est peint sur la peau ni sur le torse. */
const CHUTES = {
  mi: [[1, 6], [1, 6], [1, 6], [2, 4], [], [], [], []],
  longue: [[1, 6], [1, 6], [1, 6], [1, 6], [1, 6], [2, 4], [2, 4], [3, 2]],
  fleuve: [[0, 8], [0, 8], [1, 6], [1, 6], [1, 6], [1, 6], [2, 4], [2, 4]],
  queue: [[3, 2], [3, 2], [3, 2], [3, 2], [3, 2], [3, 2], [3, 2], [3, 2]],
  tresse: [[3, 2], [3, 2], [3, 2], [3, 2], [3, 2], [4, 1], [3, 2], [4, 1]],
  nattes: [[1, 2], [1, 2], [1, 2], [1, 2], [1, 2], [1, 2], [2, 1], [2, 1]],
  couettes: [[0, 2], [0, 2], [0, 2], [1, 1], [], [], [], []],
  mulet: [[], [], [], [1, 6], [1, 6], [2, 4], [2, 4], [3, 2]]
};
const MIROIR_CHUTE = { nattes: 1, couettes: 1 };

function peindreChevelure(g, coiffure, cheveux, T) {
  const co = COIFFURES.filter(c => c.id === coiffure)[0];
  if (!co || !co.chute) return;
  const lignes = CHUTES[co.chute];
  if (!lignes) return;
  const col = T.chevelureEtheree ? shade(cheveux, 0.18) : cheveux;
  const clair = shade(col, 0.1), sombre = shade(col, -0.2);
  const b = brosse(g, F('chevelure', 'devant'));
  const pose = (x, y) => {
    const n = bruit(x, y, 47);
    b.px(x, y, n > 0.84 ? clair : n < 0.22 ? sombre : col);
  };
  lignes.forEach((bande, y) => {
    if (!bande.length) return;
    for (let i = 0; i < bande[1]; i++) {
      pose(bande[0] + i, y);
      if (MIROIR_CHUTE[co.chute]) pose(8 - bande[0] - bande[1] + i, y);
    }
  });
}

function peindreBarbe(g, style, cheveux) {
  const spec = BARBES.filter(o => o.id === style)[0];
  if (!spec || !spec.rangs) return;
  const clair = shade(cheveux, 0.1), sombre = shade(cheveux, -0.22);
  const b = brosse(g, F('barbe', 'devant'));
  spec.rangs.forEach((bandes, y) => bandes.forEach(([x, w]) => {
    for (let i = 0; i < w; i++) {
      const n = bruit(x + i, y, 31);
      b.px(x + i, y, n > 0.84 ? clair : n < 0.2 ? sombre : cheveux);
    }
  }));
  if (style === 'tressee' || style === 'nattee') b.px(3, 5, sombre, 1, 3);
}

