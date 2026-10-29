//! Modèles 3D des créatures. Chaque espèce est dessinée avec le modèle glTF à squelette et la
//! texture que cite sa description (`assets/creatures/*.json`). Son animation de marche est jouée
//! à la vitesse à laquelle la bête avance vraiment, et se fige quand elle s'arrête.
//!
//! Les os sont appliqués ici, sur le processeur (quelques centaines de sommets par bête) : le
//! maillage de toutes les créatures est refait à chaque image, puis dessiné par
//! `creature_render` avec l'éclairage et les ombres du relief.
//!
//! En ligne, les créatures viennent du serveur ; en solo, la simulation `CreatureWorld` tourne
//! dans le jeu. Les deux donnent les mêmes `CreatureView`. Une bête frappée rougit, une bête morte
//! bascule sur le flanc ; `aimed_creature` dit laquelle est sous le réticule.

use std::collections::HashMap;
use std::ops::Range;

use glam::Vec3;
use herocraft_common::{tangent_frame, CreatureRegistry, CreatureState};
use log::warn;

use crate::planet_mesh::PlanetVertex;
use crate::skinned_model::{skin_normal, skin_point, SkinnedModel, Texture};

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
    /// Coup reçu : 1 juste après, 0 une fois passé
    pub hurt: f32,
    /// Mort : 0 vivante, puis de 0 à 1 pendant qu'elle bascule sur le flanc
    pub death: f32,
}

impl CreatureView {
    pub fn new(species: Option<usize>, state: &CreatureState) -> Self {
        Self { id: state.creature_id, species, feet: state.position, yaw: state.yaw, hurt: state.hurt, death: state.death }
    }
}

/// Rouge d'une bête qui vient d'être frappée, multiplié à sa texture
const HURT_TINT: [f32; 3] = [1.0, 0.35, 0.35];

/// Rapidité avec laquelle la vitesse mesurée suit la vraie, par seconde : les positions arrivent
/// au rythme du réseau, pas des images
const SPEED_SMOOTHING: f32 = 6.0;
/// Sous cette fraction de son allure de marche, la bête est à l'arrêt : l'animation se fige
const STILL_RATIO: f32 = 0.1;
/// L'animation ne va jamais plus vite que ça fois son rythme normal
const MAX_RATE: f32 = 3.5;
/// Côté du pavé gris d'une espèce sans modèle, en blocs
const PLACEHOLDER_SIZE: f32 = 0.6;

/// Les modèles des espèces du registre, dans son ordre (`None` : modèle illisible)
pub struct CreatureModels {
    models: Vec<Option<SkinnedModel>>,
}

impl CreatureModels {
    /// Lit le modèle et la texture de chaque espèce. Les textures, dans le même ordre, sont pour le
    /// rendu ; un fichier illisible est signalé, l'espèce est alors dessinée en pavé gris.
    pub fn load(registry: &CreatureRegistry) -> (Self, Vec<Option<Texture>>) {
        let mut models = Vec::new();
        let mut textures = Vec::new();
        for species in registry.iter() {
            let model = SkinnedModel::load(&registry.asset_path(&species.model.mesh));
            let texture = Texture::load(&registry.asset_path(&species.model.texture));
            match (model, texture) {
                (Ok(model), Ok(texture)) => {
                    if model.animation(&species.model.walk_animation).is_none() {
                        warn!("Créature {} : pas d'animation « {} », elle ne bougera pas les pattes.", species.id, species.model.walk_animation);
                    }
                    models.push(Some(model));
                    textures.push(Some(texture));
                }
                (Err(e), _) | (_, Err(e)) => {
                    warn!("Créature {} : modèle ou texture illisible ({e}), dessinée en pavé gris.", species.id);
                    models.push(None);
                    textures.push(None);
                }
            }
        }
        (Self { models }, textures)
    }

    fn get(&self, species: Option<usize>) -> Option<&SkinnedModel> {
        self.models.get(species?)?.as_ref()
    }
}

/// L'allure de chaque créature : vitesse mesurée et instant de son animation de marche
#[derive(Default)]
pub struct CreatureAnimator {
    gaits: HashMap<u64, Gait>,
}

