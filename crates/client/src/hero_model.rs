//! Le corps du héros : les boîtes du patron 64 × 64 (`tools/heros/construire.py` de la branche
//! `develop`), aux proportions de son espèce, partagé par l'aperçu du créateur et par la vue à la
//! troisième personne, où le héros est ajouté au maillage des créatures avec sa propre planche.
//!
//! **En relief.** La maquette extrudait les pixels de la planche plutôt que de les poser à plat :
//! - la couche 2 (cheveux, capuche, tenue, halo…) n'est plus une coque lisse mais un texel = un
//!   pavé posé sur la couche 1, si bien qu'une mèche ou un col a de l'épaisseur et que ce qui est
//!   transparent ne se dessine pas du tout ;
//! - les pièces des cases libres de la planche prennent du volume : barbe et cheveux tombants,
//!   appendices crâniens (oreilles de bête, cornes, bois, couronnes, cristaux) dressés sur le
//!   crâne, queue accrochée au bas du dos et ailes spectrales déployées dans le dos ;
//! - les proportions de l'espèce et de sa variante (`mods`) : tête, torse, bras, jambes, dos voûté
//!   et lévitation du sylphe.
//!
//! Une pièce dont la case est vide (pas de barbe, pas de queue…) ne donne aucun pavé : la figure
//! se lit sur la planche peinte, et rien d'autre ne décide de ce qui se voit.

use glam::{vec3, Quat, Vec3};
use herocraft_common::hero::{atlas, paint, Appearance, Sheet, SHEET_SIZE};
use herocraft_common::tangent_frame;

use crate::creatures::CreatureFrame;
use crate::planet_mesh::PlanetVertex;

/// Indice de texture réservé à la planche du héros dans les plages du maillage des créatures
pub const HERO_TEXTURE: usize = usize::MAX;

/// Un quadrilatère de la figure : coins vus de dehors (bas gauche, bas droit, haut droit, haut
/// gauche) et le rectangle de la planche qu'il montre, en texels
#[derive(Clone, Copy, Debug)]
pub struct Quad {
    pub corners: [Vec3; 4],
    pub uv: [f32; 4],
}

impl Quad {
    /// Normale sortante (non normée)
    pub fn normal(&self) -> Vec3 {
        (self.corners[1] - self.corners[0]).cross(self.corners[3] - self.corners[0])
    }

    pub fn centre(&self) -> Vec3 {
        (self.corners[0] + self.corners[1] + self.corners[2] + self.corners[3]) * 0.25
    }
}

/// Un membre qui se balance en marchant : hauteur de son pivot et sens du balancement
#[derive(Clone, Copy, Debug)]
pub struct Swing {
    pub pivot: f32,
    pub sign: f32,
}

/// Une pièce de la figure : ses faces, son balancement s'il s'agit d'un membre, et si elle suit le
/// haut du corps quand le dos se voûte
#[derive(Clone, Debug)]
pub struct Piece {
    pub quads: Vec<Quad>,
    pub swing: Option<Swing>,
    pub upper: bool,
}

/// Le héros en unités du patron, pieds à zéro, face vers +z, sa gauche vers +x
#[derive(Clone, Debug)]
pub struct Figure {
    pub pieces: Vec<Piece>,
    /// Hauteur du corps, des pieds au sommet du crâne (sans couche 2 ni appendice)
    pub body_height: f32,
    /// Plus haut point de la figure, appendices compris
    pub top: f32,
    /// Hauteur des hanches, pivot du dos voûté
    hips: f32,
    /// Inclinaison du haut du corps vers l'avant, en radians
    stoop: f32,
    /// Hauteur à laquelle flotte un sylphe
    pub hover: f32,
}

