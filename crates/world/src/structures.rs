//! Villes, villages et ruines posés par le générateur de terrain.
//!
//! Chaque face du cube est découpée en cases de `CELL` × `CELL` blocs. La graine du monde décide,
//! case par case et une fois pour toutes, si elle reçoit une ville, un village, des ruines ou rien,
//! puis cherche dans la case un site qui s'y prête (terre ferme, relief assez plat, biome) et y
//! dessine le plan : rues, maisons, puits, champs, remparts, colonnes effondrées… Un plan ne sort
//! jamais de sa case, ni donc de sa face : il n'y a jamais de structure à cheval sur une arête du cube.
//!
//! Le plan est calculé à la première lecture d'un bloc de la case, puis gardé. Il ne dépend que de la
//! graine : le client, le serveur et chaque joueur en ligne voient exactement le même village.
//!
//! Un plan se lit colonne par colonne : `Settlement::block` donne le bloc qu'il impose en (x, y, z),
//! le sol naturel de la colonne étant connu (`ground`, l'altitude du premier bloc d'air au-dessus
//! du relief). Les bâtiments ont un plancher plat : sous lui, des fondations comblent la pente ; au-
//! dessus, le relief est dégagé. Les rues, les champs et les remparts suivent le terrain.

use std::ops::Range;
use std::sync::OnceLock;

use herocraft_common::{BlockId, PlanetFace};

use crate::generator::{BiomeType, PlanetWorldGenerator, SEA_LEVEL_BLOCKS};

/// Côté d'une case de structures, en blocs (8 × 8 cases par face)
pub const CELL: i32 = 128;
/// Cases par arête de face
pub const CELLS_PER_FACE: i32 = 1024 / CELL;

/// Blocs des structures, par leur numéro de fiche (`assets/blocks/`, numéros figés)
pub mod block {
    use herocraft_common::BlockId;

    pub const STONE: BlockId = BlockId::STONE;
    pub const WATER: BlockId = BlockId::WATER;
    pub const OAK_LOG: BlockId = BlockId::WOOD;
    pub const CRYSTAL: BlockId = BlockId::CRYSTAL;
    pub const BIRCH_LOG: BlockId = BlockId(14);
    pub const PINE_LOG: BlockId = BlockId(17);
    pub const FURNACE: BlockId = BlockId(20);
    pub const WORKBENCH: BlockId = BlockId(23);
    pub const BRICK: BlockId = BlockId(26);
    pub const WHITE_BRICK: BlockId = BlockId(27);
    pub const GLASS: BlockId = BlockId(41);
    pub const CUT_STONE: BlockId = BlockId(43);
    pub const PLANK: BlockId = BlockId(51);
    pub const WOOD_PANEL: BlockId = BlockId(55);
    pub const WHEAT: BlockId = BlockId(59);
    pub const CARROT: BlockId = BlockId(63);
    pub const SQUASH: BlockId = BlockId(67);
    pub const POTATO: BlockId = BlockId(75);
    pub const FARMLAND: BlockId = BlockId(84);
    pub const LADDER: BlockId = BlockId(88);
    pub const TORCH: BlockId = BlockId(92);
    pub const COBBLE: BlockId = BlockId(120);
    pub const GRAVEL: BlockId = BlockId(123);
    pub const HARD_STONE: BlockId = BlockId(124);
    pub const PLANK_SLAB: BlockId = BlockId(136);
    pub const PLANK_STAIRS: BlockId = BlockId(137);
    pub const COBBLE_SLAB: BlockId = BlockId(138);
    pub const COBBLE_STAIRS: BlockId = BlockId(139);
    pub const BRICK_SLAB: BlockId = BlockId(140);
    pub const BRICK_STAIRS: BlockId = BlockId(141);

    /// Tous les blocs que posent les structures (pour vérifier qu'ils ont leur fiche)
    pub const ALL: [(BlockId, &str); 30] = [
        (STONE, "stone"),
        (WATER, "water"),
        (OAK_LOG, "oakTrunk"),
        (CRYSTAL, "crystal"),
        (BIRCH_LOG, "birchTrunk"),
        (PINE_LOG, "pineTrunk"),
        (FURNACE, "furnace"),
        (WORKBENCH, "workbench"),
        (BRICK, "brick"),
        (WHITE_BRICK, "brickWhitewashed"),
        (GLASS, "glass"),
        (CUT_STONE, "hardStoneCut"),
        (PLANK, "plank"),
        (WOOD_PANEL, "woodPanel"),
        (WHEAT, "bleStade4"),
        (CARROT, "carotteStade4"),
        (SQUASH, "courgeStade4"),
        (POTATO, "pommeDeTerreStade4"),
        (FARMLAND, "terreLabouree"),
        (LADDER, "ladder"),
        (TORCH, "torch"),
        (COBBLE, "cobbleStone"),
        (GRAVEL, "gravel"),
        (HARD_STONE, "hardStone"),
        (PLANK_SLAB, "plankSlab"),
        (PLANK_STAIRS, "plankStairs"),
        (COBBLE_SLAB, "cobbleStoneSlab"),
        (COBBLE_STAIRS, "cobbleStoneStairs"),
        (BRICK_SLAB, "brickSlab"),
        (BRICK_STAIRS, "brickStairs"),
    ];
}

use block::*;

/// Le bloc `b` tourné : escalier ou échelle dont le dos (la marche haute, la plaque) regarde `back`
fn facing(b: BlockId, back: Side) -> BlockId {
    // Variantes horizontales d'une forme (`BlockRotation::Horizontal`) : dos en +Z, -X, -Z, +X
    let rotation: u16 = match back {
        Side::South => 0,
        Side::West => 1,
        Side::North => 2,
        Side::East => 3,
    };
    BlockId(b.0 | (rotation << BlockId::ROTATION_SHIFT))
}

// ---------------------------------------------------------------------------
// Hasard déterministe
// ---------------------------------------------------------------------------

fn mix(mut x: u64) -> u64 {
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58476d1ce4e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d049bb133111eb);
    x ^= x >> 31;
    x
}

/// Empreinte d'une position (x, z) pour la graine `seed`
fn hash2(seed: u64, x: i32, z: i32) -> u64 {
    mix(seed ^ (x as u32 as u64).wrapping_mul(0x9E3779B97F4A7C15) ^ (z as u32 as u64).wrapping_mul(0xC2B2AE3D27D4EB4F))
}

/// Nombre dans [0, 1) tiré d'une empreinte
fn unit(h: u64) -> f64 {
    (h >> 11) as f64 / (1u64 << 53) as f64
}

/// Petit tirage pas à pas pour dessiner un plan
struct Dice(u64);

impl Dice {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E3779B97F4A7C15);
        mix(self.0)
    }
    fn chance(&mut self, p: f64) -> bool {
        unit(self.next()) < p
    }
    /// Entier dans [lo, hi) (hi > lo)
    fn range(&mut self, lo: i32, hi: i32) -> i32 {
        lo + (self.next() % (hi - lo).max(1) as u64) as i32
    }
    fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[(self.next() % items.len() as u64) as usize]
    }
}

// ---------------------------------------------------------------------------
// Géométrie du plan
// ---------------------------------------------------------------------------

/// Rectangle de colonnes (x0..x1, z0..z1), bornes hautes exclues
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x0: i32,
    pub z0: i32,
    pub x1: i32,
    pub z1: i32,
}

impl Rect {
    pub fn new(x0: i32, z0: i32, x1: i32, z1: i32) -> Self {
        Self { x0, z0, x1, z1 }
    }

    /// Rectangle de `w` × `d` colonnes à partir de (x, z)
    pub fn sized(x: i32, z: i32, w: i32, d: i32) -> Self {
        Self::new(x, z, x + w, z + d)
    }

    /// Carré de côté 2r + 1 centré en (x, z)
    pub fn around(x: i32, z: i32, r: i32) -> Self {
        Self::new(x - r, z - r, x + r + 1, z + r + 1)
    }

    #[inline]
    pub fn contains(&self, x: i32, z: i32) -> bool {
        x >= self.x0 && x < self.x1 && z >= self.z0 && z < self.z1
    }

    pub fn intersects(&self, other: &Rect) -> bool {
        self.x0 < other.x1 && other.x0 < self.x1 && self.z0 < other.z1 && other.z0 < self.z1
    }

    pub fn expand(&self, by: i32) -> Rect {
        Rect::new(self.x0 - by, self.z0 - by, self.x1 + by, self.z1 + by)
    }

