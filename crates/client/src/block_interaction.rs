//! Destruction et pose de blocs : le bloc sous le réticule est visé à chaque frame. Le clic
//! gauche gardé enfoncé le casse peu à peu (plus ou moins vite selon sa matière) ; il tombe
//! alors au sol, à ramasser. Le clic droit pose contre la face visée le bloc de la case choisie
//! de la barre d'objets, et répète la pose tant qu'il reste enfoncé. La modification est
//! appliquée et remaillée dans la frame même.

use std::collections::HashMap;

use glam::Vec3;
use herocraft_common::{BlockId, PlanetFace};
use herocraft_world::generator::ColumnSample;
use herocraft_world::{raycast_blocks, BlockGeometry, BlockPos, PlanetWorldGenerator, RayHit, TerrainEdits, TerrainQuery};
use winit::event::MouseButton;

use crate::camera::{footprint, OrbitCamera, PLAYER_HEIGHT};
use crate::inventory::Inventory;
use crate::lod::PlanetLodManager;

/// Portée de la visée, en blocs depuis la tête du joueur
pub const REACH_BLOCKS: f64 = 6.0;
/// Délai entre deux poses tant que le bouton reste enfoncé
const REPEAT_DELAY: f32 = 0.25;
/// Pause entre un bloc cassé et le début du suivant, bouton toujours enfoncé
const BREAK_COOLDOWN: f32 = 0.15;
/// Intervalle entre deux coups entendus pendant qu'un bloc se casse
const HIT_INTERVAL: f32 = 0.25;
/// Au-delà, le cache de colonnes de la visée est vidé
const MAX_CACHED_COLUMNS: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockAction {
    Break,
    Place,
}

/// Bloc en train d'être cassé
#[derive(Clone, Copy)]
struct Mining {
    pos: BlockPos,
    /// Avancement, de 0 (intact) à 1 (cassé)
    progress: f32,
    /// Secondes avant le prochain coup entendu
    hit_in: f32,
}

/// Rayon de la dernière visée et terrain visé : identiques, la visée n'est pas refaite
#[derive(Clone, Copy, PartialEq)]
struct AimKey {
    eye: Vec3,
    dir: Vec3,
    reach: f64,
    terrain: usize,
}

#[derive(Default)]
pub struct BlockInteraction {
    target: Option<RayHit>,
    /// Contenu du bloc visé
    target_block: BlockId,
    held: Option<BlockAction>,
    repeat_in: f32,
    mining: Option<Mining>,
    /// Coup porté au bloc en train de casser, pas encore entendu
    hit: Option<BlockId>,
    last_aim: Option<AimKey>,
    /// Colonnes du générateur déjà échantillonnées, gardées d'une frame à l'autre : en tournant la
    /// vue, le rayon repasse sur les mêmes colonnes et le bruit du relief n'est pas recalculé
    columns: HashMap<(PlanetFace, i32, i32), ColumnSample>,
}

impl BlockInteraction {
    pub fn new() -> Self {
        Self::default()
    }

    /// Bloc visé à la dernière mise à jour
    pub fn target(&self) -> Option<&RayHit> {
        self.target.as_ref()
    }

    /// Bouton de souris (souris capturée) : renvoie l'action à appliquer tout de suite (la pose ;
    /// la casse, elle, avance à chaque `tick` tant que le bouton reste enfoncé)
    pub fn on_mouse_button(&mut self, button: MouseButton, pressed: bool) -> Option<BlockAction> {
        let action = match button {
            MouseButton::Left => BlockAction::Break,
            MouseButton::Right => BlockAction::Place,
            _ => return None,
        };
        if pressed {
            self.held = Some(action);
            self.mining = None;
            match action {
                BlockAction::Break => {
                    self.repeat_in = 0.0;
                    None
                }
                BlockAction::Place => {
                    self.repeat_in = REPEAT_DELAY;
                    Some(action)
                }
            }
        } else {
            if self.held == Some(action) {
                self.held = None;
                self.mining = None;
            }
            None
        }
    }

