//! Habitants et voyageurs à l'écran : des créatures humanoïdes dessinées avec le corps des héros
//! (`hero_model`) et l'apparence de leur fiche (`VillagerCard`), qui viennent du serveur en ligne
//! et de la simulation des villages en solo. Leur action choisit leurs gestes : bras qui frappent
//! l'enclume au travail, mains qui parlent en bavardant, couchés pour dormir au bord du chemin.
//! Leur nom et leur métier flottent au-dessus de leur tête quand on les vise de près, sans bloc
//! entre eux et le joueur.

use std::collections::HashMap;
use std::f32::consts::TAU;
use std::sync::{Arc, OnceLock};

use glam::Vec3;
use herocraft_common::hero::{atlas, Appearance};
use herocraft_common::{CreatureAction, CreatureRegistry, VillagerCard};

use crate::camera::{CameraViewMode, OrbitCamera};
use crate::creature_render::SheetSource;
use crate::creatures::{CreatureFrame, CreatureView};
use crate::hero_model::{figure, push_figure, Figure, FigurePose, HeroGait};

/// Distance jusqu'où l'on lit le nom d'un habitant, en blocs
pub const NAME_TAG_BLOCKS: f32 = 10.0;
/// Figures gardées en mémoire (une par apparence)
const MAX_FIGURES: usize = 128;
/// Rouge d'un habitant qui vient d'être frappé
const HURT_TINT: [f32; 3] = [1.0, 0.35, 0.35];

/// La créature est un humanoïde (habitant, garde, voyageur)
pub fn is_humanoid(view: &CreatureView, registry: &CreatureRegistry) -> bool {
    view.species.and_then(|s| registry.get(s)).is_some_and(|s| s.humanoid)
}

/// Apparence d'un humanoïde dont la fiche n'est pas (encore) arrivée
fn stranger() -> &'static Appearance {
    static STRANGER: OnceLock<Appearance> = OnceLock::new();
    STRANGER.get_or_init(|| atlas().defaut("humain"))
}

/// La marche et les gestes de chaque humanoïde
#[derive(Default)]
pub struct VillagerAnimator {
    gaits: HashMap<u64, HeroGait>,
    /// Corps de chaque apparence (selon sa planche), et l'instant où il a servi
    figures: HashMap<String, (Figure, f32)>,
    time: f32,
}

impl VillagerAnimator {
    pub fn update(&mut self, views: &[CreatureView], registry: &CreatureRegistry, dt: f32, block: f32) {
        self.time = (self.time + dt) % 3600.0;
        let mut seen = Vec::new();
        for view in views.iter().filter(|v| is_humanoid(v, registry)) {
            self.gaits.entry(view.id).or_default().update(view.feet, dt, block);
            seen.push(view.id);
        }
        self.gaits.retain(|id, _| seen.contains(id));
    }

    /// Corps de l'apparence, taillé une fois pour toutes
    fn figure(&mut self, appearance: &Appearance) -> &Figure {
        let key = appearance.sheet_key();
        if !self.figures.contains_key(&key) {
            if self.figures.len() >= MAX_FIGURES {
                // Oublie le corps resté le plus longtemps sans servir
                if let Some(old) = self.figures.iter().min_by(|a, b| a.1 .1.total_cmp(&b.1 .1)).map(|(k, _)| k.clone()) {
                    self.figures.remove(&old);
                }
            }
            let sheet = herocraft_common::hero::paint(appearance);
            self.figures.insert(key.clone(), (figure(appearance, &sheet), self.time));
        }
        let entry = self.figures.get_mut(&key).expect("corps taillé juste au-dessus");
        entry.1 = self.time;
        &entry.0
    }

    /// Posture de l'humanoïde selon ce qu'il fait
    fn pose(&self, view: &CreatureView, registry: &CreatureRegistry) -> FigurePose {
        let swing = self.gaits.get(&view.id).map_or(0.0, |g| g.swing());
        // Chacun à son rythme, pour que deux voisins ne gesticulent pas ensemble
        let t = self.time + (view.id % 97) as f32 * 0.37;
        let hurt = view.hurt.clamp(0.0, 1.0);
        let tint = [0, 1, 2].map(|i| 1.0 + (HURT_TINT[i] - 1.0) * hurt);
        let mut pose = FigurePose { swing, tint, ..FigurePose::default() };
        let pause = match view.action {
            CreatureAction::Pause(i) => view.species.and_then(|s| registry.get(s)).and_then(|s| s.pauses().nth(i as usize)).map(|(name, _)| name),
            _ => None,
        };
        match (view.action, pause) {
            (CreatureAction::Attaque, _) => pose.raise_right_arm(1.2 + 0.9 * (t * TAU * 2.5).sin().max(0.0)),
            (_, Some("travaille")) => pose.raise_right_arm(1.0 + 0.6 * (t * TAU / 0.8).sin()),
            (_, Some("discute")) => {
                pose.raise_right_arm(0.35 + 0.3 * (t * TAU / 1.6).sin().max(0.0));
                pose.raise_left_arm(0.15 + 0.15 * (t * TAU / 2.1 + 1.0).sin().max(0.0));
            }
            (_, Some("sommeil")) => pose.lying = 1.0,
            _ => {}
        }
        // Mort : il tombe à la renverse
        pose.lying = pose.lying.max(view.death.clamp(0.0, 1.0));
        pose
    }
}