    pub fn union(&self, other: &Rect) -> Rect {
        Rect::new(self.x0.min(other.x0), self.z0.min(other.z0), self.x1.max(other.x1), self.z1.max(other.z1))
    }

    pub fn width(&self) -> i32 {
        self.x1 - self.x0
    }

    pub fn depth(&self) -> i32 {
        self.z1 - self.z0
    }

    pub fn center(&self) -> (i32, i32) {
        ((self.x0 + self.x1 - 1) / 2, (self.z0 + self.z1 - 1) / 2)
    }

    /// La colonne est sur le pourtour
    #[inline]
    fn on_edge(&self, x: i32, z: i32) -> bool {
        x == self.x0 || x == self.x1 - 1 || z == self.z0 || z == self.z1 - 1
    }

    /// La colonne est un coin
    #[inline]
    fn on_corner(&self, x: i32, z: i32) -> bool {
        (x == self.x0 || x == self.x1 - 1) && (z == self.z0 || z == self.z1 - 1)
    }
}

/// Côté d'un bâtiment, dans la grille d'une face : nord vers -Z, sud vers +Z, ouest vers -X, est vers +X
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    North,
    South,
    West,
    East,
}

impl Side {
    pub const ALL: [Side; 4] = [Side::North, Side::South, Side::West, Side::East];

    fn delta(self) -> (i32, i32) {
        match self {
            Side::North => (0, -1),
            Side::South => (0, 1),
            Side::West => (-1, 0),
            Side::East => (1, 0),
        }
    }

    fn opposite(self) -> Side {
        match self {
            Side::North => Side::South,
            Side::South => Side::North,
            Side::West => Side::East,
            Side::East => Side::West,
        }
    }
}

// ---------------------------------------------------------------------------
// Pièces du plan
// ---------------------------------------------------------------------------

/// Matériaux d'un bâtiment
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Style {
    pub wall: BlockId,
    pub corner: BlockId,
    pub floor: BlockId,
    pub foundation: BlockId,
    /// Escalier du toit en pente (`None` : toit plat en terrasse)
    pub roof: Option<BlockId>,
    /// Faîte du toit en pente, ou dalle du toit plat
    pub ridge: BlockId,
}

impl Style {
    /// Maison de village, selon le climat
    fn village(biome: BiomeType, dice: &mut Dice) -> Style {
        match biome {
            BiomeType::Desert | BiomeType::Savanna => Style {
                wall: WHITE_BRICK,
                corner: WHITE_BRICK,
                floor: PLANK,
                foundation: HARD_STONE,
                roof: None,
                ridge: WHITE_BRICK,
            },
            BiomeType::Taiga | BiomeType::Tundra => Style {
                wall: PINE_LOG,
                corner: PLANK,
                floor: PLANK,
                foundation: COBBLE,
                roof: Some(COBBLE_STAIRS),
                ridge: COBBLE,
            },
            BiomeType::Jungle => Style {
                wall: WOOD_PANEL,
                corner: BIRCH_LOG,
                floor: PLANK,
                foundation: COBBLE,
                roof: Some(PLANK_STAIRS),
                ridge: PLANK,
            },
            _ => {
                if dice.chance(0.3) {
                    Style { wall: WHITE_BRICK, corner: OAK_LOG, floor: PLANK, foundation: COBBLE, roof: Some(PLANK_STAIRS), ridge: PLANK }
                } else {
                    Style { wall: PLANK, corner: OAK_LOG, floor: PLANK, foundation: COBBLE, roof: Some(PLANK_STAIRS), ridge: PLANK }
                }
            }
        }
    }

    /// Maison de ville : briques, pierre taillée
    fn town(biome: BiomeType, dice: &mut Dice) -> Style {
        if matches!(biome, BiomeType::Desert | BiomeType::Savanna) {
            return Style { wall: WHITE_BRICK, corner: CUT_STONE, floor: PLANK, foundation: CUT_STONE, roof: None, ridge: WHITE_BRICK };
        }
        match dice.range(0, 3) {
            0 => Style { wall: BRICK, corner: CUT_STONE, floor: PLANK, foundation: CUT_STONE, roof: Some(COBBLE_STAIRS), ridge: COBBLE },
            1 => Style { wall: WHITE_BRICK, corner: OAK_LOG, floor: PLANK, foundation: COBBLE, roof: Some(BRICK_STAIRS), ridge: BRICK },
            _ => Style { wall: PLANK, corner: CUT_STONE, floor: WOOD_PANEL, foundation: COBBLE, roof: Some(BRICK_STAIRS), ridge: BRICK },
        }
    }
}

/// Une maison : un rez-de-chaussée (et un étage si `storeys` vaut 2), une porte, des fenêtres, un toit
#[derive(Debug, Clone, Copy)]
pub struct House {
    /// Murs extérieurs
    pub foot: Rect,
    /// Altitude du premier bloc d'air au-dessus du plancher
    pub floor: i32,
    pub storeys: i32,
    pub door: Side,
    pub style: Style,
}

impl House {
    fn wall_height(&self) -> i32 {
        if self.storeys > 1 { 7 } else { 4 }
    }

    /// Le faîte court le long de x (le toit descend vers le nord et le sud)
    fn ridge_along_x(&self) -> bool {
        self.foot.width() >= self.foot.depth()
    }

    /// Altitude du toit à cette colonne (avant-toits compris), et le côté vers lequel il monte
    fn roof_level(&self, x: i32, z: i32) -> (i32, i32, Option<Side>) {
        let (c, c0, c1, low, high) = if self.ridge_along_x() {
            (z, self.foot.z0 - 1, self.foot.z1 + 1, Side::South, Side::North)
        } else {
            (x, self.foot.x0 - 1, self.foot.x1 + 1, Side::East, Side::West)
        };
        let n = c1 - c0;
        let k = (c - c0).min(c1 - 1 - c);
        let k_max = (n - 1) / 2;
        let base = self.floor + self.wall_height() - 1;
        // Rang du milieu d'une largeur impaire : faîte plein ; sinon l'escalier monte vers le milieu
        let rising = if n % 2 == 1 && k == k_max {
            None
        } else if c - c0 < c1 - 1 - c {
            Some(low)
        } else {
            Some(high)
        };
        (base + k, base + k_max, rising)
    }

    fn top(&self) -> i32 {
        match self.style.roof {
            Some(_) => self.roof_level(self.foot.x0, self.foot.z0).1 + 1,
            None => self.floor + self.wall_height() + 2,
        }
    }

    /// Colonne de la porte
    fn door_column(&self) -> (i32, i32) {
        let (cx, cz) = self.foot.center();
        match self.door {
            Side::North => (cx, self.foot.z0),
            Side::South => (cx, self.foot.z1 - 1),
            Side::West => (self.foot.x0, cz),
            Side::East => (self.foot.x1 - 1, cz),
        }
    }

