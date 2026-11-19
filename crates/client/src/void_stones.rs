//! Pierres du Néant côté client : la file des événements de l'image et l'unique point où ils
//! sont distribués (`dispatch`). Un système qui veut réagir à une Pierre obtenue, touchée ou
//! absorbée (cinématique de la vision, quête, temple) se branche dans `dispatch`.
//!
//! Les règles et les types sont dans `herocraft_common::void_stones`.
//!
//! Le bloc posé (`voidStonePlaced`) a une coque de cristal violet, dessinée avec le terrain ;
//! `VoidStoneSites` y ajoute, par les effets, une fenêtre taillée dans la Pierre qui donne sur le
//! néant (un ciel noir étoilé fixe, qu'on voit défiler en tournant autour), au bord rongé qui
//! ondule et entouré d'une ombre qui boit la lumière du cristal, et des poussières aspirées.

use glam::Vec3;
use herocraft_common::{blocks, BlockId, ItemId, StoneEvent, StoneEvents, StoneKind};
use herocraft_world::{BlockGeometry, BlockPos, TerrainEdits};

use crate::effects::{EffectQuad, KIND_SPRITE, KIND_VOID, NO_TEXTURE};

use crate::chat::Chat;
use crate::inventory::Inventory;

/// La Pierre entière tenue en main, si c'en est une (un fragment ne se touche pas)
pub fn held_stone(inventory: &Inventory) -> Option<ItemId> {
    inventory.slot(inventory.selected()).map(|s| s.item).filter(|&item| StoneKind::of(item) == Some(StoneKind::Whole))
}

/// Le joueur touche la Pierre qu'il tient (clic droit) : signale `Touched`
pub fn touch(item: ItemId, events: &mut StoneEvents) {
    events.push(StoneEvent::Touched { item });
}

/// La Pierre `item` donne ses dernières forces : elle quitte l'inventaire, détruite, et
/// `Absorbed` est signalé. Faux si l'inventaire ne la contient pas.
#[allow(dead_code)] // appelée par la fin de la vision tutoriel (fil des cinématiques)
pub fn absorb(item: ItemId, inventory: &mut Inventory, events: &mut StoneEvents) -> bool {
    if StoneKind::of(item) != Some(StoneKind::Whole) || !inventory.take_one(item) {
        return false;
    }
    events.push(StoneEvent::Absorbed { item });
    true
}

/// Distribue les événements de l'image. C'est ici que se branchent les autres systèmes.
pub fn dispatch(events: &mut StoneEvents, chat: &mut Chat) {
    for event in events.drain() {
        log::info!("Pierre du Néant : {event:?}");
        match event {
            StoneEvent::Obtained { kind: StoneKind::Whole, .. } => chat.info("Vous avez obtenu une Pierre du Néant. Une mémoire ancienne y dort."),
            StoneEvent::Obtained { kind: StoneKind::Fragment, .. } => chat.info("Vous avez obtenu un fragment de Pierre du Néant."),
            StoneEvent::Touched { .. } => chat.info("La Pierre du Néant vibre sous vos doigts…"),
            StoneEvent::Absorbed { .. } => chat.info("La Pierre du Néant s'éteint et se brise : sa force est en vous."),
        }
    }
}

/// Identifiant de la fiche du bloc posé
pub const PLACED_BLOCK: &str = "voidStonePlaced";
/// Intervalle entre deux recherches des Pierres posées, en secondes
const SCAN_INTERVAL: f32 = 0.5;
/// Distance au plus (en blocs) d'une Pierre posée dont on dessine le néant
const SCAN_RADIUS_BLOCKS: f32 = 64.0;
/// Poussières aspirées par Pierre
const DUST_PER_STONE: u32 = 28;
/// Durée de la chute d'une poussière vers le cœur, en secondes
const DUST_SECONDS: f32 = 2.6;
/// Avancée de la fenêtre sur le néant devant le centre de la Pierre, en blocs : juste devant la
/// coque, qui mesure 5/16 de bloc de demi-largeur
const WINDOW_DEPTH_BLOCKS: f32 = 0.33;
/// Taille du losange dessiné, rapportée à la fenêtre : l'ombre qui assombrit le cristal autour
/// d'elle déborde (`VOID_FRAME` de `effects.wgsl`)
const VOID_FRAME: f32 = 1.5;
/// Rayon de départ des poussières, en blocs
const DUST_RADIUS_BLOCKS: f32 = 1.3;

/// Le bloc posé, s'il a sa fiche
pub fn placed_block() -> Option<BlockId> {
    blocks().find(PLACED_BLOCK)
}

/// Les Pierres posées près du joueur, retrouvées dans les blocs modifiés
#[derive(Debug, Default)]
pub struct VoidStoneSites {
    /// Centre de chaque Pierre, repère du monde
    centers: Vec<Vec3>,
    since_scan: f32,
}

