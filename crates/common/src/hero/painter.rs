//! Le peintre : la planche 64 × 64 à deux couches d'un héros, peinte texel par texel depuis son
//! apparence.
//!
//! **Un portage au texel près.** La référence est la maquette (`hero-atlas.js`) et le peintre Java
//! de la branche `develop` (`Peintre.java`) ; le test compare chaque planche à cent planches que la
//! maquette a tirées elle-même. Le calcul garde donc l'écriture de la maquette : des `f64`, le même
//! ordre d'opérations, le sinus de fdlibm (`libm`, celui de V8 et de `StrictMath`) pour le bruit,
//! et chaque couleur arrondie au `#rrggbb` comme le font `shade` et `mix`. Entre deux arrondis, une
//! couleur voyage en `0xRRGGBB`.
//!
//! Ailes, barbe, cheveux, appendices crâniens et queue se peignent dans les cases libres de la
//! même planche.

use super::atlas::{atlas, Rect, Traits};
use super::Appearance;

/// Côté de la planche, en texels
pub const SHEET_SIZE: usize = 64;

const MEMBRES1: [&str; 6] = ["tete", "torse", "brasD", "brasG", "jambeD", "jambeG"];
const BRAS: [&str; 2] = ["brasD", "brasG"];
const JAMBES: [&str; 2] = ["jambeD", "jambeG"];
const COTES: [&str; 4] = ["devant", "derriere", "droite", "gauche"];
const OS: u32 = 0xEDE7D6;

type Bandes = &'static [&'static [[i32; 2]]];

/// Appendices crâniens : par rang de la case 8 × 8, les bandes `[x, largeur]` ; les deux côtés
/// partagent la case
fn appendice(kind: &str) -> Option<(Bandes, Option<&'static str>)> {
    // matière : absente pour une oreille (elle prend la peau), `accent` pour l'accent de la variante
    Some(match kind {
        "oreillesFelin" => (&[&[], &[], &[], &[[0, 2], [6, 2]], &[[0, 2], [6, 2]], &[[0, 3], [5, 3]], &[[1, 2], [5, 2]], &[[1, 2], [5, 2]]], None),
        "oreillesLupin" => (
            &[&[[0, 2], [6, 2]], &[[0, 2], [6, 2]], &[[0, 2], [6, 2]], &[[0, 3], [5, 3]], &[[0, 3], [5, 3]], &[[1, 3], [4, 3]], &[[1, 2], [5, 2]], &[[1, 2], [5, 2]]],
            None,
        ),
        "oreillesVulpin" => (
            &[&[[1, 2], [5, 2]], &[[0, 3], [5, 3]], &[[0, 3], [5, 3]], &[[0, 3], [5, 3]], &[[0, 3], [5, 3]], &[[1, 3], [4, 3]], &[[1, 2], [5, 2]], &[[2, 2], [4, 2]]],
            None,
        ),
        "cornes" => (
            &[&[], &[[0, 2], [6, 2]], &[[0, 2], [6, 2]], &[[1, 2], [5, 2]], &[[1, 2], [5, 2]], &[[1, 2], [5, 2]], &[[2, 2], [4, 2]], &[[2, 2], [4, 2]]],
            Some("#D8CAA8"),
        ),
        "cornesCourtes" => (&[&[], &[], &[], &[], &[[0, 2], [6, 2]], &[[1, 2], [5, 2]], &[[1, 2], [5, 2]], &[[2, 2], [4, 2]]], Some("#CBBA96")),
        "bois" => (
            &[&[[0, 1], [7, 1]], &[[0, 1], [2, 1], [5, 1], [7, 1]], &[[1, 1], [2, 1], [5, 1], [6, 1]], &[[1, 2], [5, 2]], &[[2, 1], [5, 1]], &[[2, 1], [5, 1]], &[[2, 2], [4, 2]], &[[3, 2]]],
            Some("#8A6A44"),
        ),
        "couronneOs" => (
            &[&[], &[[0, 1], [7, 1]], &[[0, 1], [3, 2], [7, 1]], &[[0, 1], [3, 2], [7, 1]], &[[0, 2], [3, 2], [6, 2]], &[[0, 2], [3, 2], [6, 2]], &[[0, 8]], &[[0, 8]]],
            Some("#E8DFC8"),
        ),
        "cristaux" => (
            &[&[[1, 1], [6, 1]], &[[1, 2], [5, 2]], &[[1, 2], [3, 2], [5, 2]], &[[2, 1], [3, 2], [5, 1]], &[[2, 2], [4, 2]], &[[3, 2]], &[[3, 2]], &[[2, 4]]],
            Some("accent"),
        ),
        "couronne" => (&[&[], &[], &[[0, 1], [2, 1], [5, 1], [7, 1]], &[[0, 1], [2, 1], [5, 1], [7, 1]], &[[0, 8]], &[[0, 8]], &[], &[]], Some("accent")),
        _ => return None,
    })
}

/// Queues : par rang, la bande `[x, largeur]`
fn queue(kind: &str) -> Option<[[i32; 2]; 8]> {
    Some(match kind {
        "longue" => [[3, 2]; 8],
        "touffue" => [[3, 2], [3, 3], [2, 4], [2, 4], [2, 4], [2, 4], [3, 3], [3, 2]],
        "panache" => [[3, 2], [2, 4], [2, 4], [1, 6], [1, 6], [2, 4], [2, 4], [3, 2]],
        _ => return None,
    })
}

/// Mèches pendantes : par rang, la bande `[x, largeur]`, vide pour aucune
fn chute(kind: &str) -> Option<&'static [&'static [i32]]> {
    Some(match kind {
        "mi" => &[&[1, 6], &[1, 6], &[1, 6], &[2, 4], &[], &[], &[], &[]],
        "longue" => &[&[1, 6], &[1, 6], &[1, 6], &[1, 6], &[1, 6], &[2, 4], &[2, 4], &[3, 2]],
        "fleuve" => &[&[0, 8], &[0, 8], &[1, 6], &[1, 6], &[1, 6], &[1, 6], &[2, 4], &[2, 4]],
        "queue" => &[&[3, 2], &[3, 2], &[3, 2], &[3, 2], &[3, 2], &[3, 2], &[3, 2], &[3, 2]],
        "tresse" => &[&[3, 2], &[3, 2], &[3, 2], &[3, 2], &[3, 2], &[4, 1], &[3, 2], &[4, 1]],
        "nattes" => &[&[1, 2], &[1, 2], &[1, 2], &[1, 2], &[1, 2], &[1, 2], &[2, 1], &[2, 1]],
        "couettes" => &[&[0, 2], &[0, 2], &[0, 2], &[1, 1], &[], &[], &[], &[]],
        "mulet" => &[&[], &[], &[], &[1, 6], &[1, 6], &[2, 4], &[2, 4], &[3, 2]],
        _ => return None,
    })
}