    fn block(&self, x: i32, y: i32, z: i32, ground: i32) -> Option<BlockId> {
        let f = self.floor;
        let h = self.wall_height();
        let foot = &self.foot;
        let inside = foot.contains(x, z);
        let pitched = self.style.roof.is_some();

        if !inside {
            // Avant-toit : seulement l'escalier du toit en pente
            if !pitched {
                return None;
            }
            let (level, _, rising) = self.roof_level(x, z);
            if y == level {
                return Some(self.roof_block(rising));
            }
            return (y > level && y < ground).then_some(BlockId::AIR);
        }

        if y < f - 1 {
            return (y >= ground).then_some(self.style.foundation);
        }
        if y == f - 1 {
            return Some(if foot.on_edge(x, z) { self.style.foundation } else { self.style.floor });
        }
        let edge = foot.on_edge(x, z);
        if y < f + h {
            let level = y - f;
            if edge {
                if foot.on_corner(x, z) {
                    return Some(self.style.corner);
                }
                if (x, z) == self.door_column() && level < 2 {
                    return Some(BlockId::AIR);
                }
                // Fenêtres : deux colonnes sur quatre du mur (des paires de vitres), à hauteur d'yeux de
                // chaque niveau
                let along = if z == foot.z0 || z == foot.z1 - 1 { x - foot.x0 } else { z - foot.z0 };
                let door_near = {
                    let (dx, dz) = self.door_column();
                    (dx - x).abs() + (dz - z).abs() <= 1 && (dx == x || dz == z)
                };
                if matches!(along % 4, 1 | 2) && (level == 1 || level == 4) && !door_near {
                    return Some(GLASS);
                }
                return Some(self.style.wall);
            }
            // Intérieur : plancher de l'étage, échelle, établi et fourneau
            if self.storeys > 1 {
                let ladder = self.ladder_column();
                if (x, z) == ladder.0 && level < 4 {
                    return Some(facing(LADDER, ladder.1));
                }
                if level == 3 {
                    return Some(self.style.floor);
                }
            }
            if level == 0 {
                let back = self.door.opposite();
                let (bx, bz) = match back {
                    Side::North => (foot.x0 + 1, foot.z0 + 1),
                    Side::South => (foot.x1 - 2, foot.z1 - 2),
                    Side::West => (foot.x0 + 1, foot.z1 - 2),
                    Side::East => (foot.x1 - 2, foot.z0 + 1),
                };
                if (x, z) == (bx, bz) {
                    return Some(WORKBENCH);
                }
                if foot.width() >= 6 && foot.depth() >= 6 {
                    let (fx, fz) = match back {
                        Side::North => (foot.x1 - 2, foot.z0 + 1),
                        Side::South => (foot.x0 + 1, foot.z1 - 2),
                        Side::West => (foot.x0 + 1, foot.z0 + 1),
                        Side::East => (foot.x1 - 2, foot.z1 - 2),
                    };
                    if (x, z) == (fx, fz) {
                        return Some(FURNACE);
                    }
                }
            }
            return Some(BlockId::AIR);
        }

        // Toit
        if pitched {
            let (level, _, rising) = self.roof_level(x, z);
            if y == level {
                return Some(self.roof_block(rising));
            }
            if y < level {
                // Pignons : le mur monte sous le toit aux deux bouts du faîte ; combles vides
                let gable = if self.ridge_along_x() { x == foot.x0 || x == foot.x1 - 1 } else { z == foot.z0 || z == foot.z1 - 1 };
                return Some(if gable { self.style.wall } else { BlockId::AIR });
            }
            return (y < ground).then_some(BlockId::AIR);
        }
        // Toit plat en terrasse, les angles montant d'un bloc au-dessus
        if y == f + h {
            return Some(self.style.ridge);
        }
        if y == f + h + 1 {
            return Some(if foot.on_corner(x, z) { self.style.corner } else { BlockId::AIR });
        }
        (y < ground).then_some(BlockId::AIR)
    }

    fn roof_block(&self, rising: Option<Side>) -> BlockId {
        match (self.style.roof, rising) {
            (Some(stairs), Some(side)) => facing(stairs, side),
            _ => self.style.ridge,
        }
    }

    /// Échelle de l'étage : contre le mur du fond, à côté de l'angle opposé à l'établi
    fn ladder_column(&self) -> ((i32, i32), Side) {
        let foot = &self.foot;
        let back = self.door.opposite();
        let pos = match back {
            Side::North => (foot.x1 - 3, foot.z0 + 1),
            Side::South => (foot.x0 + 2, foot.z1 - 2),
            Side::West => (foot.x0 + 1, foot.z0 + 2),
            Side::East => (foot.x1 - 2, foot.z1 - 3),
        };
        (pos, back)
    }
}

/// Bâtiment en ruine : murs à moitié effondrés, gravats, plancher troué
#[derive(Debug, Clone, Copy)]
pub struct RuinHouse {
    pub foot: Rect,
    pub floor: i32,
    pub height: i32,
    pub seed: u64,
}

impl RuinHouse {
    fn block(&self, x: i32, y: i32, z: i32, ground: i32) -> Option<BlockId> {
        let f = self.floor;
        let foot = &self.foot;
        let h = hash2(self.seed, x, z);
        let rubble = |h: u64| match h % 5 {
            0 | 1 => COBBLE,
            2 => STONE,
            3 => HARD_STONE,
            _ => GRAVEL,
        };
        if y < f - 1 {
            return (y >= ground).then_some(COBBLE);
        }
        if y == f - 1 {
            // Plancher dallé, troué par endroits (le sol nu reparaît)
            return if unit(h) < 0.65 { Some(rubble(h >> 8)) } else if y >= ground { Some(GRAVEL) } else { None };
        }
        if foot.on_edge(x, z) {
            // Hauteur du mur : un profil irrégulier le long du pourtour, plus haut aux angles
            let along = (x - foot.x0) + (z - foot.z0);
            let a = unit(hash2(self.seed ^ 0x51, along.div_euclid(3), 0));
            let b = unit(hash2(self.seed ^ 0x51, along.div_euclid(3) + 1, 0));
            let t = along.rem_euclid(3) as f64 / 3.0;
            let profile = a + (b - a) * t;
            let corner_boost = if foot.on_corner(x, z) { 2 } else { 0 };
            let wall = ((self.height as f64 * profile).round() as i32 + corner_boost).min(self.height);
            let level = y - f;
            if level < wall {
                // Baie de fenêtre encore lisible
                if level == 1 && along % 4 == 2 && !foot.on_corner(x, z) {
                    return Some(BlockId::AIR);
                }
                return Some(rubble(h >> (8 + level as u64 % 16)));
            }
            return (y < ground).then_some(BlockId::AIR);
        }
        // Gravats éparpillés au sol
        if y == f && unit(h >> 3) < 0.10 {
            return Some(rubble(h >> 20));
        }
        (y < ground).then_some(BlockId::AIR)
    }

    fn top(&self) -> i32 {
        self.floor + self.height + 1
    }
}

/// Temple en ruine : un podium de pierre taillée, des colonnes dont beaucoup sont brisées, un autel
#[derive(Debug, Clone, Copy)]
pub struct Temple {
    pub foot: Rect,
    /// Dessus du podium (premier bloc d'air)
    pub floor: i32,
    pub seed: u64,
}

impl Temple {
    const COLUMN_HEIGHT: i32 = 6;

    fn column_height(&self, x: i32, z: i32) -> Option<i32> {
        let inner = self.foot.expand(-1);
        let (dx, dz) = (x - inner.x0, z - inner.z0);
        if !inner.on_edge(x, z) || dx % 2 != 0 || dz % 2 != 0 {
            return None;
        }
        let r = unit(hash2(self.seed ^ 0x7e, x, z));
        Some(if r < 0.35 { Self::COLUMN_HEIGHT } else if r < 0.85 { 1 + (r * 37.0) as i32 % (Self::COLUMN_HEIGHT - 1) } else { 0 })
    }

    fn block(&self, x: i32, y: i32, z: i32, ground: i32) -> Option<BlockId> {
        let f = self.floor;
        let foot = &self.foot;
        if y < f - 2 {
            return (y >= ground).then_some(HARD_STONE);
        }
        if y < f {
            // Podium de deux blocs, son rebord en dalles brisées
            if y == f - 1 && foot.on_edge(x, z) && unit(hash2(self.seed ^ 0x33, x, z)) < 0.3 {
                return Some(COBBLE_SLAB);
            }
            return Some(CUT_STONE);
        }
        let level = y - f;
        if let Some(height) = self.column_height(x, z) {
            if level < height {
                return Some(CUT_STONE);
            }
            if level == height && height == Self::COLUMN_HEIGHT {
                return Some(COBBLE_SLAB);
            }
        }
        // Autel au milieu, un cristal posé dessus
        let (cx, cz) = foot.center();
        if (x, z) == (cx, cz) {
            match level {
                0 => return Some(CUT_STONE),
                1 => return Some(CRYSTAL),
                _ => {}
            }
        }
        // Fûts tombés : des blocs de pierre taillée couchés au pied des colonnes brisées
        if level == 0 && unit(hash2(self.seed ^ 0x99, x, z)) < 0.06 {
            return Some(CUT_STONE);
        }
        (y < ground).then_some(BlockId::AIR)
    }

    fn top(&self) -> i32 {
        self.floor + Self::COLUMN_HEIGHT + 1
    }
}

/// Tour ronde en ruine
#[derive(Debug, Clone, Copy)]
pub struct RuinTower {
    pub center: (i32, i32),
    pub radius: i32,
    pub floor: i32,
    pub height: i32,
    pub seed: u64,
}

