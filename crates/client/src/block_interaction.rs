//! Destruction et pose de blocs : le bloc sous le réticule est visé à chaque frame, le clic
//! gauche le détruit, le clic droit pose un bloc de pierre contre la face visée. Un bouton gardé
//! enfoncé répète l'action. La modification est appliquée et remaillée dans la frame même.

use glam::Vec3;
use herocraft_common::BlockId;
use herocraft_world::{raycast_blocks, BlockGeometry, BlockPos, PlanetWorldGenerator, RayHit, TerrainEdits, TerrainQuery};
use winit::event::MouseButton;

use crate::camera::OrbitCamera;
use crate::lod::PlanetLodManager;

/// Portée de la visée, en blocs depuis la tête du joueur
pub const REACH_BLOCKS: f64 = 6.0;
/// Bloc posé au clic droit (la pierre pour l'instant)
pub const PLACED_BLOCK: BlockId = BlockId::STONE;
/// Délai entre deux actions tant que le bouton reste enfoncé
const REPEAT_DELAY: f32 = 0.25;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockAction {
    Break,
    Place,
}

#[derive(Default)]
pub struct BlockInteraction {
    target: Option<RayHit>,
    held: Option<BlockAction>,
    repeat_in: f32,
}

impl BlockInteraction {
    pub fn new() -> Self {
        Self::default()
    }

    /// Bloc visé à la dernière mise à jour
    #[allow(dead_code)]
    pub fn target(&self) -> Option<&RayHit> {
        self.target.as_ref()
    }

    /// Bouton de souris (souris capturée) : renvoie l'action à appliquer tout de suite
    pub fn on_mouse_button(&mut self, button: MouseButton, pressed: bool) -> Option<BlockAction> {
        let action = match button {
            MouseButton::Left => BlockAction::Break,
            MouseButton::Right => BlockAction::Place,
            _ => return None,
        };
        if pressed {
            self.held = Some(action);
            self.repeat_in = REPEAT_DELAY;
            Some(action)
        } else {
            if self.held == Some(action) {
                self.held = None;
            }
            None
        }
    }

    /// Plus de visée ni de bouton tenu (souris libérée : menu, vue satellite, perte du focus)
    pub fn reset(&mut self) {
        self.target = None;
        self.held = None;
    }

    /// Répétition de l'action tant que son bouton reste enfoncé
    pub fn tick(&mut self, dt: f32) -> Option<BlockAction> {
        let action = self.held?;
        self.repeat_in -= dt;
        if self.repeat_in > 0.0 {
            return None;
        }
        self.repeat_in = REPEAT_DELAY;
        Some(action)
    }

    /// Vise le premier bloc solide sous le réticule, à portée de la tête du joueur
    pub fn update_target(&mut self, camera: &OrbitCamera, generator: &PlanetWorldGenerator, edits: &TerrainEdits) {
        herocraft_common::prof_scope!("blocs::visee");
        self.target = camera.view_ray().and_then(|(eye, dir, arm)| {
            let geometry = BlockGeometry::new(camera.base_radius as f64);
            let mut query = TerrainQuery::new(generator, edits);
            let reach = arm as f64 + REACH_BLOCKS * geometry.block_height;
            raycast_blocks(&geometry, eye.as_dvec3(), dir.as_dvec3(), reach, |pos| is_targetable(query.block(pos)))
        });
    }

    /// Applique l'action au bloc visé, puis vise à nouveau. Renvoie `true` si le terrain a changé
    /// (les maillages touchés sont déjà refaits : il reste à les envoyer au GPU).
    pub fn apply(&mut self, action: BlockAction, camera: &OrbitCamera, generator: &PlanetWorldGenerator, lod: &mut PlanetLodManager) -> bool {
        herocraft_common::prof_scope!("blocs::modification");
        let Some(hit) = self.target else {
            return false;
        };
        let changed = match action {
            BlockAction::Break => lod.set_block(generator, hit.block, BlockId::AIR),
            BlockAction::Place => match hit.before {
                Some(pos) if !overlaps_player(camera, pos) => lod.set_block(generator, pos, PLACED_BLOCK),
                _ => false,
            },
        };
        if changed {
            let edits = lod.terrain_edits().clone();
            self.update_target(camera, generator, &edits);
        }
        changed
    }

