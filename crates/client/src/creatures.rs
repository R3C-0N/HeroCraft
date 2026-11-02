//! Modèles 3D des créatures. Chaque espèce est dessinée avec le modèle glTF à squelette et la
//! texture que cite sa description (`assets/creatures/*.json`).
//!
//! Les animations aussi viennent de la fiche : pour chacune (`repos`, `marche`, `course`,
//! `attaque`, `coup`, et les inactivités comme `broute` ou `sommeil`), les os qui oscillent autour
//! d'une pose. L'action que diffuse la simulation (`CreatureAction`) choisit l'animation ; la marche
//! et la course avancent au rythme du chemin parcouru, pour que les pattes ne glissent pas, et l'on
//! passe d'une animation à l'autre en fondu.
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

use glam::{EulerRot, Quat, Vec3};
use herocraft_common::{tangent_frame, CreatureAction, CreatureRegistry, CreatureSpecies, CreatureState};
use log::warn;

use crate::planet_mesh::PlanetVertex;
use crate::skinned_model::{skin_normal, skin_point, Animation, SkinnedModel, Texture};

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
    /// Ce qu'elle fait, pour choisir son animation
    pub action: CreatureAction,
}

impl CreatureView {
    pub fn new(species: Option<usize>, state: &CreatureState) -> Self {
        Self { id: state.creature_id, species, feet: state.position, yaw: state.yaw, hurt: state.hurt, death: state.death, action: state.action }
    }
}

/// Rouge d'une bête qui vient d'être frappée, multiplié à sa texture
const HURT_TINT: [f32; 3] = [1.0, 0.35, 0.35];

/// Rapidité avec laquelle la vitesse mesurée suit la vraie, par seconde : les positions arrivent
/// au rythme du réseau, pas des images
const SPEED_SMOOTHING: f32 = 6.0;
/// Sous cette fraction de son allure de marche, la bête est à l'arrêt : elle quitte la marche
const STILL_RATIO: f32 = 0.1;
/// Durée du fondu d'une animation à la suivante, en secondes
const BLEND_TIME: f32 = 0.3;
/// Côté du pavé gris d'une espèce sans modèle, en blocs
const PLACEHOLDER_SIZE: f32 = 0.6;

/// Mouvement d'un os de la fiche, son nœud trouvé dans le modèle, angles en radians
#[derive(Debug, Clone, Copy)]
struct Motion {
    node: usize,
    rotation: Vec3,
    rotation_fixe: Vec3,
    translation: Vec3,
    translation_fixe: Vec3,
    phase: f32,
    frequence: f32,
}

/// Une animation de la fiche, prête à jouer sur son modèle
#[derive(Debug, Clone)]
struct RigAnimation {
    /// Animation du glTF jouée dessous
    clip: Option<String>,
    cycle: f32,
    foulee: Option<f32>,
    motions: Vec<Motion>,
}

/// Le modèle d'une espèce et ses animations
struct Rig {
    model: SkinnedModel,
    animations: HashMap<String, RigAnimation>,
}

impl Rig {
    /// Associe les animations de la fiche aux os du modèle. Renvoie aussi les os de la fiche que le
    /// modèle n'a pas. Une fiche sans animations joue l'animation `repos` du glTF en marchant,
    /// comme le bestiaire.
    fn new(model: SkinnedModel, species: &CreatureSpecies) -> (Self, Vec<String>) {
        let mut animations = HashMap::new();
        let mut missing = Vec::new();
        for (name, animation) in &species.model.animations {
            let mut motions = Vec::new();
            for (pattern, m) in &animation.os {
                let nodes = model.nodes_matching(pattern);
                if nodes.is_empty() {
                    missing.push(format!("{name}/{pattern}"));
                }
                let radians = |a: [f32; 3]| Vec3::from(a.map(f32::to_radians));
                motions.extend(nodes.into_iter().enumerate().map(|(k, node)| Motion {
                    node,
                    rotation: radians(m.rotation),
                    rotation_fixe: radians(m.rotation_fixe),
                    translation: Vec3::from(m.translation),
                    translation_fixe: Vec3::from(m.translation_fixe),
                    phase: m.phase + k as f32 * m.decalage,
                    frequence: m.frequence,
                }));
            }
            if let Some(clip) = animation.clip.as_ref().filter(|c| model.animation(c).is_none()) {
                missing.push(format!("{name}/clip {clip}"));
            }
            animations.insert(name.clone(), RigAnimation { clip: animation.clip.clone(), cycle: animation.cycle, foulee: animation.foulee, motions });
        }
        if animations.is_empty() {
            if let Some(clip) = model.animation("repos") {
                let cycle = clip.duration.max(1e-3);
                let foulee = Some(species.walk_speed.max(0.1) * cycle);
                animations.insert("marche".into(), RigAnimation { clip: Some("repos".into()), cycle, foulee, motions: Vec::new() });
            }
        }
        (Self { model, animations }, missing)
    }