/// Mèches peintes deux fois, en miroir
const MIROIR_CHUTE: [&str; 2] = ["nattes", "couettes"];

/* ---------- couleurs ---------- */

/// `#rrggbb` en `0xRRGGBB`
pub fn rgb(hex: &str) -> u32 {
    u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(0) & 0xFF_FFFF
}

/// `Math.round` de Java : l'entier le plus proche, le plus grand à égalité
fn round(v: f64) -> i64 {
    (v + 0.5).floor() as i64
}

fn canal(v: f64) -> u32 {
    round(v).clamp(0, 255) as u32
}

fn rouge(c: u32) -> f64 {
    (c >> 16 & 255) as f64
}

fn vert(c: u32) -> f64 {
    (c >> 8 & 255) as f64
}

fn bleu(c: u32) -> f64 {
    (c & 255) as f64
}

/// Éclaircit vers le blanc (`amt > 0`) ou assombrit vers le noir (`amt < 0`), puis arrondit
pub fn shade(c: u32, amt: f64) -> u32 {
    let f = |v: f64| if amt >= 0.0 { v + (255.0 - v) * amt } else { v * (1.0 + amt) };
    canal(f(rouge(c))) << 16 | canal(f(vert(c))) << 8 | canal(f(bleu(c)))
}

/// Mélange de `a` vers `b` de `t`, puis arrondit
pub fn mix(a: u32, b: u32, t: f64) -> u32 {
    let f = |x: f64, y: f64| canal(x + (y - x) * t);
    f(rouge(a), rouge(b)) << 16 | f(vert(a), vert(b)) << 8 | f(bleu(a), bleu(b))
}

/// Bruit de hachage dans `[0, 1)`, celui de la maquette au bit près
fn bruit(x: i32, y: i32, s: i32) -> f64 {
    let v = libm::sin((x + 1) as f64 * 12.9898 + (y + 1) as f64 * 78.233 + s as f64 * 3.77) * 43758.5453;
    v - v.floor()
}

/// `Number(a.toFixed(2))` : le multiple de 0,01 le plus proche de la valeur exacte de `a`, le plus
/// grand à égalité, qui arrive : 0,92 - 0,045 vaut exactement 0,875. Le produit est séparé par une
/// multiplication-addition fusionnée pour que la comparaison au milieu soit exacte.
fn au_centieme(a: f64) -> f64 {
    if a < 0.0 {
        return -au_centieme(-a);
    }
    let p = a * 100.0;
    let reste = a.mul_add(100.0, -p);
    let mut n = p.floor();
    if p == n && reste < 0.0 {
        n -= 1.0;
    }
    let au_dessus = p - (n + 0.5) >= -reste;
    (n + if au_dessus { 1.0 } else { 0.0 }) / 100.0
}

/* ---------- vérité à la JavaScript ---------- */

fn vrai_f(v: Option<f64>) -> bool {
    v.is_some_and(|v| v != 0.0 && !v.is_nan())
}

fn vrai_i(v: Option<i32>) -> bool {
    v.is_some_and(|v| v != 0)
}

fn vrai_b(v: Option<bool>) -> bool {
    v == Some(true)
}

fn vrai_s(v: &Option<String>) -> Option<&str> {
    v.as_deref().filter(|s| !s.is_empty())
}

/* ---------- toile ---------- */

/// Les quelques appels de toile du peintre, avec la sémantique exacte de la maquette :
/// composition « source-over » en alpha droit, en `f64`, chaque canal arrondi à l'écriture. Part
/// entièrement transparente.
struct Toile {
    px: Vec<u8>,
    global_alpha: f64,
    pile: Vec<f64>,
}

impl Toile {
    fn new() -> Self {
        Self { px: vec![0; SHEET_SIZE * SHEET_SIZE * 4], global_alpha: 1.0, pile: Vec::new() }
    }

    fn save(&mut self) {
        self.pile.push(self.global_alpha);
    }

    fn restore(&mut self) {
        self.global_alpha = self.pile.pop().unwrap_or(1.0);
    }

    /// Pose une couleur d'opacité `a0` (fois l'alpha global) sur un rectangle, rogné à la planche
    fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, rgb: u32, a0: f64) {
        let (r, g, b) = (rouge(rgb), vert(rgb), bleu(rgb));
        let a = a0 * self.global_alpha;
        let n = SHEET_SIZE as i32;
        for j in y.max(0)..(y + h).min(n) {
            for i in x.max(0)..(x + w).min(n) {
                let k = ((j * n + i) * 4) as usize;
                let da = self.px[k + 3] as f64 / 255.0;
                let oa = a + da * (1.0 - a);
                if oa <= 0.0 {
                    self.px[k..k + 4].fill(0);
                    continue;
                }
                let canal = |src: f64, dst: u8| round((src * a + dst as f64 * da * (1.0 - a)) / oa) as u8;
                self.px[k] = canal(r, self.px[k]);
                self.px[k + 1] = canal(g, self.px[k + 1]);
                self.px[k + 2] = canal(b, self.px[k + 2]);
                self.px[k + 3] = round(oa * 255.0) as u8;
            }
        }
    }

    fn rect(&mut self, r: Rect, col: u32) {
        self.fill_rect(r.x, r.y, r.largeur, r.hauteur, col, 1.0);
    }
}

/// Une planche peinte : RGBA droit (non prémultiplié), rang par rang depuis le coin haut gauche
#[derive(Clone, PartialEq, Eq)]
pub struct Sheet {
    pub rgba: Vec<u8>,
}

impl Sheet {
    /// Le texel `(x, y)` en `[r, g, b, a]`
    pub fn texel(&self, x: usize, y: usize) -> [u8; 4] {
        let k = (y * SHEET_SIZE + x) * 4;
        [self.rgba[k], self.rgba[k + 1], self.rgba[k + 2], self.rgba[k + 3]]
    }
}

impl std::fmt::Debug for Sheet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Sheet({}x{})", SHEET_SIZE, SHEET_SIZE)
    }
}

/* ---------- pinceaux ---------- */

fn face(part: &str, face: &str) -> Rect {
    atlas().partie(part).face(face)
}