    /// Plus de visée ni de bouton tenu (souris libérée : menu, vue satellite, perte du focus)
    pub fn reset(&mut self) {
        self.target = None;
        self.held = None;
        self.mining = None;
        self.last_aim = None;
    }

    /// Avance l'action dont le bouton reste enfoncé : renvoie `Break` quand le bloc visé finit de
    /// casser, `Place` à chaque répétition de la pose
    pub fn tick(&mut self, dt: f32) -> Option<BlockAction> {
        let action = self.held?;
        self.repeat_in -= dt;
        if self.repeat_in > 0.0 {
            return None;
        }
        match action {
            BlockAction::Break => self.tick_break(dt),
            BlockAction::Place => {
                self.repeat_in = REPEAT_DELAY;
                Some(action)
            }
        }
    }

    /// Casse peu à peu le bloc visé ; viser un autre bloc reprend de zéro
    fn tick_break(&mut self, dt: f32) -> Option<BlockAction> {
        let Some(hit) = self.target else {
            self.mining = None;
            return None;
        };
        let mut mining = match self.mining {
            Some(mining) if mining.pos == hit.block => mining,
            _ => Mining { pos: hit.block, progress: 0.0, hit_in: 0.0 },
        };
        if mining.hit_in <= 0.0 {
            self.hit = Some(self.target_block);
            mining.hit_in = HIT_INTERVAL;
        }
        mining.hit_in -= dt;
        mining.progress += dt / self.target_block.break_seconds().max(0.05);
        if mining.progress >= 1.0 {
            self.mining = None;
            self.repeat_in = BREAK_COOLDOWN;
            return Some(BlockAction::Break);
        }
        self.mining = Some(mining);
        None
    }

    /// Le clic gauche est tenu enfoncé (casse en cours, ou coup porté à une bête)
    pub fn is_breaking_held(&self) -> bool {
        self.held == Some(BlockAction::Break)
    }

    /// Oublie l'avancement de la casse (le clic gauche frappe autre chose que le bloc)
    pub fn pause_mining(&mut self) {
        self.mining = None;
    }

    /// Coup porté depuis le dernier appel au bloc en train de casser : son contenu, pour le bruit
    pub fn take_hit(&mut self) -> Option<BlockId> {
        self.hit.take()
    }

    /// Coins du bloc en train de casser et son avancement (0 à 1), pour ses fissures
    pub fn cracks(&self, camera: &OrbitCamera) -> Option<([Vec3; 8], f32)> {
        let mining = self.mining.filter(|m| self.target.is_some_and(|t| t.block == m.pos))?;
        let geometry = BlockGeometry::new(camera.base_radius as f64);
        Some((geometry.block_corners(mining.pos).map(|c| c.as_vec3()), mining.progress))
    }

    /// Vise le premier bloc solide sous le réticule, à portée de la tête du joueur
    pub fn update_target(&mut self, camera: &OrbitCamera, generator: &PlanetWorldGenerator, edits: &TerrainEdits) {
        herocraft_common::prof_scope!("blocs::visee");
        let Some((eye, dir, arm)) = camera.view_ray() else {
            self.target = None;
            self.last_aim = None;
            return;
        };
        let geometry = BlockGeometry::new(camera.base_radius as f64);
        let reach = arm as f64 + REACH_BLOCKS * geometry.block_height;
        let aim = AimKey { eye, dir, reach, terrain: edits as *const TerrainEdits as usize };
        if self.last_aim == Some(aim) {
            return;
        }
        if self.last_aim.is_some_and(|last| last.terrain != aim.terrain) || self.columns.len() > MAX_CACHED_COLUMNS {
            // Autre planète (autre générateur) ou cache trop gros
            self.columns.clear();
        }
        self.last_aim = Some(aim);
        let mut query = TerrainQuery::with_columns(generator, edits, std::mem::take(&mut self.columns));
        self.target = raycast_blocks(&geometry, eye.as_dvec3(), dir.as_dvec3(), reach, |pos| Some(query.block(pos)).filter(|&b| is_targetable(b)));
        self.target_block = self.target.map_or(BlockId::AIR, |hit| query.block(hit.block));
        self.columns = query.into_columns();
    }