    /// Animation `name` de la fiche, ou celle qui la remplace : la course faute de mieux devient la
    /// marche, le reste le repos (`None` : la pose de repos du modèle)
    fn resolve<'a>(&self, name: &'a str) -> Option<&'a str> {
        let fallbacks: &[&'a str] = match name {
            "course" => &[name, "marche", "repos"],
            _ => &[name, "repos"],
        };
        fallbacks.iter().copied().find(|n| self.animations.contains_key(*n))
    }

    /// Ajoute à `pose` (angles d'Euler et déplacement de chaque nœud) l'animation `name` à la
    /// fraction `phase` de son cycle, avec le poids `weight`
    fn accumulate(&self, name: &str, phase: f32, weight: f32, pose: &mut [(Vec3, Vec3)]) {
        let Some(animation) = self.animations.get(name) else { return };
        for m in &animation.motions {
            let wave = (std::f32::consts::TAU * (phase * m.frequence + m.phase)).sin();
            let (angles, offset) = &mut pose[m.node];
            *angles += (m.rotation_fixe + m.rotation * wave) * weight;
            *offset += (m.translation_fixe + m.translation * wave) * weight;
        }
    }

    /// Animation du glTF de l'animation `name`, et l'instant à jouer pour la fraction `phase`
    fn clip(&self, name: &str, phase: f32) -> Option<(&Animation, f32)> {
        let clip = self.model.animation(self.animations.get(name)?.clip.as_ref()?)?;
        Some((clip, phase.rem_euclid(1.0) * clip.duration))
    }
}

/// Les modèles des espèces du registre, dans son ordre (`None` : modèle illisible)
pub struct CreatureModels {
    rigs: Vec<Option<Rig>>,
}

impl CreatureModels {
    /// Lit le modèle et la texture de chaque espèce. Les textures, dans le même ordre, sont pour le
    /// rendu ; un fichier illisible est signalé, l'espèce est alors dessinée en pavé gris.
    pub fn load(registry: &CreatureRegistry) -> (Self, Vec<Option<Texture>>) {
        let mut rigs = Vec::new();
        let mut textures = Vec::new();
        for species in registry.iter() {
            let model = SkinnedModel::load(&registry.asset_path(&species.model.mesh));
            let texture = Texture::load(&registry.asset_path(&species.model.texture));
            match (model, texture) {
                (Ok(model), Ok(texture)) => {
                    let (rig, missing) = Rig::new(model, species);
                    if !missing.is_empty() {
                        warn!("Créature {} : os ou animations inconnus du modèle, ignorés : {}.", species.id, missing.join(", "));
                    }
                    rigs.push(Some(rig));
                    textures.push(Some(texture));
                }
                (Err(e), _) | (_, Err(e)) => {
                    warn!("Créature {} : modèle ou texture illisible ({e}), dessinée en pavé gris.", species.id);
                    rigs.push(None);
                    textures.push(None);
                }
            }
        }
        (Self { rigs }, textures)
    }

    fn get(&self, species: Option<usize>) -> Option<&Rig> {
        self.rigs.get(species?)?.as_ref()
    }
}

/// Pose d'une créature : l'animation du glTF jouée dessous et son instant, puis la rotation et le
/// déplacement ajoutés à chaque nœud
type Pose<'r> = (Option<(&'r Animation, f32)>, Vec<(Quat, Vec3)>);

/// L'allure de chaque créature : vitesse mesurée, animation jouée et celle qu'elle quitte
#[derive(Default)]
pub struct CreatureAnimator {
    gaits: HashMap<u64, Gait>,
}