impl Figure {
    /// Les faces dans leur pose : membres balancés de `swing` radians, dos voûté, élargies de
    /// `width` en travers et en profondeur
    pub fn posed(&self, swing: f32, width: f32) -> impl Iterator<Item = Quad> + '_ {
        let scale = vec3(width, 1.0, width);
        let hips = vec3(0.0, self.hips, 0.0);
        let stoop = Quat::from_rotation_x(self.stoop);
        let lift = vec3(0.0, self.hover, 0.0);
        self.pieces.iter().flat_map(move |piece| {
            let (pivot, turn) = match piece.swing {
                Some(s) => (vec3(0.0, s.pivot, 0.0), Quat::from_rotation_x(s.sign * swing)),
                None => (Vec3::ZERO, Quat::IDENTITY),
            };
            let upper = piece.upper;
            piece.quads.iter().map(move |q| {
                let corners = q.corners.map(|p| {
                    let mut p = pivot + turn * (p - pivot);
                    if upper {
                        p = hips + stoop * (p - hips);
                    }
                    p * scale + lift
                });
                Quad { corners, uv: q.uv }
            })
        })
    }

    /// Nombre de faces
    #[cfg(test)]
    pub fn quad_count(&self) -> usize {
        self.pieces.iter().map(|p| p.quads.len()).sum()
    }
}

/* -------------------------------------------------------------------- boîtes et pavés */

/// Les six faces d'une boîte dépliée à la manière de Minecraft depuis son coin du patron `(u, v)`.
/// `size` : dimensions de la boîte, `pat` : celles de son dépliage en texels (elles diffèrent
/// quand l'espèce étire un membre), `g` : gonflement de chaque côté.
fn boite(cx: f32, bottom: f32, size: Vec3, pat: [f32; 3], u: f32, v: f32, g: f32) -> [Quad; 6] {
    let (x0, x1) = (cx - size.x / 2.0 - g, cx + size.x / 2.0 + g);
    let (y0, y1) = (bottom - g, bottom + size.y + g);
    let (z0, z1) = (-size.z / 2.0 - g, size.z / 2.0 + g);
    let [w, h, d] = pat;
    [
        Quad { corners: [vec3(x0, y0, z1), vec3(x1, y0, z1), vec3(x1, y1, z1), vec3(x0, y1, z1)], uv: [u + d, v + d, w, h] },
        Quad { corners: [vec3(x1, y0, z0), vec3(x0, y0, z0), vec3(x0, y1, z0), vec3(x1, y1, z0)], uv: [u + d + w + d, v + d, w, h] },
        Quad { corners: [vec3(x1, y0, z1), vec3(x1, y0, z0), vec3(x1, y1, z0), vec3(x1, y1, z1)], uv: [u + d + w, v + d, d, h] },
        Quad { corners: [vec3(x0, y0, z0), vec3(x0, y0, z1), vec3(x0, y1, z1), vec3(x0, y1, z0)], uv: [u, v + d, d, h] },
        Quad { corners: [vec3(x0, y1, z1), vec3(x1, y1, z1), vec3(x1, y1, z0), vec3(x0, y1, z0)], uv: [u + d, v, w, d] },
        Quad { corners: [vec3(x0, y0, z0), vec3(x1, y0, z0), vec3(x1, y0, z1), vec3(x0, y0, z1)], uv: [u + d + w, v, w, d] },
    ]
}

/// Un pavé de coin `o` et d'arêtes `a`, `b`, `c`, chaque face tournée vers le dehors et montrant
/// le texel `uv`. `show` : faces −a, +a, −b, +b, −c, +c à dessiner.
fn pave(out: &mut Vec<Quad>, o: Vec3, a: Vec3, b: Vec3, c: Vec3, uv: [f32; 4], show: [bool; 6]) {
    let centre = o + (a + b + c) * 0.5;
    let faces = [
        [o, o + b, o + b + c, o + c],
        [o + a, o + a + b, o + a + b + c, o + a + c],
        [o, o + a, o + a + c, o + c],
        [o + b, o + a + b, o + a + b + c, o + b + c],
        [o, o + a, o + a + b, o + b],
        [o + c, o + a + c, o + a + b + c, o + b + c],
    ];
    for (mut corners, shown) in faces.into_iter().zip(show) {
        if !shown {
            continue;
        }
        let quad = Quad { corners, uv };
        let middle = quad.centre();
        if quad.normal().dot(middle - centre) < 0.0 {
            corners.reverse();
        }
        out.push(Quad { corners, uv });
    }
}