    /// Applique l'action au bloc visé, puis vise à nouveau. Le bloc posé est pris dans la case
    /// choisie de la barre d'objets. Renvoie le bloc changé, son nouveau contenu et l'ancien (le
    /// bloc cassé, à faire tomber au sol) si le terrain a changé (les maillages touchés sont déjà
    /// refaits : il reste à les envoyer au GPU).
    pub fn apply(
        &mut self,
        action: BlockAction,
        camera: &OrbitCamera,
        generator: &PlanetWorldGenerator,
        lod: &mut PlanetLodManager,
        inventory: &mut Inventory,
    ) -> Option<(BlockPos, BlockId, BlockId)> {
        herocraft_common::prof_scope!("blocs::modification");
        let hit = self.target?;
        let (pos, block) = match action {
            BlockAction::Break => (hit.block, BlockId::AIR),
            BlockAction::Place => match (hit.before, inventory.selected_block()) {
                // Une plante se pose même là où se tient le joueur : on la traverse
                (Some(pos), Some(block)) if !block.is_solid() || !overlaps_player(camera, pos) => {
                    (pos, block.with_rotation(facing_rotation(camera, pos)))
                }
                _ => return None,
            },
        };
        let previous = lod.terrain_edits().block(generator, pos);
        if !lod.set_block(generator, pos, block) {
            return None;
        }
        if action == BlockAction::Place {
            inventory.take_selected();
        }
        let edits = lod.terrain_edits().clone();
        self.last_aim = None;
        self.update_target(camera, generator, &edits);
        Some((pos, block, previous))
    }

    /// Centre d'un bloc dans le repère du monde (là où tombe le bloc cassé)
    pub fn block_center(camera: &OrbitCamera, pos: BlockPos) -> Vec3 {
        let geometry = BlockGeometry::new(camera.base_radius as f64);
        (geometry.block_corners(pos).iter().sum::<glam::DVec3>() / 8.0).as_vec3()
    }

    /// Le terrain a changé ailleurs (un autre joueur) : la visée est refaite à la prochaine frame
    pub fn invalidate(&mut self) {
        self.last_aim = None;
    }

    /// Coins du bloc visé, pour son contour : la boîte qui englobe ses pavés s'il a une forme
    pub fn outline(&self, camera: &OrbitCamera) -> Option<[Vec3; 8]> {
        let hit = self.target?;
        let geometry = BlockGeometry::new(camera.base_radius as f64);
        let pos = hit.block;
        let Some(shape) = self.target_block.shape() else {
            return Some(geometry.block_corners(pos).map(|c| c.as_vec3()));
        };
        let b = shape.bounds(self.target_block.rotation()).map(|v| v as f64 / 16.0);
        Some(std::array::from_fn(|i| {
            let pick = |bit: usize, axis: usize| if (i >> bit) & 1 == 1 { b[axis + 3] } else { b[axis] };
            geometry.corner(pos.face, pos.x as f64 + pick(0, 0), pos.y as f64 + pick(1, 1), pos.z as f64 + pick(2, 2)).as_vec3()
        }))
    }
}

/// Quarts de tour d'un bloc posé en `pos` pour que l'avant de sa forme (son côté z = 0) regarde le
/// joueur : selon l'axe de la grille le plus proche de la direction du regard
fn facing_rotation(camera: &OrbitCamera, pos: BlockPos) -> u8 {
    let Some((_, look, _)) = camera.view_ray() else {
        return 0;
    };
    let geometry = BlockGeometry::new(camera.base_radius as f64);
    let origin = geometry.corner(pos.face, pos.x as f64, pos.y as f64, pos.z as f64);
    let axis_x = (geometry.corner(pos.face, pos.x as f64 + 1.0, pos.y as f64, pos.z as f64) - origin).normalize();
    let axis_z = (geometry.corner(pos.face, pos.x as f64, pos.y as f64, pos.z as f64 + 1.0) - origin).normalize();
    let (x, z) = (look.as_dvec3().dot(axis_x), look.as_dvec3().dot(axis_z));
    if z.abs() >= x.abs() {
        if z > 0.0 { 0 } else { 2 }
    } else if x < 0.0 {
        1
    } else {
        3
    }
}