impl VoidStoneSites {
    pub fn new() -> Self {
        Self { centers: Vec::new(), since_scan: SCAN_INTERVAL }
    }

    #[allow(dead_code)]
    pub fn centers(&self) -> &[Vec3] {
        &self.centers
    }

    /// Cherche de temps en temps les Pierres posées dans les chunks modifiés proches de `player`
    pub fn update(&mut self, dt: f32, edits: &TerrainEdits, geometry: &BlockGeometry, player: Vec3) {
        self.since_scan += dt;
        if self.since_scan < SCAN_INTERVAL {
            return;
        }
        self.since_scan = 0.0;
        self.scan(edits, geometry, player);
    }

    /// Cherche tout de suite (une Pierre vient d'être posée ou cassée)
    pub fn scan(&mut self, edits: &TerrainEdits, geometry: &BlockGeometry, player: Vec3) {
        self.centers.clear();
        let Some(stone) = placed_block() else { return };
        let h = geometry.block_height as f32;
        let size = herocraft_voxel_engine::chunk::CHUNK_SIZE as i32;
        let reach = (SCAN_RADIUS_BLOCKS + size as f32 * 1.8) * h;
        for (coord, chunk) in edits.snapshot() {
            let origin = BlockPos::new(coord.face, coord.x * size, coord.altitude_layer * size, coord.y * size);
            let corner = geometry.block_corners(origin)[0].as_vec3();
            if corner.distance(player) > reach {
                continue;
            }
            if !chunk.voxels.contains(&stone) {
                continue;
            }
            for x in 0..size {
                for y in 0..size {
                    for z in 0..size {
                        if chunk.get_voxel(x as u8, y as u8, z as u8) != stone {
                            continue;
                        }
                        let pos = BlockPos::new(coord.face, origin.x + x, origin.y + y, origin.z + z);
                        let corners = geometry.block_corners(pos);
                        let center = (corners.iter().copied().sum::<glam::DVec3>() / 8.0).as_vec3();
                        if center.distance(player) <= SCAN_RADIUS_BLOCKS * h {
                            self.centers.push(center);
                        }
                    }
                }
            }
        }
    }

    /// Le cœur et les poussières de chaque Pierre, vus de `eye` (`block` : hauteur d'un bloc)
    pub fn push_quads(&self, time: f32, eye: Vec3, block: f32, out: &mut Vec<EffectQuad>) {
        for (index, &center) in self.centers.iter().enumerate() {
            push_stone(center, index as u32, time, eye, block, out);
        }
    }
}

fn hash(n: u32) -> f32 {
    let mut x = n.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
    x = ((x >> ((x >> 28) + 4)) ^ x).wrapping_mul(277_803_737);
    x = (x >> 22) ^ x;
    x as f32 / u32::MAX as f32
}