/// Le texel `(x, y)` de la planche est-il peint ?
fn peint(sheet: &Sheet, x: i32, y: i32) -> bool {
    let n = SHEET_SIZE as i32;
    (0..n).contains(&x) && (0..n).contains(&y) && sheet.texel(x as usize, y as usize)[3] > 0
}

/// La couche 2 d'une boîte en relief : chaque texel peint de ses faces devient un pavé de
/// profondeur `t` posé sur la couche 1. Les faces sont prises sur la boîte gonflée de `t`, si
/// bien que les pavés de deux faces voisines se rejoignent sur l'arête.
///
/// `taper` : sur les quatre côtés, les deux derniers rangs avant le bas d'une plage peinte
/// s'amincissent. Une chevelure finit ainsi en lisière, plaquée sur la nuque et les tempes, au
/// lieu d'une marche nette qui la ferait paraître coupée au couteau.
#[allow(clippy::too_many_arguments)]
fn relief(out: &mut Vec<Quad>, sheet: &Sheet, cx: f32, bottom: f32, size: Vec3, pat: [f32; 3], u: f32, v: f32, t: f32, taper: bool) {
    for (index, face) in boite(cx, bottom, size, pat, u, v, t).into_iter().enumerate() {
        let [fu, fv, fw, fh] = face.uv.map(|x| x as i32);
        let [c0, c1, _, c3] = face.corners;
        let (sx, sy) = ((c1 - c0) / fw as f32, (c3 - c0) / fh as f32);
        let normal = face.normal().normalize_or_zero();
        let on = |i: i32, j: i32| (0..fw).contains(&i) && (0..fh).contains(&j) && peint(sheet, fu + i, fv + j);
        // Épaisseur d'un texel, nulle s'il n'est pas peint
        let thickness = |i: i32, j: i32| -> f32 {
            if !on(i, j) {
                return 0.0;
            }
            if !taper || index >= 4 {
                return t;
            }
            let below = (1..=2).take_while(|d| on(i, j + d)).count();
            t * [0.35, 0.7, 1.0][below]
        };
        for j in 0..fh {
            for i in 0..fw {
                let d = thickness(i, j);
                if d <= 0.0 {
                    continue;
                }
                // Rang j compté du haut de la face, la face montée du bas ; le pavé part de la
                // couche 1 et s'arrête à `d` au-dessus
                let depth = normal * d;
                let o = c0 + sx * i as f32 + sy * (fh - 1 - j) as f32 - normal * t;
                // Un côté se voit s'il borde un texel vide ou plus mince (pas au bord de la face,
                // que le pavé de la face voisine couvre)
                let open = |ni: i32, nj: i32| (0..fw).contains(&ni) && (0..fh).contains(&nj) && thickness(ni, nj) < d;
                let show = [open(i - 1, j), open(i + 1, j), open(i, j + 1), open(i, j - 1), false, true];
                pave(out, o, sx, sy, depth, [(fu + i) as f32, (fv + j) as f32, 1.0, 1.0], show);
            }
        }
    }
}

/// Une pièce plate extrudée : chaque texel peint du rectangle `(u, v, w, h)` devient un pavé.
/// `origin` : coin du texel (0, 0) côté haut gauche, `step_x` / `step_y` : pas d'une colonne et
/// d'un rang (rangs descendants), `thick` : épaisseur, centrée sur le plan.
#[allow(clippy::too_many_arguments)]
fn extrude(out: &mut Vec<Quad>, sheet: &Sheet, (u, v, w, h): (i32, i32, i32, i32), origin: Vec3, step_x: Vec3, step_y: Vec3, thick: Vec3, mirror: bool) {
    let col = |i: i32| if mirror { w - 1 - i } else { i };
    let on = |i: i32, j: i32| (0..w).contains(&i) && (0..h).contains(&j) && peint(sheet, u + col(i), v + j);
    for j in 0..h {
        for i in 0..w {
            if !on(i, j) {
                continue;
            }
            let o = origin + step_x * i as f32 + step_y * (j + 1) as f32 - thick * 0.5;
            let show = [!on(i - 1, j), !on(i + 1, j), !on(i, j + 1), !on(i, j - 1), true, true];
            pave(out, o, step_x, -step_y, thick, [(u + col(i)) as f32, (v + j) as f32, 1.0, 1.0], show);
        }
    }
}

