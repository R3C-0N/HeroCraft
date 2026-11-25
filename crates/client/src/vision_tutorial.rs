//! Le tutoriel de la vision : « L'éclat » (bible, section 8).
//!
//! Le héros arrive en simple soldat au milieu de l'armée qui se masse devant la grande porte.
//! Après la cinématique `l-eclat`, le temps est figé et un éclat de la Pierre du Néant murmure,
//! pris dans une roche luisante, un peu plus loin entre les rangs. Le joueur :
//! 1. marche vers le murmure, entre les soldats figés (déplacement) ;
//! 2. trouve la roche, trop dure à mains nues : il ouvre son sac et glisse la pioche du
//!    paquetage dans sa barre d'objets (inventaire) ;
//! 3. prend la pioche en main et brise la roche (outils) ;
//! 4. ramasse l'éclat : des rayons de lumière jaillissent, il s'éveille en héros et les
//!    premières compétences de sa voie s'ouvrent (`l-eveil`), puis le temps repart sur le cri
//!    de la charge.
//!
//! Ce module tient les étapes, les consignes affichées, le volume du murmure et la lueur de la
//! roche ; la boucle du jeu (`main.rs`) l'informe et applique ce qu'il demande. En solo, il pose
//! aussi les soldats figés ; en ligne, le serveur tient les compétences (`ClientPacket::VisionAwaken`).

use glam::Vec3;
use herocraft_common::{BlockId, StoneEvent, StoneKind};
use herocraft_world::{BlockGeometry, BlockPos, PlanetWorldGenerator, TerrainEdits};

use crate::effects::{EffectQuad, KIND_SPRITE, NO_TEXTURE};
use crate::input::{Action, Controls};
use crate::inventory::Inventory;

/// Distance (en blocs, du joueur au centre de la roche) où il « trouve » la roche
const FOUND_BLOCKS: f32 = 4.0;
/// Au-delà de tant de blocs, le murmure se tait presque
const WHISPER_REACH_BLOCKS: f32 = 45.0;
/// Murmure le plus faible, tant qu'il n'a pas trouvé l'éclat : on l'entend toujours un peu
const WHISPER_FLOOR: f32 = 0.12;
/// Espèce des soldats figés
const SOLDIER_SPECIES: &str = "soldatAllie";
/// Écart entre deux soldats d'un rang, en blocs
const SOLDIER_SPACING_BLOCKS: f32 = 2.6;
/// Les rangs débordent de tant de blocs autour du héros et de la roche
const SOLDIER_MARGIN_BLOCKS: f32 = 7.0;
/// Pas de soldat à moins de tant de blocs du héros ni de la roche
const SOLDIER_CLEARING_BLOCKS: f32 = 2.2;
/// Au plus tant de soldats
const MAX_SOLDIERS: usize = 70;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// La cinématique d'entrée se joue
    Arriving,
    /// Marcher vers le murmure
    Walk,
    /// La roche est trouvée : prendre la pioche dans le sac
    Inventory,
    /// La pioche est dans la barre : la prendre en main
    Hold,
    /// Briser la roche
    Break,
    /// Ramasser l'éclat
    Pickup,
    /// L'éclat est ramassé : le héros s'éveille, le tutoriel est fini
    Awakened,
}

/// Ce que la boucle du jeu doit faire après une mise à jour
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Outcome {
    /// Figer le temps (la cinématique d'entrée ne l'a pas fait : déjà vue, ou passée trop tôt)
    pub freeze: bool,
}

pub struct VisionTutorial {
    step: Step,
    /// Bloc de la roche à l'éclat
    shard: BlockPos,
    /// Centre de ce bloc, repère du monde
    shard_center: Vec3,
    /// L'inventaire du présent, rendu au retour (le soldat porte son paquetage)
    present_inventory: Option<Inventory>,
    /// Soldats figés posés en solo, retirés au retour
    soldiers: Vec<u64>,
    /// Murmure lissé, de 0 à 1
    whisper: f32,
    /// Secondes depuis le début, pour la lueur
    clock: f32,
    /// La charge qui suit l'éveil (porte, bélier, herse), quand la ville-souvenir est connue
    pub charge: Option<crate::vision_charge::VisionCharge>,
    /// Passé la herse : les rues, les trois places, le héraut et la Pierre
    pub assault: Option<crate::vision_assault::VisionAssault>,
}