/// Un pinceau lié à une face : coordonnées locales, et un rectangle qui déborde de la face est
/// abandonné
struct Brosse {
    r: Rect,
}

impl Brosse {
    fn sur(part: &str, f: &str) -> Self {
        Self { r: face(part, f) }
    }

    fn plein(&self, g: &mut Toile, col: u32) {
        g.rect(self.r, col);
    }

    fn px(&self, g: &mut Toile, x: i32, y: i32, col: u32) {
        self.pxa(g, x, y, col, 1.0, 1, 1);
    }

    fn pxw(&self, g: &mut Toile, x: i32, y: i32, col: u32, w: i32, h: i32) {
        self.pxa(g, x, y, col, 1.0, w, h);
    }

    #[allow(clippy::too_many_arguments)]
    fn pxa(&self, g: &mut Toile, x: i32, y: i32, col: u32, alpha: f64, largeur: i32, hauteur: i32) {
        let w = if largeur != 0 { largeur } else { 1 };
        let h = if hauteur != 0 { hauteur } else { 1 };
        if x < 0 || y < 0 || x + w > self.r.largeur || y + h > self.r.hauteur {
            return;
        }
        g.fill_rect(self.r.x + x, self.r.y + y, w, h, col, alpha);
    }

    /// Le même bloc et son miroir horizontal sur la face
    fn sym(&self, g: &mut Toile, x: i32, y: i32, col: u32, largeur: i32, hauteur: i32) {
        let w = if largeur != 0 { largeur } else { 1 };
        let h = if hauteur != 0 { hauteur } else { 1 };
        self.pxw(g, x, y, col, w, h);
        self.pxw(g, self.r.largeur - x - w, y, col, w, h);
    }
}

/// Une face remplie de grain sous un éclairage fixe : bord haut éclairé, bord bas ombré
fn plaque(g: &mut Toile, r: Rect, col: u32, seed: i32, amp: f64) {
    g.rect(r, col);
    let sombre = shade(col, -amp);
    let clair = shade(col, amp * 0.9);
    for y in 0..r.hauteur {
        for x in 0..r.largeur {
            let n = bruit(x + r.x, y + r.y, seed);
            if n < 0.2 {
                g.fill_rect(r.x + x, r.y + y, 1, 1, sombre, 1.0);
            } else if n > 0.88 {
                g.fill_rect(r.x + x, r.y + y, 1, 1, clair, 1.0);
            }
        }
    }
    g.fill_rect(r.x, r.y, r.largeur, 1, shade(col, amp * 1.4), 1.0);
    g.fill_rect(r.x, r.y + r.hauteur - 1, r.largeur, 1, shade(col, -amp * 1.6), 1.0);
}

/* ---------- peinture ---------- */

/// La planche de cette apparence
pub fn paint(cfg: &Appearance) -> Sheet {
    let a = atlas();
    let sp = a.espece(&cfg.espece);
    let t = a.traits_de(cfg);
    let mut g = Toile::new();
    let g = &mut g;

    let peau = rgb(&cfg.peau);
    let peau_f = shade(peau, -0.1);
    let cheveux = rgb(&cfg.couleur_cheveux);
    let relief = if vrai_f(t.relief) { t.relief.unwrap() } else { 1.0 };

    g.global_alpha = if vrai_f(t.alpha) { t.alpha.unwrap() } else { 1.0 };

    /* 1 -- peau */
    for (i, membre) in MEMBRES1.iter().enumerate() {
        for (j, f) in a.FACES.iter().enumerate() {
            let base = match f.as_str() {
                "droite" | "gauche" => peau_f,
                "dessous" => shade(peau, -0.18),
                _ => peau,
            };
            let amp = if vrai_f(t.pelage) { 0.13 * t.pelage.unwrap() } else { 0.06 };
            plaque(g, face(membre, f), base, (i * 7 + j) as i32, amp);
        }
    }
    if vrai_f(t.pelage) {
        pelage(g, peau, &t);
    }
    if let Some(m) = vrai_s(&t.motif) {
        let acc = vrai_s(&t.accent).map(rgb).unwrap_or_else(|| shade(peau, 0.18));
        motif(g, m, peau, acc);
    }
    if let Some(r) = vrai_s(&t.reflet) {
        reflets(g, rgb(r));
    }
    if vrai_i(t.veines) {
        veines(g, &t);
    }

    /* 2 -- visage */
    visage(g, cfg, &t, peau, cheveux, relief);

    /* 3 -- tenue, puis les marques qui se posent par-dessus */
    tenue(g, cfg, &t, peau);
    marques(g, cfg, peau);

    /* 4 -- pilosité : uniquement des pièces de couche 2, jamais sur la peau */
    peindre_barbe(g, vrai_s(&t.barbe_forcee).unwrap_or(&cfg.barbe), cheveux);
    peindre_chevelure(g, &cfg.coiffure, cheveux, &t);

    /* 5 -- couche 2 */
    g.global_alpha = 1.0;
    couche2(g, cfg, &t, cheveux, peau);
    if vrai_b(sp.mods.ailes) {
        peindre_ailes(g, rgb(vrai_s(&t.reflet).unwrap_or("#E8C46A")));
    }
    Sheet { rgba: std::mem::take(&mut g.px) }
}

/// Aile spectrale : les plumes s'effacent vers la pointe ; le bord intérieur (x = 0) touche le dos
fn peindre_ailes(g: &mut Toile, teinte: u32) {
    let larg = [7, 7, 7, 6, 6, 5, 5, 4, 4, 3, 3, 2, 2, 1];
    for (k, f) in ["devant", "derriere"].iter().enumerate() {
        let b = Brosse::sur("aile", f);
        for (y, &w) in larg.iter().enumerate() {
            let y = y as i32;
            for x in 0..w {
                let bord = x == w - 1;
                let plume = (x + y) % 3 == 0;
                let alpha = au_centieme(0.92 - y as f64 * 0.045 - x as f64 * 0.03);
                // la face arrière reflète l'avant : c'est elle qu'on voit sur l'aile droite, tournée d'un demi-tour
                let l = if bord { 0.42 } else if plume { 0.2 } else if k != 0 { 0.0 } else { 0.08 };
                b.pxa(g, if k != 0 { 6 - x } else { x }, y, eclaircir(teinte, l), alpha, 1, 1);
            }
        }
        b.pxa(g, if k != 0 { 5 } else { 0 }, 0, eclaircir(teinte, 0.5), 0.95, 2, 1);
        b.pxa(g, if k != 0 { 6 } else { 0 }, 1, eclaircir(teinte, 0.34), 0.9, 1, 3);
    }
}