impl RuinTower {
    fn block(&self, x: i32, y: i32, z: i32, ground: i32) -> Option<BlockId> {
        let (dx, dz) = ((x - self.center.0) as f64, (z - self.center.1) as f64);
        let d = (dx * dx + dz * dz).sqrt();
        let r = self.radius as f64;
        if d > r + 0.5 {
            return None;
        }
        let f = self.floor;
        if y < f - 1 {
            return (y >= ground).then_some(COBBLE);
        }
        if y == f - 1 {
            return Some(if unit(hash2(self.seed, x, z)) < 0.7 { COBBLE } else { GRAVEL });
        }
        if d > r - 0.5 {
            // Hauteur du mur selon l'angle : une brèche profonde d'un côté, un pan haut de l'autre
            let angle = dz.atan2(dx);
            let phase = unit(self.seed) * std::f64::consts::TAU;
            let profile = 0.5 + 0.5 * (angle + phase).cos();
            let jag = (hash2(self.seed ^ 0x44, x, z) % 3) as i32;
            let wall = (self.height as f64 * profile) as i32 + jag;
            let level = y - f;
            // Porte au pied de la brèche
            let door = (angle + phase - std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) < 0.35;
            if level < wall && !(door && level < 2) {
                // Meurtrières
                if level % 4 == 2 && hash2(self.seed ^ 0x45, x, z) % 4 == 0 {
                    return Some(BlockId::AIR);
                }
                return Some(if hash2(self.seed ^ 0x46, x, y) % 4 == 0 { STONE } else { COBBLE });
            }
            return (y < ground).then_some(BlockId::AIR);
        }
        if y == f && unit(hash2(self.seed ^ 0x47, x, z)) < 0.12 {
            return Some(GRAVEL);
        }
        (y < ground).then_some(BlockId::AIR)
    }

    fn top(&self) -> i32 {
        self.floor + self.height + 3
    }
}

/// Une pièce du plan d'un établissement
#[derive(Debug, Clone, Copy)]
pub enum Piece {
    /// Rue ou place : le dessus du sol pavé, qui suit le terrain
    Paving { rect: Rect, block: BlockId },
    /// Champ cultivé, bordé de rondins et irrigué par des rigoles
    Field { rect: Rect, crop: BlockId, rows_along_x: bool },
    House(House),
    /// Puits de 3 × 3 couvert d'un petit toit
    Well { rect: Rect, floor: i32 },
    /// Fontaine de 5 × 5, une colonne au milieu
    Fountain { rect: Rect, floor: i32 },
    /// Lampadaire : un poteau et une torche
    Lamp { x: i32, z: i32 },
    /// Pan de rempart d'un bloc d'épaisseur, crénelé, qui suit le terrain
    Rampart { rect: Rect },
    /// Porte de ville percée dans le rempart
    Gate { rect: Rect },
    /// Tour d'angle du rempart
    Tower { rect: Rect, floor: i32 },
    RuinHouse(RuinHouse),
    Temple(Temple),
    RuinTower(RuinTower),
}

impl Piece {
    /// Colonnes que la pièce peut changer
    pub fn rect(&self) -> Rect {
        match self {
            Piece::Paving { rect, .. }
            | Piece::Field { rect, .. }
            | Piece::Well { rect, .. }
            | Piece::Fountain { rect, .. }
            | Piece::Rampart { rect }
            | Piece::Gate { rect }
            | Piece::Tower { rect, .. } => *rect,
            Piece::House(house) => if house.style.roof.is_some() { house.foot.expand(1) } else { house.foot },
            Piece::Lamp { x, z } => Rect::sized(*x, *z, 1, 1),
            Piece::RuinHouse(r) => r.foot,
            Piece::Temple(t) => t.foot,
            Piece::RuinTower(t) => Rect::around(t.center.0, t.center.1, t.radius),
        }
    }

    /// Altitudes que la pièce peut changer dans une colonne de sol naturel `ground`
    fn span(&self, ground: i32) -> Range<i32> {
        let span = |floor: i32, below: i32, top: i32| floor.min(ground) - below..top.max(ground);
        match self {
            Piece::Paving { .. } => ground - 1..ground,
            Piece::Field { .. } => ground - 1..ground + 1,
            Piece::Lamp { .. } => ground..ground + 3,
            Piece::Rampart { .. } | Piece::Gate { .. } => ground - 1..ground + Self::RAMPART_HEIGHT + 1,
            Piece::House(h) => span(h.floor, 1, h.top()),
            Piece::Well { floor, .. } => span(*floor, 5, floor + 4),
            Piece::Fountain { floor, .. } => span(*floor, 3, floor + 3),
            Piece::Tower { floor, .. } => span(*floor, 1, floor + Self::TOWER_HEIGHT + 1),
            Piece::RuinHouse(r) => span(r.floor, 1, r.top()),
            Piece::Temple(t) => span(t.floor, 3, t.top()),
            Piece::RuinTower(t) => span(t.floor, 1, t.top()),
        }
    }

    const RAMPART_HEIGHT: i32 = 5;
    const TOWER_HEIGHT: i32 = 9;

    fn block(&self, x: i32, y: i32, z: i32, ground: i32) -> Option<BlockId> {
        match self {
            Piece::Paving { block, .. } => (y == ground - 1).then_some(*block),
            Piece::Field { rect, crop, rows_along_x } => {
                if rect.on_edge(x, z) {
                    return (y == ground - 1).then_some(OAK_LOG);
                }
                let row = if *rows_along_x { z - rect.z0 } else { x - rect.x0 };
                if row % 4 == 0 {
                    return (y == ground - 1).then_some(WATER);
                }
                if y == ground - 1 {
                    Some(FARMLAND)
                } else if y == ground {
                    Some(*crop)
                } else {
                    None
                }
            }
            Piece::House(house) => house.block(x, y, z, ground),
            Piece::Well { rect, floor } => {
                let f = *floor;
                let (cx, cz) = rect.center();
                if (x, z) == (cx, cz) {
                    if y >= f - 5 && y < f {
                        return Some(WATER);
                    }
                    return (y >= f && y < f + 3).then_some(BlockId::AIR).or((y == f + 3).then_some(PLANK_SLAB));
                }
                if y < f - 1 {
                    return (y >= ground).then_some(COBBLE);
                }
                if y == f - 1 || y == f {
                    return Some(COBBLE);
                }
                if y < f + 3 {
                    return Some(if rect.on_corner(x, z) { OAK_LOG } else { BlockId::AIR });
                }
                (y == f + 3).then_some(PLANK_SLAB)
            }
            Piece::Fountain { rect, floor } => {
                let f = *floor;
                let (cx, cz) = rect.center();
                if y < f - 2 {
                    return (y >= ground).then_some(CUT_STONE);
                }
                if (x, z) == (cx, cz) {
                    return match y - f {
                        -2..=1 => Some(CUT_STONE),
                        2 => Some(TORCH),
                        _ => (y < ground).then_some(BlockId::AIR),
                    };
                }
                if rect.on_edge(x, z) {
                    return if y < f + 1 { Some(CUT_STONE) } else { (y < ground).then_some(BlockId::AIR) };
                }
                if y == f - 2 {
                    return Some(CUT_STONE);
                }
                if y < f {
                    return Some(WATER);
                }
                (y < ground || y == f).then_some(BlockId::AIR)
            }
            Piece::Lamp { .. } => match y - ground {
                0 | 1 => Some(OAK_LOG),
                2 => Some(TORCH),
                _ => None,
            },
            Piece::Rampart { .. } => {
                let level = y - ground;
                if level < 0 {
                    return (level == -1).then_some(COBBLE);
                }
                if level < Self::RAMPART_HEIGHT {
                    return Some(if (x * 7 + y * 3 + z * 5).rem_euclid(11) == 0 { HARD_STONE } else { COBBLE });
                }
                (level == Self::RAMPART_HEIGHT && (x + z) % 2 == 0).then_some(COBBLE)
            }
            Piece::Gate { .. } => {
                let level = y - ground;
                match level {
                    -1 => Some(COBBLE),
                    0..=2 => Some(BlockId::AIR),
                    3 => Some(CUT_STONE),
                    4 => Some(COBBLE),
                    l if l == Self::RAMPART_HEIGHT => ((x + z) % 2 == 0).then_some(COBBLE),
                    _ => None,
                }
            }
            Piece::Tower { rect, floor } => {
                let f = *floor;
                let h = Self::TOWER_HEIGHT;
                if y < f - 1 {
                    return (y >= ground).then_some(COBBLE);
                }
                if y == f - 1 {
                    return Some(COBBLE);
                }
                let level = y - f;
                if rect.on_edge(x, z) {
                    if level < h {
                        // Meurtrières au milieu de chaque côté
                        let (cx, cz) = rect.center();
                        if (level == 3 || level == 6) && !rect.on_corner(x, z) && (x == cx || z == cz) {
                            return Some(BlockId::AIR);
                        }
                        return Some(if level == h - 1 { CUT_STONE } else { COBBLE });
                    }
                    if level == h {
                        return Some(if (x + z) % 2 == 0 { COBBLE } else { BlockId::AIR });
                    }
                    return (y < ground).then_some(BlockId::AIR);
                }
                if level == h - 1 {
                    return Some(PLANK);
                }
                if level < h + 1 {
                    return Some(BlockId::AIR);
                }
                (y < ground).then_some(BlockId::AIR)
            }
            Piece::RuinHouse(r) => r.block(x, y, z, ground),
            Piece::Temple(t) => t.block(x, y, z, ground),
            Piece::RuinTower(t) => t.block(x, y, z, ground),
        }
    }
}