/* -------------------------------------------------------------------- la figure */

/// Épaisseur du relief de la couche 2 : sur la tête, sur le reste du corps
const RELIEF_TETE: f32 = 0.75;
const RELIEF_CORPS: f32 = 0.5;
/// Hauteur à laquelle flotte un sylphe, en texels
const HOVER: f32 = 2.0;

/// Le corps du héros pour l'apparence `cfg`, en relief sur sa planche peinte `sheet`
pub fn figure(cfg: &Appearance, sheet: &Sheet) -> Figure {
    let a = atlas();
    let m = a.mods_de(cfg);
    let ratio = |v: Option<f64>| v.filter(|v| *v > 0.0).unwrap_or(1.0) as f32;
    let (k, sx, ab, lj) = (ratio(m.tete), ratio(m.torse), ratio(m.bras), ratio(m.jambes));
    let legs = 12.0 * lj;
    let shoulders = legs + 12.0;
    let crown = shoulders + 8.0 * k;
    let arm_x = 4.0 * sx + 2.0 * ab;

    let arm = |sign| Some(Swing { pivot: legs + 10.0, sign });
    let leg = |sign| Some(Swing { pivot: legs, sign });
    let mut pieces = Vec::new();
    let mut piece = |swing: Option<Swing>, upper: bool, build: &dyn Fn(&mut Vec<Quad>)| {
        let mut quads = Vec::new();
        build(&mut quads);
        pieces.push(Piece { quads, swing, upper });
    };

    // Chaque membre : sa boîte de couche 1, puis sa couche 2 en relief.
    // (centre x, bas, taille, dépliage, coin couche 1, coin couche 2, relief, balancement, haut du corps)
    #[allow(clippy::type_complexity)]
    let membres: [(f32, f32, Vec3, [f32; 3], (f32, f32), (f32, f32), f32, Option<Swing>, bool); 6] = [
        (0.0, shoulders, Vec3::splat(8.0 * k), [8.0, 8.0, 8.0], (0.0, 0.0), (32.0, 0.0), RELIEF_TETE, None, true),
        (0.0, legs, vec3(8.0 * sx, 12.0, 4.0 * sx), [8.0, 12.0, 4.0], (16.0, 16.0), (16.0, 32.0), RELIEF_CORPS, None, true),
        (-arm_x, legs, vec3(4.0 * ab, 12.0, 4.0 * ab), [4.0, 12.0, 4.0], (40.0, 16.0), (40.0, 32.0), RELIEF_CORPS, arm(1.0), true),
        (arm_x, legs, vec3(4.0 * ab, 12.0, 4.0 * ab), [4.0, 12.0, 4.0], (32.0, 48.0), (48.0, 48.0), RELIEF_CORPS, arm(-1.0), true),
        (-2.0, 0.0, vec3(4.0, legs, 4.0), [4.0, 12.0, 4.0], (0.0, 16.0), (0.0, 32.0), RELIEF_CORPS, leg(-1.0), false),
        (2.0, 0.0, vec3(4.0, legs, 4.0), [4.0, 12.0, 4.0], (16.0, 48.0), (0.0, 48.0), RELIEF_CORPS, leg(1.0), false),
    ];
    for (cx, bottom, size, pat, (u1, v1), (u2, v2), t, swing, upper) in membres {
        piece(swing, upper, &|q| {
            q.extend(boite(cx, bottom, size, pat, u1, v1, 0.0));
            relief(q, sheet, cx, bottom, size, pat, u2, v2, t, t == RELIEF_TETE);
        });
    }

    let mouth = shoulders + 4.0 * k;
    let front = 4.0 * k + 0.05;
    let back = 4.0 * sx;
    piece(None, true, &|q| {
        // Barbe : couvre le bas du visage et tombe sous le menton
        extrude(q, sheet, (0, 0, 8, 8), vec3(-4.0 * k, mouth, front + 0.5), Vec3::X * k, Vec3::NEG_Y * k, Vec3::Z, false);
        // Cheveux tombants : derrière la tête, vus de dos
        extrude(q, sheet, (24, 0, 8, 8), vec3(4.0 * k, mouth, -front - 0.5), Vec3::NEG_X * k, Vec3::NEG_Y * k, Vec3::Z, false);
        // Appendice crânien : plan dressé sur le crâne, les deux côtés dans la même case ; son
        // rang peint le plus bas s'enfonce un peu dans la chevelure
        let empty = (0..8).rev().take_while(|&j| (0..8).all(|i| !peint(sheet, 32 + i, j))).count();
        let base = crown + 0.25 - empty as f32 * k;
        extrude(q, sheet, (32, 0, 8, 8), vec3(-4.0 * k, base + 8.0 * k, 0.0), Vec3::X * k, Vec3::NEG_Y * k, Vec3::Z * k, false);
        // Ailes : racine contre le dos, déployées vers l'arrière et les côtés
        let span = 1.3_f32;
        let sweep = 30.0_f32.to_radians();
        for side in [1.0_f32, -1.0] {
            let out = vec3(side * sweep.cos(), 0.0, -sweep.sin()) * span;
            let origin = vec3(side * 1.0, shoulders + 3.0, -back / 2.0 - 0.8);
            let thick = out.cross(Vec3::Y).normalize() * 0.4;
            extrude(q, sheet, (56, 16, 7, 14), origin, out, Vec3::NEG_Y * span, thick, false);
        }
        // Queue : accrochée au bas du dos, elle part vers l'arrière en tombant
        let down = vec3(0.0, -0.55, -0.83).normalize();
        let root = vec3(4.0, legs + 1.5, -back / 2.0 + 0.3);
        let thick = Vec3::NEG_X.cross(down).normalize();
        extrude(q, sheet, (56, 0, 8, 8), root, Vec3::NEG_X, down, thick, false);
    });

    let top = pieces.iter().flat_map(|p| p.quads.iter()).flat_map(|q| q.corners).fold(crown, |t, c| t.max(c.y));
    Figure {
        pieces,
        body_height: crown,
        top,
        hips: legs,
        stoop: (m.voute.unwrap_or(0) as f32).to_radians(),
        hover: if m.flotte == Some(true) { HOVER } else { 0.0 },
    }
}

