//! Le corps du héros : les boîtes et les plaques du patron 64 × 64 (`tools/heros/construire.py`
//! de la branche `develop`), partagées par l'aperçu du créateur et par la vue à la troisième
//! personne, où le héros est ajouté au maillage des créatures avec sa propre planche.

use glam::{vec3, Quat, Vec3};
use herocraft_common::hero::{Appearance, SHEET_SIZE};
use herocraft_common::tangent_frame;

use crate::creatures::CreatureFrame;
use crate::planet_mesh::PlanetVertex;

/// Indice de texture réservé à la planche du héros dans les plages du maillage des créatures
pub const HERO_TEXTURE: usize = usize::MAX;

/// Un quadrilatère de la figure : coins vus de dehors (bas gauche, bas droit, haut droit, haut
/// gauche) et le rectangle de la planche qu'il montre
pub struct Quad {
    pub corners: [Vec3; 4],
    pub uv: [f32; 4],
}

/// Un membre qui se balance en marchant : hauteur de son pivot et sens du balancement
#[derive(Clone, Copy)]
pub struct Swing {
    pub pivot: f32,
    pub sign: f32,
}

/// Une pièce de la figure : ses faces, son centre (pour l'ordre de dessin), sa couche et son
/// balancement s'il s'agit d'un membre
pub struct Piece {
    pub quads: Vec<Quad>,
    pub centre: Vec3,
    pub layer: u8,
    pub swing: Option<Swing>,
}

/// Une boîte dépliée à la manière de Minecraft depuis son coin du patron, gonflée de `g`
#[allow(clippy::too_many_arguments)]
pub fn boite(cx: f32, bottom: f32, w: f32, h: f32, d: f32, u: f32, v: f32, g: f32, layer: u8) -> Piece {
    let (x0, x1) = (cx - w / 2.0 - g, cx + w / 2.0 + g);
    let (y0, y1) = (bottom - g, bottom + h + g);
    let (z0, z1) = (-d / 2.0 - g, d / 2.0 + g);
    let quads = vec![
        Quad { corners: [vec3(x0, y0, z1), vec3(x1, y0, z1), vec3(x1, y1, z1), vec3(x0, y1, z1)], uv: [u + d, v + d, w, h] },
        Quad { corners: [vec3(x1, y0, z0), vec3(x0, y0, z0), vec3(x0, y1, z0), vec3(x1, y1, z0)], uv: [u + d + w + d, v + d, w, h] },
        Quad { corners: [vec3(x1, y0, z1), vec3(x1, y0, z0), vec3(x1, y1, z0), vec3(x1, y1, z1)], uv: [u + d + w, v + d, d, h] },
        Quad { corners: [vec3(x0, y0, z0), vec3(x0, y0, z1), vec3(x0, y1, z1), vec3(x0, y1, z0)], uv: [u, v + d, d, h] },
        Quad { corners: [vec3(x0, y1, z1), vec3(x1, y1, z1), vec3(x1, y1, z0), vec3(x0, y1, z0)], uv: [u + d, v, w, d] },
        Quad { corners: [vec3(x0, y0, z0), vec3(x1, y0, z0), vec3(x1, y0, z1), vec3(x0, y0, z1)], uv: [u + d + w, v, w, d] },
    ];
    Piece { quads, centre: vec3(cx, bottom + h / 2.0, 0.0), layer, swing: None }
}

/// Plaque 8 × 8 des épaules vers le haut (barbe, cheveux tombants) : ses deux grandes faces lisent
/// toute la case
pub fn plaque(bottom: f32, z0: f32, z1: f32, u: f32, v: f32) -> Piece {
    let (x0, x1, y0, y1) = (-4.0, 4.0, bottom, bottom + 8.0);
    let quads = vec![
        Quad { corners: [vec3(x0, y0, z1), vec3(x1, y0, z1), vec3(x1, y1, z1), vec3(x0, y1, z1)], uv: [u, v, 8.0, 8.0] },
        Quad { corners: [vec3(x1, y0, z0), vec3(x0, y0, z0), vec3(x0, y1, z0), vec3(x1, y1, z0)], uv: [u, v, 8.0, 8.0] },
    ];
    Piece { quads, centre: vec3(0.0, bottom + 4.0, (z0 + z1) / 2.0), layer: 3, swing: None }
}