// ---------------------------------------------------------------------------
// Établissements
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettlementKind {
    /// Ville fortifiée : rues pavées en damier, maisons de briques, fontaine, remparts et tours
    Town,
    /// Village : puits, chemins de gravier, maisons, champs et lampadaires
    Village,
    /// Ruines : maisons effondrées, temple à colonnes brisées ou vieille tour
    Ruins,
}

impl SettlementKind {
    pub fn name(&self) -> &'static str {
        match self {
            SettlementKind::Town => "ville",
            SettlementKind::Village => "village",
            SettlementKind::Ruins => "ruines",
        }
    }

    /// Demi-côté du site
    fn radius(&self) -> i32 {
        match self {
            SettlementKind::Town => 40,
            SettlementKind::Village => 32,
            SettlementKind::Ruins => 14,
        }
    }

    /// Écart d'altitude admis sur le site
    fn max_relief(&self) -> i32 {
        match self {
            SettlementKind::Town => 16,
            SettlementKind::Village => 14,
            SettlementKind::Ruins => 22,
        }
    }

    fn accepts(&self, biome: BiomeType) -> bool {
        match biome {
            BiomeType::DeepOcean | BiomeType::Ocean | BiomeType::Sea | BiomeType::SnowPeaks => false,
            BiomeType::Mountains => *self == SettlementKind::Ruins,
            BiomeType::Beach => *self != SettlementKind::Town,
            _ => true,
        }
    }
}

/// Une ville, un village ou des ruines : son plan complet
#[derive(Debug, Clone)]
pub struct Settlement {
    pub kind: SettlementKind,
    pub face: PlanetFace,
    /// Colonne du centre
    pub center: (i32, i32),
    /// Biome du centre, qui a choisi les matériaux
    pub biome: BiomeType,
    /// Toutes les colonnes que le plan peut changer
    pub bounds: Rect,
    /// Plus haut bloc (exclu) que le plan pose
    pub top: i32,
    pub pieces: Vec<Piece>,
}

impl Settlement {
    /// Pièces qui couvrent la colonne (x, z)
    fn pieces_at(&self, x: i32, z: i32) -> impl Iterator<Item = &Piece> {
        self.pieces.iter().filter(move |p| p.rect().contains(x, z))
    }

    /// Bloc imposé par le plan en (x, y, z), `ground` étant l'altitude du premier bloc d'air
    /// au-dessus du relief naturel de la colonne. `None` : le terrain naturel reste.
    pub fn block(&self, x: i32, y: i32, z: i32, ground: i32) -> Option<BlockId> {
        if !self.bounds.contains(x, z) {
            return None;
        }
        self.pieces_at(x, z).find_map(|p| if p.span(ground).contains(&y) { p.block(x, y, z, ground) } else { None })
    }

    /// Altitudes que le plan peut changer dans la colonne (x, z) (vide s'il n'y touche pas)
    pub fn span(&self, x: i32, z: i32, ground: i32) -> Range<i32> {
        if !self.bounds.contains(x, z) {
            return 0..0;
        }
        self.pieces_at(x, z).map(|p| p.span(ground)).reduce(|a, b| a.start.min(b.start)..a.end.max(b.end)).unwrap_or(0..0)
    }

    /// La colonne est sous une pièce du plan, ou à moins de `margin` colonnes d'une pièce (là où
    /// rien d'autre, un arbre par exemple, ne devrait pousser)
    pub fn occupies(&self, x: i32, z: i32, margin: i32) -> bool {
        self.bounds.expand(margin).contains(x, z) && self.pieces.iter().any(|p| p.rect().expand(margin).contains(x, z))
    }
}

/// Les établissements d'une planète, case par case, dessinés à la première lecture
#[derive(Debug)]
pub struct StructureMap {
    cells: Vec<OnceLock<Option<Settlement>>>,
}

impl Default for StructureMap {
    fn default() -> Self {
        Self::new()
    }
}

impl StructureMap {
    pub fn new() -> Self {
        Self { cells: (0..6 * CELLS_PER_FACE * CELLS_PER_FACE).map(|_| OnceLock::new()).collect() }
    }

    fn cell_index(face: PlanetFace, cx: i32, cz: i32) -> Option<usize> {
        if !(0..CELLS_PER_FACE).contains(&cx) || !(0..CELLS_PER_FACE).contains(&cz) {
            return None;
        }
        Some((face_index(face) * CELLS_PER_FACE * CELLS_PER_FACE + cz * CELLS_PER_FACE + cx) as usize)
    }

    /// L'établissement de la case (cx, cz) d'une face, s'il y en a un
    pub fn cell(&self, generator: &PlanetWorldGenerator, face: PlanetFace, cx: i32, cz: i32) -> Option<&Settlement> {
        let index = Self::cell_index(face, cx, cz)?;
        self.cells[index].get_or_init(|| plan_cell(generator, face, cx, cz)).as_ref()
    }

    /// L'établissement dont le plan couvre la colonne (x, z) d'une face, s'il y en a un
    pub fn at(&self, generator: &PlanetWorldGenerator, face: PlanetFace, x: i32, z: i32) -> Option<&Settlement> {
        let settlement = self.cell(generator, face, x.div_euclid(CELL), z.div_euclid(CELL))?;
        settlement.bounds.contains(x, z).then_some(settlement)
    }

    /// Les établissements dont le plan touche le rectangle de colonnes `area` d'une face
    pub fn touching(&self, generator: &PlanetWorldGenerator, face: PlanetFace, area: Rect) -> Vec<&Settlement> {
        let mut found = Vec::new();
        for cz in area.z0.div_euclid(CELL)..=(area.z1 - 1).div_euclid(CELL) {
            for cx in area.x0.div_euclid(CELL)..=(area.x1 - 1).div_euclid(CELL) {
                if let Some(s) = self.cell(generator, face, cx, cz) {
                    if s.bounds.intersects(&area) {
                        found.push(s);
                    }
                }
            }
        }
        found
    }
}

fn face_index(face: PlanetFace) -> i32 {
    PlanetFace::ALL.iter().position(|f| *f == face).unwrap_or(0) as i32
}

// ---------------------------------------------------------------------------
// Choix du site et dessin du plan
// ---------------------------------------------------------------------------

/// Sol naturel d'une colonne : altitude du premier bloc d'air au-dessus du relief
fn ground_at(generator: &PlanetWorldGenerator, face: PlanetFace, x: i32, z: i32) -> (i32, BiomeType) {
    let column = generator.sample_block_column(face, x, z);
    (column.elevation.floor() as i32, column.biome)
}

/// Relève le terrain du carré de demi-côté `radius` autour de (x, z), par pas de `step` colonnes.
/// Faux si une colonne est sous la mer, d'un biome que `kind` refuse, ou si le relief est trop accidenté.
fn site_fits(generator: &PlanetWorldGenerator, face: PlanetFace, x: i32, z: i32, radius: i32, step: i32, kind: SettlementKind) -> bool {
    let (mut min, mut max) = (i32::MAX, i32::MIN);
    let mut dz = -radius;
    while dz <= radius {
        let mut dx = -radius;
        while dx <= radius {
            let (g, biome) = ground_at(generator, face, x + dx, z + dz);
            if !kind.accepts(biome) || (g as f64) <= SEA_LEVEL_BLOCKS + 1.0 {
                return false;
            }
            min = min.min(g);
            max = max.max(g);
            if max - min > kind.max_relief() {
                return false;
            }
            dx += step;
        }
        dz += step;
    }
    true
}