/// Le néant d'une Pierre posée : un losange tourné vers l'œil, droit sur la planète, et des
/// poussières qui spiralent vers lui
fn push_stone(center: Vec3, seed: u32, time: f32, eye: Vec3, block: f32, out: &mut Vec<EffectQuad>) {
    let up = center.normalize_or_zero();
    let to_center = (center - eye).normalize_or_zero();
    // Axe vertical : le haut de la planète, ramené dans le plan face à l'œil
    let mut v = (up - to_center * up.dot(to_center)).normalize_or_zero();
    if v == Vec3::ZERO {
        v = herocraft_common::tangent_frame(up).0;
    }
    let u = v.cross(to_center).normalize_or_zero();
    // Le cœur bat lentement. Il est posé juste devant la coque, côté œil : le cristal l'encadre
    // comme une fenêtre taillée dans la Pierre, et le néant qu'on y voit dépend de la direction
    // du regard, pas de la position du losange.
    let beat = 1.0 + 0.06 * (time * 1.7 + seed as f32).sin();
    let window = center - to_center * WINDOW_DEPTH_BLOCKS * block;
    out.push(EffectQuad {
        center: [window.x, window.y, window.z, KIND_VOID as f32],
        // Paramètre 0 : l'horloge du néant (dérive des brumes, scintillement), en secondes
        axis_u: (u * 0.2 * VOID_FRAME * block * beat).extend(time % 3600.0).to_array(),
        axis_v: (v * 0.34 * VOID_FRAME * block * beat).extend(0.0).to_array(),
        color: [1.0, 1.0, 1.0, 1.0],
        extra: [NO_TEXTURE as f32, 0.0, 0.0, 0.0],
    });

    // Poussières : chacune part du bord, tourne en se rapprochant et s'éteint au cœur
    let (t1, t2) = herocraft_common::tangent_frame(up);
    for i in 0..DUST_PER_STONE {
        let key = seed.wrapping_mul(97).wrapping_add(i);
        let phase = (time / DUST_SECONDS + hash(key)).fract();
        let angle = hash(key ^ 0x9e37) * std::f32::consts::TAU + phase * 5.0;
        let tilt = (hash(key ^ 0x51ed) - 0.5) * 1.6;
        let radius = (1.0 - phase).powf(1.4) * DUST_RADIUS_BLOCKS * block;
        let dir = (t1 * angle.cos() + t2 * angle.sin() + up * tilt).normalize_or_zero();
        let p = center + dir * radius;
        // Visible au départ, puis absorbée par le néant
        let fade = (phase * 6.0).min(1.0) * (1.0 - phase).sqrt();
        let size = (0.03 + 0.03 * hash(key ^ 0x2545)) * block;
        let to_p = (p - eye).normalize_or_zero();
        let mut pv = (up - to_p * up.dot(to_p)).normalize_or_zero();
        if pv == Vec3::ZERO {
            pv = t1;
        }
        let pu = pv.cross(to_p).normalize_or_zero();
        // Une sur trois est une poussière sombre (voile), les autres luisent d'un violet froid
        let dark = i % 3 == 0;
        let (color, glow) = if dark { ([0.02, 0.0, 0.05, 0.85 * fade], 0.0) } else { ([0.55 * 1.6, 0.25 * 1.6, 1.0 * 1.6, fade], 1.0) };
        out.push(EffectQuad {
            center: [p.x, p.y, p.z, KIND_SPRITE as f32],
            axis_u: (pu * size).extend(0.0).to_array(),
            axis_v: (pv * size).extend(0.0).to_array(),
            color,
            extra: [NO_TEXTURE as f32, glow, 0.0, 0.0],
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absorbing_destroys_the_held_stone() {
        let stone = StoneKind::Whole.item().unwrap();
        let fragment = StoneKind::Fragment.item().unwrap();
        let mut inventory = Inventory::default();
        let mut events = StoneEvents::new();
        assert!(!absorb(stone, &mut inventory, &mut events), "pas de Pierre, rien à absorber");
        assert!(inventory.add(stone));
        assert!(inventory.add(fragment));
        assert!(!absorb(fragment, &mut inventory, &mut events), "un fragment ne s'absorbe pas");
        assert!(absorb(stone, &mut inventory, &mut events));
        assert_eq!(inventory.count(stone), 0);
        assert_eq!(inventory.count(fragment), 1);
        assert_eq!(events.drain(), vec![StoneEvent::Absorbed { item: stone }]);
    }

    #[test]
    fn the_placed_stone_is_a_crystal_that_gives_back_the_stone() {
        let block = placed_block().expect("fiche voidStonePlaced");
        assert_eq!(block.light(), 9);
        let sheet = blocks().get(block).unwrap();
        assert!(format!("{:?}", sheet.loot).contains("voidStone"), "{:?}", sheet.loot);
        assert_eq!(blocks().drop_of(block), None, "cassée, elle ne rend que la Pierre (son butin)");
    }

    #[test]
    fn a_placed_stone_draws_its_void_and_dust() {
        let block = 1.0;
        let center = Vec3::new(0.0, 1000.0, 0.0);
        let sites = VoidStoneSites { centers: vec![center], since_scan: 0.0 };
        let mut out = Vec::new();
        sites.push_quads(3.0, center + Vec3::new(4.0, 1.0, 0.0), block, &mut out);
        assert_eq!(out.len(), 1 + DUST_PER_STONE as usize);
        assert_eq!(out[0].center[3] as u32, KIND_VOID);
        // Le losange se dresse le long du haut de la planète
        let v = Vec3::from_slice(&out[0].axis_v[..3]);
        assert!(v.normalize().dot(Vec3::Y) > 0.9, "{v}");
        for dust in &out[1..] {
            let p = Vec3::from_slice(&dust.center[..3]);
            assert!(p.distance(center) <= DUST_RADIUS_BLOCKS * block + 1e-3);
        }
    }

    #[test]
    fn a_stone_placed_in_the_edits_is_found() {
        let stone = placed_block().unwrap();
        let geometry = BlockGeometry::new(1000.0);
        let generator = herocraft_world::PlanetWorldGenerator::new(herocraft_world::PlanetDescriptor { seed: 2026, ..Default::default() });
        let edits = TerrainEdits::new(None);
        let pos = BlockPos::new(herocraft_common::PlanetFace::ALL[0], 100, 300, 120);
        assert!(edits.set_block(&generator, pos, stone));
        let near = geometry.block_corners(pos)[0].as_vec3();
        let mut sites = VoidStoneSites::new();
        sites.scan(&edits, &geometry, near);
        assert_eq!(sites.centers().len(), 1);
        assert!(sites.centers()[0].distance(near) < geometry.block_height as f32 * 2.0);
        // Trop loin : rien
        sites.scan(&edits, &geometry, near * 2.0);
        assert!(sites.centers().is_empty());
    }
}