/// Une figure et sa planche, gardées tant que l'apparence ne change pas ce qui s'y voit
#[derive(Default)]
pub struct FigureCache {
    entry: Option<(String, Figure)>,
}

impl FigureCache {
    pub fn get(&mut self, cfg: &Appearance) -> &Figure {
        let key = cfg.sheet_key();
        if self.entry.as_ref().is_none_or(|(k, _)| *k != key) {
            let sheet = paint(cfg);
            self.entry = Some((key, figure(cfg, &sheet)));
        }
        &self.entry.as_ref().expect("figure posée juste au-dessus").1
    }
}

/* -------------------------------------------------------------------- la marche */

/// Ampleur du balancement des membres à pleine allure, en radians
const SWING_MAX: f32 = 0.7;
/// Allure (blocs par seconde) à laquelle le balancement atteint son ampleur
const FULL_STRIDE_SPEED: f32 = 4.0;

/// La marche du héros : le balancement de ses membres suit son déplacement
#[derive(Default)]
pub struct HeroGait {
    last_feet: Option<Vec3>,
    phase: f32,
    amplitude: f32,
}

impl HeroGait {
    /// Avance la marche de `dt` secondes vers la position `feet` ; `block` : taille d'un bloc
    pub fn update(&mut self, feet: Vec3, dt: f32, block: f32) {
        let last = self.last_feet.replace(feet).unwrap_or(feet);
        if dt <= 0.0 {
            return;
        }
        let up = feet.normalize_or_zero();
        let step = feet - last;
        // Seul le pas sur le sol compte ; un saut d'un monde à l'autre ne fait pas courir
        let ground = (step - up * step.dot(up)).length() / block;
        let speed = if ground > 50.0 { 0.0 } else { ground / dt };
        self.phase = (self.phase + ground * 0.9) % std::f32::consts::TAU;
        let target = (speed / FULL_STRIDE_SPEED).min(1.0) * SWING_MAX;
        // Les bras retombent en douceur à l'arrêt
        self.amplitude += (target - self.amplitude) * (dt * 10.0).min(1.0);
    }