/// Canaux `rgba()` de l'aile : vers le blanc de `l`, arrondis sans écrêtage
fn eclaircir(c: u32, l: f64) -> u32 {
    let f = |v: f64| round(v + (255.0 - v) * l) as u32;
    f(rouge(c)) << 16 | f(vert(c)) << 8 | f(bleu(c))
}

fn pelage(g: &mut Toile, peau: u32, t: &Traits) {
    let clair = shade(peau, 0.18);
    let sombre = shade(peau, -0.22);
    let force = if vrai_f(t.pelage) { t.pelage.unwrap() } else { 1.0 };
    let seuil = 0.86 - 0.08 * (force - 1.0);
    let zones = [("torse", "devant"), ("torse", "derriere"), ("brasD", "devant"), ("brasG", "devant"), ("jambeD", "devant"), ("jambeG", "devant")];
    for (i, (p, f)) in zones.iter().enumerate() {
        let b = Brosse::sur(p, f);
        for y in 0..b.r.hauteur {
            for x in 0..b.r.largeur {
                let n = bruit(x * 3 + i as i32, y * 2, 11);
                if n > seuil {
                    b.px(g, x, y, sombre);
                } else if n < 0.14 {
                    b.px(g, x, y, clair);
                }
            }
        }
    }
    let b = Brosse::sur("torse", "devant");
    for y in 5..12 {
        b.pxw(g, 3, y, clair, 2, 1);
    }
}

/* ---------- motifs corporels de sous-race ----------
   Chaque motif couvre le crâne et les mains, qu'aucune tenue ne cache : la sous-race reste
   lisible même en armure de plates. */

fn motif(g: &mut Toile, motif: &str, peau: u32, acc: u32) {
    match motif {
        "rayures" => rayures(g, acc),
        "selle" => selle(g, peau, acc),
        "contreOmbre" => contre_ombre(g, peau, acc),
        "front" => front(g, peau, acc),
        "taches" => taches(g, acc),
        "joues" => joues(g, acc),
        "poussiere" => poussiere(g, acc),
        "plisse" => plisse(g, acc),
        "suie" => suie(g, acc),
        "runes" => runes(g, acc),
        "lichen" => bruite(g, acc, &LICHEN, |x, y, i| bruit(x * 4 + i * 2, y * 3, 67) > 0.88),
        "dorure" => dorure(g, acc),
        "givre" => givre(g, acc),
        "ecailles" => ecailles(g, acc),
        "craquelures" => craquelures(g, acc),
        _ => {}
    }
}

fn rayures(g: &mut Toile, c: u32) {
    let torse = [[1, 0, 1, 5], [4, 0, 1, 4], [6, 1, 1, 5], [2, 6, 1, 4], [5, 7, 1, 4]];
    for f in ["devant", "derriere"] {
        let b = Brosse::sur("torse", f);
        for o in torse {
            b.pxw(g, o[0], o[1], c, o[2], o[3]);
        }
    }
    for f in ["droite", "gauche"] {
        let b = Brosse::sur("torse", f);
        b.pxw(g, 1, 1, c, 1, 4);
        b.pxw(g, 2, 7, c, 1, 3);
    }
    for p in ["brasD", "brasG", "jambeD", "jambeG"] {
        let rangs = if p.starts_with("bras") { [1, 4, 7] } else { [0, 3, 6] };
        for f in COTES {
            let b = Brosse::sur(p, f);
            for y in rangs {
                b.pxw(g, 0, y, c, 4, 1);
            }
        }
    }
    let bt = Brosse::sur("tete", "devant");
    bt.sym(g, 1, 0, c, 1, 2);
    bt.sym(g, 0, 3, c, 1, 1);
    for f in ["droite", "gauche"] {
        let b = Brosse::sur("tete", f);
        b.pxw(g, 2, 1, c, 1, 3);
        b.pxw(g, 5, 2, c, 1, 3);
    }
    Brosse::sur("tete", "dessus").pxw(g, 2, 0, c, 1, 8);
    Brosse::sur("tete", "derriere").pxw(g, 3, 0, c, 2, 5);
}

fn selle(g: &mut Toile, peau: u32, acc: u32) {
    let s = shade(peau, -0.32);
    Brosse::sur("torse", "derriere").pxw(g, 0, 0, s, 8, 6);
    for f in ["droite", "gauche"] {
        Brosse::sur("torse", f).pxw(g, 0, 0, s, 4, 5);
    }
    Brosse::sur("torse", "devant").pxw(g, 2, 0, acc, 4, 4);
    for p in BRAS {
        for f in COTES {
            Brosse::sur(p, f).pxw(g, 0, 0, s, 4, 4);
        }
    }
    let bt = Brosse::sur("tete", "devant");
    bt.sym(g, 0, 0, s, 3, 2);
    bt.pxw(g, 2, 5, shade(peau, -0.18), 4, 3);
    Brosse::sur("tete", "dessus").pxw(g, 0, 0, s, 8, 5);
    Brosse::sur("tete", "derriere").pxw(g, 0, 0, s, 8, 5);
}

fn contre_ombre(g: &mut Toile, peau: u32, acc: u32) {
    let noir = shade(peau, -0.62);
    let bt = Brosse::sur("tete", "devant");
    bt.pxw(g, 2, 5, acc, 4, 3);
    bt.sym(g, 0, 4, acc, 1, 2);
    for f in ["devant", "derriere"] {
        Brosse::sur("torse", f).pxw(g, 2, 4, acc, 4, 8);
    }
    for p in ["brasD", "brasG", "jambeD", "jambeG"] {
        for f in COTES {
            Brosse::sur(p, f).pxw(g, 0, 8, noir, 4, 4);
        }
        Brosse::sur(p, "dessous").plein(g, noir);
    }
    Brosse::sur("tete", "dessus").pxw(g, 0, 0, shade(peau, -0.2), 8, 3);
}

fn front(g: &mut Toile, peau: u32, acc: u32) {
    let bt = Brosse::sur("tete", "devant");
    bt.pxw(g, 1, 0, acc, 6, 2);
    bt.pxw(g, 2, 5, shade(peau, 0.14), 4, 1);
    Brosse::sur("tete", "dessus").pxw(g, 1, 3, acc, 6, 5);
    for f in ["devant", "derriere"] {
        Brosse::sur("torse", f).pxw(g, 1, 0, shade(peau, 0.1), 6, 2);
    }
    for p in ["brasD", "brasG", "jambeD", "jambeG"] {
        for f in COTES {
            Brosse::sur(p, f).pxw(g, 0, 10, shade(peau, -0.5), 4, 2);
        }
    }
}