/// Ajoute les humanoïdes au maillage des créatures ; `texture` donne l'indice de la planche d'une
/// apparence ou d'une image fixe. Une invocation humanoïde (Chevalier de la Mort, Seigneur Liche)
/// porte sa propre planche et son arme dans la main droite.
pub fn push_villagers(
    frame: &mut CreatureFrame,
    views: &[CreatureView],
    registry: &CreatureRegistry,
    card: impl Fn(u64) -> Option<Arc<VillagerCard>>,
    animator: &mut VillagerAnimator,
    mut texture: impl FnMut(SheetSource) -> usize,
    block: f32,
) {
    for view in views.iter().filter(|v| is_humanoid(v, registry)) {
        let look = view.species.and_then(|s| registry.get(s)).and_then(|s| crate::skill_models::summon_look(&s.id));
        if let Some(look) = look {
            let mut pose = animator.pose(view, registry);
            // L'arme tenue devant soi, sauf quand le bras frappe déjà
            if pose.bone_mut(crate::hero_model::Bone::RightArm).rotation == glam::Quat::IDENTITY {
                pose.raise_right_arm(0.35);
            }
            let slot = texture(SheetSource::Fixed(&look.id, &look.sheet));
            push_figure(frame, &look.appearance, &look.figure, view.feet, view.yaw, &pose, slot, block);
            if let Some(weapon) = &look.weapon {
                let slot = texture(SheetSource::Fixed(&weapon.id, &weapon.texture));
                crate::skill_models::push_graft(frame, &weapon.model, &look.appearance, &look.figure, view.feet, view.yaw, &pose, slot, block);
            }
            continue;
        }
        let card = card(view.id);
        let appearance = card.as_ref().map_or(stranger(), |c| &c.appearance);
        let pose = animator.pose(view, registry);
        let slot = texture(SheetSource::Painted(appearance));
        push_figure(frame, appearance, animator.figure(appearance), view.feet, view.yaw, &pose, slot, block);
    }
}

/// Nom et métier de l'humanoïde visé (`looked_at`, qu'aucun bloc ne cache), à l'écran
/// (position, texte)
pub fn name_tags(views: &[CreatureView], card: impl Fn(u64) -> Option<Arc<VillagerCard>>, looked_at: Option<u64>, camera: &OrbitCamera, width: f32, height: f32) -> Vec<(egui::Pos2, String)> {
    let Some(looked_at) = looked_at.filter(|_| camera.view_mode() != CameraViewMode::SatelliteOrbit) else {
        return Vec::new();
    };
    let block = herocraft_world::BlockGeometry::new(camera.base_radius as f64).block_height as f32;
    let view_proj = camera.build_view_projection_matrix();
    let eye = camera.eye_position();
    views
        .iter()
        .filter(|v| v.id == looked_at && v.death <= 0.0)
        .filter_map(|view| {
            let card = card(view.id)?;
            let head = view.feet + view.feet.normalize_or_zero() * (card.appearance.taille as f32 + 0.35) * block;
            if head.distance(eye) > NAME_TAG_BLOCKS * block {
                return None;
            }
            let clip = view_proj * head.extend(1.0);
            if clip.w <= 0.0 {
                return None;
            }
            let ndc = clip.truncate() / clip.w;
            if ndc.x.abs() > 1.0 || ndc.y.abs() > 1.0 {
                return None;
            }
            let label = if card.profession.is_empty() { card.name.clone() } else { format!("{} · {}", card.name, card.profession) };
            Some((egui::pos2((ndc.x * 0.5 + 0.5) * width, (0.5 - ndc.y * 0.5) * height), label))
        })
        .collect()
}