    /// Coins du bloc visé, pour son contour
    pub fn outline(&self, camera: &OrbitCamera) -> Option<[Vec3; 8]> {
        let hit = self.target?;
        let geometry = BlockGeometry::new(camera.base_radius as f64);
        Some(geometry.block_corners(hit.block).map(|c| c.as_vec3()))
    }
}

/// Ce que vise le réticule : tout bloc sauf l'air et l'eau (on vise à travers l'eau)
fn is_targetable(block: BlockId) -> bool {
    !block.is_air() && block != BlockId::WATER
}

/// Le bloc occuperait la place du joueur (des pieds à la tête)
fn overlaps_player(camera: &OrbitCamera, pos: BlockPos) -> bool {
    let geometry = BlockGeometry::new(camera.base_radius as f64);
    let feet = camera.player_pos.as_dvec3();
    let up = feet.normalize();
    [0.05, 0.9, 1.75].iter().any(|&h| geometry.block_at_point(feet + up * h * geometry.block_height) == Some(pos))
}

#[cfg(test)]
mod tests {
    use super::*;
    use herocraft_world::PlanetDescriptor;

    #[test]
    fn test_button_press_acts_once_then_repeats_while_held() {
        let mut interaction = BlockInteraction::new();
        assert_eq!(interaction.on_mouse_button(MouseButton::Left, true), Some(BlockAction::Break));
        assert_eq!(interaction.tick(0.1), None);
        assert_eq!(interaction.tick(0.2), Some(BlockAction::Break));
        assert_eq!(interaction.on_mouse_button(MouseButton::Right, false), None);
        assert_eq!(interaction.tick(0.3), Some(BlockAction::Break), "relâcher l'autre bouton ne change rien");
        assert_eq!(interaction.on_mouse_button(MouseButton::Left, false), None);
        assert_eq!(interaction.tick(1.0), None);
        assert_eq!(interaction.on_mouse_button(MouseButton::Middle, true), None);
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
        let top = (0..528).find(|&y| generator.block_at(column.face, column.x, y, column.z).is_air()).unwrap();
        let centre = geometry.block_corners(BlockPos { y: top, ..column }).iter().sum::<glam::DVec3>() / 8.0;
        camera.player_pos = (centre.normalize() * (geometry.base_radius + (top as f64 + 0.01) * geometry.block_height)).as_vec3();
        camera.update_camera_bounds(&generator);
        let mut lod = PlanetLodManager::new(50.0, 2026);
        let mut interaction = BlockInteraction::new();

        interaction.update_target(&camera, &generator, &lod.terrain_edits().clone());
        let first = interaction.target().copied().expect("le sol devant le joueur est visé");
        assert!(first.before.is_some());
        assert!(interaction.outline(&camera).is_some());

        assert!(interaction.apply(BlockAction::Break, &camera, &generator, &mut lod));
        assert_eq!(lod.terrain_edits().block(&generator, first.block), BlockId::AIR);
        let second = interaction.target().copied().expect("le bloc suivant est visé");
        assert_ne!(second.block, first.block);

        // Poser contre la face visée remet un bloc de pierre là où était l'air
        let before = second.before.expect("une face libre est visée");
        assert!(interaction.apply(BlockAction::Place, &camera, &generator, &mut lod));
        assert_eq!(lod.terrain_edits().block(&generator, before), PLACED_BLOCK);
        assert_eq!(interaction.target().map(|t| t.block), Some(before));
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
        assert!(!overlaps_player(&camera, BlockPos { x: at_feet.x + 1, ..at_feet }));
    }
}