struct Gait {
    last_feet: Vec3,
    /// Vitesse lissée, en blocs par seconde
    speed: f32,
    /// Instant dans l'animation, en secondes
    time: f32,
}

impl CreatureAnimator {
    /// Avance l'animation de chaque créature selon le chemin parcouru depuis l'image précédente
    /// (`dt` secondes) ; `block` : taille d'un bloc
    pub fn update(&mut self, views: &[CreatureView], registry: &CreatureRegistry, models: &CreatureModels, dt: f32, block: f32) {
        self.gaits.retain(|id, _| views.iter().any(|v| v.id == *id));
        for view in views {
            let gait = self.gaits.entry(view.id).or_insert(Gait { last_feet: view.feet, speed: 0.0, time: 0.0 });
            let up = view.feet.normalize_or_zero();
            let step = view.feet - gait.last_feet;
            let moved = (step - up * step.dot(up)).length() / block;
            gait.last_feet = view.feet;
            if dt <= 0.0 {
                continue;
            }
            gait.speed += (moved / dt - gait.speed) * (SPEED_SMOOTHING * dt).min(1.0);
            let species = view.species.and_then(|s| registry.get(s));
            let nominal = species.map_or(1.0, |s| s.walk_speed).max(0.1);
            let ratio = gait.speed / nominal;
            let rate = if ratio < STILL_RATIO { 0.0 } else { ratio.min(MAX_RATE) };
            let duration = species
                .zip(models.get(view.species))
                .and_then(|(s, m)| m.animation(&s.model.walk_animation))
                .map_or(1.0, |a| a.duration.max(1e-3));
            gait.time = (gait.time + dt * rate) % duration;
        }
    }

    fn time(&self, id: u64) -> f32 {
        self.gaits.get(&id).map_or(0.0, |g| g.time)
    }
}

/// Le maillage des créatures d'une image : sommets (comme le relief), coordonnées de texture, et
/// une plage d'indices par espèce
#[derive(Default)]
pub struct CreatureFrame {
    pub vertices: Vec<PlanetVertex>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
    /// Plages d'indices, chacune avec la texture de son espèce (`None` : pavés gris, sans texture)
    pub draws: Vec<(Option<usize>, Range<u32>)>,
}

/// Construit le maillage de toutes les créatures ; `block` : taille d'un bloc
pub fn build_frame(views: &[CreatureView], registry: &CreatureRegistry, models: &CreatureModels, animator: &CreatureAnimator, block: f32) -> CreatureFrame {
    let mut frame = CreatureFrame::default();
    let mut views = views.to_vec();
    // Une espèce sans modèle passe avec les pavés gris : une plage par texture
    let texture_of = |v: &CreatureView| v.species.filter(|_| models.get(v.species).is_some());
    views.sort_by_key(|v| texture_of(v));
    for view in &views {
        let texture = texture_of(view);
        let start = frame.indices.len() as u32;
        // Repère de la créature : x vers sa gauche, y vers le haut, z vers l'avant
        let up = view.feet.normalize_or_zero();
        let (east, north) = tangent_frame(up);
        let forward = north * view.yaw.cos() + east * view.yaw.sin();
        let left = up.cross(forward);
        // Morte, elle bascule sur le flanc autour de son axe de la tête à la queue
        let roll = view.death.clamp(0.0, 1.0) * std::f32::consts::FRAC_PI_2;
        let (left, up) = (left * roll.cos() - up * roll.sin(), up * roll.cos() + left * roll.sin());
        let hurt = view.hurt.clamp(0.0, 1.0);
        let tint = [0, 1, 2].map(|i| 1.0 + (HURT_TINT[i] - 1.0) * hurt);
        let to_world = |p: Vec3| view.feet + (left * p.x + up * p.y + forward * p.z) * block;
        let rotate = |n: Vec3| (left * n.x + up * n.y + forward * n.z).normalize_or_zero();

        match (models.get(texture), texture.and_then(|s| registry.get(s))) {
            (Some(model), Some(species)) => {
                let animation = model.animation(&species.model.walk_animation);
                let matrices = model.skin_matrices(animation, animator.time(view.id));
                let base = frame.vertices.len() as u32;
                for v in &model.vertices {
                    let p = skin_point(&matrices, v, v.position) + Vec3::Y * model.feet_height;
                    frame.vertices.push(PlanetVertex { position: to_world(p).to_array(), normal: rotate(skin_normal(&matrices, v)).to_array(), color: [tint[0], tint[1], tint[2], 1.0] });
                    frame.uvs.push(v.uv);
                }
                frame.indices.extend(model.indices.iter().map(|i| base + i));
            }
            _ => push_placeholder(&mut frame, to_world, rotate, tint),
        }

        let end = frame.indices.len() as u32;
        match frame.draws.last_mut() {
            Some((last, range)) if *last == texture && range.end == start => range.end = end,
            _ => frame.draws.push((texture, start..end)),
        }
    }
    frame
}

