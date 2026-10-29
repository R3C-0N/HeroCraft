//! Modèles 3D des créatures. Chaque espèce est dessinée d'après le modèle de sa description
//! (`assets/creatures/*.json`) : des pavés colorés, dont certains (pattes, tête, queue) se balancent
//! au rythme du pas. Le maillage de toutes les créatures est refait à chaque image et dessiné avec
//! le pipeline du relief : même éclairage, mêmes ombres, même perspective aérienne.
//!
//! En ligne, les créatures viennent du serveur ; en solo, la simulation `CreatureWorld` tourne
//! dans le jeu. Les deux donnent les mêmes `CreatureView`.

use std::collections::HashMap;
use std::f32::consts::TAU;
use std::sync::OnceLock;

use glam::Vec3;
use herocraft_common::{tangent_frame, Color, CreatureModel, CreatureRegistry, ModelPart};

use crate::planet_mesh::PlanetVertex;

/// Une créature à dessiner
#[derive(Debug, Clone, Copy)]
pub struct CreatureView {
    pub id: u64,
    /// Rang de son espèce dans le registre (`None` : espèce inconnue de ce client)
    pub species: Option<usize>,
    /// Pieds (repère du centre de la planète)
    pub feet: Vec3,
    /// Cap dans le plan tangent (0 = nord, π/2 = est)
    pub yaw: f32,
}

/// Au-dessus de cette vitesse (en blocs par seconde), la créature marche : ses pièces se balancent
const WALKING_SPEED: f32 = 0.2;
/// Rapidité avec laquelle le balancement s'installe ou s'éteint, par seconde
const SWING_BLEND_RATE: f32 = 6.0;
/// Les 6 faces d'un pavé, par indices de coins (bit 0 = x max, bit 1 = y max, bit 2 = z max), et
/// l'axe de leur normale sortante (0 = x, 1 = y, 2 = z ; signe)
const FACES: [([usize; 4], usize, f32); 6] = [
    ([0, 4, 6, 2], 0, -1.0),
    ([1, 3, 7, 5], 0, 1.0),
    ([0, 1, 5, 4], 1, -1.0),
    ([2, 6, 7, 3], 1, 1.0),
    ([0, 2, 3, 1], 2, -1.0),
    ([4, 5, 7, 6], 2, 1.0),
];

/// Le pas de chaque créature : où il en est dans son cycle, et s'il marche
#[derive(Default)]
pub struct CreatureAnimator {
    gaits: HashMap<u64, Gait>,
}

struct Gait {
    last_feet: Vec3,
    /// Position dans le cycle du pas, de 0 à 1
    phase: f32,
    /// Ampleur du balancement, de 0 (immobile) à 1 (en marche)
    amount: f32,
}

impl CreatureAnimator {
    /// Avance le pas de chaque créature selon le chemin parcouru depuis l'image précédente (`dt`
    /// secondes) ; `block` : taille d'un bloc
    pub fn update(&mut self, views: &[CreatureView], registry: &CreatureRegistry, dt: f32, block: f32) {
        self.gaits.retain(|id, _| views.iter().any(|v| v.id == *id));
        for view in views {
            let gait = self.gaits.entry(view.id).or_insert(Gait { last_feet: view.feet, phase: 0.0, amount: 0.0 });
            let up = view.feet.normalize_or_zero();
            let step = view.feet - gait.last_feet;
            let moved = (step - up * step.dot(up)).length() / block;
            gait.last_feet = view.feet;
            // Un cycle de pas couvre à peu près la longueur du corps
            let stride = view.species.and_then(|s| registry.get(s)).map_or(1.0, |s| (s.body.half_length * 2.5).max(0.5));
            gait.phase = (gait.phase + moved / stride).fract();
            let walking = dt > 0.0 && moved / dt > WALKING_SPEED;
            let target = if walking { 1.0 } else { 0.0 };
            gait.amount += (target - gait.amount) * (SWING_BLEND_RATE * dt).min(1.0);
        }
    }

    fn gait(&self, id: u64) -> (f32, f32) {
        self.gaits.get(&id).map_or((0.0, 0.0), |g| (g.phase, g.amount))
    }
}

/// Modèle d'une créature dont l'espèce est inconnue de ce client : un pavé gris
fn fallback_model() -> &'static CreatureModel {
    static MODEL: OnceLock<CreatureModel> = OnceLock::new();
    MODEL.get_or_init(|| CreatureModel {
        units_per_block: 16.0,
        parts: vec![ModelPart {
            name: "inconnue".into(),
            from: [-5.0, 0.0, -5.0],
            to: [5.0, 10.0, 5.0],
            color: Color([0.5, 0.5, 0.5]),
            pivot: None,
            swing: None,
        }],
    })
}

/// Maillage (sommets, indices) de toutes les créatures ; `block` : taille d'un bloc
pub fn build_mesh(views: &[CreatureView], registry: &CreatureRegistry, animator: &CreatureAnimator, block: f32) -> (Vec<PlanetVertex>, Vec<u32>) {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for view in views {
        let model = view.species.and_then(|s| registry.get(s)).map_or(fallback_model(), |s| &s.model);
        let (phase, amount) = animator.gait(view.id);
        // Repère de la créature : x vers sa gauche, y vers le haut, z vers l'avant
        let up = view.feet.normalize_or_zero();
        let (east, north) = tangent_frame(up);
        let forward = north * view.yaw.cos() + east * view.yaw.sin();
        let left = up.cross(forward);
        let scale = block / model.units_per_block;
        let to_world = |p: Vec3| view.feet + (left * p.x + up * p.y + forward * p.z) * scale;
        for part in &model.parts {
            let angle = part.swing.map_or(0.0, |s| s.amplitude.to_radians() * (TAU * (phase + s.phase)).sin() * amount);
            push_part(&mut vertices, &mut indices, part, angle, [left, up, forward], to_world);
        }
    }
    (vertices, indices)
}