/* Pigment, usure, gravure : des motifs qui tiennent tous sur le crâne et les mains, la seule peau
   qu'aucune tenue ne cache. */

fn taches(g: &mut Toile, acc: u32) {
    let bt = Brosse::sur("tete", "devant");
    for [x, y] in [[1, 4], [2, 5], [5, 4], [6, 5], [2, 3], [5, 3], [1, 6], [6, 6]] {
        bt.px(g, x, y, acc);
    }
    for p in BRAS {
        let b = Brosse::sur(p, "devant");
        for [x, y] in [[1, 3], [2, 5], [0, 6], [3, 8]] {
            b.px(g, x, y, acc);
        }
    }
}

fn joues(g: &mut Toile, acc: u32) {
    let bt = Brosse::sur("tete", "devant");
    bt.sym(g, 1, 5, acc, 2, 1);
    bt.pxw(g, 3, 4, acc, 2, 1);
    for f in ["droite", "gauche"] {
        Brosse::sur("tete", f).pxw(g, 3, 5, acc, 2, 1);
    }
}

fn poussiere(g: &mut Toile, acc: u32) {
    let bt = Brosse::sur("tete", "devant");
    bt.pxw(g, 1, 0, acc, 6, 1);
    bt.sym(g, 0, 2, acc, 1, 1);
    Brosse::sur("tete", "dessus").pxw(g, 1, 1, acc, 6, 3);
    for f in ["devant", "derriere"] {
        Brosse::sur("torse", f).pxw(g, 0, 0, acc, 8, 1);
    }
}

fn plisse(g: &mut Toile, acc: u32) {
    let bt = Brosse::sur("tete", "devant");
    bt.pxw(g, 1, 1, acc, 6, 1);
    bt.sym(g, 1, 2, acc, 2, 1);
    bt.sym(g, 0, 5, acc, 2, 1);
    bt.sym(g, 1, 6, acc, 1, 1);
}

fn suie(g: &mut Toile, acc: u32) {
    let bt = Brosse::sur("tete", "devant");
    bt.sym(g, 0, 5, acc, 2, 1);
    bt.px(g, 2, 2, acc);
    bt.px(g, 5, 3, acc);
    for p in BRAS {
        for f in COTES {
            Brosse::sur(p, f).pxw(g, 0, 9, acc, 4, 3);
        }
    }
}

fn runes(g: &mut Toile, acc: u32) {
    let bt = Brosse::sur("tete", "devant");
    bt.px(g, 3, 1, acc);
    bt.px(g, 4, 2, acc);
    bt.sym(g, 1, 1, acc, 1, 1);
    for p in BRAS {
        let b = Brosse::sur(p, "devant");
        for o in [[1, 4, 2, 1], [2, 5, 1, 1], [1, 6, 2, 1], [1, 7, 1, 1]] {
            b.pxw(g, o[0], o[1], acc, o[2], o[3]);
        }
    }
    Brosse::sur("torse", "devant").pxw(g, 3, 2, acc, 2, 1);
}

const LICHEN: [(&str, &str); 8] = [
    ("tete", "devant"),
    ("tete", "droite"),
    ("tete", "gauche"),
    ("torse", "devant"),
    ("brasD", "devant"),
    ("brasG", "devant"),
    ("jambeD", "devant"),
    ("jambeG", "devant"),
];

/// Texels de `acc` là où le bruit de la zone `i` le veut
fn bruite(g: &mut Toile, acc: u32, zones: &[(&str, &str)], garde: impl Fn(i32, i32, i32) -> bool) {
    for (i, (p, f)) in zones.iter().enumerate() {
        let b = Brosse::sur(p, f);
        for y in 0..b.r.hauteur {
            for x in 0..b.r.largeur {
                if garde(x, y, i as i32) {
                    b.px(g, x, y, acc);
                }
            }
        }
    }
}

fn dorure(g: &mut Toile, acc: u32) {
    let bt = Brosse::sur("tete", "devant");
    bt.pxw(g, 1, 0, acc, 6, 1);
    bt.sym(g, 0, 4, acc, 1, 1);
    bt.sym(g, 1, 2, acc, 1, 1);
    for f in ["devant", "derriere"] {
        let b = Brosse::sur("torse", f);
        b.pxw(g, 0, 0, acc, 8, 1);
        b.pxw(g, 3, 1, acc, 2, 1);
    }
    for p in BRAS {
        Brosse::sur(p, "devant").pxw(g, 0, 0, acc, 4, 1);
    }
}

fn givre(g: &mut Toile, acc: u32) {
    let bt = Brosse::sur("tete", "devant");
    bt.sym(g, 0, 1, acc, 1, 2);
    bt.sym(g, 1, 0, acc, 1, 1);
    Brosse::sur("tete", "dessus").pxw(g, 0, 0, acc, 8, 2);
    for f in ["devant", "derriere"] {
        Brosse::sur("torse", f).pxw(g, 0, 0, acc, 8, 1);
    }
    for p in BRAS {
        Brosse::sur(p, "dessus").plein(g, acc);
    }
}

fn ecailles(g: &mut Toile, acc: u32) {
    let zones = [("tete", "devant"), ("tete", "droite"), ("tete", "gauche"), ("torse", "devant"), ("torse", "derriere"), ("brasD", "devant"), ("brasG", "devant")];
    for (i, (p, f)) in zones.iter().enumerate() {
        let b = Brosse::sur(p, f);
        for y in 0..b.r.hauteur {
            let mut x = y % 2;
            while x < b.r.largeur {
                if bruit(x + i as i32, y, 71) > 0.45 {
                    b.px(g, x, y, acc);
                }
                x += 2;
            }
        }
    }
}

fn craquelures(g: &mut Toile, acc: u32) {
    for (i, (p, f)) in LICHEN.iter().enumerate() {
        let b = Brosse::sur(p, f);
        let largeur = b.r.largeur;
        for y in 0..b.r.hauteur {
            let n = bruit(y * 3 + i as i32, 0, 83);
            if n <= 0.6 {
                continue;
            }
            let x = (n * largeur as f64).floor() as i32 % largeur;
            b.px(g, x, y, acc);
            b.px(g, (x + 1) % largeur, y, acc);
        }
    }
}