    /// Angle de balancement du moment
    pub fn swing(&self) -> f32 {
        self.amplitude * self.phase.sin()
    }
}

/// Ajoute le héros au maillage des créatures, debout sur `feet` et tourné vers `yaw` (0 = nord) ;
/// `block` : taille d'un bloc, `swing` : balancement des membres
pub fn push_hero(frame: &mut CreatureFrame, cfg: &Appearance, figure: &Figure, feet: Vec3, yaw: f32, swing: f32, block: f32) {
    let up = feet.normalize_or_zero();
    let (east, north) = tangent_frame(up);
    let forward = north * yaw.cos() + east * yaw.sin();
    let left = up.cross(forward);
    // La taille du héros va des pieds au sommet du crâne, quelles que soient ses proportions
    let unit = cfg.taille as f32 / figure.body_height * block;
    let to_world = |p: Vec3| feet + (left * p.x + up * p.y + forward * p.z) * unit;
    let rotate = |n: Vec3| left * n.x + up * n.y + forward * n.z;
    let sheet = SHEET_SIZE as f32;
    let margin = 0.01_f32;

    let start = frame.indices.len() as u32;
    for quad in figure.posed(swing, cfg.width_scale()) {
        let c = quad.corners;
        let normal = rotate(quad.normal().normalize_or_zero());
        let [u, v, w, h] = quad.uv;
        let (u0, v0, u1, v1) = ((u + margin) / sheet, (v + margin) / sheet, (u + w - margin) / sheet, (v + h - margin) / sheet);
        let uvs = [[u0, v1], [u1, v1], [u1, v0], [u0, v0]];
        let base = frame.vertices.len() as u32;
        for (p, uv) in c.iter().zip(uvs) {
            frame.vertices.push(PlanetVertex { position: to_world(*p).to_array(), normal: normal.to_array(), color: [1.0, 1.0, 1.0, 1.0], texture: PlanetVertex::UNTEXTURED });
            frame.uvs.push(uv);
        }
        frame.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    frame.draws.push((Some(HERO_TEXTURE), start..frame.indices.len() as u32));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hero(espece: &str, variante: i32) -> (Appearance, Figure) {
        let mut cfg = atlas().defaut(espece);
        cfg.variante = variante;
        cfg.retouch(true);
        let f = figure(&cfg, &paint(&cfg));
        (cfg, f)
    }

    fn points(f: &Figure) -> Vec<Vec3> {
        f.posed(0.0, 1.0).flat_map(|q| q.corners).collect()
    }

    #[test]
    fn the_figure_shows_front_faces_only() {
        // De face, la tête montre sa face avant et pas l'arrière
        let head = boite(0.0, 24.0, Vec3::splat(8.0), [8.0, 8.0, 8.0], 0.0, 0.0, 0.0);
        let visible: Vec<usize> = head.iter().enumerate().filter(|(_, q)| q.normal().z > 1e-4).map(|(i, _)| i).collect();
        assert_eq!(visible, vec![0]);
    }

    #[test]
    fn boxes_turn_every_face_outwards() {
        let mut quads = Vec::new();
        pave(&mut quads, Vec3::ZERO, Vec3::X, Vec3::Y, Vec3::Z, [0.0; 4], [true; 6]);
        assert_eq!(quads.len(), 6);
        let centre = Vec3::splat(0.5);
        assert!(quads.iter().all(|q| q.normal().dot(q.centre() - centre) > 0.0));
    }

    #[test]
    fn the_second_layer_stands_out_texel_by_texel() {
        let (_, f) = hero("humain", 0);
        // Bien plus de faces que les six boîtes de la couche 1 : la couche 2 est en pavés
        assert!(f.quad_count() > 36 + 150, "{} faces", f.quad_count());
        // Les cheveux dépassent du crâne
        let top = points(&f).iter().fold(f32::MIN, |t, p| t.max(p.y));
        assert!(top > f.body_height + 0.5, "sommet {top}, crâne {}", f.body_height);
    }

    #[test]
    fn species_keep_their_proportions() {
        let (_, man) = hero("humain", 0);
        let (_, dwarf) = hero("nain", 0);
        let (_, gnome) = hero("gnome", 0);
        // Jambes courtes du nain, grosse tête du gnome
        assert!(dwarf.hips < man.hips - 2.0, "{} / {}", dwarf.hips, man.hips);
        assert!(gnome.body_height - gnome.hips - 12.0 > 9.0, "tête du gnome");
        // Le briseur cornu se voûte, le sylphe flotte
        let (_, horned) = hero("hommeBete", 3);
        assert!(horned.stoop > 0.1, "{}", horned.stoop);
        let (_, sylph) = hero("sylphe", 0);
        assert!(sylph.hover > 0.0);
        let low = points(&sylph).iter().fold(f32::MAX, |l, p| l.min(p.y));
        assert!(low > 1.0, "pieds du sylphe à {low}");
    }

    #[test]
    fn cranial_appendages_rise_above_the_skull() {
        // Porte-cornes : des cornes sur le crâne
        let (_, horns) = hero("demiOrque", 2);
        let (_, plain) = hero("demiOrque", 0);
        assert!(horns.top > horns.body_height + 2.0, "{} / {}", horns.top, horns.body_height);
        assert!(plain.top < plain.body_height + 1.5, "{} / {}", plain.top, plain.body_height);
    }

    #[test]
    fn beast_folk_carry_a_tail_and_aasimar_spread_wings() {
        let (_, tiger) = hero("hommeBete", 0);
        let tail = points(&tiger).into_iter().filter(|p| p.z < -5.5 && p.y < tiger.hips + 2.0).count();
        assert!(tail > 0, "pas de queue");
        let (_, man) = hero("humain", 0);
        assert!(points(&man).iter().all(|p| p.z > -5.5), "un humain n'a ni queue ni ailes");

        let (_, winged) = hero("aasimar", 0);
        let wing = points(&winged).into_iter().filter(|p| p.x.abs() > 6.0 && p.z < -4.0).count();
        assert!(wing > 0, "pas d'ailes");
    }

    #[test]
    fn the_hero_stands_on_his_feet_at_his_height_facing_his_yaw() {
        let (cfg, f) = hero("humain", 0);
        let feet = Vec3::new(0.0, 0.0, 1000.0);
        let (_, north) = tangent_frame(feet.normalize());
        let mut frame = CreatureFrame::default();
        push_hero(&mut frame, &cfg, &f, feet, 0.0, 0.0, 1.0);
        assert_eq!(frame.draws.len(), 1);
        assert_eq!(frame.draws[0].0, Some(HERO_TEXTURE));
        assert_eq!(frame.uvs.len(), frame.vertices.len());
        let heights: Vec<f32> = frame.vertices.iter().map(|v| Vec3::from(v.position).length() - feet.length()).collect();
        let (low, high) = heights.iter().fold((f32::MAX, f32::MIN), |(lo, hi), h| (lo.min(*h), hi.max(*h)));
        assert!(low > -0.05 && low < 0.01, "pieds à {low}");
        // Le relief de la couche 2 dépasse un peu la taille du héros
        assert!((high - cfg.taille as f32).abs() < 0.12, "tête à {high}, taille {}", cfg.taille);
        // Le visage (première face de la tête) regarde vers le nord quand le cap est nul
        assert!(frame.vertices[..4].iter().all(|v| (Vec3::from(v.position) - feet).dot(north) > 0.2));
    }

    #[test]
    fn walking_swings_the_limbs_and_standing_still_lets_them_fall() {
        let mut gait = HeroGait::default();
        let mut feet = Vec3::new(0.0, 0.0, 1000.0);
        for _ in 0..60 {
            feet += Vec3::X * (4.0 / 60.0);
            gait.update(feet, 1.0 / 60.0, 1.0);
        }
        assert!(gait.amplitude > 0.6, "{}", gait.amplitude);
        for _ in 0..120 {
            gait.update(feet, 1.0 / 60.0, 1.0);
        }
        assert!(gait.swing().abs() < 0.01);
    }
}