/// Pieds d'un humanoïde, pour savoir s'il est à portée de parole
pub fn feet_of(views: &[CreatureView], id: u64) -> Option<Vec3> {
    views.iter().find(|v| v.id == id).map(|v| v.feet)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hero_model::Bone;

    #[test]
    fn humanoid_summons_wear_their_own_sheet_and_hold_their_weapon() {
        let registry = CreatureRegistry::load_default();
        let mut animator = VillagerAnimator::default();
        for (k, id) in ["deathKnight", "lichLord"].into_iter().enumerate() {
            let species = registry.find(id).unwrap_or_else(|| panic!("{id}"));
            let view = CreatureView { id: k as u64, species: Some(species), feet: Vec3::new(0.0, 1000.0, 0.0), yaw: 0.0, hurt: 0.0, death: 0.0, action: CreatureAction::Repos, scale: 1.0, mood: 0 };
            let mut frame = CreatureFrame::default();
            let mut asked = Vec::new();
            push_villagers(&mut frame, &[view], &registry, |_| None, &mut animator, |source| {
                asked.push(match source {
                    SheetSource::Painted(_) => "peinte".to_string(),
                    SheetSource::Fixed(key, _) => key.to_string(),
                });
                asked.len() - 1
            }, 1.0);
            let look = crate::skill_models::summon_look(id).unwrap();
            assert_eq!(asked, vec![id.to_string(), look.weapon.as_ref().unwrap().id.clone()], "sa planche puis son arme");
            assert_eq!(frame.draws.len(), 2);
        }
    }

    #[test]
    fn work_raises_the_arm_and_sleep_lays_down() {
        let registry = CreatureRegistry::load_default();
        let species = registry.find("villageois").unwrap();
        let s = registry.get(species).unwrap();
        let pause = |name: &str| CreatureAction::Pause(s.pauses().position(|(n, _)| n == name).unwrap() as u8);
        let view = |action| CreatureView { id: 1, species: Some(species), feet: Vec3::new(0.0, 0.0, 1000.0), yaw: 0.0, hurt: 0.0, death: 0.0, action, scale: 1.0, mood: 0 };
        let mut animator = VillagerAnimator::default();
        assert!(is_humanoid(&view(CreatureAction::Repos), &registry));
        let still = animator.pose(&view(CreatureAction::Repos), &registry);
        assert_eq!((still.arm_raise(Bone::RightArm), still.lying), (0.0, 0.0));
        assert!(animator.pose(&view(pause("travaille")), &registry).arm_raise(Bone::RightArm) > 0.3);
        assert_eq!(animator.pose(&view(pause("sommeil")), &registry).lying, 1.0);
        let mut frame = CreatureFrame::default();
        push_villagers(&mut frame, &[view(CreatureAction::Marche)], &registry, |_| None, &mut animator, |_| 7, 1.0);
        assert_eq!(frame.draws.len(), 1);
        assert_eq!(frame.draws[0].0, Some(7));
    }
}

/// Portraits d'habitants tirés comme dans les villages, trois femmes puis trois hommes, avec leur
/// nom : `HEROCRAFT_CAPTURES=dossier cargo test -p herocraft_client portraits_habitants -- --ignored`
#[cfg(test)]
mod portraits {
    use crate::hero_creator::Preview;

    #[test]
    #[ignore]
    fn portraits_habitants() {
        let Ok(dir) = std::env::var("HEROCRAFT_CAPTURES") else { return };
        let seed: u64 = std::env::var("HEROCRAFT_GRAINE").ok().and_then(|s| s.parse().ok()).unwrap_or(2026);
        let cards: Vec<_> = (0..6u64).filter_map(|i| herocraft_world::villagers::sample_villager(seed * 31 + i, i < 3)).collect();
        let (width, height) = (1200_usize, 560_usize);
        let ctx = egui::Context::default();
        let mut previews: Vec<Preview> = cards.iter().map(|_| Preview::default()).collect();
        let mut snapshot = crate::ui_snapshot::Snapshot::new(width, height);
        let mut last = None;
        for frame in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width as f32, height as f32))),
                time: Some(frame as f64),
                ..Default::default()
            };
            let output = ctx.run(input, |ctx| {
                egui::CentralPanel::default().frame(egui::Frame::none().fill(egui::Color32::from_rgb(52, 58, 66))).show(ctx, |ui| {
                    let painter = ui.painter().clone();
                    let cell = width as f32 / cards.len() as f32;
                    for (i, (card, preview)) in cards.iter().zip(previews.iter_mut()).enumerate() {
                        let x = i as f32 * cell;
                        let frame = egui::Rect::from_min_size(egui::pos2(x + 8.0, 20.0), egui::vec2(cell - 16.0, 420.0));
                        painter.rect_filled(frame, 6.0, egui::Color32::from_rgb(72, 80, 90));
                        preview.paint(&painter, frame, &card.appearance);
                        let sex = if i < 3 { "Femme" } else { "Homme" };
                        painter.text(egui::pos2(x + cell / 2.0, 462.0), egui::Align2::CENTER_CENTER, &card.name, egui::FontId::proportional(24.0), egui::Color32::WHITE);
                        painter.text(egui::pos2(x + cell / 2.0, 494.0), egui::Align2::CENTER_CENTER, format!("{} · {}", card.profession, card.culture), egui::FontId::proportional(16.0), egui::Color32::from_rgb(220, 210, 180));
                        painter.text(egui::pos2(x + cell / 2.0, 520.0), egui::Align2::CENTER_CENTER, sex, egui::FontId::proportional(14.0), egui::Color32::from_rgb(170, 180, 190));
                    }
                });
            });
            snapshot.apply_textures(&output.textures_delta);
            last = Some(output);
        }
        let output = last.unwrap();
        let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
        snapshot.paint(&primitives, output.pixels_per_point);
        std::fs::create_dir_all(&dir).unwrap();
        snapshot.save(&std::path::Path::new(&dir).join("habitants.png"));
        for card in &cards {
            println!("{} · {} · {} · barbe {} · coiffure {}", card.name, card.profession, card.culture, card.appearance.barbe, card.appearance.coiffure);
        }
    }
}