/// Plancher d'un bâtiment posé sur `foot` : l'altitude moyenne du sol sous lui. `None` si la pente
/// dépasse `max_relief`.
fn floor_for(generator: &PlanetWorldGenerator, face: PlanetFace, foot: &Rect, max_relief: i32) -> Option<i32> {
    let (cx, cz) = foot.center();
    let points = [(cx, cz), (foot.x0, foot.z0), (foot.x1 - 1, foot.z0), (foot.x0, foot.z1 - 1), (foot.x1 - 1, foot.z1 - 1)];
    let grounds: Vec<i32> = points.iter().map(|&(x, z)| ground_at(generator, face, x, z).0).collect();
    let (lo, hi) = (*grounds.iter().min()?, *grounds.iter().max()?);
    if hi - lo > max_relief || (lo as f64) <= SEA_LEVEL_BLOCKS + 1.0 {
        return None;
    }
    let mean = grounds.iter().sum::<i32>() as f64 / grounds.len() as f64;
    Some(mean.round() as i32)
}

/// Plan en cours de dessin
struct Planner<'a> {
    generator: &'a PlanetWorldGenerator,
    face: PlanetFace,
    /// Colonnes où le plan doit rester (la case, moins une marge)
    area: Rect,
    pieces: Vec<Piece>,
    /// Emprises déjà prises (bâtiments, rues), contre lesquelles un nouveau bâtiment ne doit pas buter
    taken: Vec<Rect>,
}

impl<'a> Planner<'a> {
    fn free(&self, rect: &Rect) -> bool {
        self.area.contains(rect.x0, rect.z0)
            && self.area.contains(rect.x1 - 1, rect.z1 - 1)
            && !self.taken.iter().any(|t| t.intersects(rect))
    }

    fn take(&mut self, piece: Piece, keep_clear: i32) {
        self.taken.push(piece.rect().expand(keep_clear));
        self.pieces.push(piece);
    }

    /// Pose une maison si la place est libre et le sol assez plat
    fn try_house(&mut self, foot: Rect, door: Side, storeys: i32, style: Style) -> bool {
        let rect = foot.expand(1);
        if !self.free(&rect) {
            return false;
        }
        let Some(floor) = floor_for(self.generator, self.face, &foot, 5) else {
            return false;
        };
        self.take(Piece::House(House { foot, floor, storeys, door, style }), 1);
        true
    }

    /// Maison collée à une rue : `side` est le côté de la rue où elle se trouve, `at` la colonne
    /// de la rue en face de laquelle elle se centre, `street_half` la demi-largeur de la rue
    fn house_along(&mut self, at: (i32, i32), street_along_x: bool, side: i32, street_half: i32, w: i32, d: i32, storeys: i32, style: Style) -> bool {
        let gap = street_half + 2;
        let (foot, door) = if street_along_x {
            let z0 = if side > 0 { at.1 + gap } else { at.1 - gap - d + 1 };
            (Rect::sized(at.0 - w / 2, z0, w, d), if side > 0 { Side::North } else { Side::South })
        } else {
            let x0 = if side > 0 { at.0 + gap } else { at.0 - gap - d + 1 };
            (Rect::sized(x0, at.1 - w / 2, d, w), if side > 0 { Side::West } else { Side::East })
        };
        self.try_house(foot, door, storeys, style)
    }

    fn finish(self, kind: SettlementKind, center: (i32, i32), biome: BiomeType) -> Option<Settlement> {
        if self.pieces.is_empty() {
            return None;
        }
        let bounds = self.pieces.iter().map(|p| p.rect()).reduce(|a, b| a.union(&b))?;
        // Les pièces qui suivent le terrain restent à quelques blocs du relief : le plus haut bloc est
        // celui d'un bâtiment
        let top = self.pieces.iter().map(piece_floor_top).max()?;
        Some(Settlement { kind, face: self.face, center, biome, bounds, top, pieces: self.pieces })
    }
}

/// Plus haut bloc d'une pièce posée sur un plancher fixe (les pièces qui suivent le terrain n'en ont pas)
fn piece_floor_top(piece: &Piece) -> i32 {
    match piece {
        Piece::House(h) => h.top(),
        Piece::Well { floor, .. } => floor + 4,
        Piece::Fountain { floor, .. } => floor + 3,
        Piece::Tower { floor, .. } => floor + Piece::TOWER_HEIGHT + 1,
        Piece::RuinHouse(r) => r.top(),
        Piece::Temple(t) => t.top(),
        Piece::RuinTower(t) => t.top(),
        _ => i32::MIN,
    }
}

/// Graine propre à une case
fn cell_seed(generator: &PlanetWorldGenerator, face: PlanetFace, cx: i32, cz: i32) -> u64 {
    mix(generator.planet.seed ^ 0x5717_c7ae_0000_0000 ^ ((face_index(face) as u64) << 40) ^ hash2(0xC311, cx, cz))
}

/// Dessine l'établissement d'une case, s'il y en a un
fn plan_cell(generator: &PlanetWorldGenerator, face: PlanetFace, cx: i32, cz: i32) -> Option<Settlement> {
    herocraft_common::prof_scope!("world::structures::plan_cell");
    let mut dice = Dice(cell_seed(generator, face, cx, cz));
    let roll = unit(dice.next());
    let wanted = if roll < 0.16 {
        SettlementKind::Town
    } else if roll < 0.44 {
        SettlementKind::Village
    } else if roll < 0.66 {
        SettlementKind::Ruins
    } else {
        return None;
    };
    let cell = Rect::sized(cx * CELL, cz * CELL, CELL, CELL);
    // Un site refusé à une ville peut accueillir un village, puis des ruines
    let kinds: &[SettlementKind] = match wanted {
        SettlementKind::Town => &[SettlementKind::Town, SettlementKind::Village],
        SettlementKind::Village => &[SettlementKind::Village, SettlementKind::Ruins],
        SettlementKind::Ruins => &[SettlementKind::Ruins],
    };
    for &kind in kinds {
        let margin = kind.radius() + 4;
        for _ in 0..10 {
            let x = dice.range(cell.x0 + margin, cell.x1 - margin);
            let z = dice.range(cell.z0 + margin, cell.z1 - margin);
            let (g, biome) = ground_at(generator, face, x, z);
            if !kind.accepts(biome) || (g as f64) <= SEA_LEVEL_BLOCKS + 1.0 {
                continue;
            }
            let step = if kind == SettlementKind::Ruins { 7 } else { 8 };
            if !site_fits(generator, face, x, z, kind.radius(), step, kind) {
                continue;
            }
            let planner = Planner { generator, face, area: cell.expand(-2), pieces: Vec::new(), taken: Vec::new() };
            let settlement = match kind {
                SettlementKind::Town => plan_town(planner, (x, z), biome, &mut dice),
                SettlementKind::Village => plan_village(planner, (x, z), biome, &mut dice),
                SettlementKind::Ruins => plan_ruins(planner, (x, z), biome, &mut dice),
            };
            if settlement.is_some() {
                return settlement;
            }
        }
    }
    None
}