fn reflets(g: &mut Toile, col: u32) {
    g.save();
    g.global_alpha *= 0.32;
    for (p, f) in [("torse", "devant"), ("torse", "derriere"), ("brasD", "devant"), ("brasG", "devant"), ("tete", "devant")] {
        let b = Brosse::sur(p, f);
        for x in 0..b.r.largeur {
            b.px(g, x, 1, col);
        }
        b.pxw(g, 0, 2, col, b.r.largeur, 1);
    }
    g.restore();
}

fn veines(g: &mut Toile, t: &Traits) {
    let enc = if vrai_b(t.craquelee) { 0x211B1B } else { 0x2A2426 };
    let dense = t.veines.unwrap_or(0) > 1;
    let zones: &[(&str, &str)] = if dense {
        &[("tete", "devant"), ("torse", "devant"), ("brasD", "devant"), ("brasG", "devant"), ("torse", "derriere"), ("jambeD", "devant"), ("jambeG", "devant"), ("tete", "droite"), ("tete", "gauche")]
    } else {
        &[("tete", "devant"), ("torse", "devant"), ("brasD", "devant"), ("brasG", "devant")]
    };
    let seuil = if dense { 0.9 } else { 0.95 };
    bruite(g, enc, zones, |x, y, i| bruit(x * 5 + i * 3, y * 7, 23) > seuil);
}

fn visage(g: &mut Toile, cfg: &Appearance, t: &Traits, peau: u32, cheveux: u32, relief: f64) {
    let a = atlas();
    let b = Brosse::sur("tete", "devant");
    let creux = shade(peau, -0.13 * relief);
    let creux_f = shade(peau, -0.24 * relief);

    /* forme de tête : où tombe l'ombre */
    match cfg.forme_tete.as_str() {
        "ovale" => {
            b.sym(g, 0, 0, creux, 1, 1);
            b.sym(g, 0, 7, creux, 1, 1);
            b.sym(g, 0, 1, creux, 1, 1);
        }
        "anguleuse" => {
            b.sym(g, 0, 5, creux, 1, 1);
            b.sym(g, 0, 6, creux_f, 1, 1);
            b.sym(g, 1, 7, creux, 1, 1);
        }
        _ => b.sym(g, 0, 6, creux, 1, 2),
    }
    if vrai_b(t.machoire) || vrai_b(t.machoire_lourde) {
        b.pxw(g, 1, 7, creux_f, 6, 1);
        if vrai_b(t.machoire_lourde) {
            b.pxw(g, 0, 6, creux_f, 8, 1);
        }
    }

    /* nez */
    b.pxw(g, 3, 4, creux, 2, 1);
    b.pxw(g, 3, 5, creux_f, 2, 1);

    /* sourcils */
    b.sym(g, 1, 2, shade(cheveux, -0.2), 2, 1);

    /* yeux */
    let regard = t.regard.as_deref();
    let lumineux = regard == Some("lumineux");
    let vitreux = regard == Some("vitreux");
    let iris = if lumineux {
        rgb(vrai_s(&t.lueur).unwrap_or("#F2DDA4"))
    } else if vitreux {
        0xD8D4CE
    } else {
        rgb(&cfg.couleur_yeux)
    };
    let blanc = if lumineux || vitreux { shade(iris, 0.25) } else { 0xF4F1EA };
    let paupiere = shade(peau, -0.3);
    let cil = shade(cheveux, -0.3);
    let spec = if lumineux || vitreux { a.yeux("vides") } else { a.yeux(&cfg.yeux) }.unwrap_or(&a.YEUX[0]);
    for o in &spec.ops {
        let col = match o.4.as_str() {
            "b" => blanc,
            "i" => iris,
            "l" => paupiere,
            _ => cil,
        };
        b.sym(g, o.0, o.1, col, o.2, o.3);
    }

    /* bouche */
    let lev = mix(peau, 0x8A3B33, 0.45);
    let bouche = a.bouche(&cfg.bouche).unwrap_or(&a.BOUCHES[0]);
    for o in &bouche.ops {
        let col = match o.4.as_str() {
            "l" => lev,
            "f" => shade(lev, -0.3),
            "d" => OS,
            _ => shade(peau, -0.12),
        };
        b.pxw(g, o.0, o.1, col, o.2, o.3);
    }

    /* canines */
    if vrai_i(t.canines) {
        b.sym(g, 2, 7, OS, 1, 1);
        if t.canines.unwrap_or(0) > 1 {
            b.sym(g, 2, 6, OS, 1, 1);
        }
    }

    /* touffes latérales de l'homme-bête */
    if vrai_f(t.pelage) {
        for f in ["droite", "gauche"] {
            Brosse::sur("tete", f).pxw(g, 3, 1, shade(peau, -0.2), 2, 1);
        }
    }
}

fn marques(g: &mut Toile, cfg: &Appearance, peau: u32) {
    let Some(spec) = atlas().marque(&cfg.marques) else { return };
    if spec.ops.is_empty() {
        return;
    }
    let teinte = spec.teinte.as_deref().unwrap_or("#000000");
    let col = if teinte == "peauF" { shade(peau, -0.38) } else { rgb(teinte) };
    for o in &spec.ops {
        let cible = match o.0.as_str() {
            "bras" => {
                for p in BRAS {
                    Brosse::sur(p, "devant").pxw(g, o.1, o.2, col, o.3, o.4);
                }
                continue;
            }
            "tete" => face("tete", "devant"),
            "teteD" => face("tete", "droite"),
            "teteG" => face("tete", "gauche"),
            "torse" => face("torse", "devant"),
            _ => continue,
        };
        Brosse { r: cible }.pxw(g, o.1, o.2, col, o.3, o.4);
    }
}