struct Gait {
    last_feet: Vec3,
    /// Vitesse lissée, en blocs par seconde
    speed: f32,
    /// Animation jouée et fraction de son cycle
    current: String,
    phase: f32,
    /// Animation quittée, qui s'efface pendant le fondu, et fraction de son cycle
    previous: Option<(String, f32)>,
    /// Avancée du fondu, de 0 à 1
    blend: f32,
}

impl CreatureAnimator {
    /// Choisit et avance l'animation de chaque créature selon son action et le chemin parcouru
    /// depuis l'image précédente (`dt` secondes) ; `block` : taille d'un bloc
    pub fn update(&mut self, views: &[CreatureView], registry: &CreatureRegistry, models: &CreatureModels, dt: f32, block: f32) {
        self.gaits.retain(|id, _| views.iter().any(|v| v.id == *id));
        for view in views {
            let gait = self.gaits.entry(view.id).or_insert(Gait { last_feet: view.feet, speed: 0.0, current: String::new(), phase: 0.0, previous: None, blend: 1.0 });
            let up = view.feet.normalize_or_zero();
            let step = view.feet - gait.last_feet;
            let moved = (step - up * step.dot(up)).length() / block;
            gait.last_feet = view.feet;
            if dt <= 0.0 {
                continue;
            }
            gait.speed += (moved / dt - gait.speed) * (SPEED_SMOOTHING * dt).min(1.0);
            let (Some(species), Some(rig)) = (view.species.and_then(|s| registry.get(s)), models.get(view.species)) else { continue };

            let wanted = wanted_animation(view, species, rig, gait.speed);
            if wanted != gait.current {
                // De la marche à la course, les pattes gardent leur pas
                let stride = |name: &str| rig.animations.get(name).is_some_and(|a| a.foulee.is_some());
                let keep = stride(&gait.current) && stride(wanted);
                let phase = if keep { gait.phase } else { 0.0 };
                gait.previous = (!gait.current.is_empty()).then(|| (std::mem::take(&mut gait.current), gait.phase));
                gait.current = wanted.to_string();
                gait.phase = phase;
                gait.blend = if gait.previous.is_some() { 0.0 } else { 1.0 };
            }
            let advance = |name: &str, phase: f32| {
                let Some(a) = rig.animations.get(name) else { return phase };
                let step = match a.foulee {
                    Some(foulee) => moved / foulee,
                    None => dt / a.cycle,
                };
                (phase + step).rem_euclid(1.0)
            };
            gait.phase = advance(&gait.current, gait.phase);
            gait.blend = (gait.blend + dt / BLEND_TIME).min(1.0);
            gait.previous = if gait.blend >= 1.0 { None } else { gait.previous.take().map(|(name, phase)| (name.clone(), advance(&name, phase))) };
        }
    }

    /// Animation jouée par la créature `id`
    #[cfg(test)]
    pub fn current(&self, id: u64) -> Option<&str> {
        self.gaits.get(&id).map(|g| g.current.as_str())
    }

    /// Pose de la créature : l'animation du glTF à jouer dessous, et la rotation et le déplacement
    /// ajoutés à chaque nœud
    fn pose<'r>(&self, view: &CreatureView, species: &CreatureSpecies, rig: &'r Rig) -> Pose<'r> {
        let mut pose = vec![(Vec3::ZERO, Vec3::ZERO); rig.model.node_count()];
        let (current, phase, previous, blend) = match self.gaits.get(&view.id).filter(|g| !g.current.is_empty()) {
            Some(g) => (g.current.as_str(), g.phase, g.previous.as_ref().map(|(n, p)| (n.as_str(), *p)), g.blend),
            None => (rig.resolve(view.action.animation_name(species)).unwrap_or(""), 0.0, None, 1.0),
        };
        rig.accumulate(current, phase, blend, &mut pose);
        if let Some((name, phase)) = previous {
            rig.accumulate(name, phase, 1.0 - blend, &mut pose);
        }
        // Frappée, elle sursaute : un cycle de `coup` pendant qu'elle rougit
        let hurt = view.hurt.clamp(0.0, 1.0);
        if hurt > 0.0 && view.death <= 0.0 {
            rig.accumulate("coup", 1.0 - hurt, 1.0, &mut pose);
        }
        let offsets = pose.into_iter().map(|(a, t)| (Quat::from_euler(EulerRot::YXZ, a.y, a.x, a.z), t)).collect();
        (rig.clip(current, phase), offsets)
    }
}