fn plan_village(mut p: Planner, center: (i32, i32), biome: BiomeType, dice: &mut Dice) -> Option<Settlement> {
    let (x, z) = center;
    let well = Rect::around(x, z, 1);
    let floor = floor_for(p.generator, p.face, &well, 4)?;
    p.take(Piece::Well { rect: well, floor }, 0);
    let path = if matches!(biome, BiomeType::Desert | BiomeType::Savanna | BiomeType::Beach) { COBBLE } else { GRAVEL };
    p.take(Piece::Paving { rect: Rect::around(x, z, 3), block: path }, 0);

    // Deux à quatre chemins partent de la place
    let mut arms: Vec<Side> = Side::ALL.into_iter().filter(|_| dice.chance(0.8)).collect();
    if arms.len() < 2 {
        arms = vec![Side::West, Side::East];
    }
    let mut houses = 0;
    for &arm in &arms {
        let length = dice.range(18, p.area.width().min(2 * SettlementKind::Village.radius()) / 2 + 1);
        let (dx, dz) = arm.delta();
        let along_x = dz == 0;
        // Le chemin, large de 3
        let (a, b) = ((x + dx * 4, z + dz * 4), (x + dx * length, z + dz * length));
        let road = Rect::new(a.0.min(b.0) - (!along_x) as i32, a.1.min(b.1) - along_x as i32, a.0.max(b.0) + 1 + (!along_x) as i32, a.1.max(b.1) + 1 + along_x as i32);
        p.take(Piece::Paving { rect: road, block: path }, 0);
        // Maisons des deux côtés, porte sur le chemin
        let mut t = 7;
        while t + 3 <= length {
            for side in [-1, 1] {
                if !dice.chance(0.85) {
                    continue;
                }
                let (w, d) = (dice.range(5, 8), dice.range(5, 8));
                let style = Style::village(biome, dice);
                let storeys = if dice.chance(0.12) { 2 } else { 1 };
                if p.house_along((x + dx * t, z + dz * t), along_x, side, 1, w, d, storeys, style) {
                    houses += 1;
                }
            }
            // Un lampadaire au bord du chemin, de loin en loin
            if t % 2 == 1 && dice.chance(0.5) {
                let side = if dice.chance(0.5) { 2 } else { -2 };
                let lamp = if along_x { (x + dx * t, z + side) } else { (x + side, z + dz * t) };
                let rect = Rect::sized(lamp.0, lamp.1, 1, 1);
                if p.free(&rect) {
                    p.take(Piece::Lamp { x: lamp.0, z: lamp.1 }, 0);
                }
            }
            t += dice.range(6, 9);
        }
    }
    if houses < 2 {
        return None;
    }
    // Champs aux abords, sur le terrain le plus plat
    let crops = [WHEAT, WHEAT, CARROT, POTATO, SQUASH];
    let fields = dice.range(1, 4);
    let mut placed = 0;
    for _ in 0..fields * 4 {
        if placed >= fields {
            break;
        }
        let (w, d) = (dice.range(7, 12), dice.range(6, 10));
        let r = SettlementKind::Village.radius();
        let fx = x + dice.range(-r, r - w);
        let fz = z + dice.range(-r, r - d);
        let rect = Rect::sized(fx, fz, w, d);
        if !p.free(&rect.expand(1)) || floor_for(p.generator, p.face, &rect, 2).is_none() {
            continue;
        }
        p.take(Piece::Field { rect, crop: dice.pick(&crops), rows_along_x: dice.chance(0.5) }, 1);
        placed += 1;
    }
    p.finish(SettlementKind::Village, center, biome)
}

fn plan_town(mut p: Planner, center: (i32, i32), biome: BiomeType, dice: &mut Dice) -> Option<Settlement> {
    let (x, z) = center;
    // Le damier : rues tous les 12 blocs, remparts à 36 blocs du centre
    const STEP: i32 = 12;
    const HALF: i32 = 36;
    let walls = Rect::new(x - HALF, z - HALF, x + HALF + 1, z + HALF + 1);

    // Fontaine au milieu de la grand-place (posée en premier : elle passe avant le pavage)
    let fountain = Rect::around(x, z, 2);
    let floor = floor_for(p.generator, p.face, &fountain, 3)?;
    p.take(Piece::Fountain { rect: fountain, floor }, 0);
    // Tours d'angle, puis portes et remparts
    for (tx, tz) in [(walls.x0, walls.z0), (walls.x1 - 1, walls.z0), (walls.x0, walls.z1 - 1), (walls.x1 - 1, walls.z1 - 1)] {
        let rect = Rect::around(tx, tz, 2);
        let floor = floor_for(p.generator, p.face, &rect, 6)?;
        p.take(Piece::Tower { rect, floor }, 0);
    }
    let gates = [
        Rect::new(x - 1, walls.z0, x + 2, walls.z0 + 1),
        Rect::new(x - 1, walls.z1 - 1, x + 2, walls.z1),
        Rect::new(walls.x0, z - 1, walls.x0 + 1, z + 2),
        Rect::new(walls.x1 - 1, z - 1, walls.x1, z + 2),
    ];
    for gate in gates {
        p.take(Piece::Gate { rect: gate }, 0);
    }
    for rect in [
        Rect::new(walls.x0, walls.z0, walls.x1, walls.z0 + 1),
        Rect::new(walls.x0, walls.z1 - 1, walls.x1, walls.z1),
        Rect::new(walls.x0, walls.z0, walls.x0 + 1, walls.z1),
        Rect::new(walls.x1 - 1, walls.z0, walls.x1, walls.z1),
    ] {
        p.pieces.push(Piece::Rampart { rect });
    }
    // Les remparts et leurs abords sont pris, l'intérieur reste à bâtir
    for side in [
        Rect::new(walls.x0 - 2, walls.z0 - 2, walls.x1 + 2, walls.z0 + 2),
        Rect::new(walls.x0 - 2, walls.z1 - 2, walls.x1 + 2, walls.z1 + 2),
        Rect::new(walls.x0 - 2, walls.z0 - 2, walls.x0 + 2, walls.z1 + 2),
        Rect::new(walls.x1 - 2, walls.z0 - 2, walls.x1 + 2, walls.z1 + 2),
    ] {
        p.taken.push(side);
    }

    // Grand-place pavée autour de la fontaine
    let plaza = Rect::around(x, z, STEP - 2);
    p.pieces.push(Piece::Paving { rect: plaza, block: COBBLE });
    p.taken.push(plaza.expand(1));
    // Rues pavées
    let mut offset = -(HALF / STEP) * STEP;
    while offset <= HALF {
        if offset.abs() < HALF {
            let street_x = Rect::new(walls.x0 + 1, z + offset - 1, walls.x1 - 1, z + offset + 2);
            let street_z = Rect::new(x + offset - 1, walls.z0 + 1, x + offset + 2, walls.z1 - 1);
            for street in [street_x, street_z] {
                p.pieces.push(Piece::Paving { rect: street, block: COBBLE });
                p.taken.push(street);
            }
        }
        offset += STEP;
    }
    // Un pâté de maisons entre chaque paire de rues : une grande maison ou deux petites
    let mut houses = 0;
    let mut bz = -HALF;
    while bz < HALF {
        let mut bx = -HALF;
        while bx < HALF {
            // Pâté de 9 × 9 entre deux rues de 3 de large
            let block = Rect::new(x + bx + 2, z + bz + 2, x + bx + STEP - 1, z + bz + STEP - 1);
            bx += STEP;
            if !dice.chance(0.9) {
                continue;
            }
            // Porte sur la rue la plus proche du centre
            let (bcx, bcz) = block.center();
            let door = if (bcx - x).abs() > (bcz - z).abs() {
                if bcx > x { Side::West } else { Side::East }
            } else if bcz > z {
                Side::North
            } else {
                Side::South
            };
            let style = Style::town(biome, dice);
            let storeys = if dice.chance(0.55) { 2 } else { 1 };
            // Maison de 5 à 7 de côté, avant-toits compris dans le pâté
            let (w, d) = (dice.range(5, 8), dice.range(5, 8));
            let fx = block.x0 + 1 + dice.range(0, 8 - w);
            let fz = block.z0 + 1 + dice.range(0, 8 - d);
            if p.try_house(Rect::sized(fx, fz, w, d), door, storeys, style) {
                houses += 1;
            }
        }
        bz += STEP;
    }
    if houses < 6 {
        return None;
    }
    // Lampadaires aux carrefours, au coin des rues
    let mut lx = -(HALF / STEP) * STEP + STEP;
    while lx < HALF {
        let mut lz = -(HALF / STEP) * STEP + STEP;
        while lz < HALF {
            if (lx, lz) != (0, 0) && dice.chance(0.7) {
                p.pieces.push(Piece::Lamp { x: x + lx + 1, z: z + lz + 1 });
            }
            lz += STEP;
        }
        lx += STEP;
    }
    p.finish(SettlementKind::Town, center, biome)
}