/// Créature vivante sous le réticule : la première que touche le rayon parti de `eye` dans la
/// direction `dir` (unitaire), à moins de `max_distance`. Renvoie son identifiant et la distance.
/// Chaque bête est une boîte de son corps (`CreatureSpecies::body`), tournée selon son cap ; `block`
/// : taille d'un bloc.
pub fn aimed_creature(views: &[CreatureView], registry: &CreatureRegistry, eye: Vec3, dir: Vec3, max_distance: f32, block: f32) -> Option<(u64, f32)> {
    views
        .iter()
        .filter(|v| v.death <= 0.0)
        .filter_map(|view| {
            let body = view.species.and_then(|s| registry.get(s)).map(|s| s.body);
            let (half_width, half_length, height) = body.map_or((PLACEHOLDER_SIZE * 0.5, PLACEHOLDER_SIZE * 0.5, PLACEHOLDER_SIZE), |b| (b.half_width, b.half_length, b.height));
            let up = view.feet.normalize_or_zero();
            let (east, north) = tangent_frame(up);
            let forward = north * view.yaw.cos() + east * view.yaw.sin();
            let left = up.cross(forward);
            // Le rayon dans le repère de la bête, en blocs, pieds à l'origine
            let o = (eye - view.feet) / block;
            let (o, d) = (Vec3::new(o.dot(left), o.dot(up), o.dot(forward)), Vec3::new(dir.dot(left), dir.dot(up), dir.dot(forward)));
            let (lo, hi) = (Vec3::new(-half_width, 0.0, -half_length), Vec3::new(half_width, height, half_length));
            let (mut near, mut far) = (0.0_f32, max_distance / block);
            for axis in 0..3 {
                if d[axis].abs() < 1e-6 {
                    if o[axis] < lo[axis] || o[axis] > hi[axis] {
                        return None;
                    }
                    continue;
                }
                let (a, b) = ((lo[axis] - o[axis]) / d[axis], (hi[axis] - o[axis]) / d[axis]);
                near = near.max(a.min(b));
                far = far.min(a.max(b));
            }
            (near <= far).then_some((view.id, near * block))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
}

/// Pavé gris d'une espèce sans modèle
fn push_placeholder(frame: &mut CreatureFrame, to_world: impl Fn(Vec3) -> Vec3, rotate: impl Fn(Vec3) -> Vec3, tint: [f32; 3]) {
    const FACES: [([usize; 4], Vec3); 6] = [
        ([0, 4, 6, 2], Vec3::NEG_X),
        ([1, 3, 7, 5], Vec3::X),
        ([0, 1, 5, 4], Vec3::NEG_Y),
        ([2, 6, 7, 3], Vec3::Y),
        ([0, 2, 3, 1], Vec3::NEG_Z),
        ([4, 5, 7, 6], Vec3::Z),
    ];
    let h = PLACEHOLDER_SIZE * 0.5;
    let corners: [Vec3; 8] = std::array::from_fn(|i| {
        Vec3::new(if i & 1 == 0 { -h } else { h }, if i & 2 == 0 { 0.0 } else { PLACEHOLDER_SIZE }, if i & 4 == 0 { -h } else { h })
    });
    for (face, normal) in FACES {
        let base = frame.vertices.len() as u32;
        for corner in face {
            frame.vertices.push(PlanetVertex { position: to_world(corners[corner]).to_array(), normal: rotate(normal).to_array(), color: [0.5 * tint[0], 0.5 * tint[1], 0.5 * tint[2], 1.0] });
            frame.uvs.push([0.0, 0.0]);
        }
        frame.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (CreatureRegistry, CreatureModels) {
        let registry = CreatureRegistry::load_default();
        let (models, textures) = CreatureModels::load(&registry);
        assert!(textures.iter().all(Option::is_some));
        (registry, models)
    }

    #[test]
    fn test_model_stands_on_the_feet_and_faces_its_heading() {
        let (registry, models) = setup();
        let boar = registry.find("sanglier").unwrap();
        let feet = Vec3::new(0.0, 0.0, 1000.0);
        let (east, north) = tangent_frame(feet.normalize());
        let animator = CreatureAnimator::default();
        for (yaw, heading) in [(0.0, north), (std::f32::consts::FRAC_PI_2, east)] {
            let view = CreatureView { id: 1, species: Some(boar), feet, yaw, hurt: 0.0, death: 0.0 };
            let frame = build_frame(&[view], &registry, &models, &animator, 1.0);
            assert_eq!(frame.draws, vec![(Some(boar), 0..frame.indices.len() as u32)]);
            assert_eq!(frame.vertices.len(), frame.uvs.len());
            let points: Vec<Vec3> = frame.vertices.iter().map(|v| Vec3::from(v.position)).collect();
            // Les pattes touchent le sol, rien n'est dessous
            let lowest = points.iter().map(|p| p.length() - feet.length()).fold(f32::MAX, f32::min);
            assert!(lowest.abs() < 1e-3, "{lowest}");
            // Le groin (le point le plus en avant) est du côté du cap
            let front = points.iter().map(|p| (*p - feet).dot(heading)).fold(f32::MIN, f32::max);
            let back = points.iter().map(|p| (*p - feet).dot(heading)).fold(f32::MAX, f32::min);
            assert!(front > -back, "tête à {front}, queue à {back}");
        }
    }

    #[test]
    fn test_legs_move_only_while_walking() {
        let (registry, models) = setup();
        let boar = registry.find("sanglier").unwrap();
        let feet = Vec3::new(0.0, 0.0, 1000.0);
        let (_, north) = tangent_frame(feet.normalize());
        let mut animator = CreatureAnimator::default();
        let mut view = CreatureView { id: 7, species: Some(boar), feet, yaw: 0.0, hurt: 0.0, death: 0.0 };
        let mesh = |animator: &CreatureAnimator, view: CreatureView| build_frame(&[view], &registry, &models, animator, 1.0).vertices;
        let still = mesh(&animator, view);

        // Immobile : la pose ne change pas
        for _ in 0..10 {
            animator.update(&[view], &registry, &models, 0.05, 1.0);
        }
        let idle = mesh(&animator, view);
        assert!(still.iter().zip(&idle).all(|(a, b)| Vec3::from(a.position).distance(Vec3::from(b.position)) < 1e-4));

        // En marche, à son allure : l'animation avance
        let speed = registry.get(boar).unwrap().walk_speed;
        for _ in 0..20 {
            view.feet += north * speed * 0.05;
            animator.update(&[view], &registry, &models, 0.05, 1.0);
        }
        let walking = mesh(&animator, view);
        let shift = view.feet - feet;
        let moved = still.iter().zip(&walking).map(|(a, b)| (Vec3::from(a.position) + shift).distance(Vec3::from(b.position))).fold(0.0, f32::max);
        assert!(moved > 0.02, "rien ne bouge ({moved})");
    }

    #[test]
    fn test_unknown_species_gets_a_placeholder() {
        let (registry, models) = setup();
        let views = [
            CreatureView { id: 3, species: None, feet: Vec3::new(0.0, 1000.0, 0.0), yaw: 1.0, hurt: 0.0, death: 0.0 },
            CreatureView { id: 4, species: registry.find("lapin"), feet: Vec3::new(0.0, 1000.0, 0.0), yaw: 1.0, hurt: 0.0, death: 0.0 },
        ];
        let frame = build_frame(&views, &registry, &models, &CreatureAnimator::default(), 1.0);
        assert_eq!(frame.draws.len(), 2);
        assert_eq!(frame.draws[0].0, None);
        assert_eq!(frame.draws[0].1, 0..36);
    }

    #[test]
    fn test_hurt_creature_turns_red_and_dead_one_lies_on_its_side() {
        let (registry, models) = setup();
        let cow = registry.find("vache").unwrap();
        let feet = Vec3::new(0.0, 0.0, 1000.0);
        let animator = CreatureAnimator::default();
        let view = CreatureView { id: 1, species: Some(cow), feet, yaw: 0.0, hurt: 0.0, death: 0.0 };
        let frame = |view: CreatureView| build_frame(&[view], &registry, &models, &animator, 1.0).vertices;
        assert!(frame(view).iter().all(|v| v.color == [1.0; 4]));
        assert!(frame(CreatureView { hurt: 1.0, ..view }).iter().all(|v| v.color[0] == 1.0 && v.color[1] < 0.5));

        // Debout, la bête est plus haute que large ; couchée, sa hauteur passe sur le côté
        let height = |vertices: Vec<PlanetVertex>| vertices.iter().map(|v| Vec3::from(v.position).z - feet.z).fold(0.0, f32::max);
        let species = registry.get(cow).unwrap();
        assert!(height(frame(view)) > species.body.half_width * 2.0);
        assert!(height(frame(CreatureView { death: 1.0, ..view })) < species.body.height * 0.75);
    }

    #[test]
    fn test_crosshair_aims_at_the_nearest_living_creature() {
        let (registry, _) = setup();
        let boar = registry.find("sanglier").unwrap();
        let feet = Vec3::new(0.0, 0.0, 1000.0);
        let (_, north) = tangent_frame(feet.normalize());
        let near = CreatureView { id: 1, species: Some(boar), feet: feet + north * 3.0, yaw: 1.0, hurt: 0.0, death: 0.0 };
        let far = CreatureView { id: 2, feet: feet + north * 5.0, ..near };
        let eye = feet + Vec3::Z * 0.4;
        assert_eq!(aimed_creature(&[far, near], &registry, eye, north, 6.0, 1.0).map(|(id, _)| id), Some(1));
        let (_, distance) = aimed_creature(&[near], &registry, eye, north, 6.0, 1.0).unwrap();
        assert!(distance > 2.0 && distance < 3.0, "{distance}");
        // Trop loin, à côté, ou déjà morte : rien n'est visé
        assert!(aimed_creature(&[near], &registry, eye, north, 2.0, 1.0).is_none());
        assert!(aimed_creature(&[near], &registry, eye, -north, 6.0, 1.0).is_none());
        let dead = CreatureView { death: 0.5, ..near };
        assert_eq!(aimed_creature(&[dead, far], &registry, eye, north, 6.0, 1.0).map(|(id, _)| id), Some(2));
    }
}

#[cfg(test)]
mod apercu {
    use super::*;
    use crate::skinned_model::Texture;

    /// Aperçu logiciel (z-buffer, texels au plus proche), hors jeu
    #[test]
    #[ignore]
    fn apercu_creatures() {
        let registry = CreatureRegistry::load_default();
        let (models, textures) = CreatureModels::load(&registry);
        let (cols, cell) = (5usize, 220usize);
        let rows = registry.len().div_ceil(cols);
        let (w, h) = (cols * cell, rows * cell);
        let mut img = vec![[200u8, 222, 236]; w * h];
        let mut depth = vec![f32::MAX; w * h];
        let feet = Vec3::new(0.0, 0.0, 1000.0);
        let mut animator = CreatureAnimator::default();
        for (i, species) in registry.iter().enumerate() {
            let view = CreatureView { id: i as u64, species: Some(i), feet, yaw: 0.6, hurt: 0.0, death: 0.0 };
            // Un peu de marche pour une pose animée
            let mut v = view;
            let (_, north) = tangent_frame(feet.normalize());
            for _ in 0..7 {
                v.feet += north * species.walk_speed.max(0.5) * 0.05;
                animator.update(&[v], &registry, &models, 0.05, 1.0);
            }
            let frame = build_frame(&[CreatureView { feet, ..v }], &registry, &models, &animator, 1.0);
            let tex: &Texture = textures[i].as_ref().unwrap();
            let scale = cell as f32 * 0.8 / (species.body.height.max(species.body.half_length * 2.0) * 1.6).max(0.5);
            let (ox, oy) = ((i % cols) * cell, (i / cols) * cell);
            // Vue de trois quarts : x = est, y = haut (z monde), profondeur = nord, inclinée
            let (ca, sa) = (0.5f32.cos(), 0.5f32.sin());
            let project = |p: Vec3| {
                let q = p - feet;
                let (x, up, n) = (q.x, q.z, q.y);
                let (y, d) = (up * ca - n * sa, up * sa + n * ca);
                (ox as f32 + cell as f32 * 0.5 + x * scale, oy as f32 + cell as f32 * 0.85 - y * scale, d)
            };
            let light = Vec3::new(0.4, 0.5, 0.75).normalize();
            for t in frame.indices.chunks(3) {
                let ids = [t[0] as usize, t[1] as usize, t[2] as usize];
                let p: Vec<(f32, f32, f32)> = ids.iter().map(|&k| project(Vec3::from(frame.vertices[k].position))).collect();
                let n = Vec3::from(frame.vertices[ids[0]].normal);
                let shade = 0.55 + 0.45 * Vec3::new(n.x, n.z, n.y).dot(light).abs();
                let (minx, maxx) = (p.iter().map(|a| a.0).fold(f32::MAX, f32::min).max(ox as f32) as usize, p.iter().map(|a| a.0).fold(f32::MIN, f32::max).min((ox + cell - 1) as f32) as usize);
                let (miny, maxy) = (p.iter().map(|a| a.1).fold(f32::MAX, f32::min).max(oy as f32) as usize, p.iter().map(|a| a.1).fold(f32::MIN, f32::max).min((oy + cell - 1) as f32) as usize);
                let area = (p[1].0 - p[0].0) * (p[2].1 - p[0].1) - (p[2].0 - p[0].0) * (p[1].1 - p[0].1);
                if area.abs() < 1e-6 {
                    continue;
                }
                for y in miny..=maxy {
                    for x in minx..=maxx {
                        let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                        let w0 = ((p[1].0 - px) * (p[2].1 - py) - (p[2].0 - px) * (p[1].1 - py)) / area;
                        let w1 = ((p[2].0 - px) * (p[0].1 - py) - (p[0].0 - px) * (p[2].1 - py)) / area;
                        let w2 = 1.0 - w0 - w1;
                        if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                            continue;
                        }
                        let d = w0 * p[0].2 + w1 * p[1].2 + w2 * p[2].2;
                        if d >= depth[y * w + x] {
                            continue;
                        }
                        let uv = [0, 1].map(|c| w0 * frame.uvs[ids[0]][c] + w1 * frame.uvs[ids[1]][c] + w2 * frame.uvs[ids[2]][c]);
                        let tx = ((uv[0] * tex.width as f32) as u32).min(tex.width - 1);
                        let ty = ((uv[1] * tex.height as f32) as u32).min(tex.height - 1);
                        let o = ((ty * tex.width + tx) * 4) as usize;
                        if tex.rgba[o + 3] < 128 {
                            continue;
                        }
                        depth[y * w + x] = d;
                        img[y * w + x] = [0, 1, 2].map(|c| (tex.rgba[o + c] as f32 * shade).min(255.0) as u8);
                    }
                }
            }
        }
        let out = std::env::var("APERCU").unwrap_or("apercu.png".into());
        let file = std::fs::File::create(out).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
        enc.set_color(png::ColorType::Rgb);
        enc.write_header().unwrap().write_image_data(&img.concat()).unwrap();
    }
}