impl VisionTutorial {
    /// Le héros entre dans la vision : il revêt le paquetage du soldat et l'éclat est posé dans
    /// sa roche, en `shard`. `inventory` devient le paquetage ; celui du présent est gardé.
    pub fn begin(shard: BlockPos, geometry: &BlockGeometry, inventory: &mut Inventory) -> Self {
        let present = std::mem::replace(inventory, Inventory::soldier_kit());
        let corners = geometry.block_corners(shard);
        let shard_center = (corners.iter().copied().sum::<glam::DVec3>() / 8.0).as_vec3();
        Self { step: Step::Arriving, shard, shard_center, present_inventory: Some(present), soldiers: Vec::new(), whisper: 0.0, clock: 0.0, charge: None, assault: None }
    }

    /// La roche à poser (avec `lod.set_block`) : le bloc et sa place
    pub fn shard_rock(&self) -> Option<(BlockPos, BlockId)> {
        herocraft_common::blocks().find(herocraft_common::vision::SHARD_ROCK).map(|rock| (self.shard, rock))
    }

    #[allow(dead_code)]
    pub fn step(&self) -> Step {
        self.step
    }

    #[allow(dead_code)]
    pub fn shard_center(&self) -> Vec3 {
        self.shard_center
    }

    /// Le héros revient au présent : son inventaire lui est rendu ; rend les soldats à retirer
    pub fn end(mut self, inventory: &mut Inventory) -> Vec<u64> {
        if let Some(present) = self.present_inventory.take() {
            *inventory = present;
        }
        std::mem::take(&mut self.soldiers)
    }

    /// Les places des soldats figés, en solo : des rangs serrés qui couvrent le terrain du héros
    /// (`feet`) jusqu'à la roche, à poser au sol avec `ground`
    pub fn soldier_spots(&self, feet: Vec3, block: f32) -> Vec<Vec3> {
        let up = feet.normalize_or_zero();
        let (t1, t2) = herocraft_common::tangent_frame(up);
        let rel = self.shard_center - feet;
        let (sx, sz) = (rel.dot(t1) / block, rel.dot(t2) / block);
        let (x0, x1) = (sx.min(0.0) - SOLDIER_MARGIN_BLOCKS, sx.max(0.0) + SOLDIER_MARGIN_BLOCKS);
        let (z0, z1) = (sz.min(0.0) - SOLDIER_MARGIN_BLOCKS, sz.max(0.0) + SOLDIER_MARGIN_BLOCKS);
        let mut spots = Vec::new();
        let mut row = 0;
        let mut z = z0;
        while z <= z1 {
            // Rangs en quinconce, un peu de désordre dans chaque rang
            let mut x = x0 + if row % 2 == 1 { SOLDIER_SPACING_BLOCKS * 0.5 } else { 0.0 };
            while x <= x1 {
                let key = (row as u32).wrapping_mul(7919) ^ (spots.len() as u32).wrapping_mul(104_729);
                let jx = (hash(key) - 0.5) * 0.8;
                let jz = (hash(key ^ 0x5bd1) - 0.5) * 0.8;
                let (px, pz) = (x + jx, z + jz);
                let clear_hero = px * px + pz * pz > SOLDIER_CLEARING_BLOCKS * SOLDIER_CLEARING_BLOCKS;
                let clear_rock = (px - sx).powi(2) + (pz - sz).powi(2) > SOLDIER_CLEARING_BLOCKS * SOLDIER_CLEARING_BLOCKS;
                if clear_hero && clear_rock {
                    spots.push((feet + (t1 * px + t2 * pz) * block).normalize_or_zero());
                }
                x += SOLDIER_SPACING_BLOCKS;
            }
            z += SOLDIER_SPACING_BLOCKS;
            row += 1;
        }
        // Les plus proches du chemin d'abord, si la place manque
        let path = |p: &Vec3| {
            let d = *p * feet.length() - feet;
            let (px, pz) = (d.dot(t1) / block, d.dot(t2) / block);
            let len2 = (sx * sx + sz * sz).max(1e-3);
            let t = ((px * sx + pz * sz) / len2).clamp(0.0, 1.0);
            (px - sx * t).powi(2) + (pz - sz * t).powi(2)
        };
        spots.sort_by(|a, b| path(a).total_cmp(&path(b)));
        spots.truncate(MAX_SOLDIERS);
        spots
    }

    /// Les soldats posés (à retirer au retour)
    pub fn add_soldiers(&mut self, ids: impl IntoIterator<Item = u64>) {
        self.soldiers.extend(ids);
    }