fn tenue(g: &mut Toile, cfg: &Appearance, t: &Traits, peau: u32) {
    let a = atlas();
    let spec = a.tenue(&cfg.tenue).unwrap_or(&a.TENUES[0]);
    let base = rgb(&cfg.couleur_tenue);
    let fer = 0x98A1A8;
    let cuir = 0x5A3B25;
    let cuir_f = 0x3E2818;
    let botte = 0x3F2A1C;
    let corps = match spec.ton.as_str() {
        "sombre" => shade(base, -0.24),
        "clair" => shade(base, 0.16),
        "plaque" => mix(base, fer, 0.4),
        _ => base,
    };
    let trim = shade(corps, 0.16);
    let pantalon = mix(base, 0x2A2320, 0.5);

    if !spec.torse_nu {
        for (j, f) in a.FACES.iter().enumerate() {
            plaque(g, face("torse", f), corps, 40 + j as i32, 0.07);
        }
        if spec.col {
            for f in COTES {
                let b = Brosse::sur("torse", f);
                b.pxw(g, 0, 0, trim, b.r.largeur, 1);
            }
        }
    }
    if spec.ceinture {
        for f in COTES {
            let b = Brosse::sur("torse", f);
            b.pxw(g, 0, 8, cuir, b.r.largeur, 2);
            b.pxw(g, 0, 9, cuir_f, b.r.largeur, 1);
        }
        Brosse::sur("torse", "devant").pxw(g, 3, 8, fer, 2, 2);
    }
    if !spec.extras.is_empty() {
        let b = Brosse::sur("torse", "devant");
        for o in &spec.extras {
            let col = match o.4.as_str() {
                "fer" => fer,
                "clair" => trim,
                "sombre" => shade(corps, -0.26),
                "cuir" => cuir,
                "peau" => peau,
                _ => corps,
            };
            b.pxw(g, o.0, o.1, col, o.2, o.3);
        }
    }

    /* bras : manches puis mains */
    for (i, p) in BRAS.iter().enumerate() {
        if spec.manche > 0 {
            for (j, f) in a.FACES.iter().enumerate() {
                if f == "dessous" {
                    continue;
                }
                let r = face(p, f);
                let h = if f == "dessus" { r.hauteur } else { spec.manche.min(r.hauteur) };
                plaque(g, Rect { hauteur: h, ..r }, corps, 60 + i as i32 * 5 + j as i32, 0.07);
            }
        }
        let main = if vrai_b(t.mains_massives) { 3 } else { 2 };
        for f in COTES {
            let bb = Brosse::sur(p, f);
            bb.pxw(g, 0, 12 - main, shade(peau, -0.05), bb.r.largeur, main);
        }
        if vrai_b(t.griffes) {
            let b = Brosse::sur(p, "devant");
            b.px(g, 0, 11, OS);
            b.px(g, 3, 11, OS);
        }
    }

    /* jambes */
    let robe = spec.jambes == "robe";
    let fin = if robe { 10 } else { 8 };
    for (i, p) in JAMBES.iter().enumerate() {
        for (j, f) in a.FACES.iter().enumerate() {
            if f == "dessous" {
                continue;
            }
            let r = face(p, f);
            let h = if f == "dessus" { r.hauteur } else { fin };
            plaque(g, Rect { hauteur: h, ..r }, if robe { corps } else { pantalon }, 80 + i as i32 * 5 + j as i32, 0.06);
        }
        for f in ["devant", "derriere", "droite", "gauche", "dessous"] {
            let r = face(p, f);
            let dessous = f == "dessous";
            let haut = if dessous { 0 } else { fin };
            let h = if dessous { r.hauteur } else { 12 - fin };
            plaque(g, Rect { y: r.y + haut, hauteur: h, ..r }, botte, 95 + i as i32, 0.05);
        }
        Brosse::sur(p, "devant").pxw(g, 0, fin, shade(botte, 0.18), 4, 1);
    }
}

/// Texel de cheveux avec son grain : plus clair ou plus sombre selon le bruit
fn pose_cheveu(g: &mut Toile, b: &Brosse, x: i32, y: i32, col: u32) {
    let n = bruit(x * 3 + y, y * 5, 61);
    b.px(g, x, y, if n > 0.86 { shade(col, 0.1) } else if n < 0.2 { shade(col, -0.16) } else { col });
}

/// La lisière suit un profil par colonne, jamais une coupe droite. Les côtés vont de l'avant vers
/// l'arrière ; le côté droit est inversé.
fn colonnes(g: &mut Toile, face: &str, profil: Option<&Vec<i32>>, col: u32, inverse: bool, plafond: i32) {
    let Some(profil) = profil else { return };
    let b = Brosse::sur("tete2", face);
    for x in 0..8 {
        let index = if inverse { 7 - x } else { x };
        let p = profil.get(index as usize).copied().unwrap_or(0);
        let h = 8.min((if plafond != 0 { plafond } else { 8 }).min(p));
        for y in 0..h {
            pose_cheveu(g, &b, x, y, col);
        }
    }
}

fn couche2(g: &mut Toile, cfg: &Appearance, t: &Traits, cheveux: u32, peau: u32) {
    let a = atlas();
    let co = a.coiffure(&cfg.coiffure).unwrap_or(&a.COIFFURES[0]);
    let volume = if vrai_b(t.chevelure_etheree) { shade(cheveux, 0.2) } else { cheveux };
    let ombre = shade(volume, -0.12);

    let plafond = match &co.der {
        Some(der) if !der.is_empty() => *der.iter().min().unwrap(),
        _ => 8,
    };
    if co.dev.is_some() || co.cot.is_some() || co.der.is_some() {
        plaque(g, face("tete2", "dessus"), volume, 12, 0.08);
        colonnes(g, "devant", co.dev.as_ref(), volume, false, 8);
        colonnes(g, "gauche", co.cot.as_ref(), volume, false, plafond);
        colonnes(g, "droite", co.cot.as_ref(), volume, true, plafond);
        colonnes(g, "derriere", co.der.as_ref(), ombre, false, 8);
    }
    for o in &co.extras {
        Brosse::sur("tete2", &o.0).pxw(g, o.1, o.2, volume, o.3, o.4);
    }

    /* oreilles pointues */
    if vrai_i(t.oreilles) {
        let l = t.oreilles.unwrap();
        for f in ["droite", "gauche"] {
            let b = Brosse::sur("tete2", f);
            b.pxw(g, 4, 3, peau, l, 1);
            b.pxw(g, 4 + l - 1, 2, peau, 1, 1);
            b.pxw(g, 4, 4, shade(peau, -0.15), (l - 1).max(1), 1);
        }
    }

    /* oreilles animales */
    if let Some(forme) = vrai_s(&t.oreilles_animales) {
        let b = Brosse::sur("tete2", "dessus");
        let interieur = mix(peau, 0x8A4A4A, 0.4);
        let w = 2;
        b.pxw(g, 1, 3, peau, w, 2);
        b.pxw(g, 8 - 1 - w, 3, peau, w, 2);
        b.px(g, 1, 4, interieur);
        b.px(g, 8 - 2, 4, interieur);
        let bb = Brosse::sur("tete2", "devant");
        let h = if forme == "lupin" { 2 } else { 1 };
        bb.pxw(g, 1, 0, peau, w, h);
        bb.pxw(g, 8 - 1 - w, 0, peau, w, h);
    }

    /* épaulettes des tenues qui en portent */
    if let Some(spec) = a.tenue(&cfg.tenue).filter(|s| s.epaulettes) {
        let metal = if spec.ton == "plaque" { 0x98A1A8 } else { 0x5A3B25 };
        for (i, p) in BRAS.iter().enumerate() {
            let nom = format!("{p}2");
            for f in COTES {
                let r = face(&nom, f);
                plaque(g, Rect { hauteur: 3, ..r }, metal, 120 + i as i32, 0.08);
            }
            plaque(g, face(&nom, "dessus"), shade(metal, 0.08), 130 + i as i32, 0.06);
        }
    }

    /* appendices crâniens et queue : pièces de couche 2 extrudées par la figure */
    let acc = vrai_s(&t.accent).map(rgb).unwrap_or_else(|| shade(peau, 0.2));
    if let Some(kind) = vrai_s(&t.appendice) {
        peindre_appendice(g, kind, peau, acc);
    }
    if let Some(kind) = vrai_s(&t.queue) {
        peindre_queue(g, kind, peau, acc);
    }

    /* halo d'aasimar */
    if vrai_b(t.halo) {
        let col = rgb(vrai_s(&t.reflet).unwrap_or("#E8C46A"));
        g.save();
        g.global_alpha = 0.55;
        let b = Brosse::sur("torse2", "derriere");
        b.pxw(g, 0, 1, col, 8, 1);
        b.pxw(g, 0, 2, shade(col, -0.2), 8, 1);
        g.restore();
    }
}