/// Ce que vise le réticule : tout bloc sauf l'air et les liquides (on vise à travers l'eau)
fn is_targetable(block: BlockId) -> bool {
    block.is_targetable()
}

/// Le bloc occuperait la place du joueur (des pieds à la tête, sur toute sa largeur)
fn overlaps_player(camera: &OrbitCamera, pos: BlockPos) -> bool {
    let geometry = BlockGeometry::new(camera.base_radius as f64);
    let h = geometry.block_height as f32;
    let up = camera.player_pos.normalize();
    [0.05, 0.9, PLAYER_HEIGHT].iter().any(|&height| {
        footprint(camera.player_pos + up * height * h, h).iter().any(|p| geometry.block_at_point(p.as_dvec3()) == Some(pos))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use herocraft_world::PlanetDescriptor;

    #[test]
    fn test_button_press_acts_once_then_repeats_while_held() {
        let mut interaction = BlockInteraction::new();
        assert_eq!(interaction.on_mouse_button(MouseButton::Right, true), Some(BlockAction::Place));
        assert_eq!(interaction.tick(0.1), None);
        assert_eq!(interaction.tick(0.2), Some(BlockAction::Place));
        assert_eq!(interaction.on_mouse_button(MouseButton::Left, false), None);
        assert_eq!(interaction.tick(0.3), Some(BlockAction::Place), "relâcher l'autre bouton ne change rien");
        assert_eq!(interaction.on_mouse_button(MouseButton::Right, false), None);
        assert_eq!(interaction.tick(1.0), None);
        assert_eq!(interaction.on_mouse_button(MouseButton::Middle, true), None);
        // Le clic gauche ne casse rien tout de suite, et rien sans bloc visé
        assert_eq!(interaction.on_mouse_button(MouseButton::Left, true), None);
        assert_eq!(interaction.tick(10.0), None);
    }

    #[test]
    fn test_break_then_place_at_the_crosshair() {
        let generator = PlanetWorldGenerator::new(PlanetDescriptor { seed: 2026, ..Default::default() });
        let mut camera = OrbitCamera::new(165.0, 50.0, &generator);
        camera.distance = 0.0;
        camera.target_distance = 0.0;
        camera.player_pitch = -0.6; // regard vers le sol, quelques blocs devant
        // Les pieds posés sur le dessus des blocs de leur colonne (la physique suit le relief lissé)
        let geometry = BlockGeometry::new(50.0);
        let column = geometry.block_at_point(camera.player_pos.as_dvec3()).unwrap();
        let top = (0..528).rev().find(|&y| !generator.block_at(column.face, column.x, y, column.z).is_air()).unwrap() + 1;
        let centre = geometry.block_corners(BlockPos { y: top, ..column }).iter().sum::<glam::DVec3>() / 8.0;
        camera.player_pos = (centre.normalize() * (geometry.base_radius + (top as f64 + 0.01) * geometry.block_height)).as_vec3();
        camera.update_camera_bounds(&generator);
        let mut lod = PlanetLodManager::new(50.0, 2026);
        let mut interaction = BlockInteraction::new();
        let mut inventory = Inventory::default();

        interaction.update_target(&camera, &generator, &lod.terrain_edits().clone());
        let first = interaction.target().copied().expect("le sol devant le joueur est visé");
        assert!(first.before.is_some());
        assert!(interaction.outline(&camera).is_some());

        // Inventaire vide : rien à poser
        assert!(interaction.apply(BlockAction::Place, &camera, &generator, &mut lod, &mut inventory).is_none());

        let broken = lod.terrain_edits().block(&generator, first.block);
        let (_, air, previous) = interaction.apply(BlockAction::Break, &camera, &generator, &mut lod, &mut inventory).unwrap();
        assert_eq!((air, previous), (BlockId::AIR, broken), "le bloc cassé est renvoyé pour tomber au sol");
        assert_eq!(lod.terrain_edits().block(&generator, first.block), BlockId::AIR);
        assert_eq!(inventory.selected_block(), None, "il n'entre pas directement dans l'inventaire");
        let gained = crate::inventory::drop_for(broken).expect("un bloc solide se ramasse");
        inventory.add(gained);
        let second = interaction.target().copied().expect("le bloc suivant est visé");
        assert_ne!(second.block, first.block);

        // Poser contre la face visée remet le bloc ramassé là où était l'air, et le retire de l'inventaire
        let before = second.before.expect("une face libre est visée");
        assert!(interaction.apply(BlockAction::Place, &camera, &generator, &mut lod, &mut inventory).is_some());
        assert_eq!(lod.terrain_edits().block(&generator, before), gained);
        assert_eq!(interaction.target().map(|t| t.block), Some(before));
        assert_eq!(inventory.selected_block(), None);
    }

    #[test]
    fn test_held_left_button_breaks_the_block_over_time() {
        let generator = PlanetWorldGenerator::new(PlanetDescriptor { seed: 2026, ..Default::default() });
        let mut camera = OrbitCamera::new(165.0, 50.0, &generator);
        camera.distance = 0.0;
        camera.target_distance = 0.0;
        camera.player_pitch = -0.6;
        camera.update_camera_bounds(&generator);
        let edits = TerrainEdits::new(None);
        let mut interaction = BlockInteraction::new();
        interaction.update_target(&camera, &generator, &edits);
        let target = interaction.target().copied().expect("le sol devant le joueur est visé");
        let seconds = TerrainQuery::new(&generator, &edits).block(target.block).break_seconds();

        assert_eq!(interaction.on_mouse_button(MouseButton::Left, true), None);
        let mut elapsed = 0.0;
        let mut hits = 0;
        let mut last_progress = 0.0;
        loop {
            let action = interaction.tick(0.01);
            elapsed += 0.01;
            hits += usize::from(interaction.take_hit().is_some());
            if action == Some(BlockAction::Break) {
                break;
            }
            let (_, progress) = interaction.cracks(&camera).expect("le bloc se fissure");
            assert!(progress > last_progress, "les fissures progressent");
            last_progress = progress;
            assert!(elapsed < seconds + 0.1, "le bloc devait casser en {seconds} s");
        }
        assert!(elapsed > seconds - 0.05, "cassé en {elapsed} s au lieu de {seconds} s");
        assert!(hits >= 2, "des coups s'entendent pendant la casse ({hits})");
        assert!(interaction.cracks(&camera).is_none());

        // Relâcher le bouton oublie l'avancement
        interaction.tick(BREAK_COOLDOWN);
        interaction.tick(0.2);
        assert!(interaction.cracks(&camera).is_some());
        interaction.on_mouse_button(MouseButton::Left, false);
        assert!(interaction.cracks(&camera).is_none());
    }

    #[test]
    fn test_no_block_is_placed_inside_the_player() {
        let generator = PlanetWorldGenerator::new(PlanetDescriptor { seed: 2026, ..Default::default() });
        let camera = OrbitCamera::new(165.0, 50.0, &generator);
        let geometry = BlockGeometry::new(50.0);
        let feet = camera.player_pos.as_dvec3();
        let up = feet.normalize();
        let at_feet = geometry.block_at_point(feet + up * 0.5 * geometry.block_height).unwrap();
        assert!(overlaps_player(&camera, at_feet));
        assert!(overlaps_player(&camera, BlockPos { y: at_feet.y + 1, ..at_feet }));
        assert!(!overlaps_player(&camera, BlockPos { y: at_feet.y + 3, ..at_feet }));
        // Le joueur fait 0.6 bloc de large : il peut déborder sur le bloc voisin, jamais sur le suivant
        assert!(!overlaps_player(&camera, BlockPos { x: at_feet.x + 2, ..at_feet }));
    }
}