    /// Les soldats posés en solo
    pub fn soldiers(&self) -> &[u64] {
        &self.soldiers
    }

    /// Espèce des soldats figés
    pub fn soldier_species() -> &'static str {
        SOLDIER_SPECIES
    }

    /// Avance le tutoriel : `player` (pieds du joueur), `block` (hauteur d'un bloc),
    /// `cinematic` (une cinématique se joue), `frozen` (le temps est figé)
    pub fn update(&mut self, dt: f32, player: Vec3, block: f32, cinematic: bool, frozen: bool, inventory: &Inventory, generator: &PlanetWorldGenerator, edits: &TerrainEdits) -> Outcome {
        self.clock += dt;
        let mut outcome = Outcome::default();
        let distance = (player + player.normalize_or_zero() * block - self.shard_center).length() / block;
        loop {
            let next = match self.step {
                Step::Arriving if !cinematic => {
                    outcome.freeze = !frozen;
                    Some(Step::Walk)
                }
                Step::Walk if distance <= FOUND_BLOCKS => Some(if inventory.pickaxe_in_hotbar() { Step::Hold } else { Step::Inventory }),
                Step::Inventory if inventory.pickaxe_in_hotbar() => Some(Step::Hold),
                Step::Hold if inventory.holds_pickaxe() => Some(Step::Break),
                Step::Hold if !inventory.pickaxe_in_hotbar() => Some(Step::Inventory),
                Step::Break | Step::Hold | Step::Inventory if !self.rock_stands(generator, edits) => Some(Step::Pickup),
                Step::Break if !inventory.holds_pickaxe() => Some(Step::Hold),
                _ => None,
            };
            match next {
                Some(step) if step != self.step => self.step = step,
                _ => break,
            }
        }
        // Le murmure forcit à l'approche de l'éclat, puis s'efface à l'éveil
        let target = match self.step {
            Step::Arriving | Step::Awakened => 0.0,
            _ => (1.0 - distance / WHISPER_REACH_BLOCKS).clamp(WHISPER_FLOOR, 1.0).powf(1.5),
        };
        let rate = if self.step == Step::Awakened { 3.0 } else { 1.5 };
        self.whisper += (target - self.whisper) * (1.0 - (-rate * dt).exp());
        outcome
    }

    fn rock_stands(&self, generator: &PlanetWorldGenerator, edits: &TerrainEdits) -> bool {
        self.shard_rock().is_some_and(|(pos, rock)| edits.block(generator, pos) == rock)
    }

    /// Un éclat ramassé pendant le tutoriel : le héros s'éveille (vrai une seule fois)
    pub fn on_stone(&mut self, event: &StoneEvent) -> bool {
        let fragment = matches!(event, StoneEvent::Obtained { kind: StoneKind::Fragment, .. });
        if fragment && matches!(self.step, Step::Pickup | Step::Break | Step::Hold | Step::Inventory | Step::Walk) {
            self.step = Step::Awakened;
            return true;
        }
        false
    }

    /// La roche ne cède qu'à la pioche : le joueur qui la frappe d'autre chose n'avance pas
    /// (la porte, le bélier et la herse de la charge, eux, ne cèdent à rien)
    pub fn blocks_mining(&self, target: Option<BlockPos>, inventory: &Inventory) -> bool {
        let siege = target.is_some_and(|pos| self.charge.as_ref().is_some_and(|c| c.protects(pos)));
        siege || (self.step != Step::Awakened && target == Some(self.shard) && !inventory.holds_pickaxe())
    }

    /// Volume du murmure, de 0 à 1
    pub fn whisper(&self) -> f32 {
        self.whisper
    }

    /// La consigne de l'étape, avec les touches du joueur ; aucune hors des étapes guidées
    pub fn hint(&self, controls: &Controls, aiming_rock: bool) -> Option<String> {
        let key = |a: Action| controls.action_key_label(a);
        Some(match self.step {
            Step::Arriving => return None,
            Step::Awakened => return self.charge.as_ref().and_then(|c| c.hint()),
            Step::Walk => format!(
                "Un murmure t'appelle, un peu plus loin entre les soldats figés. Suis-le : {} {} {} {} pour marcher.",
                key(Action::MoveForward),
                key(Action::MoveLeft),
                key(Action::MoveBackward),
                key(Action::MoveRight)
            ),
            Step::Inventory => format!(
                "La roche luit d'un éclat violet, mais elle ne cédera pas à mains nues. Ouvre ton sac ({}) et glisse la pioche dans ta barre d'objets.",
                key(Action::OpenInventory)
            ),
            Step::Hold => "Prends la pioche en main : sa touche (1 à 9) ou la molette.".to_string(),
            Step::Break if aiming_rock => "Maintiens le clic gauche pour briser la roche.".to_string(),
            Step::Break => "Vise la roche luisante et maintiens le clic gauche pour la briser.".to_string(),
            Step::Pickup => "Un éclat de Pierre du Néant est tombé. Ramasse-le.".to_string(),
        })
    }

    /// La lueur de la roche, visible de loin entre les rangs : un halo violet qui respire et
    /// des poussières qui en montent ; plus rien une fois la roche brisée
    pub fn push_quads(&self, eye: Vec3, block: f32, out: &mut Vec<EffectQuad>) {
        if matches!(self.step, Step::Pickup | Step::Awakened) {
            return;
        }
        let center = self.shard_center;
        let up = center.normalize_or_zero();
        let to_eye = (eye - center).normalize_or_zero();
        let billboard = |p: Vec3| {
            let to_p = (p - eye).normalize_or_zero();
            let mut pv = (up - to_p * up.dot(to_p)).normalize_or_zero();
            if pv == Vec3::ZERO {
                pv = herocraft_common::tangent_frame(up).0;
            }
            (pv.cross(to_p).normalize_or_zero(), pv)
        };
        // Halo : il grossit avec la distance pour rester visible de loin
        let far = ((eye - center).length() / block / 25.0).clamp(1.0, 3.0);
        let breath = 0.75 + 0.25 * (self.clock * 2.2).sin();
        let p = center + to_eye * 0.6 * block;
        let (pu, pv) = billboard(p);
        let size = 1.4 * far * breath * block;
        out.push(EffectQuad {
            center: [p.x, p.y, p.z, KIND_SPRITE as f32],
            axis_u: (pu * size).extend(0.0).to_array(),
            axis_v: (pv * size).extend(0.0).to_array(),
            color: [0.55 * 2.2, 0.0, 2.2, 0.55 * breath],
            extra: [NO_TEXTURE as f32, 1.0, 0.0, 0.0],
        });
        // Poussières de lumière qui montent en spirale
        let (t1, t2) = herocraft_common::tangent_frame(up);
        for i in 0..18u32 {
            let phase = (self.clock / 3.0 + hash(i)).fract();
            let angle = hash(i ^ 0x9e37) * std::f32::consts::TAU + phase * 3.0;
            let radius = (0.4 + 0.5 * hash(i ^ 0x51ed)) * block;
            let p = center + (t1 * angle.cos() + t2 * angle.sin()) * radius + up * (phase * 2.5 - 0.3) * block;
            let fade = (phase * 5.0).min(1.0) * (1.0 - phase);
            let (pu, pv) = billboard(p);
            let size = (0.04 + 0.03 * hash(i ^ 0x2545)) * block;
            out.push(EffectQuad {
                center: [p.x, p.y, p.z, KIND_SPRITE as f32],
                axis_u: (pu * size).extend(0.0).to_array(),
                axis_v: (pv * size).extend(0.0).to_array(),
                color: [0.9 * 2.0, 0.5 * 2.0, 2.0, fade],
                extra: [NO_TEXTURE as f32, 1.0, 0.0, 0.0],
            });
        }
    }
}