fn plan_ruins(mut p: Planner, center: (i32, i32), biome: BiomeType, dice: &mut Dice) -> Option<Settlement> {
    let (x, z) = center;
    let seed = dice.next();
    match dice.range(0, 3) {
        0 => {
            // Temple : podium de 9 à 13 de côté, colonnes une sur deux sur le pourtour
            let side = 9 + 2 * dice.range(0, 3);
            let foot = Rect::around(x, z, side / 2);
            let floor = floor_for(p.generator, p.face, &foot, 8)? + 1;
            p.take(Piece::Temple(Temple { foot, floor, seed }), 0);
        }
        1 => {
            // Vieille tour, avec une maison effondrée à son pied
            let radius = dice.range(3, 5);
            let tower = Rect::around(x, z, radius);
            let floor = floor_for(p.generator, p.face, &tower, 8)?;
            let height = dice.range(8, 15);
            p.take(Piece::RuinTower(RuinTower { center, radius, floor, height, seed }), 1);
            let (w, d) = (dice.range(5, 8), dice.range(5, 8));
            let foot = Rect::sized(x + radius + 2, z - d / 2, w, d);
            if p.free(&foot) {
                if let Some(floor) = floor_for(p.generator, p.face, &foot, 6) {
                    p.take(Piece::RuinHouse(RuinHouse { foot, floor, height: dice.range(2, 5), seed: seed ^ 1 }), 1);
                }
            }
        }
        _ => {
            // Hameau abandonné : deux à quatre maisons effondrées
            let mut count = 0;
            for i in 0..8 {
                if count >= 4 {
                    break;
                }
                let (w, d) = (dice.range(5, 9), dice.range(5, 9));
                let fx = x + dice.range(-10, 11 - w);
                let fz = z + dice.range(-10, 11 - d);
                let foot = Rect::sized(fx, fz, w, d);
                if !p.free(&foot.expand(1)) {
                    continue;
                }
                let Some(floor) = floor_for(p.generator, p.face, &foot, 6) else { continue };
                p.take(Piece::RuinHouse(RuinHouse { foot, floor, height: dice.range(2, 6), seed: seed ^ (i as u64 + 7) }), 1);
                count += 1;
            }
            if count < 2 {
                return None;
            }
        }
    }
    p.finish(SettlementKind::Ruins, center, biome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cubed_sphere::PlanetDescriptor;

    fn generator(seed: u64) -> PlanetWorldGenerator {
        PlanetWorldGenerator::new(PlanetDescriptor { seed, ..Default::default() })
    }

    fn all_settlements(generator: &PlanetWorldGenerator) -> Vec<&Settlement> {
        let mut found = Vec::new();
        for face in PlanetFace::ALL {
            for cz in 0..CELLS_PER_FACE {
                for cx in 0..CELLS_PER_FACE {
                    if let Some(s) = generator.structures.cell(generator, face, cx, cz) {
                        found.push(s);
                    }
                }
            }
        }
        found
    }

    #[test]
    fn test_structure_blocks_have_sheets() {
        for (block, id) in block::ALL {
            assert_eq!(herocraft_common::blocks().find(id), Some(block), "{id}");
        }
        assert!(herocraft_common::blocks().is_known(facing(PLANK_STAIRS, Side::West)));
        assert!(herocraft_common::blocks().is_known(facing(LADDER, Side::East)));
    }

    #[test]
    fn test_planet_has_towns_villages_and_ruins() {
        for seed in [2026, 1234567890] {
            let generator = generator(seed);
            let settlements = all_settlements(&generator);
            let count = |kind| settlements.iter().filter(|s| s.kind == kind).count();
            let (towns, villages, ruins) = (count(SettlementKind::Town), count(SettlementKind::Village), count(SettlementKind::Ruins));
            println!("graine {seed} : {towns} villes, {villages} villages, {ruins} ruines");
            assert!(towns >= 1, "aucune ville");
            assert!(villages >= 5, "seulement {villages} villages");
            assert!(ruins >= 5, "seulement {ruins} ruines");
            for s in &settlements {
                // Le plan reste dans sa case, donc sur sa face
                let (cx, cz) = (s.center.0.div_euclid(CELL), s.center.1.div_euclid(CELL));
                let cell = Rect::sized(cx * CELL, cz * CELL, CELL, CELL);
                assert!(cell.contains(s.bounds.x0, s.bounds.z0) && cell.contains(s.bounds.x1 - 1, s.bounds.z1 - 1), "{:?} sort de sa case", s.kind);
                assert!(s.top <= crate::edits::WORLD_HEIGHT);
            }
        }
    }

    #[test]
    fn test_plans_do_not_depend_on_reading_order() {
        let a = generator(77);
        let b = generator(77);
        let first: Vec<_> = all_settlements(&a).iter().map(|s| (s.face, s.center, s.kind, s.pieces.len())).collect();
        // Lecture dans l'ordre inverse
        let mut second = Vec::new();
        for face in PlanetFace::ALL.iter().rev() {
            for cz in (0..CELLS_PER_FACE).rev() {
                for cx in (0..CELLS_PER_FACE).rev() {
                    if let Some(s) = b.structures.cell(&b, *face, cx, cz) {
                        second.push((s.face, s.center, s.kind, s.pieces.len()));
                    }
                }
            }
        }
        second.sort_by_key(|k| (face_index(k.0), k.1 .1.div_euclid(CELL), k.1 .0.div_euclid(CELL)));
        let mut first_sorted = first.clone();
        first_sorted.sort_by_key(|k| (face_index(k.0), k.1 .1.div_euclid(CELL), k.1 .0.div_euclid(CELL)));
        assert_eq!(first_sorted, second);
    }

    #[test]
    fn test_chunks_single_blocks_and_support_agree_in_a_town() {
        use crate::edits::{BlockPos, TerrainEdits, TerrainQuery, WORLD_HEIGHT};
        use herocraft_common::PlanetChunkCoord;
        let generator = generator(2026);
        let town = all_settlements(&generator).into_iter().find(|s| s.kind == SettlementKind::Town).expect("une ville");
        let face = town.face;
        let edits = TerrainEdits::new(None);
        let mut query = TerrainQuery::new(&generator, &edits);
        // Le chunk du centre (la fontaine) et celui d'une tour d'angle, sur toute leur hauteur utile
        for (bx, bz) in [town.center, (town.bounds.x0 + 2, town.bounds.z0 + 2)] {
            let (cx, cz) = (bx.div_euclid(32), bz.div_euclid(32));
            let ground = generator.sample_block_column(face, bx, bz).elevation.floor() as i32;
            let mut structure_blocks = 0;
            for layer in (ground - 10).div_euclid(32)..=(town.top).div_euclid(32) {
                let chunk = generator.generate_chunk(&PlanetChunkCoord::new(face, cx, cz, layer));
                for z in (0..32).step_by(3) {
                    for x in 0..32 {
                        for y in 0..32 {
                            let (gx, gy, gz) = (cx * 32 + x, layer * 32 + y, cz * 32 + z);
                            let block = generator.block_at(face, gx, gy, gz);
                            assert_eq!(chunk.get_voxel(x as u8, y as u8, z as u8), block, "({gx}, {gy}, {gz})");
                            if !block.is_air() && !matches!(block, BlockId::STONE | BlockId::DIRT | BlockId::GRASS | BlockId::SAND | BlockId::WATER) {
                                structure_blocks += 1;
                            }
                        }
                    }
                }
            }
            assert!(structure_blocks > 100, "seulement {structure_blocks} blocs bâtis");
            // Le dessus porteur d'une colonne bâtie : le premier bloc non vide en descendant
            for (x, z) in [(bx, bz), (bx + 1, bz), (bx + 3, bz + 2)] {
                for from_y in [WORLD_HEIGHT - 1, town.top + 2, ground + 3, ground, ground - 3] {
                    let naive = (0..=from_y).rev().find(|&y| crate::edits::bears(edits.block(&generator, BlockPos::new(face, x, y, z)))).map_or(0, |y| y + 1);
                    assert_eq!(query.support_top(face, x, z, from_y), naive, "({x}, {z}) depuis {from_y}");
                }
            }
        }
    }

    #[test]
    fn test_houses_have_walls_roof_and_open_door() {
        let generator = generator(2026);
        let village = all_settlements(&generator).into_iter().find(|s| s.kind == SettlementKind::Village).expect("un village");
        let house = village.pieces.iter().find_map(|p| if let Piece::House(h) = p { Some(*h) } else { None }).unwrap();
        let face = village.face;
        let block = |x: i32, y: i32, z: i32| generator.block_at(face, x, y, z);
        let (dx, dz) = house.door_column();
        assert!(block(dx, house.floor, dz).is_air() && block(dx, house.floor + 1, dz).is_air(), "porte bouchée");
        assert!(!block(dx, house.floor - 1, dz).is_air(), "seuil manquant");
        let (cx, cz) = house.foot.center();
        // Plancher, air dedans, toit au-dessus
        assert!(!block(cx, house.floor - 1, cz).is_air());
        assert!(block(cx, house.floor + 1, cz).is_air() || house.storeys > 1);
        assert!((house.floor + house.wall_height()..house.top()).any(|y| !block(cx, y, cz).is_air()), "pas de toit");
        // Un coin du mur
        assert_eq!(block(house.foot.x0, house.floor, house.foot.z0), house.style.corner);
    }
}