/// Le corps du héros en unités du patron (un héros mesure 32), pieds à zéro, face vers +z, sa
/// gauche vers +x
pub fn figure() -> Vec<Piece> {
    // Bras pendus aux épaules, jambes aux hanches ; bras droit et jambe gauche vont ensemble
    let arm = |sign| Some(Swing { pivot: 22.0, sign });
    let leg = |sign| Some(Swing { pivot: 12.0, sign });
    let limb = |mut p: Piece, swing| {
        p.swing = swing;
        p
    };
    vec![
        // couche 1 : centre x, bas, taille, coin du patron
        boite(0.0, 24.0, 8.0, 8.0, 8.0, 0.0, 0.0, 0.0, 1),
        boite(0.0, 12.0, 8.0, 12.0, 4.0, 16.0, 16.0, 0.0, 1),
        limb(boite(-6.0, 12.0, 4.0, 12.0, 4.0, 40.0, 16.0, 0.0, 1), arm(1.0)),
        limb(boite(6.0, 12.0, 4.0, 12.0, 4.0, 32.0, 48.0, 0.0, 1), arm(-1.0)),
        limb(boite(-2.0, 0.0, 4.0, 12.0, 4.0, 0.0, 16.0, 0.0, 1), leg(-1.0)),
        limb(boite(2.0, 0.0, 4.0, 12.0, 4.0, 16.0, 48.0, 0.0, 1), leg(1.0)),
        // couche 2
        boite(0.0, 24.0, 8.0, 8.0, 8.0, 32.0, 0.0, 0.5, 2),
        boite(0.0, 12.0, 8.0, 12.0, 4.0, 16.0, 32.0, 0.25, 2),
        limb(boite(-6.0, 12.0, 4.0, 12.0, 4.0, 40.0, 32.0, 0.25, 2), arm(1.0)),
        limb(boite(6.0, 12.0, 4.0, 12.0, 4.0, 48.0, 48.0, 0.25, 2), arm(-1.0)),
        limb(boite(-2.0, 0.0, 4.0, 12.0, 4.0, 0.0, 32.0, 0.25, 2), leg(-1.0)),
        limb(boite(2.0, 0.0, 4.0, 12.0, 4.0, 0.0, 48.0, 0.25, 2), leg(1.0)),
        // barbe et cheveux tombants
        plaque(20.0, 4.05, 4.45, 0.0, 0.0),
        plaque(20.0, -4.45, -4.05, 24.0, 0.0),
    ]
}

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
pub fn push_hero(frame: &mut CreatureFrame, cfg: &Appearance, feet: Vec3, yaw: f32, swing: f32, block: f32) {
    let up = feet.normalize_or_zero();
    let (east, north) = tangent_frame(up);
    let forward = north * yaw.cos() + east * yaw.sin();
    let left = up.cross(forward);
    let width = cfg.width_scale();
    let unit = cfg.taille as f32 / 32.0 * block;
    let scale = vec3(width, 1.0, width) * unit;
    let to_world = |p: Vec3| feet + left * p.x + up * p.y + forward * p.z;
    let rotate = |n: Vec3| left * n.x + up * n.y + forward * n.z;
    let sheet = SHEET_SIZE as f32;
    let margin = 0.01_f32;

    let start = frame.indices.len() as u32;
    for piece in figure() {
        // Un membre tourne autour de l'axe des épaules ou des hanches
        let (pivot, turn) = match piece.swing {
            Some(s) => (vec3(0.0, s.pivot, 0.0), Quat::from_rotation_x(s.sign * swing)),
            None => (Vec3::ZERO, Quat::IDENTITY),
        };
        for quad in &piece.quads {
            let c = quad.corners.map(|p| (pivot + turn * (p - pivot)) * scale);
            let normal = rotate((c[1] - c[0]).cross(c[3] - c[0]).normalize_or_zero());
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
    }
    frame.draws.push((Some(HERO_TEXTURE), start..frame.indices.len() as u32));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_figure_shows_front_faces_only() {
        // De face, la tête montre sa face avant et pas l'arrière
        let head = boite(0.0, 24.0, 8.0, 8.0, 8.0, 0.0, 0.0, 0.0, 1);
        let visible: Vec<usize> = head.quads.iter().enumerate().filter(|(_, q)| (q.corners[1] - q.corners[0]).cross(q.corners[3] - q.corners[0]).z > 1e-4).map(|(i, _)| i).collect();
        assert_eq!(visible, vec![0]);
    }

    #[test]
    fn the_hero_stands_on_his_feet_at_his_height_facing_his_yaw() {
        let cfg = herocraft_common::hero::atlas::atlas().defaut("humain");
        let feet = Vec3::new(0.0, 0.0, 1000.0);
        let (_, north) = tangent_frame(feet.normalize());
        let mut frame = CreatureFrame::default();
        push_hero(&mut frame, &cfg, feet, 0.0, 0.0, 1.0);
        assert_eq!(frame.draws.len(), 1);
        assert_eq!(frame.draws[0].0, Some(HERO_TEXTURE));
        assert_eq!(frame.uvs.len(), frame.vertices.len());
        let heights: Vec<f32> = frame.vertices.iter().map(|v| Vec3::from(v.position).length() - feet.length()).collect();
        let (low, high) = heights.iter().fold((f32::MAX, f32::MIN), |(lo, hi), h| (lo.min(*h), hi.max(*h)));
        assert!(low > -0.05 && low < 0.01, "pieds à {low}");
        // La tête de la couche 2 dépasse un peu la taille du héros
        assert!((high - cfg.taille as f32).abs() < 0.1, "tête à {high}, taille {}", cfg.taille);
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