/* ---------- appendices crâniens ----------
   Une case du patron porte les deux côtés : la figure en extrude les texels en un plan vertical
   posé sur le crâne. */

fn peindre_appendice(g: &mut Toile, kind: &str, peau: u32, acc: u32) {
    let Some((lignes, matiere)) = appendice(kind) else { return };
    let oreille = matiere.is_none();
    let base = match matiere {
        None => peau,
        Some("accent") => acc,
        Some(hex) => rgb(hex),
    };
    let sombre = shade(base, -0.3);
    let clair = shade(base, 0.16);
    let b = Brosse::sur("appendice", "devant");
    for (y, bandes) in lignes.iter().enumerate() {
        let y = y as i32;
        for &[x, w] in bandes.iter() {
            let droite = x < 4;
            for i in 0..w {
                let px = x + i;
                let col = if !oreille {
                    if y < 2 {
                        clair
                    } else if y % 2 != 0 {
                        shade(base, -0.18)
                    } else {
                        base
                    }
                } else {
                    let interne = if droite { px == x + w - 1 } else { px == x };
                    if y == 0 {
                        sombre
                    } else if interne && y > 1 {
                        acc
                    } else {
                        base
                    }
                };
                b.px(g, px, y, col);
            }
        }
    }
}

fn peindre_queue(g: &mut Toile, kind: &str, peau: u32, acc: u32) {
    let Some(lignes) = queue(kind) else { return };
    let sombre = shade(peau, -0.24);
    let b = Brosse::sur("queue", "devant");
    for (y, &[x, w]) in lignes.iter().enumerate() {
        let y = y as i32;
        for i in 0..w {
            let mut col = peau;
            if kind == "longue" && y % 2 != 0 {
                col = sombre;
            }
            if kind == "touffue" && bruit(x + i, y, 53) > 0.6 {
                col = sombre;
            }
            if kind == "panache" && y > 5 {
                col = acc;
            }
            b.px(g, x + i, y, col);
        }
    }
}

/* Cheveux : deux pièces peintes chacune dans sa case 8 × 8 du patron. La figure en extrude les
   texels ; rien ne se peint sur la peau ni sur le torse. */

fn peindre_chevelure(g: &mut Toile, coiffure: &str, cheveux: u32, t: &Traits) {
    let Some(co) = atlas().coiffure(coiffure) else { return };
    let Some(kind) = vrai_s(&co.chute) else { return };
    let Some(lignes) = chute(kind) else { return };
    let col = if vrai_b(t.chevelure_etheree) { shade(cheveux, 0.18) } else { cheveux };
    let clair = shade(col, 0.1);
    let sombre = shade(col, -0.2);
    let miroir = MIROIR_CHUTE.contains(&kind);
    let b = Brosse::sur("chevelure", "devant");
    for (y, bande) in lignes.iter().enumerate() {
        let y = y as i32;
        if bande.is_empty() {
            continue;
        }
        for i in 0..bande[1] {
            pose_meche(g, &b, bande[0] + i, y, col, clair, sombre);
            if miroir {
                pose_meche(g, &b, 8 - bande[0] - bande[1] + i, y, col, clair, sombre);
            }
        }
    }
}

fn pose_meche(g: &mut Toile, b: &Brosse, x: i32, y: i32, col: u32, clair: u32, sombre: u32) {
    let n = bruit(x, y, 47);
    b.px(g, x, y, if n > 0.84 { clair } else if n < 0.22 { sombre } else { col });
}

fn peindre_barbe(g: &mut Toile, style: &str, cheveux: u32) {
    let Some(rangs) = atlas().barbe(style).and_then(|b| b.rangs.as_ref()) else { return };
    let clair = shade(cheveux, 0.1);
    let sombre = shade(cheveux, -0.22);
    let b = Brosse::sur("barbe", "devant");
    for (y, bandes) in rangs.iter().enumerate() {
        let y = y as i32;
        for &[x, w] in bandes {
            for i in 0..w {
                let n = bruit(x + i, y, 31);
                b.px(g, x + i, y, if n > 0.84 { clair } else if n < 0.2 { sombre } else { cheveux });
            }
        }
    }
    if style == "tressee" || style == "nattee" {
        b.pxw(g, 3, 5, sombre, 1, 3);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(c: u32) -> String {
        format!("#{c:06x}")
    }

    #[test]
    fn shade_and_mix_round_through_hex() {
        assert_eq!(hex(shade(rgb("#E0AC7E"), -0.1)), "#ca9b71");
        assert_eq!(hex(shade(rgb("#E0AC7E"), 1.0)), "#ffffff");
        assert_eq!(hex(shade(rgb("#0A0B0C"), 0.5)), "#858586");
        assert_eq!(hex(mix(rgb("#E0AC7E"), rgb("#8A3B33"), 0.45)), "#b9795c");
    }

    #[test]
    fn hundredths_round_ties_up() {
        assert_eq!(au_centieme(0.92 - 0.045), 0.88);
        assert_eq!(au_centieme(0.124), 0.12);
        assert_eq!(au_centieme(-0.126), -0.13);
    }
}