/// L'animation que l'action de la créature demande, ou celle qui la remplace
fn wanted_animation<'a>(view: &CreatureView, species: &'a CreatureSpecies, rig: &Rig, speed: f32) -> &'a str {
    let moving = speed >= STILL_RATIO * species.walk_speed.max(0.1);
    let name = match view.action {
        // Bloquée ou en train de se tourner : elle se tient au repos
        CreatureAction::Marche | CreatureAction::Course if !moving => "repos",
        action => action.animation_name(species),
    };
    rig.resolve(name).unwrap_or("")
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
            (Some(rig), Some(species)) => {
                let model = &rig.model;
                let (clip, offsets) = animator.pose(view, species, rig);
                let matrices = model.posed_matrices(clip.map(|(a, _)| a), clip.map_or(0.0, |(_, t)| t), &offsets);
                let base = frame.vertices.len() as u32;
                for v in &model.vertices {
                    let p = skin_point(&matrices, v, v.position) + Vec3::Y * model.feet_height;
                    frame.vertices.push(PlanetVertex { position: to_world(p).to_array(), normal: rotate(skin_normal(&matrices, v)).to_array(), color: [tint[0], tint[1], tint[2], 1.0], texture: PlanetVertex::UNTEXTURED });
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
            frame.vertices.push(PlanetVertex { position: to_world(corners[corner]).to_array(), normal: rotate(normal).to_array(), color: [0.5 * tint[0], 0.5 * tint[1], 0.5 * tint[2], 1.0], texture: PlanetVertex::UNTEXTURED });
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
            let view = CreatureView { id: 1, species: Some(boar), feet, yaw, hurt: 0.0, death: 0.0, action: CreatureAction::Repos };
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

    /// Plus grand écart entre deux poses, la seconde ramenée de `shift`
    fn max_shift(a: &[PlanetVertex], b: &[PlanetVertex], shift: Vec3) -> f32 {
        a.iter().zip(b).map(|(a, b)| (Vec3::from(a.position) + shift).distance(Vec3::from(b.position))).fold(0.0, f32::max)
    }

    #[test]
    fn test_legs_swing_while_walking_and_barely_move_at_rest() {
        let (registry, models) = setup();
        let boar = registry.find("sanglier").unwrap();
        let feet = Vec3::new(0.0, 0.0, 1000.0);
        let (_, north) = tangent_frame(feet.normalize());
        let mut animator = CreatureAnimator::default();
        let mut view = CreatureView { id: 7, species: Some(boar), feet, yaw: 0.0, hurt: 0.0, death: 0.0, action: CreatureAction::Repos };
        let mesh = |animator: &CreatureAnimator, view: CreatureView| build_frame(&[view], &registry, &models, animator, 1.0).vertices;
        let still = mesh(&animator, view);

        // Au repos, elle respire à peine
        for _ in 0..10 {
            animator.update(&[view], &registry, &models, 0.05, 1.0);
        }
        assert_eq!(animator.current(7), Some("repos"));
        let idle = max_shift(&still, &mesh(&animator, view), Vec3::ZERO);
        assert!(idle < 0.03, "{idle}");

        // En marche, à son allure : les pattes balancent
        view.action = CreatureAction::Marche;
        let speed = registry.get(boar).unwrap().walk_speed;
        for _ in 0..20 {
            view.feet += north * speed * 0.05;
            animator.update(&[view], &registry, &models, 0.05, 1.0);
        }
        assert_eq!(animator.current(7), Some("marche"));
        let walking = max_shift(&still, &mesh(&animator, view), view.feet - feet);
        assert!(walking > 0.05, "rien ne bouge ({walking})");

        // Marche demandée mais bête bloquée : elle reste au repos
        for _ in 0..30 {
            animator.update(&[view], &registry, &models, 0.05, 1.0);
        }
        assert_eq!(animator.current(7), Some("repos"));
    }

    #[test]
    fn test_action_picks_the_animation_of_the_species_sheet() {
        let (registry, models) = setup();
        let feet = Vec3::new(0.0, 0.0, 1000.0);
        let (_, north) = tangent_frame(feet.normalize());
        let play = |id: &str, action: CreatureAction, speed: f32| {
            let species = registry.find(id).unwrap();
            let mut animator = CreatureAnimator::default();
            let mut view = CreatureView { id: 1, species: Some(species), feet, yaw: 0.0, hurt: 0.0, death: 0.0, action };
            for _ in 0..10 {
                view.feet += north * speed * 0.05;
                animator.update(&[view], &registry, &models, 0.05, 1.0);
            }
            animator.current(1).unwrap().to_string()
        };
        assert_eq!(play("loup", CreatureAction::Course, 3.0), "course");
        assert_eq!(play("loup", CreatureAction::Attaque, 0.0), "attaque");
        let wolf = registry.get(registry.find("loup").unwrap()).unwrap();
        let sleep = wolf.pauses().position(|(name, _)| name == "sommeil").unwrap() as u8;
        assert_eq!(play("loup", CreatureAction::Pause(sleep), 0.0), "sommeil");
        // Le lapin n'attaque pas : il reste au repos
        assert_eq!(play("lapin", CreatureAction::Attaque, 0.0), "repos");
    }

    #[test]
    fn test_sleeping_creature_lies_down_and_switch_is_blended() {
        let (registry, models) = setup();
        let cow = registry.find("vache").unwrap();
        let species = registry.get(cow).unwrap();
        let sleep = species.pauses().position(|(name, _)| name == "sommeil").unwrap() as u8;
        let feet = Vec3::new(0.0, 0.0, 1000.0);
        let height = |vertices: &[PlanetVertex]| vertices.iter().map(|v| Vec3::from(v.position).z - feet.z).fold(0.0, f32::max);
        let mut animator = CreatureAnimator::default();
        let mut view = CreatureView { id: 2, species: Some(cow), feet, yaw: 0.0, hurt: 0.0, death: 0.0, action: CreatureAction::Repos };
        animator.update(&[view], &registry, &models, 0.05, 1.0);
        let standing = height(&build_frame(&[view], &registry, &models, &animator, 1.0).vertices);
        view.action = CreatureAction::Pause(sleep);
        animator.update(&[view], &registry, &models, 0.05, 1.0);
        let midway = height(&build_frame(&[view], &registry, &models, &animator, 1.0).vertices);
        for _ in 0..20 {
            animator.update(&[view], &registry, &models, 0.05, 1.0);
        }
        let lying = height(&build_frame(&[view], &registry, &models, &animator, 1.0).vertices);
        assert!(lying < standing - 0.3, "debout {standing}, couchée {lying}");
        // Le fondu passe par une hauteur intermédiaire
        assert!(midway < standing - 0.01 && midway > lying + 0.01, "{standing} {midway} {lying}");
    }

    #[test]
    fn test_every_sheet_bone_exists_in_its_model() {
        let registry = CreatureRegistry::load_default();
        for species in registry.iter() {
            let model = SkinnedModel::load(&registry.asset_path(&species.model.mesh)).unwrap();
            let (rig, missing) = Rig::new(model, species);
            assert!(missing.is_empty(), "{} : {missing:?}", species.id);
            assert!(rig.animations.contains_key("repos"), "{} : pas de repos", species.id);
            assert!(rig.animations.contains_key("coup"), "{} : pas de coup", species.id);
            if species.walk_speed > 0.0 {
                assert!(rig.animations.contains_key("marche") && rig.animations.contains_key("course"), "{}", species.id);
            }
        }
    }

    #[test]
    fn test_unknown_species_gets_a_placeholder() {
        let (registry, models) = setup();
        let views = [
            CreatureView { id: 3, species: None, feet: Vec3::new(0.0, 1000.0, 0.0), yaw: 1.0, hurt: 0.0, death: 0.0, action: CreatureAction::Repos },
            CreatureView { id: 4, species: registry.find("lapin"), feet: Vec3::new(0.0, 1000.0, 0.0), yaw: 1.0, hurt: 0.0, death: 0.0, action: CreatureAction::Repos },
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
        let view = CreatureView { id: 1, species: Some(cow), feet, yaw: 0.0, hurt: 0.0, death: 0.0, action: CreatureAction::Repos };
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
        let near = CreatureView { id: 1, species: Some(boar), feet: feet + north * 3.0, yaw: 1.0, hurt: 0.0, death: 0.0, action: CreatureAction::Repos };
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

    /// Dessine `frame` (une bête aux pieds `feet`) dans la case de coin (`ox`, `oy`) et de côté
    /// `cell` d'une image `w` de large : vue de trois quarts, z-buffer, texels au plus proche
    #[allow(clippy::too_many_arguments)]
    fn rasterize(img: &mut [[u8; 3]], depth: &mut [f32], w: usize, frame: &CreatureFrame, tex: &Texture, feet: Vec3, (ox, oy, cell): (usize, usize, usize), scale: f32) {
        // x = est, y = haut (z monde), profondeur = nord, inclinée
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

    fn save(img: &[[u8; 3]], w: usize, h: usize) {
        let out = std::env::var("APERCU").unwrap_or("apercu.png".into());
        let file = std::fs::File::create(out).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
        enc.set_color(png::ColorType::Rgb);
        enc.write_header().unwrap().write_image_data(&img.concat()).unwrap();
    }

    /// Échelle d'une bête dans une case de côté `cell`
    fn scale_for(species: &CreatureSpecies, cell: usize) -> f32 {
        cell as f32 * 0.8 / (species.body.height.max(species.body.half_length * 2.0) * 1.3).max(0.5)
    }

    /// Toutes les créatures en marche, hors jeu
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
            let mut v = CreatureView { id: i as u64, species: Some(i), feet, yaw: 0.6, hurt: 0.0, death: 0.0, action: CreatureAction::Marche };
            // Un peu de marche pour une pose animée
            let (_, north) = tangent_frame(feet.normalize());
            for _ in 0..7 {
                v.feet += north * species.walk_speed.max(0.5) * 0.05;
                animator.update(&[v], &registry, &models, 0.05, 1.0);
            }
            let frame = build_frame(&[CreatureView { feet, ..v }], &registry, &models, &animator, 1.0);
            rasterize(&mut img, &mut depth, w, &frame, textures[i].as_ref().unwrap(), feet, ((i % cols) * cell, (i / cols) * cell, cell), scale_for(species, cell));
        }
        save(&img, w, h);
    }

    /// Une ligne par créature, une colonne par animation de sa fiche, chacune au quart de son cycle
    /// (`APERCU_PHASE` pour une autre fraction, `APERCU_ESPECES=loup,ours` pour n'en voir que
    /// quelques-unes)
    #[test]
    #[ignore]
    fn apercu_animations() {
        let registry = CreatureRegistry::load_default();
        let (models, textures) = CreatureModels::load(&registry);
        let phase: f32 = std::env::var("APERCU_PHASE").ok().and_then(|p| p.parse().ok()).unwrap_or(0.25);
        let only: Option<Vec<String>> = std::env::var("APERCU_ESPECES").ok().map(|s| s.split(',').map(String::from).collect());
        let species: Vec<usize> = (0..registry.len()).filter(|&i| only.as_ref().is_none_or(|o| o.contains(&registry.get(i).unwrap().id))).collect();
        let cell: usize = std::env::var("APERCU_CASE").ok().and_then(|c| c.parse().ok()).unwrap_or(180);
        let cols = species.iter().map(|&i| registry.get(i).unwrap().model.animations.len()).max().unwrap_or(1).max(1);
        let (w, h) = (cols * cell, species.len() * cell);
        let mut img = vec![[200u8, 222, 236]; w * h];
        let mut depth = vec![f32::MAX; w * h];
        let feet = Vec3::new(0.0, 0.0, 1000.0);
        for (row, &i) in species.iter().enumerate() {
            let s = registry.get(i).unwrap();
            for (col, name) in s.model.animations.keys().enumerate() {
                let mut animator = CreatureAnimator::default();
                let coup = name == "coup";
                animator.gaits.insert(1, Gait { last_feet: feet, speed: 0.0, current: if coup { "repos".into() } else { name.clone() }, phase, previous: None, blend: 1.0 });
                let view = CreatureView { id: 1, species: Some(i), feet, yaw: 1.2, hurt: if coup { 1.0 - phase } else { 0.0 }, death: 0.0, action: CreatureAction::Repos };
                let frame = build_frame(&[view], &registry, &models, &animator, 1.0);
                rasterize(&mut img, &mut depth, w, &frame, textures[i].as_ref().unwrap(), feet, (col * cell, row * cell, cell), scale_for(s, cell));
            }
        }
        save(&img, w, h);
    }
}