/// Ajoute un pavé du modèle, tourné de `angle` autour de l'axe gauche-droite passant par son pivot
fn push_part(vertices: &mut Vec<PlanetVertex>, indices: &mut Vec<u32>, part: &ModelPart, angle: f32, axes: [Vec3; 3], to_world: impl Fn(Vec3) -> Vec3) {
    let (min, max) = (Vec3::from(part.from).min(Vec3::from(part.to)), Vec3::from(part.from).max(Vec3::from(part.to)));
    let pivot = Vec3::from(part.pivot_point());
    let (sin, cos) = angle.sin_cos();
    // Rotation autour de x : la pièce pique du nez (angle positif) ou se relève
    let rotate = |v: Vec3| Vec3::new(v.x, v.y * cos - v.z * sin, v.y * sin + v.z * cos);
    let corners: [Vec3; 8] = std::array::from_fn(|i| {
        let p = Vec3::new(
            if i & 1 == 0 { min.x } else { max.x },
            if i & 2 == 0 { min.y } else { max.y },
            if i & 4 == 0 { min.z } else { max.z },
        );
        to_world(pivot + rotate(p - pivot))
    });
    let color = [part.color.0[0], part.color.0[1], part.color.0[2], 1.0];
    for (face, axis, sign) in FACES {
        let local = rotate(Vec3::AXES[axis] * sign);
        let normal = (axes[0] * local.x + axes[1] * local.y + axes[2] * local.z).normalize_or_zero();
        let base = vertices.len() as u32;
        for corner in face {
            vertices.push(PlanetVertex { position: corners[corner].to_array(), normal: normal.to_array(), color });
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> CreatureRegistry {
        CreatureRegistry::load_default()
    }

    #[test]
    fn test_model_stands_on_the_feet_and_faces_its_heading() {
        let registry = registry();
        let sheep = registry.find("mouton").unwrap();
        let feet = Vec3::new(0.0, 0.0, 1000.0);
        let (east, north) = tangent_frame(feet.normalize());
        let animator = CreatureAnimator::default();
        for (yaw, heading) in [(0.0, north), (std::f32::consts::FRAC_PI_2, east)] {
            let view = CreatureView { id: 1, species: Some(sheep), feet, yaw };
            let (vertices, indices) = build_mesh(&[view], &registry, &animator, 1.0);
            let parts = registry.get(sheep).unwrap().model.parts.len();
            assert_eq!(vertices.len(), parts * 24);
            assert_eq!(indices.len(), parts * 36);
            let points: Vec<Vec3> = vertices.iter().map(|v| Vec3::from(v.position)).collect();
            // Les pattes touchent le sol, rien n'est dessous
            let lowest = points.iter().map(|p| p.length() - feet.length()).fold(f32::MAX, f32::min);
            assert!(lowest.abs() < 1e-3, "{lowest}");
            // La tête (le point le plus en avant) est du côté du cap
            let front = points.iter().map(|p| (*p - feet).dot(heading)).fold(f32::MIN, f32::max);
            let back = points.iter().map(|p| (*p - feet).dot(heading)).fold(f32::MAX, f32::min);
            assert!(front > -back, "tête à {front}, queue à {back}");
        }
    }

    #[test]
    fn test_legs_swing_only_while_walking() {
        let registry = registry();
        let boar = registry.find("sanglier").unwrap();
        let feet = Vec3::new(0.0, 0.0, 1000.0);
        let (_, north) = tangent_frame(feet.normalize());
        let mut animator = CreatureAnimator::default();
        let mut view = CreatureView { id: 7, species: Some(boar), feet, yaw: 0.0 };
        let still = build_mesh(&[view], &registry, &animator, 1.0).0;

        // Immobile : le modèle reste tel quel
        animator.update(&[view], &registry, 0.05, 1.0);
        animator.update(&[view], &registry, 0.05, 1.0);
        let idle = build_mesh(&[view], &registry, &animator, 1.0).0;
        assert!(still.iter().zip(&idle).all(|(a, b)| Vec3::from(a.position).distance(Vec3::from(b.position)) < 1e-4));

        // En marche : les pattes se balancent
        for _ in 0..20 {
            view.feet += north * 0.07;
            animator.update(&[view], &registry, 0.05, 1.0);
        }
        let walking = build_mesh(&[view], &registry, &animator, 1.0).0;
        let shift = view.feet - feet;
        let moved = still.iter().zip(&walking).map(|(a, b)| (Vec3::from(a.position) + shift).distance(Vec3::from(b.position))).fold(0.0, f32::max);
        assert!(moved > 0.02, "aucune pièce ne bouge ({moved})");
    }

    #[test]
    fn test_unknown_species_gets_a_placeholder() {
        let view = CreatureView { id: 3, species: None, feet: Vec3::new(0.0, 1000.0, 0.0), yaw: 1.0 };
        let (vertices, _) = build_mesh(&[view], &CreatureRegistry::default(), &CreatureAnimator::default(), 1.0);
        assert_eq!(vertices.len(), 24);
    }
}