fn hash(n: u32) -> f32 {
    let mut x = n.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
    x = ((x >> ((x >> 28) + 4)) ^ x).wrapping_mul(277_803_737);
    x = (x >> 22) ^ x;
    x as f32 / u32::MAX as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tutorial() -> (VisionTutorial, Inventory, PlanetWorldGenerator, TerrainEdits, Vec3, f32) {
        let generator = PlanetWorldGenerator::new(herocraft_world::PlanetDescriptor { seed: 7, ..Default::default() });
        let geometry = BlockGeometry::new(crate::camera::DEFAULT_RADIUS as f64);
        let shard = BlockPos::new(herocraft_common::PlanetFace::PosY, 100, 200, 100);
        let mut inventory = Inventory::default();
        assert!(inventory.add(herocraft_common::items().find("bread").or_else(|| herocraft_common::items().find("bone")).unwrap()));
        let tutorial = VisionTutorial::begin(shard, &geometry, &mut inventory);
        let edits = TerrainEdits::new(None);
        let (pos, rock) = tutorial.shard_rock().expect("fiche de la roche");
        edits.set_block(&generator, pos, rock);
        let block = geometry.block_height as f32;
        let far = tutorial.shard_center() + tutorial.shard_center().normalize() * 0.0 + herocraft_common::tangent_frame(tutorial.shard_center().normalize()).0 * 20.0 * block;
        (tutorial, inventory, generator, edits, far, block)
    }

    #[test]
    fn the_soldier_finds_the_rock_takes_the_pickaxe_and_awakens() {
        let (mut t, mut inventory, generator, edits, far, block) = tutorial();
        assert!(!inventory.pickaxe_in_hotbar(), "la pioche est dans le sac");
        assert!(inventory.worn().len() >= 4, "la maille de fer est portée");
        // La cinématique se joue, puis le temps est figé s'il ne l'était pas
        t.update(0.1, far, block, true, false, &inventory, &generator, &edits);
        assert_eq!(t.step(), Step::Arriving);
        let out = t.update(0.1, far, block, false, false, &inventory, &generator, &edits);
        assert!(out.freeze);
        assert_eq!(t.step(), Step::Walk);
        assert!(t.hint(&Controls::default(), false).is_some());
        // Il arrive près de la roche : à mains nues, elle ne cède pas
        let near = t.shard_center() - t.shard_center().normalize() * block;
        t.update(0.1, near, block, false, true, &inventory, &generator, &edits);
        assert_eq!(t.step(), Step::Inventory);
        assert!(t.blocks_mining(Some(t.shard), &inventory));
        // La pioche passe du sac à la barre, puis en main
        let pickaxe = (crate::inventory::HOTBAR_SLOTS..crate::inventory::STORAGE_SLOTS).find(|&i| inventory.slot(i).is_some_and(|s| crate::inventory::is_pickaxe(s.item))).unwrap();
        inventory.click(pickaxe);
        inventory.click(3);
        t.update(0.1, near, block, false, true, &inventory, &generator, &edits);
        assert_eq!(t.step(), Step::Hold);
        inventory.select(3);
        t.update(0.1, near, block, false, true, &inventory, &generator, &edits);
        assert_eq!(t.step(), Step::Break);
        assert!(!t.blocks_mining(Some(t.shard), &inventory));
        // La roche brisée, l'éclat ramassé : l'éveil
        edits.set_block(&generator, t.shard, BlockId::AIR);
        t.update(0.1, near, block, false, true, &inventory, &generator, &edits);
        assert_eq!(t.step(), Step::Pickup);
        let fragment = StoneKind::Fragment.item().unwrap();
        assert!(t.on_stone(&StoneEvent::Obtained { kind: StoneKind::Fragment, item: fragment, position: [0.0; 3] }));
        assert!(!t.on_stone(&StoneEvent::Obtained { kind: StoneKind::Fragment, item: fragment, position: [0.0; 3] }), "une seule fois");
        assert!(t.hint(&Controls::default(), false).is_none());
        // Au retour, l'inventaire du présent revient
        t.end(&mut inventory);
        assert!(!inventory.pickaxe_in_hotbar() && inventory.worn().is_empty());
    }

    #[test]
    fn the_whisper_grows_near_the_shard() {
        let (mut t, inventory, generator, edits, far, block) = tutorial();
        t.update(0.1, far, block, false, true, &inventory, &generator, &edits);
        for _ in 0..200 {
            t.update(0.05, far, block, false, true, &inventory, &generator, &edits);
        }
        let distant = t.whisper();
        let near = t.shard_center() - t.shard_center().normalize() * block * 2.0;
        for _ in 0..200 {
            t.update(0.05, near, block, false, true, &inventory, &generator, &edits);
        }
        assert!(t.whisper() > distant + 0.3, "{distant} → {}", t.whisper());
    }

    #[test]
    fn soldiers_fill_the_ranks_but_not_the_path_ends() {
        let (t, _, _, _, far, block) = tutorial();
        let feet = far;
        let spots = t.soldier_spots(feet, block);
        assert!(spots.len() > 20 && spots.len() <= MAX_SOLDIERS, "{}", spots.len());
        for dir in &spots {
            let p = *dir * feet.length();
            assert!((p - feet).length() / block > SOLDIER_CLEARING_BLOCKS - 0.5);
        }
    }
}
