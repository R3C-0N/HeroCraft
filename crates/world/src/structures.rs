//! Villes, villages et ruines posés par le générateur de terrain.
//!
//! Chaque face du cube est découpée en cases de `CELL` × `CELL` blocs, regroupées en régions de
//! `REGION_CELLS` × `REGION_CELLS` cases (2 048 blocs de côté). Chaque région accueille au plus une
//! ville ou un village, dans une case tirée au sort près de son centre : deux établissements voisins
//! sont ainsi à 1 300 à 2 800 blocs l'un de l'autre, 4 à 9 minutes de marche. Les autres cases
//! reçoivent parfois des ruines. La graine du monde décide de tout, une fois pour toutes ; le
//! générateur cherche ensuite dans la case un site qui s'y prête (terre ferme, relief assez plat,
//! biome) et y dessine le plan : rues, maisons, puits, champs, remparts, colonnes effondrées… Un plan ne sort
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

use crate::culture::Culture;
use crate::generator::{BiomeType, PlanetWorldGenerator, SEA_LEVEL_BLOCKS};
use crate::temples::Epoch;

mod memory_city;
pub use memory_city::{MemoryCity, Ring};

/// Côté d'une case de structures, en blocs : le plus grand plan (une ville) y tient
pub const CELL: i32 = 128;
/// Cases par arête de face
pub const CELLS_PER_FACE: i32 = crate::edits::FACE_BLOCKS / CELL;
/// Cases par côté d'une région, qui accueille au plus une ville ou un village (2 048 blocs : à
/// 5,2 blocs par seconde, de 4 à 9 minutes de marche d'un établissement au suivant)
pub(crate) const REGION_CELLS: i32 = 16;
/// Les villes et villages s'installent à au moins tant de cases du bord de leur région : deux
/// établissements de régions voisines restent à `2 × REGION_MARGIN` cases l'un de l'autre au moins
const REGION_MARGIN: i32 = 5;
/// Régions par arête de face (la dernière est rognée si la face n'en contient pas un nombre entier)
pub const REGIONS_PER_FACE: i32 = (CELLS_PER_FACE + REGION_CELLS - 1) / REGION_CELLS;
/// Côté d'une région, en blocs
pub const REGION_BLOCKS: i32 = REGION_CELLS * CELL;
/// Cases essayées par région pour sa ville ou son village (la première peut tomber en mer)
const REGION_CANDIDATES: usize = 4;
/// Chance qu'une case hors des sites de villes et villages reçoive des ruines (environ trois
/// par région)
const RUINS_CHANCE: f64 = 0.014;

/// Blocs des structures, par leur numéro de fiche (`assets/blocks/`, numéros figés)
pub mod block {
    use herocraft_common::BlockId;

    pub const STONE: BlockId = BlockId::STONE;
    pub const WATER: BlockId = BlockId::WATER;
    pub(crate) const OAK_LOG: BlockId = BlockId::WOOD;
    pub(crate) const BIRCH_LOG: BlockId = BlockId(14);
    pub(crate) const PINE_LOG: BlockId = BlockId(17);
    pub(crate) const FURNACE: BlockId = BlockId(20);
    pub(crate) const WORKBENCH: BlockId = BlockId(23);
    pub(crate) const BRICK: BlockId = BlockId(26);
    pub(crate) const WHITE_BRICK: BlockId = BlockId(27);
    pub(crate) const GLASS: BlockId = BlockId(41);
    pub(crate) const CUT_STONE: BlockId = BlockId(43);
    pub(crate) const PLANK: BlockId = BlockId(51);
    pub(crate) const WOOD_PANEL: BlockId = BlockId(55);
    pub(crate) const WHEAT: BlockId = BlockId(59);
    pub(crate) const CARROT: BlockId = BlockId(63);
    pub(crate) const SQUASH: BlockId = BlockId(67);
    pub(crate) const POTATO: BlockId = BlockId(75);
    pub const FARMLAND: BlockId = BlockId(84);
    pub(crate) const LADDER: BlockId = BlockId(88);
    pub(crate) const TORCH: BlockId = BlockId(92);
    pub(crate) const COBBLE: BlockId = BlockId(120);
    pub(crate) const GRAVEL: BlockId = BlockId(123);
    pub(crate) const HARD_STONE: BlockId = BlockId(124);
    pub(crate) const PLANK_SLAB: BlockId = BlockId(136);
    pub(crate) const PLANK_STAIRS: BlockId = BlockId(137);
    pub(crate) const COBBLE_SLAB: BlockId = BlockId(138);
    pub(crate) const COBBLE_STAIRS: BlockId = BlockId(139);
    pub(crate) const BRICK_SLAB: BlockId = BlockId(140);
    pub(crate) const BRICK_STAIRS: BlockId = BlockId(141);
    pub(crate) const GREEN_LEAF: BlockId = BlockId(9);
    pub(crate) const DARK_LEAF: BlockId = BlockId(15);
    pub(crate) const LIGHT_LEAF: BlockId = BlockId(16);
    pub(crate) const FLINT_BRICK: BlockId = BlockId(40);
    pub(crate) const GLOWBELL_BLOOM: BlockId = BlockId(105);
    pub(crate) const BASALT: BlockId = BlockId(118);
    // Barrières, dont les poteaux des lampadaires
    pub(crate) const OAK_FENCE: BlockId = BlockId(148);
    pub(crate) const BIRCH_FENCE: BlockId = BlockId(149);
    pub(crate) const CUT_STONE_FENCE: BlockId = BlockId(154);
    pub(crate) const SILVER_FENCE: BlockId = BlockId(161);
    pub(crate) const LANTERN: BlockId = BlockId(163);
    // Ornements : l'aspect des métaux et du cristal, sans leur valeur. Cassés, ils ne donnent
    // qu'eux-mêmes et aucune recette ne les refond en lingots ni en minerai : piller une ville ne
    // rapporte ni or, ni mithril, ni cristal.
    pub(crate) const BRONZE_TRIM: BlockId = BlockId(142);
    pub(crate) const SILVER_TRIM: BlockId = BlockId(143);
    pub(crate) const GILDING: BlockId = BlockId(144);
    pub(crate) const IRON_TRIM: BlockId = BlockId(145);
    pub(crate) const CUT_CRYSTAL: BlockId = BlockId(146);
    // Sols et matériaux des villages des races sans ville
    pub const DIRT: BlockId = BlockId::DIRT;
    pub const GRASS: BlockId = BlockId::GRASS;
    pub const SAND: BlockId = BlockId::SAND;
    pub(crate) const PEAT: BlockId = BlockId(134);
    pub(crate) const VOID_ROCK: BlockId = BlockId(129);
    pub(crate) const PINE_FENCE: BlockId = BlockId(150);
    pub(crate) const BRONZE_FENCE: BlockId = BlockId(160);

    /// Tous les blocs que posent les structures (pour vérifier qu'ils ont leur fiche)
    pub const ALL: [(BlockId, &str); 52] = [
        (DIRT, "dirt"),
        (GRASS, "grass"),
        (SAND, "sand"),
        (PEAT, "peat"),
        (VOID_ROCK, "voidRock"),
        (PINE_FENCE, "barrierePin"),
        (BRONZE_FENCE, "barriereBronze"),
        (LANTERN, "lanterne"),
        (OAK_FENCE, "barriereChene"),
        (BIRCH_FENCE, "barriereBouleau"),
        (CUT_STONE_FENCE, "barrierePierreTaillee"),
        (SILVER_FENCE, "barriereMithril"),
        (IRON_TRIM, "ornementFer"),
        (STONE, "stone"),
        (WATER, "water"),
        (OAK_LOG, "oakTrunk"),
        (CUT_CRYSTAL, "cristalTaille"),
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
        (GREEN_LEAF, "greenLeaf"),
        (DARK_LEAF, "darkLeaf"),
        (LIGHT_LEAF, "lightLeaf"),
        (BRONZE_TRIM, "ornementBronze"),
        (FLINT_BRICK, "flintBrick"),
        (GILDING, "ornementOr"),
        (SILVER_TRIM, "ornementMithril"),
        (GLOWBELL_BLOOM, "glowbellBloom"),
        (BASALT, "basalt"),
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

/// Pan de toit : l'escalier `b` tourné vers `back`, ou `b` tel quel si ce n'est pas un escalier
/// (toit de feuillage des Elfes des bois)
fn roofing(b: BlockId, back: Side) -> BlockId {
    if [PLANK_STAIRS, COBBLE_STAIRS, BRICK_STAIRS].contains(&b) {
        facing(b, back)
    } else {
        b
    }
}

// ---------------------------------------------------------------------------
// Hasard déterministe
// ---------------------------------------------------------------------------

pub(crate) fn mix(mut x: u64) -> u64 {
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58476d1ce4e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d049bb133111eb);
    x ^= x >> 31;
    x
}

/// Empreinte d'une position (x, z) pour la graine `seed`
pub(crate) fn hash2(seed: u64, x: i32, z: i32) -> u64 {
    mix(seed ^ (x as u32 as u64).wrapping_mul(0x9E3779B97F4A7C15) ^ (z as u32 as u64).wrapping_mul(0xC2B2AE3D27D4EB4F))
}

/// Nombre dans [0, 1) tiré d'une empreinte
pub(crate) fn unit(h: u64) -> f64 {
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
    pub(crate) fn on_edge(&self, x: i32, z: i32) -> bool {
        x == self.x0 || x == self.x1 - 1 || z == self.z0 || z == self.z1 - 1
    }

    /// La colonne est un coin
    #[inline]
    pub(crate) fn on_corner(&self, x: i32, z: i32) -> bool {
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

    pub fn delta(self) -> (i32, i32) {
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
    /// Maison de village, selon la culture et le climat
    fn village(culture: Culture, biome: BiomeType, dice: &mut Dice) -> Style {
        match culture {
            Culture::Humains => Self::human_village(biome, dice),
            // Pierre taillée ou silex, angles de pierre dure, toit d'ardoise et faîte de basalte
            Culture::Nains => {
                let wall = if dice.chance(0.4) { FLINT_BRICK } else { CUT_STONE };
                Style { wall, corner: HARD_STONE, floor: CUT_STONE, foundation: HARD_STONE, roof: Some(COBBLE_STAIRS), ridge: BASALT }
            }
            // Bois clair, troncs aux angles, toit de feuillage vivant
            Culture::ElfesDesBois => Self::sylvan(dice),
            // Murs blanchis, angles de mithril, toit d'ardoise au faîte d'argent
            Culture::HautsElfes => Style {
                wall: WHITE_BRICK,
                corner: SILVER_TRIM,
                floor: PLANK,
                foundation: CUT_STONE,
                roof: Some(COBBLE_STAIRS),
                ridge: SILVER_TRIM,
            },
            // Hutte basse de basalte et de pin, toit de tourbe au faîte de fer
            Culture::DemiOrques => Style { wall: BASALT, corner: PINE_LOG, floor: PLANK, foundation: HARD_STONE, roof: Some(PEAT), ridge: IRON_TRIM },
            // Pierre sombre et bois flotté, toit de silex au faîte de pierre du vide
            Culture::Revenants => Style { wall: HARD_STONE, corner: OAK_LOG, floor: PLANK, foundation: OAK_LOG, roof: Some(FLINT_BRICK), ridge: VOID_ROCK },
            // Murs blancs et bouleau, toit de verre au faîte de cristal
            Culture::Sylphes => Style { wall: WHITE_BRICK, corner: BIRCH_LOG, floor: PLANK, foundation: CUT_STONE, roof: Some(GLASS), ridge: CUT_CRYSTAL },
            // Butte de terre coiffée d'herbe (`Form::Burrow`), porte cerclée de bronze
            Culture::Gnomes => Style { wall: DIRT, corner: BRONZE_TRIM, floor: PLANK, foundation: COBBLE, roof: None, ridge: GRASS },
            // Pierre blanche, angles de pierre taillée, faîte d'or
            Culture::Aasimars => Style { wall: WHITE_BRICK, corner: CUT_STONE, floor: PLANK, foundation: CUT_STONE, roof: Some(COBBLE_STAIRS), ridge: GILDING },
            // Rondins sur plateforme, toit de tourbe au faîte de feuillage
            Culture::HommesBetes => {
                let wall = dice.pick(&[OAK_LOG, PINE_LOG]);
                Style { wall, corner: OAK_LOG, floor: PLANK, foundation: OAK_LOG, roof: Some(PEAT), ridge: DARK_LEAF }
            }
        }
    }

    /// Maison de village humaine, selon le climat
    fn human_village(biome: BiomeType, dice: &mut Dice) -> Style {
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

    /// Maison des Elfes des bois : panneaux ou rondins de bouleau, troncs de chêne, feuillage
    fn sylvan(dice: &mut Dice) -> Style {
        let wall = dice.pick(&[WOOD_PANEL, BIRCH_LOG, WOOD_PANEL]);
        let corner = if wall == BIRCH_LOG { OAK_LOG } else { dice.pick(&[OAK_LOG, BIRCH_LOG]) };
        let leaf = dice.pick(&[GREEN_LEAF, DARK_LEAF, LIGHT_LEAF]);
        Style { wall, corner, floor: PLANK, foundation: OAK_LOG, roof: Some(leaf), ridge: leaf }
    }

    /// Maison de ville, selon la culture et le climat
    fn town(culture: Culture, biome: BiomeType, dice: &mut Dice) -> Style {
        match culture {
            Culture::Humains => {
                if matches!(biome, BiomeType::Desert | BiomeType::Savanna) {
                    return Style { wall: WHITE_BRICK, corner: CUT_STONE, floor: PLANK, foundation: CUT_STONE, roof: None, ridge: WHITE_BRICK };
                }
                match dice.range(0, 3) {
                    0 => Style { wall: BRICK, corner: CUT_STONE, floor: PLANK, foundation: CUT_STONE, roof: Some(COBBLE_STAIRS), ridge: COBBLE },
                    1 => Style { wall: WHITE_BRICK, corner: OAK_LOG, floor: PLANK, foundation: COBBLE, roof: Some(BRICK_STAIRS), ridge: BRICK },
                    _ => Style { wall: PLANK, corner: CUT_STONE, floor: WOOD_PANEL, foundation: COBBLE, roof: Some(BRICK_STAIRS), ridge: BRICK },
                }
            }
            // Ville naine : pierre massive, angles de bronze ou de fer, terrasses de pierre
            Culture::Nains => match dice.range(0, 3) {
                0 => Style { wall: CUT_STONE, corner: BRONZE_TRIM, floor: CUT_STONE, foundation: HARD_STONE, roof: Some(COBBLE_STAIRS), ridge: BASALT },
                1 => Style { wall: FLINT_BRICK, corner: CUT_STONE, floor: CUT_STONE, foundation: HARD_STONE, roof: Some(COBBLE_STAIRS), ridge: BRONZE_TRIM },
                _ => Style { wall: HARD_STONE, corner: IRON_TRIM, floor: CUT_STONE, foundation: BASALT, roof: None, ridge: CUT_STONE },
            },
            Culture::ElfesDesBois => Self::sylvan(dice),
            // Cité des Hauts Elfes : blanc, mithril et or
            Culture::HautsElfes => {
                let ridge = if dice.chance(0.5) { GILDING } else { SILVER_TRIM };
                let corner = if dice.chance(0.5) { SILVER_TRIM } else { CUT_STONE };
                Style { wall: WHITE_BRICK, corner, floor: PLANK, foundation: CUT_STONE, roof: Some(COBBLE_STAIRS), ridge }
            }
            // Les autres peuples ne bâtissent pas de ville
            _ => Self::village(culture, biome, dice),
        }
    }
}

/// Façon de bâtir d'une culture : hauteur des maisons, forme des toits
#[derive(Debug, Clone, Copy)]
struct Habits {
    /// Chance qu'une maison ait un étage, au village et en ville
    upstairs: (f64, f64),
    /// Chance d'un toit à quatre pans
    hip: f64,
    /// Chance d'un porche
    porch: f64,
    /// Chance d'une forge, d'une chapelle et d'une taverne au village
    forge: f64,
    church: f64,
    tavern: f64,
    /// Largeur et profondeur des maisons du village (bornes incluse, exclue)
    width: (i32, i32),
    depth: (i32, i32),
    /// Forme des maisons
    form: Form,
    /// Hauteur des pilotis sous le plancher (0 : maisons posées au sol)
    stilts: i32,
    /// Le porche des maisons sur pilotis descend en marche jusqu'au sol (sinon, il faut sauter)
    steps: bool,
    /// Une grange aux abords des champs
    barn: bool,
}

impl Habits {
    /// Façon des quatre peuples bâtisseurs de villes
    const fn builders(upstairs: (f64, f64), hip: f64, porch: f64, forge: f64) -> Habits {
        Habits { upstairs, hip, porch, forge, church: 0.75, tavern: 0.6, width: (5, 9), depth: (5, 8), form: Form::Box, stilts: 0, steps: false, barn: true }
    }

    fn of(culture: Culture) -> Habits {
        let builders = Habits::builders((0.0, 0.0), 0.0, 0.0, 0.0);
        match culture {
            Culture::Humains => Habits::builders((0.15, 0.55), 0.35, 0.3, 0.6),
            // Maisons basses et trapues, une forge partout
            Culture::Nains => Habits::builders((0.05, 0.25), 0.8, 0.1, 1.0),
            Culture::ElfesDesBois => Habits::builders((0.25, 0.5), 0.5, 0.6, 0.3),
            // Demeures élancées, toits pointus
            Culture::HautsElfes => Habits::builders((0.4, 0.85), 0.75, 0.35, 0.4),
            // Huttes basses et larges, une forge toujours, pas de chapelle ni de grange
            Culture::DemiOrques => Habits { upstairs: (0.0, 0.0), hip: 0.3, porch: 0.0, forge: 1.0, church: 0.0, tavern: 0.7, width: (6, 9), depth: (5, 7), barn: false, ..builders },
            // Maisons sur pilotis, un porche en marche devant chaque porte
            Culture::Revenants => Habits { upstairs: (0.1, 0.1), hip: 0.2, porch: 1.0, forge: 0.2, church: 0.0, tavern: 0.4, width: (5, 8), depth: (5, 7), stilts: 2, steps: true, barn: false, ..builders },
            // Tours fines à étage, toits pointus
            Culture::Sylphes => Habits { upstairs: (0.9, 0.9), hip: 0.9, porch: 0.15, forge: 0.1, church: 0.0, tavern: 0.3, width: (5, 7), depth: (5, 7), barn: false, ..builders },
            // Maisons enterrées dans des buttes, une maison de guilde pour taverne
            Culture::Gnomes => Habits { upstairs: (0.0, 0.0), hip: 0.0, porch: 0.0, forge: 0.5, church: 0.0, tavern: 0.7, width: (5, 8), depth: (5, 8), form: Form::Burrow, ..builders },
            // Demeures blanches, le sanctuaire tient lieu de chapelle
            Culture::Aasimars => Habits { upstairs: (0.5, 0.5), hip: 0.6, porch: 0.5, forge: 0.2, church: 0.0, tavern: 0.3, barn: false, ..builders },
            // Huttes sur plateformes à deux blocs, sans marche : on y monte d'un saut
            Culture::HommesBetes => Habits { upstairs: (0.0, 0.0), hip: 1.0, porch: 1.0, forge: 0.3, church: 0.0, tavern: 0.3, width: (5, 7), depth: (5, 7), stilts: 2, barn: false, ..builders },
        }
    }
}

/// Hauteur de la lumière d'un lampadaire au-dessus du sol : un socle, deux barrières, la lanterne
const LAMP_LIGHT: i32 = 3;

/// Matériaux des remparts, des tours et des lampadaires d'une culture
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Masonry {
    /// Corps du mur
    pub wall: BlockId,
    /// Pierres ou rondins semés dans le mur
    pub accent: BlockId,
    /// Créneaux, linteau des portes, haut des tours
    pub cap: BlockId,
    /// Plancher du haut des tours
    pub deck: BlockId,
    /// Poteau des lampadaires (une barrière), posé sur un socle de la pierre `accent`
    pub post: BlockId,
    /// Lumière au sommet des lampadaires
    pub light: BlockId,
    /// Palissade de pieux plutôt que mur crénelé
    pub palisade: bool,
    /// Flèche coiffant les tours (`None` : terrasse crénelée)
    pub spire: Option<BlockId>,
    /// Hauteur du rempart et des tours
    pub height: i32,
    pub tower_height: i32,
}

impl Masonry {
    pub fn of(culture: Culture) -> Masonry {
        match culture {
            Culture::Humains => Masonry {
                wall: COBBLE,
                accent: HARD_STONE,
                cap: CUT_STONE,
                deck: PLANK,
                post: OAK_FENCE,
                light: LANTERN,
                palisade: false,
                spire: None,
                height: 5,
                tower_height: 9,
            },
            // Haut mur de pierre dure, cerclé de bronze
            Culture::Nains => Masonry {
                wall: HARD_STONE,
                accent: BASALT,
                cap: CUT_STONE,
                deck: CUT_STONE,
                post: CUT_STONE_FENCE,
                light: LANTERN,
                palisade: false,
                spire: None,
                height: 6,
                tower_height: 10,
            },
            // Palissade de troncs, tours de guet coiffées de feuillage
            Culture::ElfesDesBois => Masonry {
                wall: OAK_LOG,
                accent: BIRCH_LOG,
                cap: OAK_LOG,
                deck: PLANK,
                post: BIRCH_FENCE,
                light: LANTERN,
                palisade: true,
                spire: Some(DARK_LEAF),
                height: 5,
                tower_height: 9,
            },
            // Mur blanc, couronne de mithril, flèches de cristal
            Culture::HautsElfes => Masonry {
                wall: WHITE_BRICK,
                accent: CUT_STONE,
                cap: SILVER_TRIM,
                deck: PLANK,
                post: SILVER_FENCE,
                light: LANTERN,
                palisade: false,
                spire: Some(CUT_CRYSTAL),
                height: 5,
                tower_height: 11,
            },
            // Palissade de pieux de pin autour du camp
            Culture::DemiOrques => Masonry {
                wall: PINE_LOG,
                accent: BASALT,
                cap: PINE_LOG,
                deck: PLANK,
                post: PINE_FENCE,
                light: LANTERN,
                palisade: true,
                spire: None,
                height: 4,
                tower_height: 7,
            },
            // Lanternes bleues des rivages
            Culture::Revenants => Masonry { post: OAK_FENCE, light: GLOWBELL_BLOOM, ..Masonry::of(Culture::Humains) },
            // Carillons de mithril
            Culture::Sylphes => Masonry { post: SILVER_FENCE, accent: CUT_STONE, ..Masonry::of(Culture::Humains) },
            Culture::Gnomes => Masonry { post: BRONZE_FENCE, accent: COBBLE, ..Masonry::of(Culture::Humains) },
            Culture::Aasimars => Masonry { post: SILVER_FENCE, accent: CUT_STONE, ..Masonry::of(Culture::HautsElfes) },
            Culture::HommesBetes => Masonry { post: OAK_FENCE, accent: OAK_LOG, ..Masonry::of(Culture::ElfesDesBois) },
        }
    }

    /// Plus haut niveau (exclu) d'une tour au-dessus de son plancher
    fn tower_top(&self) -> i32 {
        self.tower_height + if self.spire.is_some() { 4 } else { 1 }
    }
}

/// Ce que contient un bâtiment
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Use {
    /// Maison : établi, fourneau, un étage et son échelle s'il y en a deux
    Home,
    /// Taverne : des tables dans la salle, une cheminée
    Tavern,
    /// Grange : grande porte, bottes de blé au sol
    Barn,
    /// Église ou chapelle : bancs, autel, clocher au fond
    Church,
}

/// Forme du toit en pente
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Roof {
    /// Deux pans et deux pignons
    Gable,
    /// Quatre pans, sans pignon
    Hip,
}

/// Forme d'un bâtiment
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Form {
    /// Murs droits et toit
    Box,
    /// Butte de terre coiffée d'herbe, creusée d'une salle, porte basse (maisons des Gnomes)
    Burrow,
}

/// Hauteur d'une butte au-dessus du plancher (son sommet est le bloc `floor + BURROW_HEIGHT - 1`)
const BURROW_HEIGHT: i32 = 6;

/// Un bâtiment : un rez-de-chaussée (et un étage si `storeys` vaut 2), une porte, des fenêtres, un
/// toit, et selon le cas une cheminée, un porche, un clocher
#[derive(Debug, Clone, Copy)]
pub struct House {
    /// Murs extérieurs
    pub foot: Rect,
    /// Altitude du premier bloc d'air au-dessus du plancher
    pub floor: i32,
    pub storeys: i32,
    /// Hauteur des murs
    pub height: i32,
    pub door: Side,
    /// Largeur de la porte (1, ou 3 pour une grange)
    pub door_width: i32,
    pub style: Style,
    pub roof: Roof,
    pub chimney: bool,
    pub porch: bool,
    pub usage: Use,
    pub form: Form,
    /// Hauteur des pilotis : le plancher est à tant de blocs au-dessus du sol, porté par des poteaux
    pub stilts: i32,
    /// Le porche d'une maison sur pilotis descend d'une marche vers le sol
    pub steps: bool,
}

impl House {
    /// Maison ordinaire
    fn home(foot: Rect, floor: i32, storeys: i32, door: Side, style: Style) -> House {
        House {
            foot,
            floor,
            storeys,
            height: if storeys > 1 { 7 } else { 4 },
            door,
            door_width: 1,
            style,
            roof: Roof::Gable,
            chimney: false,
            porch: false,
            usage: Use::Home,
            form: Form::Box,
            stilts: 0,
            steps: false,
        }
    }

    fn wall_height(&self) -> i32 {
        self.height
    }

    fn door_height(&self) -> i32 {
        match self.usage {
            Use::Barn | Use::Church => 3,
            // Porte d'un bloc d'une maison de gnome : seuls les petits s'y glissent
            Use::Home if self.form == Form::Burrow => 1,
            _ => 2,
        }
    }

    /// Hauteur de la butte au-dessus du plancher en (x, z) : au moins 4 sous la salle et ses murs,
    /// au plus 2 sur le talus qui l'entoure, 0 hors de la butte
    fn burrow_height(&self, x: i32, z: i32) -> i32 {
        let f = &self.foot;
        let (cx, cz) = ((f.x0 + f.x1 - 1) as f64 / 2.0, (f.z0 + f.z1 - 1) as f64 / 2.0);
        let (a, b) = (f.width() as f64 / 2.0 + 1.0, f.depth() as f64 / 2.0 + 1.0);
        let e = ((x as f64 - cx) / a).powi(2) + ((z as f64 - cz) / b).powi(2);
        let natural = if e >= 1.0 { 0 } else { (BURROW_HEIGHT as f64 * (1.0 - e).sqrt()).round() as i32 };
        if f.contains(x, z) {
            natural.max(4)
        } else {
            natural.min(2)
        }
    }

    /// Colonnes que le bâtiment peut changer : murs, avant-toits, porche
    fn rect(&self) -> Rect {
        // Avant-toits, ou seuil devant la porte
        let mut rect = self.foot.expand(1);
        if let Some(porch) = self.porch_rect() {
            rect = rect.union(&porch);
        }
        rect
    }

    /// Le faîte court le long de x (le toit descend vers le nord et le sud)
    fn ridge_along_x(&self) -> bool {
        self.foot.width() >= self.foot.depth()
    }

    /// Altitude du toit à cette colonne (avant-toits compris), plus haute altitude du toit, et le
    /// côté vers lequel il monte (`None` : faîte)
    fn roof_level(&self, x: i32, z: i32) -> (i32, i32, Option<Side>) {
        let base = self.floor + self.wall_height() - 1;
        // Rang depuis l'avant-toit le long de chaque axe, et rang du faîte
        let (zc0, zc1) = (self.foot.z0 - 1, self.foot.z1 + 1);
        let (xc0, xc1) = (self.foot.x0 - 1, self.foot.x1 + 1);
        let kz = (z - zc0).min(zc1 - 1 - z);
        let kx = (x - xc0).min(xc1 - 1 - x);
        let (nz, nx) = (zc1 - zc0, xc1 - xc0);
        let rise_z = if z - zc0 < zc1 - 1 - z { Side::South } else { Side::North };
        let rise_x = if x - xc0 < xc1 - 1 - x { Side::East } else { Side::West };
        let along_x = self.ridge_along_x();
        // Le pan qui compte : en travers du faîte pour deux pans ; le plus proche pour quatre
        let (k, n, rise) = match self.roof {
            Roof::Gable if along_x => (kz, nz, rise_z),
            Roof::Gable => (kx, nx, rise_x),
            Roof::Hip => {
                let across = if along_x { (kz, nz, rise_z) } else { (kx, nx, rise_x) };
                let along = if along_x { (kx, nx, rise_x) } else { (kz, nz, rise_z) };
                if along.0 < across.0 { along } else { across }
            }
        };
        let n_across = if along_x { nz } else { nx };
        let k_max = (n_across - 1) / 2;
        let rising = if n_across % 2 == 1 && k == k_max && n == n_across { None } else { Some(rise) };
        (base + k, base + k_max, rising)
    }

    fn top(&self) -> i32 {
        if self.form == Form::Burrow {
            return self.floor + BURROW_HEIGHT + if self.chimney { 2 } else { 0 };
        }
        let roof_top = match self.style.roof {
            Some(_) => self.roof_level(self.foot.x0, self.foot.z0).1 + 1,
            None => self.floor + self.wall_height() + 2,
        };
        let chimney = if self.chimney { roof_top + 2 } else { roof_top };
        match self.tower_rect() {
            Some(_) => chimney.max(self.floor + self.tower_height() + 3),
            None => chimney,
        }
    }

    /// Colonnes de la porte : la colonne du milieu du côté de la porte, et ses voisines si elle est large
    fn door_column(&self) -> (i32, i32) {
        let (cx, cz) = self.foot.center();
        match self.door {
            Side::North => (cx, self.foot.z0),
            Side::South => (cx, self.foot.z1 - 1),
            Side::West => (self.foot.x0, cz),
            Side::East => (self.foot.x1 - 1, cz),
        }
    }

    /// Distance (le long du mur) de la colonne à la porte, `None` si elle n'est pas sur le mur de la porte
    fn door_offset(&self, x: i32, z: i32) -> Option<i32> {
        let (dx, dz) = self.door_column();
        match self.door {
            Side::North | Side::South => (z == dz).then_some((x - dx).abs()),
            Side::West | Side::East => (x == dx).then_some((z - dz).abs()),
        }
    }

    /// Porche devant la porte : 3 de large, 2 de profondeur
    fn porch_rect(&self) -> Option<Rect> {
        if !self.porch {
            return None;
        }
        let (dx, dz) = self.door_column();
        Some(match self.door {
            Side::North => Rect::new(dx - 1, dz - 2, dx + 2, dz),
            Side::South => Rect::new(dx - 1, dz + 1, dx + 2, dz + 3),
            Side::West => Rect::new(dx - 2, dz - 1, dx, dz + 2),
            Side::East => Rect::new(dx + 1, dz - 1, dx + 3, dz + 2),
        })
    }

    /// Colonne de la cheminée : dans le mur du fond, à un bloc de l'angle
    fn chimney_column(&self) -> (i32, i32) {
        let f = &self.foot;
        match self.door.opposite() {
            Side::North => (f.x0 + 1, f.z0),
            Side::South => (f.x1 - 2, f.z1 - 1),
            Side::West => (f.x0, f.z1 - 2),
            Side::East => (f.x1 - 1, f.z0 + 1),
        }
    }

    /// Clocher d'une église : 3 × 3 au milieu du mur du fond, dans l'emprise de la nef
    fn tower_rect(&self) -> Option<Rect> {
        if self.usage != Use::Church {
            return None;
        }
        let (cx, cz) = self.foot.center();
        let f = &self.foot;
        Some(match self.door.opposite() {
            Side::North => Rect::new(cx - 1, f.z0, cx + 2, f.z0 + 3),
            Side::South => Rect::new(cx - 1, f.z1 - 3, cx + 2, f.z1),
            Side::West => Rect::new(f.x0, cz - 1, f.x0 + 3, cz + 2),
            Side::East => Rect::new(f.x1 - 3, cz - 1, f.x1, cz + 2),
        })
    }

    fn tower_height(&self) -> i32 {
        self.wall_height() + 7
    }

    fn block(&self, x: i32, y: i32, z: i32, ground: i32) -> Option<BlockId> {
        let f = self.floor;
        if self.chimney && (x, z) == self.chimney_column() && y >= f && y < self.top() {
            return Some(if y == self.top() - 1 { COBBLE_SLAB } else if self.form == Form::Burrow { BRICK } else { COBBLE });
        }
        if self.form == Form::Burrow {
            return self.burrow_block(x, y, z, ground);
        }
        if let Some(tower) = self.tower_rect()
            && tower.contains(x, z) && y >= f {
            return self.tower_block(&tower, x, y, z, ground);
        }
        let h = self.wall_height();
        let foot = &self.foot;
        let pitched = self.style.roof.is_some();

        if !foot.contains(x, z) {
            if let Some(porch) = self.porch_rect()
                && porch.contains(x, z) && y < f + 3 {
                return self.porch_block(&porch, x, y, z, ground);
            }
            // Devant la porte : un seuil, et de l'air sur la hauteur de la porte même si le terrain monte
            let (ox, oz) = self.door.delta();
            if self.door_offset(x - ox, z - oz).is_some_and(|d| d <= self.door_width / 2) {
                if y == f - 1 {
                    return Some(self.style.foundation);
                }
                if y < f - 1 && self.stilts > 0 {
                    return None;
                }
                if y >= f && y < f + self.door_height() {
                    return Some(BlockId::AIR);
                }
            }
            // Avant-toit : seulement l'escalier du toit en pente
            if !pitched || !foot.expand(1).contains(x, z) {
                return None;
            }
            let (level, _, rising) = self.roof_level(x, z);
            if y == level {
                return Some(self.roof_block(rising));
            }
            return (y > level && y < ground).then_some(BlockId::AIR);
        }

        if y < f - 1 {
            if self.stilts > 0 {
                // Poteaux aux angles et tous les trois blocs le long des murs ; dessous, le vide
                let along = if z == foot.z0 || z == foot.z1 - 1 { x - foot.x0 } else { z - foot.z0 };
                let post = foot.on_corner(x, z) || (foot.on_edge(x, z) && along % 3 == 0);
                return (y >= ground && post).then_some(self.style.corner);
            }
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
                let door_offset = self.door_offset(x, z);
                if door_offset.is_some_and(|d| d <= self.door_width / 2) && level < self.door_height() {
                    return Some(BlockId::AIR);
                }
                let along = if z == foot.z0 || z == foot.z1 - 1 { x - foot.x0 } else { z - foot.z0 };
                let door_near = door_offset.is_some_and(|d| d <= self.door_width / 2 + 1);
                let window = match self.usage {
                    // Hautes fenêtres étroites
                    Use::Church => along % 3 == 1 && (1..=3).contains(&level),
                    Use::Barn => false,
                    // Paires de vitres à hauteur d'yeux de chaque niveau
                    _ => matches!(along % 4, 1 | 2) && (level == 1 || level == 4),
                };
                if window && !door_near {
                    return Some(GLASS);
                }
                return Some(self.style.wall);
            }
            return Some(self.interior(x, z, level));
        }

        // Toit
        if pitched {
            let (level, _, rising) = self.roof_level(x, z);
            if y == level {
                return Some(self.roof_block(rising));
            }
            if y < level {
                // Pignons d'un toit à deux pans : le mur monte sous le toit aux deux bouts du faîte
                let gable = self.roof == Roof::Gable
                    && if self.ridge_along_x() { x == foot.x0 || x == foot.x1 - 1 } else { z == foot.z0 || z == foot.z1 - 1 };
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

    /// Torches posées au sol à l'intérieur (colonne, niveau au-dessus du plancher) : dans un coin du
    /// côté de la porte à chaque étage, dans les deux coins pour une taverne ou une église, et
    /// devant l'autel d'une église
    pub fn torches(&self) -> impl Iterator<Item = (i32, i32, i32)> + use<> {
        let foot = &self.foot;
        // Les deux coins intérieurs le long du mur de la porte
        let corners = match self.door {
            Side::North => [(foot.x0 + 1, foot.z0 + 1), (foot.x1 - 2, foot.z0 + 1)],
            Side::South => [(foot.x0 + 1, foot.z1 - 2), (foot.x1 - 2, foot.z1 - 2)],
            Side::West => [(foot.x0 + 1, foot.z0 + 1), (foot.x0 + 1, foot.z1 - 2)],
            Side::East => [(foot.x1 - 2, foot.z0 + 1), (foot.x1 - 2, foot.z1 - 2)],
        };
        let both = matches!(self.usage, Use::Tavern | Use::Church);
        let pick = (hash2(self.floor as u64 ^ 0x70C4, foot.x0, foot.z0) % 2) as usize;
        let storeys = if self.storeys > 1 { 2 } else { 1 };
        let in_corners = (0..storeys).flat_map(move |storey| {
            corners.into_iter().enumerate().filter(move |(i, _)| both || *i == pick).map(move |(_, (x, z))| (x, 4 * storey, z))
        });
        in_corners.chain(self.altar_column().map(|(x, z)| (x, 1, z)))
    }

    /// Colonne de l'autel d'une église : sur l'axe de la porte, devant le clocher
    fn altar_column(&self) -> Option<(i32, i32)> {
        if self.usage != Use::Church {
            return None;
        }
        let foot = &self.foot;
        let (dx, dz) = self.door_column();
        let inner_depth = match self.door {
            Side::North | Side::South => foot.depth(),
            _ => foot.width(),
        };
        let depth = inner_depth - 5;
        Some(match self.door {
            Side::North => (dx, foot.z0 + depth),
            Side::South => (dx, foot.z1 - 1 - depth),
            Side::West => (foot.x0 + depth, dz),
            Side::East => (foot.x1 - 1 - depth, dz),
        })
    }

    /// Intérieur, au niveau `level` au-dessus du plancher
    fn interior(&self, x: i32, z: i32, level: i32) -> BlockId {
        let foot = &self.foot;
        let back = self.door.opposite();
        if self.torches().any(|t| t == (x, level, z)) {
            return TORCH;
        }
        if self.storeys > 1 {
            let ladder = self.ladder_column();
            if (x, z) == ladder.0 && level < 4 {
                return facing(LADDER, ladder.1);
            }
            if level == 3 {
                return self.style.floor;
            }
        }
        // Rang depuis le mur de la porte, et décalage depuis l'axe de la porte
        let (dx, dz) = self.door_column();
        let (depth, offset) = match self.door {
            Side::North => (z - foot.z0, x - dx),
            Side::South => (foot.z1 - 1 - z, x - dx),
            Side::West => (x - foot.x0, z - dz),
            Side::East => (foot.x1 - 1 - x, z - dz),
        };
        let inner_depth = match self.door {
            Side::North | Side::South => foot.depth(),
            _ => foot.width(),
        };
        if level == 0 {
            match self.usage {
                Use::Church => {
                    // Bancs de part et d'autre de l'allée, tournés vers l'autel ; l'autel devant le clocher
                    if depth == inner_depth - 5 && offset == 0 {
                        return CUT_STONE;
                    }
                    if depth >= 2 && depth < inner_depth - 5 && depth % 2 == 0 && offset != 0 {
                        return facing(PLANK_STAIRS, self.door);
                    }
                    return BlockId::AIR;
                }
                Use::Barn => {
                    let h = hash2(self.floor as u64 ^ 0xBA, x, z);
                    return if depth > 1 && h.is_multiple_of(4) { WHEAT } else { BlockId::AIR };
                }
                Use::Tavern => {
                    // Tables (une dalle haute) sur une grille, loin de la porte
                    if depth >= 2 && depth % 3 == 2 && offset.rem_euclid(3) == 1 && offset.abs() > 1 {
                        return BlockId(PLANK_SLAB.0 | (1 << BlockId::ROTATION_SHIFT));
                    }
                }
                Use::Home => {}
            }
            let (bx, bz) = match back {
                Side::North => (foot.x0 + 1, foot.z0 + 1),
                Side::South => (foot.x1 - 2, foot.z1 - 2),
                Side::West => (foot.x0 + 1, foot.z1 - 2),
                Side::East => (foot.x1 - 2, foot.z0 + 1),
            };
            if (x, z) == (bx, bz) {
                return WORKBENCH;
            }
            if foot.width() >= 6 && foot.depth() >= 6 {
                let (fx, fz) = match back {
                    Side::North => (foot.x1 - 2, foot.z0 + 1),
                    Side::South => (foot.x0 + 1, foot.z1 - 2),
                    Side::West => (foot.x0 + 1, foot.z0 + 1),
                    Side::East => (foot.x1 - 2, foot.z1 - 2),
                };
                if (x, z) == (fx, fz) {
                    return FURNACE;
                }
            }
        }
        BlockId::AIR
    }

    /// Clocher : murs creux montant au-dessus de la nef, abat-sons ouverts, toit pointu
    fn tower_block(&self, tower: &Rect, x: i32, y: i32, z: i32, ground: i32) -> Option<BlockId> {
        let level = y - self.floor;
        let th = self.tower_height();
        let (cx, cz) = tower.center();
        let middle = x == cx || z == cz;
        if level < th {
            if tower.on_edge(x, z) {
                if tower.on_corner(x, z) {
                    return Some(self.style.corner);
                }
                // Passage depuis la nef, du côté de la porte
                let toward_nave = match self.door {
                    Side::North => z == tower.z0,
                    Side::South => z == tower.z1 - 1,
                    Side::West => x == tower.x0,
                    Side::East => x == tower.x1 - 1,
                };
                if toward_nave && middle && level < 3 {
                    return Some(BlockId::AIR);
                }
                // Abat-sons : baies ouvertes en haut de chaque face
                if middle && (level == th - 3 || level == th - 2) {
                    return Some(BlockId::AIR);
                }
                return Some(self.style.wall);
            }
            return Some(if level == th - 4 { self.style.floor } else { BlockId::AIR });
        }
        match level - th {
            0 => Some(self.style.ridge),
            1 if (x, z) == (cx, cz) => Some(self.style.ridge),
            1 => Some(match self.style.roof {
                Some(stairs) if middle => roofing(stairs, if x < cx { Side::East } else if x > cx { Side::West } else if z < cz { Side::South } else { Side::North }),
                _ => BlockId::AIR,
            }),
            2 if (x, z) == (cx, cz) => Some(COBBLE_SLAB),
            _ => (y < ground).then_some(BlockId::AIR),
        }
    }

    /// Porche : plancher, deux poteaux devant, un auvent de dalles. Sur pilotis, le porche est une
    /// plateforme portée par ses poteaux, ou un perron dont la rangée de devant descend d'une marche
    fn porch_block(&self, porch: &Rect, x: i32, y: i32, z: i32, ground: i32) -> Option<BlockId> {
        let f = self.floor;
        let outer = match self.door {
            Side::North => z == porch.z0,
            Side::South => z == porch.z1 - 1,
            Side::West => x == porch.x0,
            Side::East => x == porch.x1 - 1,
        };
        if self.stilts > 0 && self.steps {
            // Perron : la rangée du fond au niveau du plancher, celle de devant une marche plus bas
            let deck = if outer { f - 2 } else { f - 1 };
            if y < deck {
                return (y >= ground).then_some(self.style.foundation);
            }
            if y == deck {
                return Some(self.style.floor);
            }
            if y == f - 1 {
                return Some(BlockId::AIR);
            }
        } else if self.stilts > 0 && y < f - 1 {
            return (y >= ground && outer && porch.on_corner(x, z)).then_some(self.style.corner);
        }
        if y < f - 1 {
            return (y >= ground).then_some(self.style.foundation);
        }
        if y == f - 1 {
            return Some(self.style.floor);
        }
        let level = y - f;
        if level < 2 {
            return Some(if outer && porch.on_corner(x, z) { OAK_LOG } else { BlockId::AIR });
        }
        Some(PLANK_SLAB)
    }

    /// Butte de gnome : terre pleine coiffée d'herbe, une salle de trois blocs de haut sous la butte,
    /// une porte basse cerclée de bronze et des fenêtres rondes au-dessus du talus
    fn burrow_block(&self, x: i32, y: i32, z: i32, ground: i32) -> Option<BlockId> {
        let f = self.floor;
        let foot = &self.foot;
        let h = self.burrow_height(x, z);
        let (ox, oz) = self.door.delta();
        // Le tunnel de la porte traverse le talus
        let before_door = !foot.contains(x, z) && self.door_offset(x - ox, z - oz).is_some_and(|d| d <= self.door_width / 2);
        if before_door {
            if y == f - 1 {
                return Some(self.style.foundation);
            }
            if y >= f && y < f + self.door_height() {
                return Some(BlockId::AIR);
            }
        }
        if h == 0 {
            return None;
        }
        let top = f + h - 1;
        if y < f - 1 {
            return (y >= ground).then_some(DIRT);
        }
        if y > top {
            return (y < ground).then_some(BlockId::AIR);
        }
        if y == top {
            return Some(GRASS);
        }
        if y == f - 1 {
            return Some(if foot.contains(x, z) && !foot.on_edge(x, z) { self.style.floor } else { DIRT });
        }
        let level = y - f;
        if !foot.contains(x, z) {
            return Some(DIRT);
        }
        if foot.on_edge(x, z) {
            if let Some(d) = self.door_offset(x, z) {
                let door_h = self.door_height();
                if d <= self.door_width / 2 && level < door_h {
                    return Some(BlockId::AIR);
                }
                // Cadre de bronze autour de la porte
                if (d == self.door_width / 2 + 1 && level < door_h) || (d <= self.door_width / 2 + 1 && level == door_h) {
                    return Some(self.style.corner);
                }
            }
            let along = if z == foot.z0 || z == foot.z1 - 1 { x - foot.x0 } else { z - foot.z0 };
            let door_near = self.door_offset(x, z).is_some_and(|d| d <= self.door_width / 2 + 1);
            if level == 2 && along % 4 == 2 && !door_near && !foot.on_corner(x, z) {
                return Some(GLASS);
            }
            return Some(DIRT);
        }
        if level < 3 {
            return Some(self.interior(x, z, level));
        }
        Some(DIRT)
    }

    fn roof_block(&self, rising: Option<Side>) -> BlockId {
        match (self.style.roof, rising) {
            (Some(stairs), Some(side)) => roofing(stairs, side),
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

/// Forge : un appentis ouvert, un mur du fond en pierre, l'âtre et sa cheminée, une enclume
#[derive(Debug, Clone, Copy)]
pub struct Forge {
    pub foot: Rect,
    pub floor: i32,
    /// Côté ouvert sur la rue
    pub front: Side,
}

impl Forge {
    fn top(&self) -> i32 {
        self.floor + 7
    }

    fn block(&self, x: i32, y: i32, z: i32, ground: i32) -> Option<BlockId> {
        let f = self.floor;
        let foot = &self.foot;
        if y < f - 1 {
            return (y >= ground).then_some(COBBLE);
        }
        if y == f - 1 {
            return Some(COBBLE);
        }
        let back = self.front.opposite();
        let on_back = match back {
            Side::North => z == foot.z0,
            Side::South => z == foot.z1 - 1,
            Side::West => x == foot.x0,
            Side::East => x == foot.x1 - 1,
        };
        let (cx, cz) = foot.center();
        let level = y - f;
        // Âtre et cheminée dans l'angle du fond
        let hearth = match back {
            Side::North => (foot.x0, foot.z0),
            Side::South => (foot.x1 - 1, foot.z1 - 1),
            Side::West => (foot.x0, foot.z1 - 1),
            Side::East => (foot.x1 - 1, foot.z0),
        };
        if (x, z) == hearth {
            return Some(if level < 6 { COBBLE } else if level == 6 { COBBLE_SLAB } else { return (y < ground).then_some(BlockId::AIR) });
        }
        if level < 3 {
            if on_back {
                // Le fourneau dans le mur du fond, à côté de l'âtre
                let next_to_hearth = (x - hearth.0).abs() + (z - hearth.1).abs() == 1;
                return Some(if level == 0 && next_to_hearth { FURNACE } else { COBBLE });
            }
            if foot.on_corner(x, z) {
                return Some(OAK_LOG);
            }
            if level == 0 && (x, z) == (cx, cz) {
                return Some(IRON_TRIM);
            }
            if level == 0 && foot.on_edge(x, z) && !on_back && hash2(0xF0, x, z).is_multiple_of(3) {
                return Some(WORKBENCH);
            }
            return Some(BlockId::AIR);
        }
        if level == 3 {
            return Some(COBBLE_SLAB);
        }
        (y < ground).then_some(BlockId::AIR)
    }
}

/// Étal de marché : quatre poteaux, un auvent, un comptoir garni
#[derive(Debug, Clone, Copy)]
pub struct Stall {
    pub rect: Rect,
    pub front: Side,
    pub goods: BlockId,
}

impl Stall {
    fn block(&self, x: i32, y: i32, z: i32, ground: i32) -> Option<BlockId> {
        let r = &self.rect;
        let level = y - ground;
        let front_row = match self.front {
            Side::North => z == r.z0,
            Side::South => z == r.z1 - 1,
            Side::West => x == r.x0,
            Side::East => x == r.x1 - 1,
        };
        match level {
            -1 => Some(PLANK),
            0 | 1 if r.on_corner(x, z) => Some(OAK_LOG),
            0 => Some(if front_row { WOOD_PANEL } else { BlockId::AIR }),
            1 => Some(if front_row { self.goods } else { BlockId::AIR }),
            2 => Some(PLANK_SLAB),
            _ => None,
        }
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

/// Temple rond : une enceinte circulaire dont l'intérieur sert d'arène, et au fond, derrière un mur
/// courbe bombé vers l'arène, un sanctuaire fermé où l'autel se dresse sous un puits de lumière.
/// Une base haute fait le tour de l'enceinte (on n'entre que par l'avant) ; des colonnes posées
/// dessus, un peu plus hautes vers le fond, sont reliées par une couronne ; autour du sanctuaire,
/// le tour devient un mur plein qui monte jusqu'à la couronne.
///
/// Au présent, c'est une ruine : base et colonnes brisées, couronne et toit tombés, dalles envahies
/// d'herbe, la Pierre cachée sous l'autel. Dans le passé (`temples`), le même temple est intact et
/// tenu en guerre par un peuple. Les deux époques occupent exactement les mêmes colonnes.
#[derive(Debug, Clone, Copy)]
pub struct Temple {
    /// Colonne du centre de l'enceinte
    pub center: (i32, i32),
    /// Rayon de l'enceinte, base comprise
    pub radius: i32,
    /// Côté de l'entrée
    pub door: Side,
    /// Sol de l'arène (premier bloc d'air au-dessus du dallage)
    pub floor: i32,
    /// Sol naturel devant l'entrée (premier bloc d'air), là où descend l'escalier
    pub forecourt: i32,
    pub seed: u64,
}

impl Temple {
    /// Hauteur de la base qui fait le tour de l'enceinte
    const BASE: i32 = 3;
    /// Niveau de la couronne à l'entrée ; elle monte de `CROWN_RISE` vers le fond
    const CROWN_LOW: i32 = 5;
    const CROWN_RISE: i32 = 3;
    /// Demi-largeur de l'entrée et de la porte du sanctuaire
    const GATE: i32 = 1;
    /// Niveau du toit du sanctuaire, celui de la couronne au fond
    const ROOF: i32 = Self::CROWN_LOW + Self::CROWN_RISE;
    /// Marches de l'escalier de l'entrée, au plus
    const STEPS_MAX: i32 = 4;
    /// Hauteur du podium sur lequel se dresse le temple, au-dessus du sol naturel moyen
    const PODIUM: i32 = 2;
    /// Profondeur et demi-largeur du parvis, au pied de l'escalier
    const PARVIS_DEPTH: i32 = 5;
    const PARVIS_HALF: i32 = 5;

    /// Côté de l'entrée
    pub fn door(&self) -> Side {
        self.door
    }

    /// Colonnes de l'enceinte seule, sans l'escalier ni le parvis
    fn enclosure(&self) -> Rect {
        Rect::around(self.center.0, self.center.1, self.radius + 1)
    }

    /// Toutes les colonnes que le temple change : l'enceinte, et devant l'entrée la place de
    /// l'escalier le plus long et du parvis
    pub fn rect(&self) -> Rect {
        let far = -self.radius - Self::STEPS_MAX - Self::PARVIS_DEPTH;
        let (ax, az) = self.world(-Self::PARVIS_HALF, far);
        let (bx, bz) = self.world(Self::PARVIS_HALF, far);
        self.enclosure().union(&Rect::new(ax.min(bx), az.min(bz), ax.max(bx) + 1, az.max(bz) + 1))
    }

    /// Marches de l'escalier qui descend de l'entrée jusqu'au parvis
    fn steps(&self) -> i32 {
        (self.floor - self.forecourt).clamp(0, Self::STEPS_MAX)
    }

    /// Sol du parvis (premier bloc d'air au-dessus du dallage)
    pub(crate) fn parvis_floor(&self) -> i32 {
        self.floor - self.steps()
    }

    /// Colonne (x, z) dans le repère du temple : `u` en travers de l'axe, `v` le long de l'axe, de
    /// `-radius` (l'entrée) à `radius` (le fond)
    fn local(&self, x: i32, z: i32) -> (i32, i32) {
        let (a, b) = (x - self.center.0, z - self.center.1);
        let (dx, dz) = self.door.delta();
        (dz * a - dx * b, -(dx * a + dz * b))
    }

    /// Colonne du monde d'un point (u, v) du repère du temple
    fn world(&self, u: i32, v: i32) -> (i32, i32) {
        let (dx, dz) = self.door.delta();
        (self.center.0 - dx * v + dz * u, self.center.1 - dz * v - dx * u)
    }

    fn dist(u: i32, v: i32) -> f64 {
        ((u * u + v * v) as f64).sqrt()
    }

    /// Le cercle du mur du sanctuaire : centre sur l'axe, au-delà du fond, et rayon. Le mur est
    /// bombé vers l'arène et coupe l'axe au tiers de l'enceinte.
    fn wall_circle(&self) -> (f64, f64) {
        let r = self.radius as f64;
        let c = r + 8.0;
        (c, c - r / 3.0)
    }

    fn wall_dist(&self, u: i32, v: i32) -> f64 {
        let (c, _) = self.wall_circle();
        ((u * u) as f64 + (v as f64 - c).powi(2)).sqrt()
    }

    /// Le tour de l'enceinte (base, colonnes, couronne)
    fn on_ring(&self, u: i32, v: i32) -> bool {
        let d = Self::dist(u, v);
        let r = self.radius as f64;
        d > r - 1.5 && d <= r + 0.5
    }

    /// Dans l'enceinte, en deçà du tour
    fn inside(&self, u: i32, v: i32) -> bool {
        Self::dist(u, v) <= self.radius as f64 - 1.5
    }

    /// Dans la salle du sanctuaire (en deçà de son mur)
    fn in_sanctum_local(&self, u: i32, v: i32) -> bool {
        self.inside(u, v) && self.wall_dist(u, v) < self.wall_circle().1 - 0.75
    }

    /// Sur le mur courbe du sanctuaire
    fn on_sanctum_wall(&self, u: i32, v: i32) -> bool {
        self.inside(u, v) && (self.wall_dist(u, v) - self.wall_circle().1).abs() <= 0.75
    }

    /// Le tour autour du sanctuaire, qui devient un mur plein jusqu'à la couronne
    fn ring_of_sanctum(&self, u: i32, v: i32) -> bool {
        self.on_ring(u, v) && self.wall_dist(u, v) < self.wall_circle().1 + 0.75
    }

    /// La colonne (x, z) est dans la salle du sanctuaire
    pub fn in_sanctum(&self, x: i32, z: i32) -> bool {
        let (u, v) = self.local(x, z);
        self.in_sanctum_local(u, v)
    }

    /// Niveau de la couronne (et du toit du sanctuaire) au-dessus du sol de l'arène
    fn crown(&self, v: i32) -> i32 {
        let t = (v + self.radius) as f64 / (2 * self.radius) as f64;
        Self::CROWN_LOW + (Self::CROWN_RISE as f64 * t.clamp(0.0, 1.0)).round() as i32
    }

    /// Une colonne se dresse sur la base (une tous les trois blocs du tour)
    fn is_column(&self, u: i32, v: i32) -> bool {
        let s = (u as f64).atan2(v as f64) * self.radius as f64;
        (s.round() as i32).rem_euclid(3) == 0
    }

    /// L'entrée, percée dans le tour à l'avant
    fn is_gate(&self, u: i32, v: i32) -> bool {
        v < 0 && u.abs() <= Self::GATE
    }

    /// Point (u, v) de l'autel : au centre du sanctuaire, sur l'axe
    fn altar_local(&self) -> (i32, i32) {
        let (c, rw) = self.wall_circle();
        let front = (c - rw).ceil() as i32;
        (0, (front + self.radius - 1) / 2)
    }

    /// Colonne de l'autel
    pub(crate) fn altar(&self) -> (i32, i32) {
        let (u, v) = self.altar_local();
        self.world(u, v)
    }

    /// Centre de l'arène, entre l'entrée et le mur du sanctuaire
    pub fn arena_center(&self) -> (i32, i32) {
        let (c, rw) = self.wall_circle();
        let front = (c - rw).floor() as i32;
        self.world(0, (front - self.radius) / 2)
    }

    /// Colonne au bord du parvis, face à l'escalier de l'entrée
    pub fn approach(&self) -> (i32, i32) {
        self.world(0, -self.radius - self.steps() - Self::PARVIS_DEPTH)
    }

    /// Sol de l'escalier ou du parvis au point (u, v) devant l'entrée, et demi-largeur de cette
    /// rangée ; `None` hors de l'escalier et du parvis
    fn forecourt_at(&self, u: i32, v: i32) -> Option<(i32, i32)> {
        let k = -self.radius - v;
        let n = self.steps();
        let (level, half) = if (1..=n).contains(&k) {
            (self.floor - k, (2 + k).min(Self::PARVIS_HALF))
        } else if k > n && k <= n + Self::PARVIS_DEPTH {
            (self.floor - n, Self::PARVIS_HALF)
        } else {
            return None;
        };
        (u.abs() <= half).then_some((level, half))
    }

    /// Les deux braseros du parvis, de part et d'autre de l'axe : (u, v)
    fn parvis_braziers(&self) -> [(i32, i32); 2] {
        let v = -self.radius - self.steps() - 3;
        [(-(Self::PARVIS_HALF - 1), v), (Self::PARVIS_HALF - 1, v)]
    }

    /// Bloc de l'escalier ou du parvis dont le sol est à `level`
    fn forecourt_block(&self, u: i32, v: i32, y: i32, ground: i32, level: i32, half: i32, holder: Option<Culture>, h: u64) -> Option<BlockId> {
        let past = holder.is_some();
        let masonry = holder.map(Masonry::of);
        if y < level - 1 {
            return (y >= ground).then_some(HARD_STONE);
        }
        if y == level - 1 {
            let paved = if u.abs() <= Self::GATE { WHITE_BRICK } else if u.abs() == half { COBBLE } else { CUT_STONE };
            if past {
                return Some(paved);
            }
            let roll = unit(mix(h ^ 0xF0));
            return Some(if roll < 0.3 { GRASS } else if roll < 0.42 { GRAVEL } else { paved });
        }
        let [left, right] = self.parvis_braziers();
        let lift = y - level;
        if (u, v) == left || (u, v) == right {
            match lift {
                0 => return Some(masonry.map(|m| m.wall).unwrap_or(COBBLE)),
                1 if past => return Some(masonry.map(|m| m.light).unwrap_or(LANTERN)),
                _ => {}
            }
            // Au présent, le brasero de droite est renversé
            if !past && (u, v) == right && lift == 0 {
                return (y < ground).then_some(BlockId::AIR);
            }
        }
        if !past && lift == 0 && u == right.0 && (v == right.1 - 1 || v == right.1 - 2) {
            return Some(COBBLE);
        }
        (y < ground).then_some(BlockId::AIR)
    }

    /// Rayon de la mosaïque de l'arène
    fn mosaic_radius(&self) -> i32 {
        let (c, rw) = self.wall_circle();
        let front = (c - rw).floor() as i32;
        ((front + self.radius) / 2 - 2).min(7)
    }

    /// Dallage de l'arène en (u, v) : l'allée processionnelle sur l'axe, et autour du centre la
    /// mosaïque du peuple (cercle extérieur rouge, bande sombre, cercle d'ornement, huit rayons et
    /// l'emblème là où paraît le monstre). Au présent, elle est usée et fendue.
    fn arena_paving(&self, u: i32, v: i32, trim: BlockId, past: bool, h: u64) -> BlockId {
        let (_, av) = self.local(self.arena_center().0, self.arena_center().1);
        let w = v - av;
        let r = Self::dist(u, w);
        let m = self.mosaic_radius() as f64;
        let trim = if past { trim } else { CUT_STONE };
        let tile = if r <= 1.5 {
            trim
        } else if (m - 0.5..m + 0.5).contains(&r) {
            BRICK
        } else if (m - 1.5..m - 0.5).contains(&r) {
            BASALT
        } else if (m - 2.5..m - 1.5).contains(&r) {
            trim
        } else if (r < m - 2.5 && (u == 0 || w == 0 || u.abs() == w.abs())) || (u.abs() <= Self::GATE && r >= m + 0.5) {
            // Les rayons de la mosaïque, et l'allée processionnelle autour d'elle
            WHITE_BRICK
        } else {
            CUT_STONE
        };
        if past {
            return tile;
        }
        // La fente qui traverse la mosaïque
        let a = unit(mix(self.seed ^ 0xF1)) * std::f64::consts::PI;
        if r < m + 1.0 && (u as f64 * a.sin() - w as f64 * a.cos()).abs() < 0.6 {
            return GRAVEL;
        }
        let roll = unit(h);
        if roll < 0.3 {
            GRASS
        } else if roll < 0.42 {
            GRAVEL
        } else {
            tile
        }
    }

    /// Tambours de colonnes tombés au présent : extrémité (u, v), et s'ils sont couchés le long de
    /// l'axe ; dans l'arène, puis dehors autour de l'enceinte
    fn drums(&self) -> (Vec<Drum>, Vec<Drum>) {
        let r = self.radius as f64;
        let (c, rw) = self.wall_circle();
        let front = (c - rw).floor() as i32;
        let mut inside = Vec::new();
        let mut outside = Vec::new();
        for i in 0..12u64 {
            let h = mix(self.seed ^ 0xD0 ^ (i << 8));
            let along = h & 1 == 0;
            let a = unit(mix(h ^ 1)) * std::f64::consts::TAU;
            if i < 7 {
                let d = 3.0 + unit(mix(h ^ 2)) * (r - 6.0);
                let (u, v) = ((d * a.sin()).round() as i32, (d * a.cos()).round() as i32);
                // Pas sur l'allée, ni contre le mur du sanctuaire
                let cells = (0..3).map(|k| if along { (u, v + k) } else { (u + k, v) });
                if cells.clone().all(|(cu, cv)| cu.abs() > Self::GATE + 1 && self.inside(cu, cv) && cv < front - 1) {
                    inside.push((u, v, along));
                }
            } else {
                let d = r + 2.0 + unit(mix(h ^ 2)) * 1.5;
                let (u, v) = ((d * a.sin()).round() as i32, (d * a.cos()).round() as i32);
                if v > -self.radius || u.abs() > Self::PARVIS_HALF + 1 {
                    outside.push((u, v, along));
                }
            }
        }
        (inside, outside)
    }

    fn on_drum(drums: &[Drum], u: i32, v: i32) -> bool {
        drums.iter().any(|&(du, dv, along)| if along { u == du && (dv..dv + 3).contains(&v) } else { v == dv && (du..du + 3).contains(&u) })
    }

    /// Un brasero brûle au sommet de la colonne (u, v) dans le passé : une colonne sur quatre, sur
    /// le bord extérieur du tour
    fn has_brazier(&self, u: i32, v: i32) -> bool {
        if !self.on_ring(u, v) || self.ring_of_sanctum(u, v) || self.is_gate(u, v) || !self.is_column(u, v) {
            return false;
        }
        if Self::dist(u, v) <= self.radius as f64 - 0.5 {
            return false;
        }
        let s = ((u as f64).atan2(v as f64) * self.radius as f64).round() as i32;
        s.rem_euclid(12) == 6
    }

    /// Éclats du Néant semés au présent autour de la cache de la Pierre
    fn void_shard(&self, u: i32, v: i32) -> bool {
        let (au, av) = self.altar_local();
        let ring = (u - au).abs().max((v - av).abs());
        (2..=3).contains(&ring) && unit(hash2(self.seed ^ 0x5A, u, v)) < 0.3
    }

    /// Lanternes posées sur la base dans le passé : (u, v)
    fn ring_lights(&self) -> Vec<(i32, i32)> {
        let r = self.radius as f64 - 0.5;
        [40.0f64, 75.0, 110.0, -40.0, -75.0, -110.0]
            .iter()
            .map(|deg| {
                // angle compté depuis l'entrée
                let a = (180.0 - deg).to_radians();
                ((r * a.sin()).round() as i32, (r * a.cos()).round() as i32)
            })
            .filter(|&(u, v)| self.on_ring(u, v) && !self.ring_of_sanctum(u, v) && !self.is_column(u, v))
            .collect()
    }

    /// Retrait du point (u, v) depuis le bord du toit du sanctuaire (négatif : hors du toit)
    fn roof_inset(&self, u: i32, v: i32) -> f64 {
        let to_ring = self.radius as f64 + 0.5 - Self::dist(u, v);
        let to_wall = self.wall_dist(u, v) - (self.wall_circle().1 - 0.75);
        to_ring.min(-to_wall)
    }

    /// Toit du sanctuaire intact, au niveau `level` (au moins `ROOF`) : une dalle au niveau de la
    /// couronne bordée d'une corniche, deux gradins en retrait, et au sommet un lanternon autour du
    /// puits de lumière, coiffé de cristaux à ses coins. Le puits reste ouvert jusqu'au ciel.
    fn roof_block(&self, u: i32, v: i32, level: i32, trim: BlockId) -> Option<BlockId> {
        let inset = self.roof_inset(u, v);
        if inset < 0.0 {
            return None;
        }
        let (au, av) = self.altar_local();
        let (ru, rv) = ((u - au).abs(), (v - av).abs());
        if ru <= 1 && rv <= 1 {
            return None;
        }
        let tier = level - Self::ROOF;
        let lantern = ru.max(rv) == 2;
        match tier {
            0 => Some(if inset < 1.0 { trim } else { CUT_STONE }),
            1 if inset >= 2.0 => Some(CUT_STONE),
            2 if inset >= 4.0 || lantern => Some(CUT_STONE),
            3 if lantern => Some(if ru == 2 && rv == 2 { CUT_CRYSTAL } else { trim }),
            _ => None,
        }
    }

    /// Bloc du temple à l'époque donnée : `holder` est le peuple qui le tient dans le passé, `None`
    /// au présent
    fn block_at(&self, holder: Option<Culture>, x: i32, y: i32, z: i32, ground: i32) -> Option<BlockId> {
        let (u, v) = self.local(x, z);
        let d = Self::dist(u, v);
        let h = hash2(self.seed ^ 0x7E3B, x, z);
        let past = holder.is_some();
        if d > self.radius as f64 + 0.5 {
            if let Some((level, half)) = self.forecourt_at(u, v) {
                return self.forecourt_block(u, v, y, ground, level, half, holder, h);
            }
            // Au présent, des tambours de colonnes ont roulé hors de l'enceinte
            if !past && y == ground && Self::on_drum(&self.drums().1, u, v) {
                return Some(CUT_STONE);
            }
            return None;
        }
        let f = self.floor;
        let masonry = holder.map(Masonry::of);
        let trim = holder.map(temple_trim).unwrap_or(GILDING);
        if y < f - 2 {
            return (y >= ground).then_some(HARD_STONE);
        }
        let ring = self.on_ring(u, v);
        let gate = ring && self.is_gate(u, v);
        let sanct_ring = self.ring_of_sanctum(u, v);
        let sanct = self.in_sanctum_local(u, v);
        let sanct_wall = self.on_sanctum_wall(u, v);
        let altar = (u, v) == self.altar_local();
        if y < f {
            if y == f - 2 {
                return Some(CUT_STONE);
            }
            // Le dallage : allée processionnelle sur l'axe, mosaïque autour du centre de l'arène
            if ring || sanct || sanct_wall {
                return Some(CUT_STONE);
            }
            return Some(self.arena_paving(u, v, trim, past, h));
        }
        let level = y - f;
        let crown = self.crown(v);
        let broken = |max: i32, salt: u64| -> i32 { (unit(mix(h ^ salt)) * (max + 1) as f64) as i32 };
        // Le tour de l'enceinte
        if ring && !gate {
            if sanct_ring {
                // Mur plein autour du sanctuaire, jusqu'au toit
                if past && level >= Self::ROOF {
                    return self.roof_block(u, v, level, trim).or((y < ground).then_some(BlockId::AIR));
                }
                let top = if past { Self::ROOF } else { 1 + self.fallen(u, v, Self::ROOF - 1, 0x51) };
                if level < top {
                    if !past && d > self.radius as f64 - 0.5 && unit(mix(h ^ 0x1D ^ level as u64)) < 0.18 {
                        return Some(DARK_LEAF);
                    }
                    return Some(masonry.map(|m| m.wall).unwrap_or(COBBLE));
                }
                if !past && level == top && unit(mix(h ^ 0x30)) < 0.3 {
                    return Some(GREEN_LEAF);
                }
                return (y < ground).then_some(BlockId::AIR);
            }
            let column = self.is_column(u, v);
            if past {
                if level < Self::BASE {
                    return Some(masonry.map(|m| m.wall).unwrap_or(COBBLE));
                }
                if column && level < crown {
                    return Some(CUT_STONE);
                }
                if level == crown {
                    return Some(if d > self.radius as f64 - 0.5 { trim } else { CUT_STONE });
                }
                if level == crown + 1 && self.has_brazier(u, v) {
                    return Some(masonry.map(|m| m.light).unwrap_or(LANTERN));
                }
                if level == Self::BASE && self.ring_lights().contains(&(u, v)) {
                    return Some(masonry.map(|m| m.light).unwrap_or(LANTERN));
                }
            } else {
                // La base s'est effondrée par pans, les colonnes sont brisées, la couronne est tombée
                let chunk = ((u as f64).atan2(v as f64) * self.radius as f64 / 4.0).round() as i32;
                let base = (unit(hash2(self.seed ^ 0xBA5E, chunk, 0)) * (Self::BASE + 1) as f64) as i32;
                let outer = d > self.radius as f64 - 0.5;
                if level < base.max(1) {
                    // Le lierre couvre la face extérieure par plaques
                    if outer && unit(mix(h ^ 0x1D)) < 0.2 {
                        return Some(DARK_LEAF);
                    }
                    return Some(if unit(mix(h ^ 0x7)) < 0.3 { COBBLE } else { STONE });
                }
                let stump = if column { broken(crown, 0xC01) } else { 0 };
                if level < stump {
                    return Some(if outer && unit(mix(h ^ 0x1E ^ level as u64)) < 0.15 { DARK_LEAF } else { CUT_STONE });
                }
                // La mousse coiffe ce qui reste debout
                if level == base.max(1).max(stump) && unit(mix(h ^ 0x30)) < 0.3 {
                    return Some(GREEN_LEAF);
                }
            }
            return (y < ground).then_some(BlockId::AIR);
        }
        if gate {
            // Le linteau de l'entrée porte la couronne
            if past && level == crown {
                return Some(trim);
            }
            return (y < ground).then_some(BlockId::AIR);
        }
        // Le mur courbe du sanctuaire, sa porte face à l'arène
        if sanct_wall {
            let door = u.abs() <= Self::GATE && (1..=3).contains(&level);
            if level == 0 {
                return Some(CUT_STONE);
            }
            if door {
                return Some(BlockId::AIR);
            }
            if past && level >= Self::ROOF {
                return self.roof_block(u, v, level, trim).or((y < ground).then_some(BlockId::AIR));
            }
            let top = if past { Self::ROOF - 1 } else { 2 + self.fallen(u, v, Self::ROOF - 2, 0x52) };
            if level <= top {
                return Some(masonry.map(|m| m.wall).unwrap_or(COBBLE));
            }
            return (y < ground).then_some(BlockId::AIR);
        }
        if sanct {
            // Le sol surélevé d'un bloc
            if level == 0 {
                return Some(if altar && !past { BlockId::AIR } else { CUT_STONE });
            }
            if altar {
                match level {
                    1 => return Some(if past { GILDING } else { CUT_STONE }),
                    2 => return Some(if past { BlockId::AIR } else { CUT_CRYSTAL }),
                    _ => {}
                }
            }
            if past {
                // Le toit, ouvert au-dessus de l'autel en puits de lumière
                let (au, av) = self.altar_local();
                if level >= Self::ROOF {
                    return self.roof_block(u, v, level, trim).or((y < ground).then_some(BlockId::AIR));
                }
                // Deux lanternes de part et d'autre de l'autel
                if level == 1 && v == av && (u - au).abs() == 2 {
                    return Some(masonry.map(|m| m.light).unwrap_or(LANTERN));
                }
            } else if level == 1 && self.void_shard(u, v) {
                return Some(VOID_ROCK);
            } else if level == 1 && unit(mix(h ^ 0x13)) < 0.06 {
                return Some(CUT_STONE);
            }
            return (y < ground).then_some(BlockId::AIR);
        }
        // L'arène : au présent, des tambours de colonnes tombés et des pierres éparses
        if !past && level == 0 && (Self::on_drum(&self.drums().0, u, v) || unit(mix(h ^ 0x13)) < 0.03) {
            return Some(CUT_STONE);
        }
        // Une marche devant la porte du sanctuaire
        if level == 0 && u.abs() <= Self::GATE && self.wall_dist(u, v) - self.wall_circle().1 <= 1.75 {
            return Some(COBBLE_SLAB);
        }
        (y < ground).then_some(BlockId::AIR)
    }

    /// Hauteur restante d'un pan de mur effondré au point (u, v), de 0 à `max` : les pans de quatre
    /// blocs de long tombent ensemble, si bien que la ruine garde des murs et non des piquets
    fn fallen(&self, u: i32, v: i32, max: i32, salt: u64) -> i32 {
        let chunk = ((u as f64).atan2(v as f64 - self.radius as f64) * self.radius as f64 / 4.0).round() as i32;
        let a = unit(hash2(self.seed ^ salt, chunk, 0));
        (a * a * (max + 1) as f64) as i32
    }

    /// Bloc de la ruine, au présent
    fn block(&self, x: i32, y: i32, z: i32, ground: i32) -> Option<BlockId> {
        self.block_at(None, x, y, z, ground)
    }

    /// Bloc du temple intact, dans le passé, tenu par le peuple `holder`
    pub(crate) fn past_block(&self, holder: Culture, x: i32, y: i32, z: i32, ground: i32) -> Option<BlockId> {
        self.block_at(Some(holder), x, y, z, ground)
    }

    /// Blocs qui éclairent le temple intact : position (altitude comprise)
    pub(crate) fn past_light_spots(&self) -> Vec<(i32, i32, i32)> {
        let f = self.floor;
        let mut spots: Vec<(i32, i32, i32)> = self
            .ring_lights()
            .into_iter()
            .map(|(u, v)| {
                let (x, z) = self.world(u, v);
                (x, f + Self::BASE, z)
            })
            .collect();
        let (au, av) = self.altar_local();
        for du in [-2, 2] {
            let (x, z) = self.world(au + du, av);
            spots.push((x, f + 1, z));
        }
        let r = self.radius;
        for v in -r..=r {
            for u in -r..=r {
                if self.has_brazier(u, v) {
                    let (x, z) = self.world(u, v);
                    spots.push((x, f + self.crown(v) + 1, z));
                }
            }
        }
        for (u, v) in self.parvis_braziers() {
            let (x, z) = self.world(u, v);
            spots.push((x, self.parvis_floor() + 1, z));
        }
        spots
    }

    /// Emplacement de la Pierre du Néant : au présent, la cache creusée sous l'autel ; dans le
    /// passé, sur l'autel doré, sous le puits de lumière. Le générateur y laisse de l'air.
    pub fn stone_slot(&self, past: bool) -> (i32, i32, i32) {
        let (x, z) = self.altar();
        (x, if past { self.floor + 2 } else { self.floor }, z)
    }

    /// Plus haut bloc (exclu) que pose le temple, à l'une ou l'autre époque
    pub(crate) fn top(&self) -> i32 {
        self.floor + Self::ROOF + 5
    }
}

/// Tambour de colonne couché, long de trois blocs : extrémité (u, v) dans le repère du temple, et
/// s'il est couché le long de l'axe
type Drum = (i32, i32, bool);

/// Ornement de la corniche d'un temple tenu par `holder`
fn temple_trim(holder: Culture) -> BlockId {
    match holder {
        Culture::Nains | Culture::Gnomes => BRONZE_TRIM,
        Culture::HautsElfes | Culture::Aasimars | Culture::Sylphes => SILVER_TRIM,
        Culture::DemiOrques | Culture::Revenants => IRON_TRIM,
        _ => GILDING,
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
                if level % 4 == 2 && hash2(self.seed ^ 0x45, x, z).is_multiple_of(4) {
                    return Some(BlockId::AIR);
                }
                return Some(if hash2(self.seed ^ 0x46, x, y).is_multiple_of(4) { STONE } else { COBBLE });
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
    Forge(Forge),
    /// Étal de marché, qui suit le terrain
    Stall(Stall),
    /// Puits de 3 × 3 couvert d'un petit toit
    Well { rect: Rect, floor: i32 },
    /// Fontaine de 5 × 5, une colonne au milieu
    Fountain { rect: Rect, floor: i32 },
    /// Lampadaire : un poteau et une lumière
    Lamp { x: i32, z: i32, masonry: Masonry },
    /// Pan de rempart d'un bloc d'épaisseur, crénelé (ou palissade de pieux), qui suit le terrain.
    /// `walk` : la rangée intérieure, sans créneaux, dont le dessus est le chemin de ronde
    Rampart { rect: Rect, masonry: Masonry, walk: bool },
    /// Porte de ville percée dans le rempart (`walk` : sous le chemin de ronde, voir `Rampart`)
    Gate { rect: Rect, masonry: Masonry, walk: bool },
    /// Échelle collée au mur `back` de sa colonne, du sol jusque sous l'altitude `top` (exclue)
    Ladder { x: i32, z: i32, back: Side, top: i32 },
    /// Tour d'angle du rempart
    Tower { rect: Rect, floor: i32, masonry: Masonry },
    RuinHouse(RuinHouse),
    Temple(Temple),
    RuinTower(RuinTower),
    /// Arène des Demi-orques : un anneau de pierre dure ouvert aux quatre vents, un sol de sable
    Arena { rect: Rect, floor: i32 },
    /// Ossuaire des Revenants : un caveau de pierre du vide sur un socle, une lumière bleue au sommet
    Ossuary { rect: Rect, floor: i32 },
    /// Obélisque des Sylphes : une aiguille de cristal sur un socle, une lanterne à la pointe
    Obelisk { rect: Rect, floor: i32 },
    /// Sanctuaire des Aasimars : colonnade, toit blanc cerclé d'or, cristal au sommet, lumière au centre
    Sanctuary { rect: Rect, floor: i32 },
    /// Terrasse des Aasimars : une plateforme blanche portée par une colonne, qu'on atteint en volant.
    /// `floor` : sol au pied de la colonne ; la plateforme est à `floor + height`
    Terrace { rect: Rect, floor: i32, height: i32 },
    /// Mât totem : un tronc coiffé et cerclé d'ornements
    Totem { x: i32, z: i32, top: BlockId },
    /// Enclos de chasse : une barrière de chêne et son passage
    Pen { rect: Rect },
    /// Donjon royal : trois étages en retrait, fenêtres, couronne dorée et flèche
    Keep { rect: Rect, floor: i32, masonry: Masonry },
    /// Douves : de l'eau jusqu'au ras du sol
    Moat { rect: Rect },
    /// Hôtel de ville : une halle à deux niveaux, son beffroi au milieu de la façade sud
    TownHall { rect: Rect, floor: i32 },
}

impl Piece {
    /// Colonnes que la pièce peut changer
    pub fn rect(&self) -> Rect {
        match self {
            Piece::Paving { rect, .. }
            | Piece::Field { rect, .. }
            | Piece::Well { rect, .. }
            | Piece::Fountain { rect, .. }
            | Piece::Rampart { rect, .. }
            | Piece::Gate { rect, .. }
            | Piece::Tower { rect, .. } => *rect,
            Piece::House(house) => house.rect(),
            Piece::Forge(forge) => forge.foot,
            Piece::Stall(stall) => stall.rect,
            Piece::Lamp { x, z, .. } | Piece::Ladder { x, z, .. } => Rect::sized(*x, *z, 1, 1),
            Piece::RuinHouse(r) => r.foot,
            Piece::Temple(t) => t.rect(),
            Piece::RuinTower(t) => Rect::around(t.center.0, t.center.1, t.radius),
            Piece::Arena { rect, .. }
            | Piece::Ossuary { rect, .. }
            | Piece::Obelisk { rect, .. }
            | Piece::Sanctuary { rect, .. }
            | Piece::Terrace { rect, .. }
            | Piece::Pen { rect }
            | Piece::Keep { rect, .. }
            | Piece::Moat { rect }
            | Piece::TownHall { rect, .. } => *rect,
            Piece::Totem { x, z, .. } => Rect::sized(*x, *z, 1, 1),
        }
    }

    /// Altitudes que la pièce peut changer dans une colonne de sol naturel `ground`
    fn span(&self, ground: i32) -> Range<i32> {
        let span = |floor: i32, below: i32, top: i32| floor.min(ground) - below..top.max(ground);
        match self {
            Piece::Paving { .. } => ground - 1..ground,
            Piece::Field { .. } => ground - 1..ground + 1,
            Piece::Lamp { .. } => ground..ground + LAMP_LIGHT + 1,
            Piece::Ladder { top, .. } => ground..(*top).max(ground),
            Piece::Rampart { masonry, .. } | Piece::Gate { masonry, .. } => ground - 1..ground + masonry.height + 1,
            Piece::House(h) => span(h.floor, 1, h.top()),
            Piece::Forge(forge) => span(forge.floor, 1, forge.top()),
            Piece::Stall(_) => ground - 1..ground + 3,
            Piece::Well { floor, .. } => span(*floor, 5, floor + 4),
            Piece::Fountain { floor, .. } => span(*floor, 3, floor + 3),
            Piece::Tower { floor, masonry, .. } => span(*floor, 1, floor + masonry.tower_top()),
            Piece::RuinHouse(r) => span(r.floor, 1, r.top()),
            Piece::Temple(t) => span(t.parvis_floor(), 3, t.top()),
            Piece::RuinTower(t) => span(t.floor, 1, t.top()),
            Piece::Arena { floor, .. } => span(*floor, 3, floor + 4),
            Piece::Ossuary { floor, .. } => span(*floor, 3, floor + 7),
            Piece::Obelisk { floor, .. } => span(*floor, 3, floor + OBELISK_HEIGHT + 2),
            Piece::Sanctuary { floor, .. } => span(*floor, 3, floor + 15),
            Piece::Terrace { floor, height, .. } => (*floor).min(ground) - 1..(floor + height + 2).max(ground),
            Piece::Totem { .. } => ground..ground + TOTEM_HEIGHT + 1,
            Piece::Pen { .. } => ground..ground + 1,
            Piece::Keep { floor, .. } => span(*floor, 1, floor + KEEP_TOP),
            Piece::Moat { .. } => ground - MOAT_DEPTH..ground,
            Piece::TownHall { floor, .. } => span(*floor, 1, floor + HALL_TOP),
        }
    }

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
            Piece::Forge(forge) => forge.block(x, y, z, ground),
            Piece::Stall(stall) => stall.block(x, y, z, ground),
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
            Piece::Lamp { masonry, .. } => match y - ground {
                // Socle, fût de barrière, lanterne au sommet
                0 => Some(masonry.accent),
                1..LAMP_LIGHT => Some(masonry.post),
                LAMP_LIGHT => Some(masonry.light),
                _ => None,
            },
            Piece::Ladder { back, .. } => Some(facing(LADDER, *back)),
            Piece::Rampart { masonry: m, walk, .. } => {
                let level = y - ground;
                if level < 0 {
                    return (level == -1).then_some(if m.palisade { m.wall } else { COBBLE });
                }
                // Chemin de ronde : le dessus plat de la rangée intérieure, sous le ciel
                if *walk && level >= m.height {
                    return Some(BlockId::AIR);
                }
                if m.palisade {
                    // Pieux jointifs, un sur deux dépassant en pointe
                    if level < m.height {
                        return Some(if (x + z).rem_euclid(5) == 0 { m.accent } else { m.wall });
                    }
                    return (level == m.height && (x + z) % 2 == 0).then_some(m.cap);
                }
                if level < m.height {
                    return Some(if (x * 7 + y * 3 + z * 5).rem_euclid(11) == 0 { m.accent } else { m.wall });
                }
                (level == m.height && (x + z) % 2 == 0).then_some(m.cap)
            }
            Piece::Gate { masonry: m, walk, .. } => {
                let level = y - ground;
                match level {
                    l if *walk && l >= m.height => Some(BlockId::AIR),
                    -1 => Some(if m.palisade { m.wall } else { COBBLE }),
                    0..=2 => Some(BlockId::AIR),
                    3 => Some(m.cap),
                    l if l < m.height => Some(m.wall),
                    l if l == m.height => ((x + z) % 2 == 0).then_some(m.cap),
                    _ => None,
                }
            }
            Piece::Tower { rect, floor, masonry: m } => {
                let f = *floor;
                let h = m.tower_height;
                if y < f - 1 {
                    return (y >= ground).then_some(m.wall);
                }
                if y == f - 1 {
                    return Some(m.wall);
                }
                let level = y - f;
                let (cx, cz) = rect.center();
                // Flèche : pyramide qui se resserre au-dessus du chemin de ronde, et sa pointe
                if let (Some(spire), true) = (m.spire, level >= h) {
                    let ring = (x - cx).abs().max((z - cz).abs());
                    let k = level - h;
                    let width = (rect.width() / 2 - k).max(0);
                    if ring <= width && k < 3 {
                        return Some(if k == 0 && rect.on_edge(x, z) { m.cap } else { spire });
                    }
                    if (x, z) == (cx, cz) && k == 3 {
                        return Some(spire);
                    }
                    return (y < ground).then_some(BlockId::AIR);
                }
                if rect.on_edge(x, z) {
                    if level < h {
                        // Meurtrières au milieu de chaque côté
                        if (level == 3 || level == 6) && !rect.on_corner(x, z) && (x == cx || z == cz) {
                            return Some(BlockId::AIR);
                        }
                        return Some(if level == h - 1 { m.cap } else if rect.on_corner(x, z) && m.palisade { m.accent } else { m.wall });
                    }
                    if level == h {
                        return Some(if (x + z) % 2 == 0 { m.cap } else { BlockId::AIR });
                    }
                    return (y < ground).then_some(BlockId::AIR);
                }
                if level == h - 1 {
                    return Some(m.deck);
                }
                if level < h + 1 {
                    return Some(BlockId::AIR);
                }
                (y < ground).then_some(BlockId::AIR)
            }
            Piece::RuinHouse(r) => r.block(x, y, z, ground),
            Piece::Temple(t) => t.block(x, y, z, ground),
            Piece::RuinTower(t) => t.block(x, y, z, ground),
            Piece::Arena { rect, floor } => arena_block(rect, *floor, x, y, z, ground),
            Piece::Ossuary { rect, floor } => ossuary_block(rect, *floor, x, y, z, ground),
            Piece::Obelisk { rect, floor } => obelisk_block(rect, *floor, x, y, z, ground),
            Piece::Sanctuary { rect, floor } => sanctuary_block(rect, *floor, x, y, z, ground),
            Piece::Terrace { rect, floor, height } => {
                let (cx, cz) = rect.center();
                let column = (x == cx || x == cx - 1) && (z == cz || z == cz - 1);
                let deck = floor + height;
                if y < deck {
                    return (column && y >= (*floor).min(ground) - 1).then_some(CUT_STONE);
                }
                if y == deck {
                    return Some(WHITE_BRICK);
                }
                if y == deck + 1 {
                    if (x, z) == (cx, cz) {
                        return Some(CUT_CRYSTAL);
                    }
                    return (rect.on_edge(x, z) && (x + z) % 2 == 0).then_some(GILDING);
                }
                None
            }
            Piece::Totem { top, .. } => match y - ground {
                // Tronc, deux anneaux d'ornements, la coiffe
                3 | 5 => Some(*top),
                0..TOTEM_HEIGHT => Some(OAK_LOG),
                TOTEM_HEIGHT => Some(*top),
                _ => None,
            },
            Piece::Pen { rect } => {
                if y != ground || !rect.on_edge(x, z) {
                    return None;
                }
                // Passage au milieu du côté nord
                let (cx, _) = rect.center();
                Some(if z == rect.z0 && (x - cx).abs() <= 0 { BlockId::AIR } else { OAK_FENCE })
            }
            Piece::Keep { rect, floor, masonry } => keep_block(rect, *floor, masonry, x, y, z, ground),
            Piece::Moat { .. } => (y >= ground - MOAT_DEPTH && y < ground).then_some(WATER),
            Piece::TownHall { rect, floor } => town_hall_block(rect, *floor, x, y, z, ground),
        }
    }
}

/// Plus haut niveau (exclu) du donjon royal au-dessus de son plancher
const KEEP_TOP: i32 = KEEP_TIERS[2].2 + 8;
/// Étages du donjon royal : retrait depuis le bord, premier et dernier niveau (exclu)
const KEEP_TIERS: [(i32, i32, i32); 3] = [(0, 0, 16), (4, 16, 28), (7, 28, 38)];
/// Profondeur des douves
const MOAT_DEPTH: i32 = 4;

/// Plus haut niveau (exclu) de l'hôtel de ville (la pointe de son beffroi)
const HALL_TOP: i32 = 33;
/// Hauteur des murs de la halle de l'hôtel de ville
const HALL_WALL: i32 = 9;

/// Bloc de l'hôtel de ville en (x, y, z) : halle de briques blanchies, chaînes d'angle et
/// bandeaux de pierre taillée, toit de briques à deux pans ; beffroi carré au milieu de la façade
/// sud (porte dorée, horloge, chambre des cloches ouverte, flèche à pointe dorée)
fn town_hall_block(rect: &Rect, floor: i32, x: i32, y: i32, z: i32, ground: i32) -> Option<BlockId> {
    let f = floor;
    if y < f - 1 {
        return (y >= ground).then_some(CUT_STONE);
    }
    if y == f - 1 {
        return Some(CUT_STONE);
    }
    let level = y - f;
    let (cx, _) = rect.center();
    let dx = x - cx;
    // Beffroi : 5 × 5 contre la façade sud
    let (bz0, bz1) = (rect.z1 - 5, rect.z1);
    if dx.abs() <= 2 && z >= bz0 && z < bz1 {
        let edge = dx.abs() == 2 || z == bz0 || z == bz1 - 1;
        let corner = dx.abs() == 2 && (z == bz0 || z == bz1 - 1);
        let front = z == bz1 - 1;
        let bz = z - (bz0 + 2);
        return match level {
            // Portail doré
            0..=2 if front && dx.abs() <= 1 => Some(BlockId::AIR),
            3 if front && dx.abs() <= 1 => Some(GILDING),
            l if l < 18 => {
                if !edge {
                    return Some(if l == 4 || l == 12 || l == 17 { PLANK } else { BlockId::AIR });
                }
                if corner || l == 17 {
                    return Some(CUT_STONE);
                }
                // Horloge
                if (front || z == bz0) && dx.abs() <= 1 && (14..=16).contains(&l) {
                    return Some(if dx == 0 && l == 15 { GILDING } else if dx == 0 || l == 15 { BRONZE_TRIM } else { WHITE_BRICK });
                }
                if (l == 7 || l == 10) && !corner && (dx == 0 || bz == 0) {
                    return Some(GLASS);
                }
                Some(WHITE_BRICK)
            }
            // Chambre des cloches : piliers d'angle, une lanterne au milieu
            18..=20 => Some(if corner { CUT_STONE } else if (dx, bz) == (0, 0) && level == 19 { LANTERN } else { BlockId::AIR }),
            21 => Some(if edge { CUT_STONE } else { PLANK }),
            22 if corner => Some(CUT_STONE),
            l => {
                let k = l - 22;
                let ring = dx.abs().max(bz.abs());
                if ring <= 2 - k / 3 && k < 9 {
                    return Some(BRICK);
                }
                if (dx, bz) == (0, 0) && (9..11).contains(&k) {
                    return Some(GILDING);
                }
                (y < ground).then_some(BlockId::AIR)
            }
        };
    }
    // Halle : toit à deux pans le long de x
    let half_d = rect.depth() / 2;
    let dz = z - (rect.z0 + half_d);
    let edge = x == rect.x0 || x == rect.x1 - 1 || z == rect.z0 || z == rect.z1 - 1;
    let corner = (x == rect.x0 || x == rect.x1 - 1) && (z == rect.z0 || z == rect.z1 - 1);
    if level < HALL_WALL {
        if !edge {
            return Some(if level == 4 { PLANK } else { BlockId::AIR });
        }
        if corner || level == 4 || level == HALL_WALL - 1 {
            return Some(CUT_STONE);
        }
        let along = if z == rect.z0 || z == rect.z1 - 1 { dx } else { dz };
        if (level == 2 || level == 6) && along.rem_euclid(2) == 0 {
            return Some(GLASS);
        }
        return Some(WHITE_BRICK);
    }
    let k = level - HALL_WALL;
    let reach = half_d - k;
    if reach >= 0 && dz.abs() <= reach && x >= rect.x0 && x < rect.x1 {
        // Pignons de brique blanche aux deux bouts, toit de brique ailleurs
        if (x == rect.x0 || x == rect.x1 - 1) && dz.abs() < reach {
            return Some(WHITE_BRICK);
        }
        return Some(if dz.abs() == reach { BRICK } else if reach == 0 { BRICK_SLAB } else { BlockId::AIR });
    }
    (y < ground).then_some(BlockId::AIR)
}

/// Bloc du donjon royal en (x, y, z)
fn keep_block(rect: &Rect, floor: i32, m: &Masonry, x: i32, y: i32, z: i32, ground: i32) -> Option<BlockId> {
    let f = floor;
    if y < f - 1 {
        return (y >= ground).then_some(m.wall);
    }
    if y == f - 1 {
        return Some(m.cap);
    }
    let level = y - f;
    let (cx, cz) = rect.center();
    let half = rect.width() / 2;
    let (dx, dz) = (x - cx, z - cz);
    let ring = dx.abs().max(dz.abs());
    let corner = dx.abs() == dz.abs();
    // Flèche au-dessus du dernier étage, couronne dorée à son pied
    let (inset, _, last) = KEEP_TIERS[2];
    if level >= last {
        let k = level - last;
        let width = half - inset - 1 - k;
        if ring == half - inset && k == 0 {
            return Some(if (x + z) % 2 == 0 { GILDING } else { BlockId::AIR });
        }
        if width >= 0 && ring <= width {
            return Some(FLINT_BRICK);
        }
        // Aiguille dorée à la pointe
        if (dx, dz) == (0, 0) && (-3..0).contains(&width) {
            return Some(GILDING);
        }
        return (y < ground).then_some(BlockId::AIR);
    }
    for (t, &(inset, lo, hi)) in KEEP_TIERS.iter().enumerate() {
        if level < lo || level >= hi {
            continue;
        }
        let edge = half - inset;
        if ring > edge {
            // Terrasse de l'étage du dessous : créneaux sur son bord
            let below = half - KEEP_TIERS[t - 1].0;
            if ring == below && level == lo {
                return Some(if (x + z) % 2 == 0 { m.cap } else { BlockId::AIR });
            }
            return (y < ground).then_some(BlockId::AIR);
        }
        if level == hi - 1 {
            return Some(if ring == edge { m.cap } else { m.deck });
        }
        if ring < edge {
            // Planchers tous les cinq niveaux, le reste vide
            return Some(if level % 5 == 4 { m.deck } else { BlockId::AIR });
        }
        // Façade : portail au sud, chaînes d'angle, bandeau doré, fenêtres
        if t == 0 && dz == half && dx.abs() <= 1 && level < 4 {
            return Some(BlockId::AIR);
        }
        if t == 0 && dz == half && dx.abs() <= 2 && level == 4 {
            return Some(GILDING);
        }
        if corner {
            return Some(m.accent);
        }
        if level == hi - 2 {
            return Some(GILDING);
        }
        let along = if dx.abs() == edge { dz } else { dx };
        if level % 4 == 2 && level > 4 && along.rem_euclid(3) == 0 {
            return Some(GLASS);
        }
        return Some(m.wall);
    }
    None
}

/// Hauteur d'un mât totem (sa coiffe est au-dessus)
const TOTEM_HEIGHT: i32 = 7;
/// Hauteur de l'aiguille de cristal d'un obélisque (sa lanterne est au-dessus)
const OBELISK_HEIGHT: i32 = 8;

/// Distance d'une colonne au centre d'un rectangle, en blocs
fn from_center(rect: &Rect, x: i32, z: i32) -> f64 {
    let (cx, cz) = ((rect.x0 + rect.x1 - 1) as f64 / 2.0, (rect.z0 + rect.z1 - 1) as f64 / 2.0);
    ((x as f64 - cx).powi(2) + (z as f64 - cz).powi(2)).sqrt()
}

/// Arène : un disque de sable cerclé d'un muret de pierre dure, quatre entrées
fn arena_block(rect: &Rect, f: i32, x: i32, y: i32, z: i32, ground: i32) -> Option<BlockId> {
    let r = rect.width() as f64 / 2.0;
    let d = from_center(rect, x, z);
    if d > r {
        return None;
    }
    let ring = d > r - 1.0;
    if y < f - 1 {
        return (y >= ground).then_some(COBBLE);
    }
    if y == f - 1 {
        return Some(if ring { HARD_STONE } else { SAND });
    }
    let (cx, cz) = rect.center();
    let entrance = (x - cx).abs() <= 1 || (z - cz).abs() <= 1;
    if y == f && ring && !entrance {
        return Some(HARD_STONE);
    }
    (y < ground || y == f).then_some(BlockId::AIR)
}

/// Ossuaire : socle de pierre taillée, caveau de pierre du vide percé d'une niche au sud, coiffe et
/// lumière bleue
fn ossuary_block(rect: &Rect, f: i32, x: i32, y: i32, z: i32, ground: i32) -> Option<BlockId> {
    let (cx, cz) = rect.center();
    let ring = (x - cx).abs().max((z - cz).abs());
    if y < f - 1 {
        return (y >= ground).then_some(CUT_STONE);
    }
    let level = y - f;
    match level {
        -1 => Some(CUT_STONE),
        0 => Some(if ring <= 2 { CUT_STONE } else { BlockId::AIR }),
        1..=3 if ring <= 1 => {
            // Niche ouverte au sud, au milieu
            if x == cx && (z == cz || z == cz + 1) && level <= 2 {
                return Some(BlockId::AIR);
            }
            Some(VOID_ROCK)
        }
        4 if ring <= 1 => Some(CUT_STONE),
        5 if ring == 0 => Some(GLOWBELL_BLOOM),
        _ => (y < ground).then_some(BlockId::AIR),
    }
}

/// Obélisque : socle de pierre taillée aux angles de mithril, aiguille de cristal, lanterne
fn obelisk_block(rect: &Rect, f: i32, x: i32, y: i32, z: i32, ground: i32) -> Option<BlockId> {
    let (cx, cz) = rect.center();
    let ring = (x - cx).abs().max((z - cz).abs());
    if y < f - 1 {
        return (y >= ground).then_some(CUT_STONE);
    }
    let level = y - f;
    match level {
        -1 => Some(CUT_STONE),
        0 if ring <= 1 => Some(CUT_STONE),
        1 if ring == 1 && x != cx && z != cz => Some(SILVER_TRIM),
        l if (1..=OBELISK_HEIGHT).contains(&l) && ring == 0 => Some(CUT_CRYSTAL),
        l if l == OBELISK_HEIGHT + 1 && ring == 0 => Some(LANTERN),
        _ => (y < ground).then_some(BlockId::AIR),
    }
}

/// Sanctuaire : dallage, colonnes tous les trois blocs sur le pourtour, toit blanc cerclé d'or, un
/// étage plus étroit, une couronne d'or et une pointe de cristal ; une fleur de lumière au centre
fn sanctuary_block(rect: &Rect, f: i32, x: i32, y: i32, z: i32, ground: i32) -> Option<BlockId> {
    let (cx, cz) = rect.center();
    let ring = (x - cx).abs().max((z - cz).abs());
    if y < f - 1 {
        return (y >= ground).then_some(CUT_STONE);
    }
    let level = y - f;
    let column = rect.on_edge(x, z) && (x - rect.x0) % 3 == 0 && (z - rect.z0) % 3 == 0;
    let column = column || rect.on_corner(x, z);
    match level {
        -1 => Some(CUT_STONE),
        0 if (x, z) == (cx, cz) => Some(GLOWBELL_BLOOM),
        0..=5 => Some(if column { CUT_STONE } else { BlockId::AIR }),
        6 => Some(WHITE_BRICK),
        7 if rect.on_edge(x, z) => Some(GILDING),
        7..=9 if ring <= 2 => Some(WHITE_BRICK),
        10 | 11 if ring <= 1 => Some(GILDING),
        12 | 13 if ring == 0 => Some(CUT_CRYSTAL),
        _ => (y < ground).then_some(BlockId::AIR),
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
            SettlementKind::Town => 46,
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
    /// Culture de la ville ou du village (`None` : ruines), qui a choisi son style et son peuple
    pub culture: Option<Culture>,
    /// Toutes les colonnes que le plan peut changer
    pub bounds: Rect,
    /// Plus haut bloc (exclu) que le plan pose
    pub top: i32,
    pub pieces: Vec<Piece>,
    /// Bouts des rues où un chemin vers un autre établissement peut s'attacher : la colonne juste
    /// au-delà du bout de la rue (ou de la porte de ville) et le côté vers lequel elle part
    pub exits: Vec<((i32, i32), Side)>,
    /// Taille d'une ville (`None` : village ou ruines)
    pub size: Option<TownSize>,
    /// Pièces rangées par carreaux de colonnes, construit à la première lecture (une capitale en
    /// compte des milliers)
    index: OnceLock<PieceIndex>,
}

/// Côté d'un carreau de l'index des pièces d'un établissement
const INDEX_TILE: i32 = 16;

/// Les pièces d'un établissement rangées par carreaux de `INDEX_TILE` colonnes, dans leur ordre
#[derive(Debug, Clone, Default)]
struct PieceIndex {
    x0: i32,
    z0: i32,
    w: i32,
    d: i32,
    tiles: Vec<Vec<u32>>,
}

impl PieceIndex {
    fn build(bounds: &Rect, pieces: &[Piece]) -> PieceIndex {
        let (x0, z0) = (bounds.x0.div_euclid(INDEX_TILE), bounds.z0.div_euclid(INDEX_TILE));
        let (w, d) = ((bounds.x1 - 1).div_euclid(INDEX_TILE) - x0 + 1, (bounds.z1 - 1).div_euclid(INDEX_TILE) - z0 + 1);
        let mut tiles = vec![Vec::new(); (w.max(0) * d.max(0)) as usize];
        for (i, piece) in pieces.iter().enumerate() {
            let r = piece.rect();
            for tz in r.z0.div_euclid(INDEX_TILE).max(z0)..=(r.z1 - 1).div_euclid(INDEX_TILE).min(z0 + d - 1) {
                for tx in r.x0.div_euclid(INDEX_TILE).max(x0)..=(r.x1 - 1).div_euclid(INDEX_TILE).min(x0 + w - 1) {
                    tiles[((tz - z0) * w + tx - x0) as usize].push(i as u32);
                }
            }
        }
        PieceIndex { x0, z0, w, d, tiles }
    }

    fn tile(&self, x: i32, z: i32) -> &[u32] {
        let (tx, tz) = (x.div_euclid(INDEX_TILE) - self.x0, z.div_euclid(INDEX_TILE) - self.z0);
        if tx < 0 || tz < 0 || tx >= self.w || tz >= self.d {
            return &[];
        }
        &self.tiles[(tz * self.w + tx) as usize]
    }
}

impl Settlement {
    fn index(&self) -> &PieceIndex {
        self.index.get_or_init(|| PieceIndex::build(&self.bounds, &self.pieces))
    }

    /// Ne garde que les pièces que `keep` retient (l'index est refait)
    pub fn retain_pieces(&mut self, keep: impl FnMut(&Piece) -> bool) {
        self.pieces.retain(keep);
        self.index = OnceLock::new();
    }

    /// Pièces qui couvrent la colonne (x, z)
    fn pieces_at(&self, x: i32, z: i32) -> impl Iterator<Item = &Piece> {
        self.index().tile(x, z).iter().map(|&i| &self.pieces[i as usize]).filter(move |p| p.rect().contains(x, z))
    }

    /// Bloc imposé par le plan en (x, y, z), `ground` étant l'altitude du premier bloc d'air
    /// au-dessus du relief naturel de la colonne. `None` : le terrain naturel reste.
    pub fn block(&self, x: i32, y: i32, z: i32, ground: i32) -> Option<BlockId> {
        self.block_in(Epoch::Present, x, y, z, ground)
    }

    /// `block` à l'époque `epoch` : dans le passé, les temples sont intacts (voir `temples`), le
    /// reste du plan est celui du présent
    pub(crate) fn block_in(&self, epoch: Epoch, x: i32, y: i32, z: i32, ground: i32) -> Option<BlockId> {
        if !self.bounds.contains(x, z) {
            return None;
        }
        self.pieces_at(x, z).find_map(|p| {
            if !p.span(ground).contains(&y) {
                return None;
            }
            match (p, epoch) {
                (Piece::Temple(t), Epoch::Past(holder)) => t.past_block(holder, x, y, z, ground),
                _ => p.block(x, y, z, ground),
            }
        })
    }

    /// Blocs qui éclairent (torches, lampadaires, fontaines) posés par le plan dans les colonnes
    /// `area` : position et bloc. `ground(x, z)` donne l'altitude du premier bloc d'air au-dessus
    /// du relief naturel de la colonne (le pied d'un lampadaire). Seuls ceux que le plan pose
    /// vraiment à cet endroit sont gardés.
    pub fn lights(&self, area: Rect, ground: impl Fn(i32, i32) -> i32) -> Vec<(i32, i32, i32, BlockId)> {
        self.lights_in(Epoch::Present, area, ground)
    }

    /// `lights` à l'époque `epoch`
    pub(crate) fn lights_in(&self, epoch: Epoch, area: Rect, ground: impl Fn(i32, i32) -> i32) -> Vec<(i32, i32, i32, BlockId)> {
        let mut found = Vec::new();
        if !self.bounds.intersects(&area) {
            return found;
        }
        for piece in self.pieces.iter().filter(|p| p.rect().intersects(&area)) {
            let mut spots: Vec<(i32, i32, i32)> = Vec::new();
            match piece {
                Piece::Lamp { x, z, .. } => spots.push((*x, ground(*x, *z) + LAMP_LIGHT, *z)),
                Piece::Fountain { rect, floor } => {
                    let (cx, cz) = rect.center();
                    spots.push((cx, floor + 2, cz));
                }
                Piece::House(house) => spots.extend(house.torches().map(|(x, level, z)| (x, house.floor + level, z))),
                Piece::Ossuary { rect, floor } => {
                    let (cx, cz) = rect.center();
                    spots.push((cx, floor + 5, cz));
                }
                Piece::Obelisk { rect, floor } => {
                    let (cx, cz) = rect.center();
                    spots.push((cx, floor + OBELISK_HEIGHT + 1, cz));
                }
                Piece::Sanctuary { rect, floor } => {
                    let (cx, cz) = rect.center();
                    spots.push((cx, *floor, cz));
                }
                Piece::Temple(t) if matches!(epoch, Epoch::Past(_)) => spots.extend(t.past_light_spots()),
                _ => continue,
            }
            for (x, y, z) in spots {
                if !area.contains(x, z) {
                    continue;
                }
                if let Some(block) = self.block_in(epoch, x, y, z, ground(x, z))
                    && block.light() > 0 {
                    found.push((x, y, z, block));
                }
            }
        }
        found
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
        if !self.bounds.expand(margin).contains(x, z) {
            return false;
        }
        let index = self.index();
        let near = Rect::around(x, z, margin);
        for tz in near.z0.div_euclid(INDEX_TILE)..=(near.z1 - 1).div_euclid(INDEX_TILE) {
            for tx in near.x0.div_euclid(INDEX_TILE)..=(near.x1 - 1).div_euclid(INDEX_TILE) {
                if index.tile(tx * INDEX_TILE, tz * INDEX_TILE).iter().any(|&i| self.pieces[i as usize].rect().expand(margin).contains(x, z)) {
                    return true;
                }
            }
        }
        false
    }

    /// Culture du `n`-ième habitant : celle de la ville le plus souvent (`culture::NATIVE_SHARE`),
    /// parfois une autre. Ne dépend que de l'établissement et de `n`. Des ruines n'ont pas
    /// d'habitants : on y prend des Humains.
    pub(crate) fn resident_culture(&self, n: u64) -> Culture {
        let home = self.culture.unwrap_or(Culture::Humains);
        let seed = mix(n ^ ((face_index(self.face) as u64) << 56) ^ 0x4AB1_7A47);
        let h = hash2(seed, self.center.0, self.center.1);
        home.resident(unit(h), unit(mix(h ^ 0x07E5)))
    }
}

/// Les établissements d'une planète, case par case, dessinés à la première lecture
#[derive(Debug)]
pub struct StructureMap {
    cells: Vec<OnceLock<Option<Settlement>>>,
    /// Les régions réservées au village de chaque race (`plan_pins`)
    pins: OnceLock<Vec<Pin>>,
    /// Case de la ville ou du village de chaque région (s'il y en a un)
    hosts: Vec<OnceLock<Option<(i32, i32)>>>,
    /// La région de la capitale de la planète (`plan_capital`)
    capital: OnceLock<Option<Capital>>,
    /// Les villes-souvenirs des temples, case par case, pour chaque peuple qui peut les tenir dans
    /// la vision (dessinées à la première lecture dans le passé)
    memories: Vec<OnceLock<Vec<OnceLock<Option<MemoryCity>>>>>,
}

/// La capitale de la planète : la seule, posée dans la case `cell` de sa région
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capital {
    pub face: PlanetFace,
    pub region: (i32, i32),
    pub cell: (i32, i32),
}

impl Default for StructureMap {
    fn default() -> Self {
        Self::new()
    }
}

impl StructureMap {
    pub fn new() -> Self {
        Self {
            cells: (0..6 * CELLS_PER_FACE * CELLS_PER_FACE).map(|_| OnceLock::new()).collect(),
            pins: OnceLock::new(),
            hosts: (0..6 * REGIONS_PER_FACE * REGIONS_PER_FACE).map(|_| OnceLock::new()).collect(),
            capital: OnceLock::new(),
            memories: Culture::ALL.iter().map(|_| OnceLock::new()).collect(),
        }
    }

    /// La capitale de la planète, choisie à la première lecture
    pub fn capital(&self, generator: &PlanetWorldGenerator) -> Option<Capital> {
        *self.capital.get_or_init(|| plan_capital(generator))
    }

    /// Les régions réservées au village de chaque race, calculées à la première lecture
    pub(crate) fn pins(&self, generator: &PlanetWorldGenerator) -> &[Pin] {
        self.pins.get_or_init(|| plan_pins(generator))
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
        let (cx, cz) = (x.div_euclid(CELL), z.div_euclid(CELL));
        if let Some(settlement) = self.cell(generator, face, cx, cz)
            && settlement.bounds.contains(x, z) {
            return Some(settlement);
        }
        // Une ville déborde de sa case, jamais de sa région
        let town = self.region_settlement(generator, face, cx.div_euclid(REGION_CELLS), cz.div_euclid(REGION_CELLS))?;
        (town.kind == SettlementKind::Town && town.bounds.contains(x, z)).then_some(town)
    }

    /// Les établissements dont le plan touche le rectangle de colonnes `area` d'une face
    pub fn touching(&self, generator: &PlanetWorldGenerator, face: PlanetFace, area: Rect) -> Vec<&Settlement> {
        let mut found = Vec::new();
        for cz in area.z0.div_euclid(CELL)..=(area.z1 - 1).div_euclid(CELL) {
            for cx in area.x0.div_euclid(CELL)..=(area.x1 - 1).div_euclid(CELL) {
                if let Some(s) = self.cell(generator, face, cx, cz)
                    && s.bounds.intersects(&area) {
                    found.push(s);
                }
            }
        }
        // Les villes des régions touchées, qui débordent de leur case
        let region = |v: i32| v.div_euclid(CELL).div_euclid(REGION_CELLS);
        for rz in region(area.z0)..=region(area.z1 - 1) {
            for rx in region(area.x0)..=region(area.x1 - 1) {
                if let Some(town) = self.region_settlement(generator, face, rx, rz)
                    && town.kind == SettlementKind::Town && town.bounds.intersects(&area) && !found.iter().any(|s| std::ptr::eq(*s, town)) {
                    found.push(town);
                }
            }
        }
        found
    }
}

impl StructureMap {
    /// La ville-souvenir du temple de la case (cx, cz) d'une face, tenue dans la vision par le
    /// peuple `holder`, s'il y a un temple dans la case
    pub fn memory_city(&self, generator: &PlanetWorldGenerator, face: PlanetFace, cx: i32, cz: i32, holder: Culture) -> Option<&MemoryCity> {
        let index = Self::cell_index(face, cx, cz)?;
        let k = Culture::ALL.iter().position(|&c| c == holder)?;
        let cells = self.memories[k].get_or_init(|| (0..self.cells.len()).map(|_| OnceLock::new()).collect());
        cells[index]
            .get_or_init(|| {
                let ruins = self.cell(generator, face, cx, cz).filter(|s| s.kind == SettlementKind::Ruins)?;
                let temple = ruins.pieces.iter().find_map(|p| match p {
                    Piece::Temple(t) => Some(*t),
                    _ => None,
                })?;
                memory_city::plan_memory_city(generator, face, temple, ruins.biome, holder)
            })
            .as_ref()
    }

    /// La ville-souvenir tenue par `holder` qui prend la colonne (x, z) d'une face. Une ville ne
    /// déborde que sur les cases voisines de celle de son temple.
    pub(crate) fn memory_at(&self, generator: &PlanetWorldGenerator, face: PlanetFace, x: i32, z: i32, holder: Culture) -> Option<&MemoryCity> {
        let (cx, cz) = (x.div_euclid(CELL), z.div_euclid(CELL));
        for dz in -1..=1 {
            for dx in -1..=1 {
                if let Some(city) = self.memory_city(generator, face, cx + dx, cz + dz, holder)
                    && city.covers(x, z) {
                    return Some(city);
                }
            }
        }
        None
    }

    /// Les villes-souvenirs tenues par `holder` dont le plan touche le rectangle `area` d'une face
    pub(crate) fn memories_touching(&self, generator: &PlanetWorldGenerator, face: PlanetFace, area: Rect, holder: Culture) -> Vec<&MemoryCity> {
        let mut found = Vec::new();
        for cz in area.z0.div_euclid(CELL) - 1..=(area.z1 - 1).div_euclid(CELL) + 1 {
            for cx in area.x0.div_euclid(CELL) - 1..=(area.x1 - 1).div_euclid(CELL) + 1 {
                if let Some(city) = self.memory_city(generator, face, cx, cz, holder)
                    && city.settlement.bounds.intersects(&area) {
                    found.push(city);
                }
            }
        }
        found
    }

    /// La ville ou le village de la région (rx, rz) d'une face (régions de `REGION_CELLS` cases de
    /// côté), s'il y en a un
    pub fn region_settlement(&self, generator: &PlanetWorldGenerator, face: PlanetFace, rx: i32, rz: i32) -> Option<&Settlement> {
        if !(0..REGIONS_PER_FACE).contains(&rx) || !(0..REGIONS_PER_FACE).contains(&rz) {
            return None;
        }
        let index = (face_index(face) * REGIONS_PER_FACE * REGIONS_PER_FACE + rz * REGIONS_PER_FACE + rx) as usize;
        let host = self.hosts[index].get_or_init(|| region_candidates(generator, face, rx, rz).into_iter().find(|&(cx, cz)| self.cell(generator, face, cx, cz).is_some()));
        host.and_then(|(cx, cz)| self.cell(generator, face, cx, cz))
    }
}

pub(crate) fn face_index(face: PlanetFace) -> i32 {
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
    site_fits_relief(generator, face, x, z, radius, step, kind, kind.max_relief())
}

/// `site_fits`, avec un écart d'altitude admis donné
fn site_fits_relief(generator: &PlanetWorldGenerator, face: PlanetFace, x: i32, z: i32, radius: i32, step: i32, kind: SettlementKind, max_relief: i32) -> bool {
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
            if max - min > max_relief {
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
    /// Bouts de rues où s'attachent les chemins (voir `Settlement::exits`)
    exits: Vec<((i32, i32), Side)>,
    /// Culture de la ville ou du village (`None` : ruines)
    culture: Option<Culture>,
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

    /// Pose un bâtiment (son plancher est calculé ici) ; sans son porche s'il gêne
    fn try_building(&mut self, house: House) -> bool {
        self.try_building_spaced(house, 1)
    }

    /// Pose un bâtiment en gardant `clear` colonnes libres autour de son emprise (avant-toits compris)
    fn try_building_spaced(&mut self, mut house: House, clear: i32) -> bool {
        // Les avant-toits peuvent toucher la rue, pas les murs
        let room = |h: &House| h.rect().union(&h.foot.expand(1));
        if house.porch && !self.free(&room(&house)) {
            // Une maison sur pilotis à perron ne se passe pas de ses marches
            if house.steps {
                return false;
            }
            house.porch = false;
        }
        if !self.free(&room(&house)) {
            return false;
        }
        let Some(floor) = floor_for(self.generator, self.face, &house.foot, 5) else {
            return false;
        };
        house.floor = floor + house.stilts;
        self.take(Piece::House(house), clear);
        true
    }

    /// Pose une forge si la place est libre
    fn try_forge(&mut self, foot: Rect, front: Side) -> bool {
        if !self.free(&foot.expand(1)) {
            return false;
        }
        let Some(floor) = floor_for(self.generator, self.face, &foot, 4) else {
            return false;
        };
        self.take(Piece::Forge(Forge { foot, floor, front }), 1);
        true
    }

    /// Emprise d'un bâtiment collé à une rue, et le côté de sa porte : `side` est le côté de la rue
    /// où il se trouve, `at` la colonne de la rue en face de laquelle il se centre, `gap` la distance
    /// de l'axe de la rue au mur de la porte, `w` sa largeur le long de la rue et `d` sa profondeur
    fn along(at: (i32, i32), street_along_x: bool, side: i32, gap: i32, w: i32, d: i32) -> (Rect, Side) {
        if street_along_x {
            let z0 = if side > 0 { at.1 + gap } else { at.1 - gap - d + 1 };
            (Rect::sized(at.0 - w / 2, z0, w, d), if side > 0 { Side::North } else { Side::South })
        } else {
            let x0 = if side > 0 { at.0 + gap } else { at.0 - gap - d + 1 };
            (Rect::sized(x0, at.1 - w / 2, d, w), if side > 0 { Side::West } else { Side::East })
        }
    }

    /// Bâtiment collé à une rue (voir `along`) ; `house` en donne tout sauf l'emprise et la porte.
    /// Un porche recule le bâtiment de deux blocs, et saute s'il ne trouve pas sa place
    fn house_along(&mut self, at: (i32, i32), street_along_x: bool, side: i32, street_half: i32, w: i32, d: i32, house: House) -> bool {
        let gap = street_half + 2;
        if house.porch {
            let (foot, door) = Self::along(at, street_along_x, side, gap + 2, w, d);
            if self.try_building(House { foot, door, ..house }) {
                return true;
            }
        }
        let (foot, door) = Self::along(at, street_along_x, side, gap, w, d);
        self.try_building(House { foot, door, porch: false, ..house })
    }

    fn finish(self, kind: SettlementKind, center: (i32, i32), biome: BiomeType) -> Option<Settlement> {
        if self.pieces.is_empty() {
            return None;
        }
        let bounds = self.pieces.iter().map(|p| p.rect()).reduce(|a, b| a.union(&b))?;
        // Les pièces qui suivent le terrain restent à quelques blocs du relief : le plus haut bloc est
        // celui d'un bâtiment
        let top = self.pieces.iter().map(piece_floor_top).max()?;
        Some(Settlement { kind, face: self.face, center, biome, culture: self.culture, bounds, top, pieces: self.pieces, exits: self.exits, size: None, index: OnceLock::new() })
    }
}

/// Plus haut bloc d'une pièce posée sur un plancher fixe (les pièces qui suivent le terrain n'en ont pas)
fn piece_floor_top(piece: &Piece) -> i32 {
    match piece {
        Piece::House(h) => h.top(),
        Piece::Forge(f) => f.top(),
        Piece::Well { floor, .. } => floor + 4,
        Piece::Fountain { floor, .. } => floor + 3,
        Piece::Tower { floor, masonry, .. } => floor + masonry.tower_top(),
        Piece::RuinHouse(r) => r.top(),
        Piece::Temple(t) => t.top(),
        Piece::RuinTower(t) => t.top(),
        Piece::Arena { floor, .. } => floor + 1,
        Piece::Ossuary { floor, .. } => floor + 6,
        Piece::Obelisk { floor, .. } => floor + OBELISK_HEIGHT + 2,
        Piece::Sanctuary { floor, .. } => floor + 14,
        Piece::Terrace { floor, height, .. } => floor + height + 2,
        Piece::Keep { floor, .. } => floor + KEEP_TOP,
        Piece::TownHall { floor, .. } => floor + HALL_TOP,
        _ => i32::MIN,
    }
}

/// Graine propre à une case
fn cell_seed(generator: &PlanetWorldGenerator, face: PlanetFace, cx: i32, cz: i32) -> u64 {
    mix(generator.planet.seed ^ 0x5717_c7ae_0000_0000 ^ ((face_index(face) as u64) << 40) ^ hash2(0xC311, cx, cz))
}

/// Écart minimal, en blocs, entre deux établissements voisins
const MIN_GAP: i32 = 48;

/// Cases où la région (rx, rz) d'une face essaie, dans l'ordre, de poser sa ville ou son village :
/// tirées dans le centre de la région, jamais côte à côte (le plan d'une case candidate ne dépend
/// ainsi d'aucune autre candidate)
fn region_candidates(generator: &PlanetWorldGenerator, face: PlanetFace, rx: i32, rz: i32) -> Vec<(i32, i32)> {
    let mut dice = Dice(mix(generator.planet.seed ^ 0x9E61_0A5E_0000_0000 ^ ((face_index(face) as u64) << 40) ^ hash2(0x2E61, rx, rz)));
    // Région rognée par le bord de la face (si la face n'en contient pas un nombre entier)
    let (x0, z0) = (rx * REGION_CELLS, rz * REGION_CELLS);
    let (x1, z1) = ((x0 + REGION_CELLS).min(CELLS_PER_FACE), (z0 + REGION_CELLS).min(CELLS_PER_FACE));
    let margin = |lo: i32, hi: i32| REGION_MARGIN.min((hi - lo - 1) / 2);
    let (mx, mz) = (margin(x0, x1), margin(z0, z1));
    let mut found: Vec<(i32, i32)> = Vec::with_capacity(REGION_CANDIDATES);
    for _ in 0..REGION_CANDIDATES * 8 {
        if found.len() == REGION_CANDIDATES {
            break;
        }
        let c = (dice.range(x0 + mx, x1 - mx), dice.range(z0 + mz, z1 - mz));
        if found.iter().all(|f| (f.0 - c.0).abs() > 1 || (f.1 - c.1).abs() > 1) {
            found.push(c);
        }
    }
    found
}

/// Cases autour d'un site de ville ou de village où rien d'autre ne se pose : une capitale déborde
/// de sa case de 210 blocs au plus
const TOWN_REACH_CELLS: i32 = 2;

/// Ce que la graine destine à une case : le rang de ville ou de village de sa région qu'elle
/// essaie (`Some(rang)`), des ruines (`None`), ou rien (`Err`)
fn cell_role(generator: &PlanetWorldGenerator, face: PlanetFace, cx: i32, cz: i32) -> Result<Option<usize>, ()> {
    let candidates = region_candidates(generator, face, cx.div_euclid(REGION_CELLS), cz.div_euclid(REGION_CELLS));
    if let Some(rank) = candidates.iter().position(|&c| c == (cx, cz)) {
        return Ok(Some(rank));
    }
    // Rien autour des sites de villes et villages : ils ne dépendent ainsi d'aucune case voisine
    if candidates.iter().any(|c| (c.0 - cx).abs() <= TOWN_REACH_CELLS && (c.1 - cz).abs() <= TOWN_REACH_CELLS) {
        return Err(());
    }
    if unit(cell_seed(generator, face, cx, cz) ^ 0x2A1E) < RUINS_CHANCE {
        Ok(None)
    } else {
        Err(())
    }
}

/// Dessine l'établissement d'une case, s'il y en a un
fn plan_cell(generator: &PlanetWorldGenerator, face: PlanetFace, cx: i32, cz: i32) -> Option<Settlement> {
    herocraft_common::prof_scope!("world::structures::plan_cell");
    match cell_role(generator, face, cx, cz) {
        Err(()) => None,
        Ok(None) => plan_site(generator, face, cx, cz, SettlementKind::Ruins, None, None),
        Ok(Some(rank)) => {
            let (rx, rz) = (cx.div_euclid(REGION_CELLS), cz.div_euclid(REGION_CELLS));
            // La région de la capitale la garde dans la case choisie, et rien ailleurs
            if let Some(capital) = generator.structures.capital(generator).filter(|c| c.face == face && c.region == (rx, rz)) {
                return if capital.cell == (cx, cz) { plan_site(generator, face, cx, cz, SettlementKind::Town, None, Some(TownSize::Capitale)) } else { None };
            }
            // Une région réservée à une race garde son village dans la case choisie, et rien ailleurs
            if let Some(pin) = generator.structures.pins(generator).iter().find(|pin| pin.face == face && pin.region == (rx, rz)) {
                return if pin.cell == (cx, cz) { plan_site(generator, face, cx, cz, SettlementKind::Village, Some(pin.culture), None) } else { None };
            }
            // La région a déjà sa ville ou son village dans une case candidate précédente
            let earlier = region_candidates(generator, face, rx, rz);
            if earlier[..rank].iter().any(|&(px, pz)| generator.structures.cell(generator, face, px, pz).is_some()) {
                return None;
            }
            let mut dice = Dice(cell_seed(generator, face, cx, cz));
            let wanted = if unit(dice.next()) < 0.46 { SettlementKind::Town } else { SettlementKind::Village };
            let size = if unit(dice.next()) < 0.4 { TownSize::GrandeVille } else { TownSize::PetiteVille };
            plan_site(generator, face, cx, cz, wanted, None, (wanted == SettlementKind::Town).then_some(size))
        }
    }
}

/// Cherche dans la case un site pour un établissement `wanted` et en dessine le plan. `forced` :
/// la culture du village, imposée (sinon tirée du climat et du royaume).
fn plan_site(generator: &PlanetWorldGenerator, face: PlanetFace, cx: i32, cz: i32, wanted: SettlementKind, forced: Option<Culture>, size: Option<TownSize>) -> Option<Settlement> {
    let mut dice = Dice(cell_seed(generator, face, cx, cz));
    if wanted != SettlementKind::Ruins {
        // Les tirages de la ville ou du village et de sa taille (voir `plan_cell`)
        dice.next();
        dice.next();
    }
    let cell = Rect::sized(cx * CELL, cz * CELL, CELL, CELL);
    // Les établissements des cases voisines qui passent avant celle-ci (rang tiré de leur graine) :
    // le site doit rester à distance d'eux. Le rang décroît strictement le long de ces appels, qui
    // finissent donc toujours, et le plan ne dépend pas de l'ordre de lecture
    let rank = |cx: i32, cz: i32| (mix(cell_seed(generator, face, cx, cz) ^ 0x7A11), cx, cz);
    let mine = rank(cx, cz);
    let neighbours: Vec<Rect> = [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)]
        .into_iter()
        .map(|(dx, dz)| (cx + dx, cz + dz))
        .filter(|&(nx, nz)| (0..CELLS_PER_FACE).contains(&nx) && (0..CELLS_PER_FACE).contains(&nz) && rank(nx, nz) > mine)
        .filter_map(|(nx, nz)| generator.structures.cell(generator, face, nx, nz).map(|s| s.bounds))
        .collect();
    // Un site refusé à une grande ville peut accueillir une petite ville, puis un village (jamais
    // une capitale, qui cherche ailleurs)
    let kinds: Vec<(SettlementKind, Option<TownSize>)> = match (wanted, size) {
        (SettlementKind::Town, Some(TownSize::Capitale)) => vec![(SettlementKind::Town, Some(TownSize::Capitale))],
        (SettlementKind::Town, Some(TownSize::GrandeVille)) => {
            vec![(SettlementKind::Town, Some(TownSize::GrandeVille)), (SettlementKind::Town, Some(TownSize::PetiteVille)), (SettlementKind::Village, None)]
        }
        (SettlementKind::Town, _) => vec![(SettlementKind::Town, Some(TownSize::PetiteVille)), (SettlementKind::Village, None)],
        (kind, _) => vec![(kind, None)],
    };
    for (kind, size) in kinds {
        // Une ville déborde de sa case : son centre y reste, son plan tient dans sa région
        let (radius, margin) = match size {
            Some(s) => (s.reach(), 8),
            None => (kind.radius(), kind.radius() + 4),
        };
        for _ in 0..10 {
            let x = dice.range(cell.x0 + margin, cell.x1 - margin);
            let z = dice.range(cell.z0 + margin, cell.z1 - margin);
            let reach = Rect::around(x, z, radius + MIN_GAP);
            if neighbours.iter().any(|n| n.intersects(&reach)) {
                continue;
            }
            let (g, biome) = ground_at(generator, face, x, z);
            if !kind.accepts(biome) || (g as f64) <= SEA_LEVEL_BLOCKS + 1.0 {
                continue;
            }
            let fits = match size {
                Some(s) => site_fits_relief(generator, face, x, z, s.half(), 12, kind, s.max_relief()),
                None => site_fits(generator, face, x, z, kind.radius(), if kind == SettlementKind::Ruins { 7 } else { 8 }, kind),
            };
            if !fits {
                continue;
            }
            let culture = match kind {
                SettlementKind::Ruins => None,
                _ => Some(forced.unwrap_or_else(|| settlement_culture(generator, face, (x, z), biome, kind == SettlementKind::Town))),
            };
            let area = match size {
                Some(s) => Rect::around(x, z, s.reach() + 4),
                None => cell.expand(-2),
            };
            let planner = Planner { generator, face, area, pieces: Vec::new(), taken: Vec::new(), exits: Vec::new(), culture };
            let settlement = match kind {
                SettlementKind::Town => plan_town_sized(planner, (x, z), biome, &mut dice, size.unwrap_or(TownSize::PetiteVille)),
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

/// Une région réservée au village d'une race : chaque race a ainsi au moins un village sur la planète
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Pin {
    pub face: PlanetFace,
    pub region: (i32, i32),
    /// La case candidate de la région où le village se pose
    pub cell: (i32, i32),
    pub culture: Culture,
}

/// Réserve une région à chaque race. Pour chacune, à son tour, les régions encore libres sont
/// rangées selon le goût de la race pour leur climat (relevé au centre de leurs cases candidates),
/// puis essayées dans l'ordre jusqu'à ce que l'une accueille son village. Le calcul ne lit aucune
/// autre case candidate que celles qu'il essaie : il ne dépend que de la graine.
fn plan_pins(generator: &PlanetWorldGenerator) -> Vec<Pin> {
    herocraft_common::prof_scope!("world::structures::plan_pins");
    let seed = generator.planet.seed ^ 0x9105_7A1E_0000_0000;
    // Climat de chaque région : celui de sa première case candidate sur la terre ferme
    let mut regions: Vec<(PlanetFace, (i32, i32), BiomeType)> = Vec::new();
    for face in PlanetFace::ALL {
        for rz in 0..REGIONS_PER_FACE {
            for rx in 0..REGIONS_PER_FACE {
                let land = region_candidates(generator, face, rx, rz).into_iter().find_map(|(cx, cz)| {
                    let (g, biome) = ground_at(generator, face, cx * CELL + CELL / 2, cz * CELL + CELL / 2);
                    ((g as f64) > SEA_LEVEL_BLOCKS + 1.0 && SettlementKind::Village.accepts(biome)).then_some(biome)
                });
                if let Some(biome) = land {
                    regions.push((face, (rx, rz), biome));
                }
            }
        }
    }
    let mut pins: Vec<Pin> = Vec::new();
    // Les races sans ville d'abord : elles sont plus rares
    let order = Culture::ALL.into_iter().filter(|c| !c.builds_towns()).chain(Culture::TOWN_BUILDERS);
    for culture in order {
        let mut choices: Vec<(f64, u64, PlanetFace, (i32, i32))> = regions
            .iter()
            .filter(|(face, region, _)| !pins.iter().any(|p| p.face == *face && p.region == *region))
            .map(|&(face, region, biome)| {
                let tie = hash2(seed ^ ((face_index(face) as u64) << 40) ^ culture as u64, region.0, region.1);
                (culture.affinity(biome), tie, face, region)
            })
            .collect();
        choices.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
        for (_, _, face, (rx, rz)) in choices {
            let site = region_candidates(generator, face, rx, rz)
                .into_iter()
                .find(|&(cx, cz)| plan_site(generator, face, cx, cz, SettlementKind::Village, Some(culture), None).is_some());
            if let Some(cell) = site {
                pins.push(Pin { face, region: (rx, rz), cell, culture });
                break;
            }
        }
    }
    pins
}

/// La capitale de la planète : la première région, dans un ordre tiré de la graine, qui n'est pas
/// réservée au village d'une race et où une capitale trouve sa place
fn plan_capital(generator: &PlanetWorldGenerator) -> Option<Capital> {
    herocraft_common::prof_scope!("world::structures::plan_capital");
    let seed = generator.planet.seed ^ 0xCA91_7A1E_0000_0000;
    let pins = generator.structures.pins(generator);
    let mut regions: Vec<(u64, PlanetFace, (i32, i32))> = Vec::new();
    for face in PlanetFace::ALL {
        for rz in 0..REGIONS_PER_FACE {
            for rx in 0..REGIONS_PER_FACE {
                if pins.iter().any(|p| p.face == face && p.region == (rx, rz)) {
                    continue;
                }
                regions.push((hash2(seed ^ ((face_index(face) as u64) << 40), rx, rz), face, (rx, rz)));
            }
        }
    }
    regions.sort_by_key(|&(h, face, region)| (h, face_index(face), region));
    for (_, face, (rx, rz)) in regions {
        for (cx, cz) in region_candidates(generator, face, rx, rz) {
            // Le centre de la case doit être sur la terre ferme, sinon inutile de chercher
            let (g, biome) = ground_at(generator, face, cx * CELL + CELL / 2, cz * CELL + CELL / 2);
            if (g as f64) <= SEA_LEVEL_BLOCKS + 1.0 || !SettlementKind::Town.accepts(biome) {
                continue;
            }
            if plan_site(generator, face, cx, cz, SettlementKind::Town, None, Some(TownSize::Capitale)).is_some() {
                return Some(Capital { face, region: (rx, rz), cell: (cx, cz) });
            }
        }
    }
    None
}

/// Culture d'une ville ou d'un village dont le centre est en `center`, au climat `biome` : tirée
/// de sa propre graine (le plan n'en dépend pas), penchant vers celle de son royaume, un carré de
/// `REALM_REGIONS` × `REALM_REGIONS` régions
fn settlement_culture(generator: &PlanetWorldGenerator, face: PlanetFace, center: (i32, i32), biome: BiomeType, town: bool) -> Culture {
    let seed = generator.planet.seed ^ 0xC017_0BE5_0000_0000 ^ ((face_index(face) as u64) << 40);
    let realm_size = REALM_REGIONS * REGION_BLOCKS;
    let realm = hash2(seed ^ 0x8EA1, center.0.div_euclid(realm_size), center.1.div_euclid(realm_size));
    let realm = Culture::ALL[(realm % Culture::ALL.len() as u64) as usize];
    Culture::choose(biome, realm, unit(hash2(seed ^ 0xC1, center.0, center.1)), town)
}

/// Côté d'un royaume, en régions : ses villes penchent vers une même culture
const REALM_REGIONS: i32 = 3;

/// Pavé des rues d'une culture
fn street_block(culture: Culture, biome: BiomeType, town: bool) -> BlockId {
    match culture {
        Culture::Humains if town => COBBLE,
        Culture::Humains if matches!(biome, BiomeType::Desert | BiomeType::Savanna | BiomeType::Beach) => COBBLE,
        Culture::Humains => GRAVEL,
        Culture::Nains => if town { CUT_STONE } else { COBBLE },
        Culture::ElfesDesBois => GRAVEL,
        Culture::HautsElfes => if town { CUT_STONE } else { GRAVEL },
        Culture::DemiOrques | Culture::HommesBetes => GRAVEL,
        Culture::Revenants => if biome == BiomeType::Beach { SAND } else { GRAVEL },
        Culture::Sylphes | Culture::Aasimars => CUT_STONE,
        Culture::Gnomes => COBBLE,
    }
}

/// Maison ordinaire tirée au sort : toit à deux ou quatre pans, cheminée, porche
fn random_home(dice: &mut Dice, storeys: i32, style: Style, porch: f64, hip: f64) -> House {
    let mut house = House::home(Rect::sized(0, 0, 1, 1), 0, storeys, Side::North, style);
    if style.roof.is_some() && dice.chance(hip) {
        house.roof = Roof::Hip;
    }
    house.chimney = dice.chance(0.35);
    house.porch = dice.chance(porch);
    house
}

/// Bâtiment particulier d'un village ou d'une ville, avec sa largeur (le long de la rue) et sa profondeur
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Special {
    Church,
    Tavern,
    Forge,
}

impl Special {
    /// Une ville a toujours une grande église, un village parfois une petite chapelle
    fn size(self, dice: &mut Dice, town: bool) -> (i32, i32) {
        match self {
            Special::Church => (if town || dice.chance(0.5) { 7 } else { 5 }, 9),
            Special::Tavern => (dice.range(7, 10), 7),
            Special::Forge => (dice.range(5, 7), 5),
        }
    }

    fn house(self, dice: &mut Dice, style: Style) -> Option<House> {
        let mut house = House::home(Rect::sized(0, 0, 1, 1), 0, 1, Side::North, style);
        match self {
            Special::Church => {
                house.usage = Use::Church;
                house.height = 6;
                if house.style.roof.is_none() {
                    house.style.roof = Some(COBBLE_STAIRS);
                    house.style.ridge = COBBLE;
                }
            }
            Special::Tavern => {
                house.usage = Use::Tavern;
                house.storeys = 2;
                house.height = 7;
                house.chimney = true;
                house.porch = true;
                if house.style.roof.is_some() && dice.chance(0.5) {
                    house.roof = Roof::Hip;
                }
            }
            Special::Forge => return None,
        }
        Some(house)
    }
}

/// Mélange une liste
fn shuffle<T>(dice: &mut Dice, items: &mut [T]) {
    for i in (1..items.len()).rev() {
        let j = dice.range(0, i as i32 + 1) as usize;
        items.swap(i, j);
    }
}

/// Le cœur d'un village, au milieu de la place : un puits, ou le monument de sa culture. Rend le
/// demi-côté de la place.
fn plan_village_heart(p: &mut Planner, culture: Culture, center: (i32, i32), dice: &mut Dice) -> Option<i32> {
    let (x, z) = center;
    let (radius, heart): (i32, fn(Rect, i32) -> Piece) = match culture {
        Culture::DemiOrques => (6, |rect, floor| Piece::Arena { rect, floor }),
        Culture::Revenants => (2, |rect, floor| Piece::Ossuary { rect, floor }),
        Culture::Sylphes => (2, |rect, floor| Piece::Obelisk { rect, floor }),
        Culture::Aasimars => (6, |rect, floor| Piece::Sanctuary { rect, floor }),
        _ => (1, |rect, floor| Piece::Well { rect, floor }),
    };
    let rect = Rect::around(x, z, radius);
    let floor = floor_for(p.generator, p.face, &rect, 4)?;
    p.take(heart(rect, floor), 0);
    // Les Hommes-bêtes dressent quatre mâts totems aux angles de la place
    if culture == Culture::HommesBetes {
        let tops = [GILDING, BRONZE_TRIM, SILVER_TRIM, IRON_TRIM];
        for (i, (dx, dz)) in [(-3, -3), (3, -3), (-3, 3), (3, 3)].into_iter().enumerate() {
            let top = tops[(i + dice.range(0, 4) as usize) % tops.len()];
            p.take(Piece::Totem { x: x + dx, z: z + dz, top }, 0);
        }
    }
    Some(match culture {
        Culture::DemiOrques | Culture::Aasimars => radius + 2,
        _ => 3,
    })
}

/// Maison du village selon les habitudes de la culture : forme, pilotis, perron
fn village_home(dice: &mut Dice, habits: &Habits, style: Style) -> House {
    let storeys = if dice.chance(habits.upstairs.0) { 2 } else { 1 };
    let mut house = random_home(dice, storeys, style, habits.porch, habits.hip);
    house.form = habits.form;
    house.stilts = habits.stilts;
    house.steps = habits.steps;
    if habits.form == Form::Burrow {
        // Toujours une cheminée d'atelier
        house.chimney = true;
        house.porch = false;
    }
    house
}

/// Palissade de pieux autour du camp (Demi-orques) : un anneau à deux blocs de tout ce qui est bâti,
/// percé d'une porte flanquée de deux mâts totems là où chaque chemin le traverse. Aucun bâtiment
/// ne touche ainsi la palissade.
fn plan_palisade(p: &mut Planner, center: (i32, i32), arms: &[Side], masonry: Masonry) {
    let (x, z) = center;
    let built = p.pieces.iter().filter(|piece| !matches!(piece, Piece::Paving { .. })).map(|piece| piece.rect()).reduce(|a, b| a.union(&b));
    let Some(built) = built else { return };
    let ring = built.expand(2);
    // Une porte de trois blocs sur chaque chemin
    let mut gates: Vec<Rect> = Vec::new();
    for &arm in arms {
        let gate = match arm {
            Side::North => Rect::new(x - 1, ring.z0, x + 2, ring.z0 + 1),
            Side::South => Rect::new(x - 1, ring.z1 - 1, x + 2, ring.z1),
            Side::West => Rect::new(ring.x0, z - 1, ring.x0 + 1, z + 2),
            Side::East => Rect::new(ring.x1 - 1, z - 1, ring.x1, z + 2),
        };
        gates.push(gate);
        // Le chemin traverse la porte et sort d'un bloc au-delà
        let (dx, dz) = arm.delta();
        let reach = match arm {
            Side::North => z - ring.z0 + 1,
            Side::South => ring.z1 - z,
            Side::West => x - ring.x0 + 1,
            Side::East => ring.x1 - x,
        };
        let (a, b) = ((x + dx * 4, z + dz * 4), (x + dx * reach, z + dz * reach));
        let along_x = dz == 0;
        let road = Rect::new(a.0.min(b.0) - (!along_x) as i32, a.1.min(b.1) - along_x as i32, a.0.max(b.0) + 1 + (!along_x) as i32, a.1.max(b.1) + 1 + along_x as i32);
        p.pieces.push(Piece::Paving { rect: road, block: GRAVEL });
        // La sortie du chemin passe au-delà de la palissade
        if let Some(exit) = p.exits.iter_mut().find(|e| e.1 == arm) {
            let out = (x + dx * (reach + 1), z + dz * (reach + 1));
            let farther = (out.0 - x).abs() + (out.1 - z).abs() > (exit.0 .0 - x).abs() + (exit.0 .1 - z).abs();
            if farther {
                exit.0 = out;
            }
        }
        // Deux mâts totems de part et d'autre, dehors
        let flank = match arm {
            Side::North | Side::South => [(x - 2, gate.z0 + dz), (x + 2, gate.z0 + dz)],
            Side::West | Side::East => [(gate.x0 + dx, z - 2), (gate.x0 + dx, z + 2)],
        };
        for (tx, tz) in flank {
            let rect = Rect::sized(tx, tz, 1, 1);
            if p.area.expand(2).contains(tx, tz) && !p.taken.iter().any(|t| t.intersects(&rect)) {
                p.pieces.push(Piece::Totem { x: tx, z: tz, top: IRON_TRIM });
            }
        }
    }
    // Les quatre côtés, coupés aux portes
    let sides = [
        Rect::new(ring.x0, ring.z0, ring.x1, ring.z0 + 1),
        Rect::new(ring.x0, ring.z1 - 1, ring.x1, ring.z1),
        Rect::new(ring.x0, ring.z0 + 1, ring.x0 + 1, ring.z1 - 1),
        Rect::new(ring.x1 - 1, ring.z0 + 1, ring.x1, ring.z1 - 1),
    ];
    for side in sides {
        let along_x = side.depth() == 1;
        let mut cuts: Vec<(i32, i32)> = gates
            .iter()
            .filter(|g| g.intersects(&side))
            .map(|g| if along_x { (g.x0, g.x1) } else { (g.z0, g.z1) })
            .collect();
        cuts.sort();
        let (lo, hi) = if along_x { (side.x0, side.x1) } else { (side.z0, side.z1) };
        let mut start = lo;
        for (c0, c1) in cuts.into_iter().chain(std::iter::once((hi, hi))) {
            if c0 > start {
                let rect = if along_x { Rect::new(start, side.z0, c0, side.z1) } else { Rect::new(side.x0, start, side.x1, c0) };
                p.taken.push(rect);
                p.pieces.push(Piece::Rampart { rect, masonry, walk: false });
            }
            start = start.max(c1);
        }
    }
    for gate in gates {
        p.taken.push(gate);
        p.pieces.push(Piece::Gate { rect: gate, masonry, walk: false });
    }
}

fn plan_village(mut p: Planner, center: (i32, i32), biome: BiomeType, dice: &mut Dice) -> Option<Settlement> {
    let (x, z) = center;
    let culture = p.culture.unwrap_or(Culture::Humains);
    let (habits, masonry) = (Habits::of(culture), Masonry::of(culture));
    let plaza = plan_village_heart(&mut p, culture, center, dice)?;
    let path = street_block(culture, biome, false);
    p.take(Piece::Paving { rect: Rect::around(x, z, plaza), block: path }, 0);

    // Deux à quatre chemins partent de la place
    let mut arms: Vec<Side> = Side::ALL.into_iter().filter(|_| dice.chance(0.8)).collect();
    if arms.len() < 2 {
        arms = vec![Side::West, Side::East];
    }
    // Une chapelle, une taverne et une forge (chacune pas toujours), posées en premier le long des chemins
    let mut specials: Vec<Special> = [(Special::Church, habits.church), (Special::Tavern, habits.tavern), (Special::Forge, habits.forge)]
        .into_iter()
        .filter(|&(_, p)| dice.chance(p))
        .map(|(s, _)| s)
        .collect();
    shuffle(dice, &mut specials);
    let mut houses = 0;
    for &arm in &arms {
        let length = dice.range(18, p.area.width().min(2 * SettlementKind::Village.radius()) / 2 + 1);
        let (dx, dz) = arm.delta();
        let along_x = dz == 0;
        // Le chemin, large de 3
        let (a, b) = ((x + dx * (plaza + 1), z + dz * (plaza + 1)), (x + dx * length, z + dz * length));
        let road = Rect::new(a.0.min(b.0) - (!along_x) as i32, a.1.min(b.1) - along_x as i32, a.0.max(b.0) + 1 + (!along_x) as i32, a.1.max(b.1) + 1 + along_x as i32);
        p.take(Piece::Paving { rect: road, block: path }, 0);
        p.exits.push(((x + dx * (length + 1), z + dz * (length + 1)), arm));
        // Bâtiments des deux côtés, porte sur le chemin
        let mut t = plaza + 4;
        while t + 3 <= length {
            for side in [-1, 1] {
                let at = (x + dx * t, z + dz * t);
                if let Some(&special) = specials.first() {
                    let (w, d) = special.size(dice, false);
                    let style = Style::village(culture, biome, dice);
                    let placed = match special.house(dice, style) {
                        Some(mut house) => {
                            // La taverne bâtit comme les maisons : butte, pilotis
                            house.form = habits.form;
                            house.stilts = habits.stilts;
                            house.steps = habits.steps;
                            if habits.form == Form::Burrow {
                                house.storeys = 1;
                                house.height = 4;
                                house.porch = false;
                            } else if habits.stilts > 0 {
                                house.porch = true;
                            }
                            p.house_along(at, along_x, side, 1, w, d, house)
                        }
                        None => {
                            let (foot, door) = Planner::along(at, along_x, side, 3, w, d);
                            p.try_forge(foot, door)
                        }
                    };
                    if placed {
                        specials.remove(0);
                        continue;
                    }
                }
                if !dice.chance(0.85) {
                    continue;
                }
                let (w, d) = (dice.range(habits.width.0, habits.width.1), dice.range(habits.depth.0, habits.depth.1));
                let style = Style::village(culture, biome, dice);
                let house = village_home(dice, &habits, style);
                if p.house_along(at, along_x, side, 1, w, d, house) {
                    houses += 1;
                }
            }
            // Un lampadaire au bord du chemin, de loin en loin
            if t % 2 == 1 && dice.chance(0.5) {
                let side = if dice.chance(0.5) { 2 } else { -2 };
                let lamp = if along_x { (x + dx * t, z + side) } else { (x + side, z + dz * t) };
                let rect = Rect::sized(lamp.0, lamp.1, 1, 1);
                if p.free(&rect) {
                    p.take(Piece::Lamp { x: lamp.0, z: lamp.1, masonry }, 0);
                }
            }
            t += dice.range(7, 10);
        }
    }
    if houses < 2 {
        return None;
    }
    if masonry.palisade && !culture.builds_towns() {
        plan_palisade(&mut p, center, &arms, masonry);
    }
    // Terrasses des Aasimars, hautes sur leur colonne, aux abords
    if culture == Culture::Aasimars {
        let r = SettlementKind::Village.radius();
        let mut placed = 0;
        for _ in 0..24 {
            if placed >= 3 {
                break;
            }
            let rect = Rect::sized(x + dice.range(-r, r - 6), z + dice.range(-r, r - 6), 6, 6);
            if !p.free(&rect.expand(1)) {
                continue;
            }
            let (cx, cz) = rect.center();
            let column = Rect::new(cx - 1, cz - 1, cx + 1, cz + 1);
            let Some(floor) = floor_for(p.generator, p.face, &column, 3) else { continue };
            p.take(Piece::Terrace { rect, floor, height: dice.range(10, 15) }, 1);
            placed += 1;
        }
    }
    // Enclos de chasse des Hommes-bêtes
    if culture == Culture::HommesBetes {
        let r = SettlementKind::Village.radius();
        let mut placed = 0;
        for _ in 0..24 {
            if placed >= 2 {
                break;
            }
            let (w, d) = (dice.range(8, 13), dice.range(7, 11));
            let rect = Rect::sized(x + dice.range(-r, r - w), z + dice.range(-r, r - d), w, d);
            if !p.free(&rect.expand(1)) || floor_for(p.generator, p.face, &rect, 3).is_none() {
                continue;
            }
            p.take(Piece::Pen { rect }, 1);
            placed += 1;
        }
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
    // Une grange aux abords, grande porte tournée vers la place
    if placed > 0 && habits.barn && dice.chance(0.7) {
        let r = SettlementKind::Village.radius();
        for _ in 0..16 {
            let (w, d) = if dice.chance(0.5) { (7, 9) } else { (9, 7) };
            let foot = Rect::sized(x + dice.range(-r, r - w), z + dice.range(-r, r - d), w, d);
            let (bcx, bcz) = foot.center();
            let door = if (bcx - x).abs() > (bcz - z).abs() {
                if bcx > x { Side::West } else { Side::East }
            } else if bcz > z {
                Side::North
            } else {
                Side::South
            };
            let mut style = Style::village(culture, biome, dice);
            if style.roof.is_none() {
                style.roof = Some(PLANK_STAIRS);
                style.ridge = PLANK;
            }
            style.wall = PLANK;
            let mut barn = House::home(foot, 0, 1, door, style);
            barn.usage = Use::Barn;
            barn.height = 5;
            barn.door_width = 3;
            if p.try_building(barn) {
                break;
            }
        }
    }
    p.finish(SettlementKind::Village, center, biome)
}

/// Taille d'une ville fortifiée : le côté de son enceinte, en multiples de celui du bourg d'origine
/// (85 blocs). Chaque quartier de la taille du bourg a sa petite place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TownSize {
    /// La ville d'origine : 85 blocs de côté, une grand-place
    Bourg,
    /// Deux fois le côté du bourg : quatre quartiers
    PetiteVille,
    /// Quatre fois le côté du bourg : seize quartiers, trois portes par côté
    GrandeVille,
    /// Huit fois le côté du bourg : soixante-quatre quartiers et une citadelle au centre
    Capitale,
}

impl TownSize {
    pub const ALL: [TownSize; 4] = [TownSize::Bourg, TownSize::PetiteVille, TownSize::GrandeVille, TownSize::Capitale];

    pub fn name(&self) -> &'static str {
        match self {
            TownSize::Bourg => "bourg",
            TownSize::PetiteVille => "petite-ville",
            TownSize::GrandeVille => "grande-ville",
            TownSize::Capitale => "capitale",
        }
    }

    /// Largeur de l'enceinte, en largeurs de bourg
    pub fn scale(&self) -> f64 {
        match self {
            TownSize::Bourg => 1.0,
            TownSize::PetiteVille => 1.5,
            TownSize::GrandeVille => 3.0,
            TownSize::Capitale => 4.0,
        }
    }

    /// Demi-largeur moyenne de l'enceinte, en blocs
    pub fn half(&self) -> i32 {
        (42.0 * self.scale()).round() as i32
    }

    /// Distance au centre que le plan ne dépasse jamais (l'enceinte ondule, les grandes rues
    /// sortent un peu des portes)
    pub fn reach(&self) -> i32 {
        (self.half() as f64 * 1.22) as i32 + 8
    }

    /// Écart d'altitude admis sur le site
    fn max_relief(&self) -> i32 {
        match self {
            TownSize::Bourg => 16,
            TownSize::PetiteVille => 20,
            TownSize::GrandeVille => 30,
            TownSize::Capitale => 34,
        }
    }
}

/// Écart entre deux rues d'une ville
const TOWN_STEP: i32 = 14;

/// Une enceinte carrée de demi-côté `half` autour de `center` : tours d'angle (et tous les
/// `TOWN_STEP × 3` blocs si `towers`), portes aux décalages `gates` sur chaque côté, double rangée
/// de remparts dont l'intérieure porte le chemin de ronde. Ses abords sont réservés.
fn plan_enclosure(p: &mut Planner, center: (i32, i32), half: i32, masonry: Masonry, gates: &[i32], towers: bool) -> Option<()> {
    let (x, z) = center;
    let walls = Rect::new(x - half, z - half, x + half + 1, z + half + 1);
    for (tx, tz) in [(walls.x0, walls.z0), (walls.x1 - 1, walls.z0), (walls.x0, walls.z1 - 1), (walls.x1 - 1, walls.z1 - 1)] {
        let rect = Rect::around(tx, tz, 2);
        let floor = floor_for(p.generator, p.face, &rect, 6)?;
        p.take(Piece::Tower { rect, floor, masonry }, 0);
    }
    if towers {
        // Au milieu d'un pâté sur trois, entre deux rues
        let mut o = -half + 3 * TOWN_STEP / 2;
        while o < half {
            for (tx, tz) in [(x + o, walls.z0), (x + o, walls.z1 - 1), (walls.x0, z + o), (walls.x1 - 1, z + o)] {
                let rect = Rect::around(tx, tz, 2);
                if let Some(floor) = floor_for(p.generator, p.face, &rect, 6) {
                    p.take(Piece::Tower { rect, floor, masonry }, 0);
                }
            }
            o += 3 * TOWN_STEP;
        }
    }
    for &o in gates {
        for (gate, walk) in [
            (Rect::new(x + o - 1, walls.z0, x + o + 2, walls.z0 + 1), false),
            (Rect::new(x + o - 1, walls.z1 - 1, x + o + 2, walls.z1), false),
            (Rect::new(walls.x0, z + o - 1, walls.x0 + 1, z + o + 2), false),
            (Rect::new(walls.x1 - 1, z + o - 1, walls.x1, z + o + 2), false),
            (Rect::new(x + o - 1, walls.z0 + 1, x + o + 2, walls.z0 + 2), true),
            (Rect::new(x + o - 1, walls.z1 - 2, x + o + 2, walls.z1 - 1), true),
            (Rect::new(walls.x0 + 1, z + o - 1, walls.x0 + 2, z + o + 2), true),
            (Rect::new(walls.x1 - 2, z + o - 1, walls.x1 - 1, z + o + 2), true),
        ] {
            p.take(Piece::Gate { rect: gate, masonry, walk }, 0);
        }
    }
    for rect in [
        Rect::new(walls.x0, walls.z0, walls.x1, walls.z0 + 1),
        Rect::new(walls.x0, walls.z1 - 1, walls.x1, walls.z1),
        Rect::new(walls.x0, walls.z0, walls.x0 + 1, walls.z1),
        Rect::new(walls.x1 - 1, walls.z0, walls.x1, walls.z1),
    ] {
        p.pieces.push(Piece::Rampart { rect, masonry, walk: false });
    }
    for rect in [
        Rect::new(walls.x0 + 1, walls.z0 + 1, walls.x1 - 1, walls.z0 + 2),
        Rect::new(walls.x0 + 1, walls.z1 - 2, walls.x1 - 1, walls.z1 - 1),
        Rect::new(walls.x0 + 1, walls.z0 + 1, walls.x0 + 2, walls.z1 - 1),
        Rect::new(walls.x1 - 2, walls.z0 + 1, walls.x1 - 1, walls.z1 - 1),
    ] {
        p.pieces.push(Piece::Rampart { rect, masonry, walk: true });
    }
    for side in [
        Rect::new(walls.x0 - 2, walls.z0 - 2, walls.x1 + 2, walls.z0 + 2),
        Rect::new(walls.x0 - 2, walls.z1 - 2, walls.x1 + 2, walls.z1 + 2),
        Rect::new(walls.x0 - 2, walls.z0 - 2, walls.x0 + 2, walls.z1 + 2),
        Rect::new(walls.x1 - 2, walls.z0 - 2, walls.x1 + 2, walls.z1 + 2),
    ] {
        p.taken.push(side);
    }
    Some(())
}

/// Une place de demi-côté `TOWN_STEP - 2` au carrefour `at` : une fontaine ou un puits au milieu,
/// parfois des étals tournés vers lui. Rend la place.
fn plan_town_square(p: &mut Planner, at: (i32, i32), street: BlockId, fountain: bool, stalls: i32, dice: &mut Dice) -> Option<Rect> {
    let (x, z) = at;
    if fountain {
        let rect = Rect::around(x, z, 2);
        let floor = floor_for(p.generator, p.face, &rect, 3)?;
        p.take(Piece::Fountain { rect, floor }, 0);
    } else {
        let rect = Rect::around(x, z, 1);
        let floor = floor_for(p.generator, p.face, &rect, 3)?;
        p.take(Piece::Well { rect, floor }, 0);
    }
    // Étals tournés vers le milieu (posés avant le pavage de la place)
    let mut spots: [(i32, i32); 8] = [(-7, -7), (7, -7), (-7, 7), (7, 7), (-4, -9), (4, 9), (9, -4), (-9, 4)];
    shuffle(dice, &mut spots);
    let goods = [WHEAT, CARROT, POTATO, SQUASH, PLANK, CUT_CRYSTAL];
    for &(ox, oz) in &spots[..stalls.clamp(0, 8) as usize] {
        let (rect, front) = if ox.abs() > oz.abs() {
            (Rect::sized(x + ox - (ox > 0) as i32, z + oz - 1, 2, 3), if ox > 0 { Side::West } else { Side::East })
        } else {
            (Rect::sized(x + ox - 1, z + oz - (oz > 0) as i32, 3, 2), if oz > 0 { Side::North } else { Side::South })
        };
        p.pieces.push(Piece::Stall(Stall { rect, front, goods: dice.pick(&goods) }));
    }
    let plaza = Rect::around(x, z, TOWN_STEP - 2);
    p.pieces.push(Piece::Paving { rect: plaza, block: street });
    p.taken.push(plaza.expand(1));
    Some(plaza)
}

/// Les morceaux de `lane` hors de `hole` (deux au plus : `lane` est une rue droite)
fn lane_around(lane: Rect, hole: &Rect) -> Vec<Rect> {
    if !lane.intersects(hole) {
        return vec![lane];
    }
    let mut parts = Vec::new();
    if lane.width() > lane.depth() {
        parts.push(Rect::new(lane.x0, lane.z0, hole.x0, lane.z1));
        parts.push(Rect::new(hole.x1, lane.z0, lane.x1, lane.z1));
    } else {
        parts.push(Rect::new(lane.x0, lane.z0, lane.x1, hole.z0));
        parts.push(Rect::new(lane.x0, hole.z1, lane.x1, lane.z1));
    }
    parts.retain(|r| r.width() > 0 && r.depth() > 0);
    parts
}

fn plan_town_sized(mut p: Planner, center: (i32, i32), biome: BiomeType, dice: &mut Dice, size: TownSize) -> Option<Settlement> {
    if size != TownSize::Bourg {
        return plan_town_organic(p, center, biome, dice, size);
    }
    let (x, z) = center;
    // Le damier : rues tous les 14 blocs, remparts à `half` blocs du centre
    const STEP: i32 = TOWN_STEP;
    let half = size.half();
    let scale: i32 = (half / 42).max(1);
    let walls = Rect::new(x - half, z - half, x + half + 1, z + half + 1);
    let culture = p.culture.unwrap_or(Culture::Humains);
    let (habits, masonry) = (Habits::of(culture), Masonry::of(culture));
    let street = street_block(culture, biome, true);

    // Portes : au milieu de chaque côté, et à mi-chemin des angles pour les plus grandes villes
    let gates: Vec<i32> = if scale >= 4 { vec![-half / 2, 0, half / 2] } else { vec![0] };
    plan_enclosure(&mut p, center, half, masonry, &gates, scale >= 2)?;
    for &o in &gates {
        p.exits.extend([
            ((x + o, walls.z0 - 1), Side::North),
            ((x + o, walls.z1), Side::South),
            ((walls.x0 - 1, z + o), Side::West),
            ((walls.x1, z + o), Side::East),
        ]);
    }
    // Échelles vers le chemin de ronde, au bout des rues qui butent contre le rempart (sur le bord
    // de la rue, qui garde deux blocs de passage)
    let mut offset = -half + STEP;
    while offset < half {
        if !gates.contains(&offset) {
            for (ladder, wall, back) in [
                ((walls.x0 + 2, z + offset + 1), (walls.x0 + 1, z + offset + 1), Side::West),
                ((walls.x1 - 3, z + offset + 1), (walls.x1 - 2, z + offset + 1), Side::East),
                ((x + offset + 1, walls.z0 + 2), (x + offset + 1, walls.z0 + 1), Side::North),
                ((x + offset + 1, walls.z1 - 3), (x + offset + 1, walls.z1 - 2), Side::South),
            ] {
                let top = ground_at(p.generator, p.face, wall.0, wall.1).0 + masonry.height;
                if ground_at(p.generator, p.face, ladder.0, ladder.1).0 < top {
                    p.pieces.push(Piece::Ladder { x: ladder.0, z: ladder.1, back, top });
                }
            }
        }
        offset += STEP;
    }

    // Le cœur de la ville : la grand-place et sa fontaine, ou, dans une capitale, la citadelle et
    // son donjon
    let mut plazas: Vec<Rect> = Vec::new();
    let mut citadel: Option<Rect> = None;
    if size == TownSize::Capitale {
        const INNER: i32 = 2 * STEP - 2;
        let keep = Rect::around(x, z, 5);
        let floor = floor_for(p.generator, p.face, &keep, 6)?;
        let tall = Masonry { tower_height: masonry.tower_height + 10, ..masonry };
        p.take(Piece::Tower { rect: keep, floor, masonry: tall }, 1);
        plan_enclosure(&mut p, center, INNER, masonry, &[0], false)?;
        let court = Rect::around(x, z, INNER - 2);
        p.pieces.push(Piece::Paving { rect: court, block: street });
        let hole = Rect::around(x, z, INNER);
        p.taken.push(hole.expand(2));
        citadel = Some(hole);
    } else {
        let stalls = dice.range(4, 7);
        plazas.push(plan_town_square(&mut p, center, street, true, stalls, dice)?);
    }
    // Une petite place au milieu de la plupart des quartiers
    if scale >= 2 {
        for i in 0..scale {
            for j in 0..scale {
                let at = (x - half + 3 * STEP + 6 * STEP * i, z - half + 3 * STEP + 6 * STEP * j);
                if scale > 2 && !dice.chance(0.55) {
                    continue;
                }
                let fountain = dice.chance(0.5);
                let stalls = if dice.chance(0.6) { dice.range(2, 6) } else { 0 };
                if let Some(plaza) = plan_town_square(&mut p, at, street, fountain, stalls, dice) {
                    plazas.push(plaza);
                }
            }
        }
    }
    // Rues pavées (elles contournent la citadelle)
    let mut offset = -(half / STEP) * STEP;
    while offset <= half {
        if offset.abs() < half {
            let street_x = Rect::new(walls.x0 + 1, z + offset - 1, walls.x1 - 1, z + offset + 2);
            let street_z = Rect::new(x + offset - 1, walls.z0 + 1, x + offset + 2, walls.z1 - 1);
            for lane in [street_x, street_z] {
                let parts = match &citadel {
                    Some(hole) => lane_around(lane, hole),
                    None => vec![lane],
                };
                for part in parts {
                    p.pieces.push(Piece::Paving { rect: part, block: street });
                    p.taken.push(part);
                }
            }
        }
        offset += STEP;
    }
    // Un bâtiment par pâté de maisons : les églises sur les places, des tavernes et des forges
    // ailleurs, des maisons partout autour
    let mut blocks = Vec::new();
    let mut bz = -half;
    while bz < half {
        let mut bx = -half;
        while bx < half {
            // Pâté de 11 × 11 entre deux rues de 3 de large
            let block = Rect::new(x + bx + 2, z + bz + 2, x + bx + STEP - 1, z + bz + STEP - 1);
            if !plazas.iter().any(|pl| block.intersects(pl)) && !citadel.is_some_and(|c| block.intersects(&c.expand(1))) {
                blocks.push(block);
            }
            bx += STEP;
        }
        bz += STEP;
    }
    // Les pâtés qui donnent sur une place d'abord : les églises en prennent
    let near_plaza = |b: &Rect| {
        let (bcx, bcz) = b.center();
        plazas.iter().any(|pl| {
            let (px, pz) = pl.center();
            b.expand(4).intersects(pl) && ((bcx - px).abs() < STEP || (bcz - pz).abs() < STEP)
        })
    };
    let (mut first, mut rest): (Vec<Rect>, Vec<Rect>) = blocks.into_iter().partition(near_plaza);
    shuffle(dice, &mut first);
    shuffle(dice, &mut rest);
    let districts = scale * scale;
    let mut specials = vec![Special::Church; (1 + (districts - 1) / 3) as usize];
    let mut others = Vec::new();
    for _ in 0..districts {
        others.push(Special::Tavern);
        others.push(Special::Forge);
        if dice.chance(0.5) {
            others.push(Special::Tavern);
        }
    }
    shuffle(dice, &mut others);
    // Les églises d'abord, autour des places ; les autres, semés ensuite dans tout le reste
    let churches = specials.len();
    let mut blocks: Vec<Rect> = first.into_iter().chain(rest).collect();
    if districts > 1 {
        let tail = blocks.split_off(churches.min(blocks.len()));
        let mut tail = tail;
        shuffle(dice, &mut tail);
        blocks.extend(tail);
    }
    specials.extend(others);
    let mut houses = 0;
    for block in blocks {
        // Porte sur la rue la plus proche du centre
        let (bcx, bcz) = block.center();
        let door = if (bcx - x).abs() > (bcz - z).abs() {
            if bcx > x { Side::West } else { Side::East }
        } else if bcz > z {
            Side::North
        } else {
            Side::South
        };
        let style = Style::town(culture, biome, dice);
        // Emprise de `w` (le long de la rue) sur `d`, contre la rue de la porte, au hasard le long d'elle
        let place = |dice: &mut Dice, w: i32, d: i32, back: i32| {
            let along = dice.range(0, 10 - w);
            let deep = back.min(10 - d - 1);
            match door {
                Side::North => Rect::sized(block.x0 + 1 + along, block.z0 + 1 + deep, w, d),
                Side::South => Rect::sized(block.x0 + 1 + along, block.z1 - 1 - deep - d, w, d),
                Side::West => Rect::sized(block.x0 + 1 + deep, block.z0 + 1 + along, d, w),
                Side::East => Rect::sized(block.x1 - 1 - deep - d, block.z0 + 1 + along, d, w),
            }
        };
        if let Some(&special) = specials.first() {
            let (w, d) = special.size(dice, true);
            let placed = match special.house(dice, style) {
                Some(house) => {
                    let back = if house.porch { 2 } else { 0 };
                    let foot = place(dice, w, d, back);
                    p.try_building(House { foot, door, ..house })
                }
                None => {
                    let foot = place(dice, w, d, 0);
                    p.try_forge(foot, door)
                }
            };
            if placed {
                specials.remove(0);
                continue;
            }
        }
        if !dice.chance(0.9) {
            continue;
        }
        let storeys = if dice.chance(habits.upstairs.1) { 2 } else { 1 };
        // Maison de 5 à 9 de côté, avant-toits compris dans le pâté
        let (w, d) = (dice.range(5, 10), dice.range(5, 10));
        let mut house = random_home(dice, storeys, style, 0.2 * habits.porch / 0.3, habits.hip);
        let back = if house.porch { 2 } else { dice.range(0, 2) };
        house.foot = place(dice, w, d, back);
        house.door = door;
        if p.try_building(house) {
            houses += 1;
        }
    }
    if houses < 6 {
        return None;
    }
    // Lampadaires aux carrefours, au coin des rues (pas au milieu des places ni dans la citadelle)
    let mut lx = -(half / STEP) * STEP + STEP;
    while lx < half {
        let mut lz = -(half / STEP) * STEP + STEP;
        while lz < half {
            let (ax, az) = (x + lx + 1, z + lz + 1);
            let in_square = plazas.iter().any(|pl| pl.center() == (x + lx, z + lz));
            let in_citadel = citadel.is_some_and(|c| c.expand(2).contains(ax, az));
            if !in_square && !in_citadel && dice.chance(0.7) {
                p.pieces.push(Piece::Lamp { x: ax, z: az, masonry });
            }
            lz += STEP;
        }
        lx += STEP;
    }
    p.finish(SettlementKind::Town, center, biome)
}

/// Ce que porte une colonne du plan d'une ville aux formes libres
mod cell {
    pub(crate) const OUT: u8 = 0;
    pub(crate) const IN: u8 = 1;
    pub(crate) const WALL: u8 = 2;
    pub(crate) const WALK: u8 = 3;
    pub(crate) const GATE: u8 = 4;
    pub(crate) const GATE_WALK: u8 = 5;
    pub(crate) const ROAD: u8 = 6;
    pub(crate) const SQUARE: u8 = 7;
    pub(crate) const BUILT: u8 = 8;
    pub(crate) const RESERVED: u8 = 9;
    /// Sous l'avant-toit d'un bâtiment, ou devant sa porte : pas de mur, mais rien d'autre non plus
    pub(crate) const EAVES: u8 = 10;
    /// Passage d'une porte de ville
    pub(crate) const GATEHOUSE: u8 = 11;
}

/// Plan colonne par colonne d'une ville aux formes libres, autour de son centre
struct TownMask {
    x0: i32,
    z0: i32,
    n: i32,
    cells: Vec<u8>,
}

impl TownMask {
    fn get(&self, x: i32, z: i32) -> u8 {
        let (i, j) = (x - self.x0, z - self.z0);
        if i < 0 || j < 0 || i >= self.n || j >= self.n {
            return cell::OUT;
        }
        self.cells[(j * self.n + i) as usize]
    }

    fn set(&mut self, x: i32, z: i32, v: u8) {
        let (i, j) = (x - self.x0, z - self.z0);
        if i >= 0 && j >= 0 && i < self.n && j < self.n {
            self.cells[(j * self.n + i) as usize] = v;
        }
    }

    /// Les colonnes que `keep` retient, en rectangles (rangées fusionnées quand elles se suivent)
    fn runs(&self, keep: impl Fn(u8) -> bool) -> Vec<Rect> {
        let mut done: Vec<Rect> = Vec::new();
        let mut open: Vec<Rect> = Vec::new();
        for j in 0..self.n {
            let mut row = Vec::new();
            let mut i = 0;
            while i < self.n {
                if keep(self.cells[(j * self.n + i) as usize]) {
                    let start = i;
                    while i < self.n && keep(self.cells[(j * self.n + i) as usize]) {
                        i += 1;
                    }
                    row.push((self.x0 + start, self.x0 + i));
                } else {
                    i += 1;
                }
            }
            let z = self.z0 + j;
            let mut next = Vec::new();
            for (a, b) in row {
                if let Some(k) = open.iter().position(|r| r.x0 == a && r.x1 == b) {
                    let mut r = open.swap_remove(k);
                    r.z1 = z + 1;
                    next.push(r);
                } else {
                    next.push(Rect::new(a, z, b, z + 1));
                }
            }
            done.append(&mut open);
            open = next;
        }
        done.append(&mut open);
        done
    }

    /// Trace une rue de demi-largeur `r` le long de `points`, en changeant les colonnes selon `paint`
    fn stroke(&mut self, points: &[(f64, f64)], r: f64, paint: impl Fn(u8) -> Option<u8>) {
        for w in points.windows(2) {
            let ((ax, az), (bx, bz)) = (w[0], w[1]);
            let len = ((bx - ax).powi(2) + (bz - az).powi(2)).sqrt();
            let steps = (len * 2.0).ceil().max(1.0) as i32;
            for s in 0..=steps {
                let t = s as f64 / steps as f64;
                let (px, pz) = (ax + (bx - ax) * t, az + (bz - az) * t);
                let reach = r.ceil() as i32 + 1;
                for dz in -reach..=reach {
                    for dx in -reach..=reach {
                        let (cx, cz) = (px.floor() as i32 + dx, pz.floor() as i32 + dz);
                        let (ex, ez) = (cx as f64 + 0.5 - px, cz as f64 + 0.5 - pz);
                        if ex * ex + ez * ez <= r * r
                            && let Some(v) = paint(self.get(cx, cz)) {
                            self.set(cx, cz, v);
                        }
                    }
                }
            }
        }
    }
}

/// Un tracé qui serpente doucement de `a` à `b` (`amp` : écart maximal à la ligne droite)
fn wobbly(a: (f64, f64), b: (f64, f64), amp: f64, dice: &mut Dice) -> Vec<(f64, f64)> {
    let (dx, dz) = (b.0 - a.0, b.1 - a.1);
    let len = (dx * dx + dz * dz).sqrt().max(1.0);
    let (nx, nz) = (-dz / len, dx / len);
    let (f1, p1, p2) = (1.0 + unit(dice.next()) * 1.5, unit(dice.next()) * std::f64::consts::TAU, unit(dice.next()) * std::f64::consts::TAU);
    let n = (len / 4.0).ceil() as i32;
    (0..=n)
        .map(|i| {
            let t = i as f64 / n as f64;
            let off = amp * (std::f64::consts::PI * t).sin() * (0.65 * (t * std::f64::consts::PI * 2.0 * f1 + p1).sin() + 0.35 * (t * std::f64::consts::PI * 5.0 + p2).sin());
            (a.0 + dx * t + nx * off, a.1 + dz * t + nz * off)
        })
        .collect()
}

/// Côté vers lequel pointe la direction (dx, dz)
fn side_toward(dx: f64, dz: f64) -> Side {
    if dx.abs() > dz.abs() {
        if dx > 0.0 { Side::East } else { Side::West }
    } else if dz > 0.0 {
        Side::South
    } else {
        Side::North
    }
}

/// Une ville aux formes libres : une enceinte qui ondule, des grandes rues qui serpentent du
/// centre aux portes, des rues en anneau et des ruelles entre elles, des maisons alignées sur les
/// rues, des places aux carrefours ; la citadelle royale au cœur d'une capitale.
fn plan_town_organic(mut p: Planner, center: (i32, i32), biome: BiomeType, dice: &mut Dice, size: TownSize) -> Option<Settlement> {
    use cell::*;
    use std::f64::consts::{PI, TAU};
    let (x, z) = center;
    let (fx, fz) = (x as f64 + 0.5, z as f64 + 0.5);
    let culture = p.culture.unwrap_or(Culture::Humains);
    let (habits, masonry) = (Habits::of(culture), Masonry::of(culture));
    let street = street_block(culture, biome, true);
    let capital = size == TownSize::Capitale;
    let big_r = size.half() as f64;

    // Contour : un cercle déformé par trois ondulations
    let waves: Vec<(f64, f64, f64)> = [(2.0, 0.07, 0.11), (3.0, 0.04, 0.07), (5.0, 0.015, 0.03)]
        .iter()
        .map(|&(k, lo, hi)| (k, lo + (hi - lo) * unit(dice.next()), unit(dice.next()) * TAU))
        .collect();
    let rad = |t: f64| big_r * (1.0 + waves.iter().map(|&(k, a, ph)| a * (k * t + ph).sin()).sum::<f64>());
    let ext = (big_r * 1.3) as i32 + 12;
    let n = 2 * ext + 1;
    let mut m = TownMask { x0: x - ext, z0: z - ext, n, cells: vec![OUT; (n * n) as usize] };
    // Colonnes prises par un étal, une fontaine, un puits ou l'hôtel de ville, et celles devant
    // une porte (rien ne s'y pose)
    let mut occupied: std::collections::HashSet<(i32, i32)> = Default::default();
    let mark = |set: &mut std::collections::HashSet<(i32, i32)>, r: &Rect| {
        for cz in r.z0..r.z1 {
            for cx in r.x0..r.x1 {
                set.insert((cx, cz));
            }
        }
    };
    for cz in z - ext..=z + ext {
        for cx in x - ext..=x + ext {
            let (dx, dz) = (cx as f64 + 0.5 - fx, cz as f64 + 0.5 - fz);
            if (dx * dx + dz * dz).sqrt() <= rad(dz.atan2(dx)) {
                m.set(cx, cz, IN);
            }
        }
    }
    // Les deux rangées du rempart : la plus à l'extérieur, puis celle du chemin de ronde
    let dirs = [(1, 0), (-1, 0), (0, 1), (0, -1)];
    for (from, to, next_to) in [(IN, WALL, OUT), (IN, WALK, WALL)] {
        let mut hits = Vec::new();
        for cz in z - ext..=z + ext {
            for cx in x - ext..=x + ext {
                if m.get(cx, cz) == from && dirs.iter().any(|&(dx, dz)| m.get(cx + dx, cz + dz) == next_to) {
                    hits.push((cx, cz));
                }
            }
        }
        for (cx, cz) in hits {
            m.set(cx, cz, to);
        }
    }

    // Portes : réparties autour de l'enceinte ; celle du sud, dans une capitale, ouvre l'avenue royale
    let gate_count = match size {
        TownSize::PetiteVille => 4,
        TownSize::GrandeVille => 6,
        _ => 8,
    };
    let turn = unit(dice.next()) * TAU;
    let gates: Vec<f64> = (0..gate_count)
        .map(|i| {
            if capital {
                PI / 2.0 + TAU * i as f64 / gate_count as f64 + if i == 0 { 0.0 } else { (unit(dice.next()) - 0.5) * 0.25 }
            } else {
                turn + TAU * i as f64 / gate_count as f64 + (unit(dice.next()) - 0.5) * 0.35
            }
        })
        .collect();
    let near_gate = |t: f64, margin: f64| gates.iter().any(|&g| ((t - g + PI).rem_euclid(TAU) - PI).abs() < margin);

    // Le cœur : la citadelle royale ou la grand-place
    if capital {
        for cz in z - CITADEL - 11..=z + CITADEL + 11 {
            for cx in x - CITADEL - 11..=x + CITADEL + 11 {
                let ring = (cx - x).abs().max((cz - z).abs());
                m.set(cx, cz, if ring <= CITADEL + 6 { RESERVED } else { SQUARE });
            }
        }
        plan_citadel(&mut p, center, masonry, street, culture, biome, dice)?;
    }
    // Places : disques un peu irréguliers, une fontaine ou un puits au milieu, des étals autour
    let mut squares: Vec<((i32, i32), i32, bool)> = Vec::new();
    if !capital {
        squares.push((center, 15, true));
    }
    let rings: &[f64] = match size {
        TownSize::PetiteVille => &[0.5],
        TownSize::GrandeVille => &[0.36, 0.68],
        _ => &[0.36, 0.58, 0.8],
    };
    for (k, &f) in rings.iter().enumerate() {
        for &g in &gates {
            if dice.chance(if k == 0 { 0.6 } else { 0.4 }) {
                let r = f * rad(g);
                let at = ((fx + r * g.cos()) as i32, (fz + r * g.sin()) as i32);
                squares.push((at, dice.range(6, 9), dice.chance(0.5)));
            }
        }
    }
    let mut plazas: Vec<Rect> = Vec::new();
    for &((sx, sz), r, fountain) in &squares {
        let wob = unit(dice.next()) * TAU;
        for dz in -r - 2..=r + 2 {
            for dx in -r - 2..=r + 2 {
                let t = (dz as f64).atan2(dx as f64);
                let rr = r as f64 * (1.0 + 0.15 * (3.0 * t + wob).sin());
                if ((dx * dx + dz * dz) as f64).sqrt() <= rr && m.get(sx + dx, sz + dz) == IN {
                    m.set(sx + dx, sz + dz, SQUARE);
                }
            }
        }
        // La grand-place : l'hôtel de ville au nord, sa fontaine devant lui
        let main = (sx, sz) == center;
        if main {
            let hall = Rect::sized(sx - 9, sz - 14, 19, 13);
            let floor = floor_for(p.generator, p.face, &hall, 6)?;
            p.take(Piece::TownHall { rect: hall, floor }, 1);
            mark(&mut occupied, &hall.expand(1));
        }
        let (sx, sz) = if main { (sx, sz + 6) } else { (sx, sz) };
        let piece = if fountain { Rect::around(sx, sz, 2) } else { Rect::around(sx, sz, 1) };
        let Some(floor) = floor_for(p.generator, p.face, &piece, 5) else { continue };
        p.take(if fountain { Piece::Fountain { rect: piece, floor } } else { Piece::Well { rect: piece, floor } }, 0);
        mark(&mut occupied, &piece);
        plazas.push(Rect::around(sx, sz, r));
        // Étals tournés vers le milieu de la place
        let goods = [WHEAT, CARROT, POTATO, SQUASH, PLANK, CUT_CRYSTAL];
        // Le marché : sur la grand-place, ou sur la première place d'une capitale
        let market = main || (capital && plazas.len() == 1);
        let stalls = if market { dice.range(4, 8) } else if dice.chance(0.6) { dice.range(2, 5) } else { 0 };
        let mut placed = 0;
        for _ in 0..stalls * 6 {
            if placed == stalls {
                break;
            }
            let t = unit(dice.next()) * TAU;
            let d = if market && !capital { 5.0 + unit(dice.next()) * 2.0 } else { (r as f64 * 0.6).max(4.0) };
            let (ox, oz) = ((t.cos() * d) as i32, (t.sin() * d) as i32);
            let (rect, front) = if ox.abs() > oz.abs() {
                (Rect::sized(sx + ox - (ox > 0) as i32, sz + oz - 1, 2, 3), if ox > 0 { Side::West } else { Side::East })
            } else {
                (Rect::sized(sx + ox - 1, sz + oz - (oz > 0) as i32, 3, 2), if oz > 0 { Side::North } else { Side::South })
            };
            if p.free(&rect.expand(1)) {
                p.take(Piece::Stall(Stall { rect, front, goods: dice.pick(&goods) }), 0);
                mark(&mut occupied, &rect);
                placed += 1;
            }
        }
    }

    // Grandes rues : du centre aux portes, un peu au-delà du rempart
    let mut exits = Vec::new();
    let mut gatehouses: Vec<Rect> = Vec::new();
    for (i, &g) in gates.iter().enumerate() {
        let r = rad(g) + 6.0;
        let end = (fx + r * g.cos(), fz + r * g.sin());
        let line = wobbly((fx, fz), end, big_r * 0.06, dice);
        let half_width = if capital && i == 0 { 2.5 } else { 1.6 };
        m.stroke(&line, half_width, |c| match c {
            IN | OUT => Some(ROAD),
            WALL => Some(GATE),
            WALK => Some(GATE_WALK),
            _ => None,
        });
        exits.push(((end.0 as i32, end.1 as i32), side_toward(g.cos(), g.sin())));
        // La porte : un passage droit de trois blocs de large, voûté, là où la rue passe le rempart
        let Some(&(px, pz)) = line.iter().find(|&&(px, pz)| {
            let (dx, dz) = (px - fx, pz - fz);
            (dx * dx + dz * dz).sqrt() >= rad(dz.atan2(dx)) - 1.5
        }) else {
            continue;
        };
        let (px, pz) = (px.floor() as i32, pz.floor() as i32);
        let wide = if capital && i == 0 { 2 } else { 1 };
        let rect = if g.cos().abs() >= g.sin().abs() {
            Rect::new(px - 2, pz - wide, px + 3, pz + wide + 1)
        } else {
            Rect::new(px - wide, pz - 2, px + wide + 1, pz + 3)
        };
        for cz in rect.z0..rect.z1 {
            for cx in rect.x0..rect.x1 {
                m.set(cx, cz, GATEHOUSE);
            }
        }
        gatehouses.push(rect);
    }
    // La rue a pu entamer le rempart à côté de la porte : il est refermé
    for cz in z - ext..=z + ext {
        for cx in x - ext..=x + ext {
            match m.get(cx, cz) {
                GATE => m.set(cx, cz, WALL),
                GATE_WALK => m.set(cx, cz, WALK),
                _ => {}
            }
        }
    }
    // Rues en anneau, et celle qui longe le rempart
    let mut ring_lines: Vec<f64> = rings.to_vec();
    ring_lines.push(-1.0);
    for &f in &ring_lines {
        let ph = unit(dice.next()) * TAU;
        let steps = (big_r * f.abs().max(0.9) * TAU / 4.0) as i32;
        let line: Vec<(f64, f64)> = (0..=steps)
            .map(|i| {
                let t = TAU * i as f64 / steps as f64;
                let r = if f < 0.0 { rad(t) - 5.0 } else { f * rad(t) + big_r * 0.025 * (4.0 * t + ph).sin() };
                (fx + r * t.cos(), fz + r * t.sin())
            })
            .collect();
        m.stroke(&line, 1.5, |c| (c == IN).then_some(ROAD));
    }
    // Ruelles entre deux anneaux (et entre le premier et le centre)
    let mut bands: Vec<(f64, f64)> = Vec::new();
    let mut inner = if capital { 0.28 } else { 0.12 };
    for &f in rings.iter().chain([&0.97]) {
        bands.push((inner, f));
        inner = f;
    }
    for &(a, b) in &bands {
        let count = ((a + b) * 0.5 * big_r * TAU / 22.0).round() as i32;
        for _ in 0..count {
            let t = unit(dice.next()) * TAU;
            if near_gate(t, 0.15) {
                continue;
            }
            let t2 = t + (unit(dice.next()) - 0.5) * 0.2;
            let from = (fx + a * rad(t) * t.cos(), fz + a * rad(t) * t.sin());
            let to = (fx + (b * rad(t2) - if b > 0.9 { 5.0 } else { 0.0 }) * t2.cos(), fz + (b * rad(t2) - if b > 0.9 { 5.0 } else { 0.0 }) * t2.sin());
            let line = wobbly(from, to, 3.0, dice);
            m.stroke(&line, 1.1, |c| (c == IN).then_some(ROAD));
        }
    }

    // Tours le long du rempart, loin des portes
    let perimeter = TAU * big_r;
    let tower_count = (perimeter / 36.0).round() as i32;
    for i in 0..tower_count {
        let t = TAU * (i as f64 + 0.5) / tower_count as f64 + turn;
        if near_gate(t, 9.0 / big_r) {
            continue;
        }
        let r = rad(t) - 1.0;
        let rect = Rect::around((fx + r * t.cos()).floor() as i32, (fz + r * t.sin()).floor() as i32, 2);
        if let Some(floor) = floor_for(p.generator, p.face, &rect, 6) {
            p.take(Piece::Tower { rect, floor, masonry }, 0);
        }
    }
    // Échelles vers le chemin de ronde, entre les tours
    for i in 0..2 * tower_count {
        let t = TAU * (i as f64 * 0.5 + 0.25) / tower_count as f64 + turn;
        if near_gate(t, 8.0 / big_r) {
            continue;
        }
        let mut prev = None;
        let mut r = rad(t) - 8.0;
        while r < rad(t) + 2.0 {
            let c = ((fx + r * t.cos()).floor() as i32, (fz + r * t.sin()).floor() as i32);
            if m.get(c.0, c.1) == WALK {
                break;
            }
            prev = Some(c);
            r += 0.5;
        }
        let Some((lx, lz)) = prev else { continue };
        if !matches!(m.get(lx, lz), IN | ROAD) {
            continue;
        }
        let Some(&(dx, dz)) = dirs.iter().find(|&&(dx, dz)| m.get(lx + dx, lz + dz) == WALK) else { continue };
        let back = match (dx, dz) {
            (1, _) => Side::East,
            (-1, _) => Side::West,
            (_, 1) => Side::South,
            _ => Side::North,
        };
        let top = ground_at(p.generator, p.face, lx + dx, lz + dz).0 + masonry.height;
        if ground_at(p.generator, p.face, lx, lz).0 < top {
            p.pieces.push(Piece::Ladder { x: lx, z: lz, back, top });
            m.set(lx, lz, BUILT);
        }
    }
    // Portes, remparts, pavés
    for &rect in &gatehouses {
        p.pieces.push(Piece::Gate { rect, masonry, walk: false });
    }
    for rect in m.runs(|c| c == WALL) {
        p.pieces.push(Piece::Rampart { rect, masonry, walk: false });
    }
    for rect in m.runs(|c| c == WALK) {
        p.pieces.push(Piece::Rampart { rect, masonry, walk: true });
    }
    for rect in m.runs(|c| c == ROAD || c == SQUARE) {
        p.pieces.push(Piece::Paving { rect, block: street });
    }
    p.exits.extend(exits);

    // Un bâtiment posé : son emprise est bâtie, ses avant-toits et son seuil restent libres de murs
    let claim = |m: &mut TownMask, used: &Rect, foot: &Rect| {
        for cz in used.z0..used.z1 {
            for cx in used.x0..used.x1 {
                if foot.contains(cx, cz) {
                    m.set(cx, cz, BUILT);
                } else if m.get(cx, cz) == IN {
                    m.set(cx, cz, EAVES);
                }
            }
        }
    };
    // Maisons alignées sur les rues, la porte sur la rue ; celles des places d'abord
    let mut fronts = Vec::new();
    for cz in z - ext..=z + ext {
        for cx in x - ext..=x + ext {
            let c = m.get(cx, cz);
            if c != ROAD && c != SQUARE {
                continue;
            }
            for &(dx, dz) in &dirs {
                if m.get(cx + dx, cz + dz) == IN {
                    fronts.push(((cx, cz), (dx, dz), c == SQUARE));
                }
            }
        }
    }
    let (mut first, mut rest): (Vec<_>, Vec<_>) = fronts.into_iter().partition(|f| f.2);
    shuffle(dice, &mut first);
    shuffle(dice, &mut rest);
    let districts = (size.scale() * size.scale()).round() as i32;
    let mut specials = vec![Special::Church; (1 + (districts - 1) / 3) as usize];
    let mut others = Vec::new();
    for _ in 0..districts {
        others.push(Special::Tavern);
        others.push(Special::Forge);
        if dice.chance(0.5) {
            others.push(Special::Tavern);
        }
    }
    shuffle(dice, &mut others);
    specials.extend(others);
    let mut houses = 0;
    for (k, ((rx, rz), (dx, dz), _)) in first.into_iter().chain(rest).enumerate() {
        if m.get(rx + dx, rz + dz) != IN {
            continue;
        }
        // Les églises sur les places ; les autres bâtiments publics semés dans toute la ville
        let special = specials.first().copied().filter(|s| *s == Special::Church || k % 7 == 0);
        let door = match (dx, dz) {
            (1, _) => Side::West,
            (-1, _) => Side::East,
            (_, 1) => Side::North,
            _ => Side::South,
        };
        let style = Style::town(culture, biome, dice);
        let (w, d, house) = match special {
            Some(s) => {
                let (w, d) = s.size(dice, true);
                (w, d, s.house(dice, style))
            }
            None => {
                if !dice.chance(0.92) {
                    continue;
                }
                let storeys = if dice.chance(habits.upstairs.1) { 2 } else { 1 };
                (dice.range(5, 10), dice.range(5, 9), Some(random_home(dice, storeys, style, 0.2 * habits.porch / 0.3, habits.hip)))
            }
        };
        let back = match &house {
            Some(h) if h.porch => 2,
            Some(_) if special.is_none() => dice.range(0, 2),
            _ => 0,
        };
        let shift = dice.range(-1, 2);
        let foot = match door {
            Side::West => Rect::sized(rx + 1 + back, rz - w / 2 + shift, d, w),
            Side::East => Rect::sized(rx - back - d, rz - w / 2 + shift, d, w),
            Side::North => Rect::sized(rx - w / 2 + shift, rz + 1 + back, w, d),
            Side::South => Rect::sized(rx - w / 2 + shift, rz - back - d, w, d),
        };
        let fits = (foot.z0..foot.z1).all(|cz| (foot.x0..foot.x1).all(|cx| m.get(cx, cz) == IN && !occupied.contains(&(cx, cz))));
        let clear = |r: &Rect| (r.z0..r.z1).all(|cz| (r.x0..r.x1).all(|cx| matches!(m.get(cx, cz), IN | ROAD | SQUARE | BUILT | EAVES)));
        if !fits || !clear(&foot.expand(1)) {
            continue;
        }
        let front = {
            let probe = House::home(foot, 0, 1, door, style);
            let (dx, dz) = probe.door_column();
            let (ox, oz) = door.delta();
            (dx + ox, dz + oz)
        };
        if occupied.contains(&front) || !matches!(m.get(front.0, front.1), IN | ROAD | SQUARE | EAVES) {
            continue;
        }
        let before = p.pieces.len();
        let placed = match house {
            Some(h) => p.try_building_spaced(House { foot, door, ..h }, 0),
            None => p.try_forge(foot, door),
        };
        if !placed {
            continue;
        }
        let used = p.pieces[before..].iter().map(|piece| piece.rect()).reduce(|a, b| a.union(&b)).unwrap_or(foot);
        claim(&mut m, &used, &foot);
        occupied.insert(front);
        if special.is_some() {
            specials.remove(0);
        } else {
            houses += 1;
        }
    }
    // Arrière-cours : de petites maisons derrière les premières, la porte vers la rue la plus proche
    let mut spots: Vec<(i32, i32)> = Vec::new();
    for cz in (z - ext..=z + ext).step_by(3) {
        for cx in (x - ext..=x + ext).step_by(3) {
            if m.get(cx, cz) == IN {
                spots.push((cx, cz));
            }
        }
    }
    shuffle(dice, &mut spots);
    for (cx, cz) in spots {
        if m.get(cx, cz) != IN {
            continue;
        }
        // La rue la plus proche dans l'une des quatre directions
        let Some((dist, (dx, dz))) = dirs
            .iter()
            .filter_map(|&(dx, dz)| (1..16).find(|&k| matches!(m.get(cx + dx * k, cz + dz * k), ROAD | SQUARE)).map(|k| (k, (dx, dz))))
            .min_by_key(|e| e.0)
        else {
            continue;
        };
        let _ = dist;
        let door = match (dx, dz) {
            (1, _) => Side::East,
            (-1, _) => Side::West,
            (_, 1) => Side::South,
            _ => Side::North,
        };
        let (w, d) = (dice.range(5, 8), dice.range(5, 8));
        let foot = Rect::sized(cx - w / 2, cz - d / 2, w, d);
        let fits = (foot.z0..foot.z1).all(|cz| (foot.x0..foot.x1).all(|cx| m.get(cx, cz) == IN && !occupied.contains(&(cx, cz))));
        let clear = (foot.z0 - 1..foot.z1 + 1).all(|cz| (foot.x0 - 1..foot.x1 + 1).all(|cx| matches!(m.get(cx, cz), IN | ROAD | SQUARE | BUILT | EAVES)));
        if !fits || !clear {
            continue;
        }
        let style = Style::town(culture, biome, dice);
        let storeys = if dice.chance(habits.upstairs.1) { 2 } else { 1 };
        let mut house = random_home(dice, storeys, style, 0.0, habits.hip);
        house.foot = foot;
        house.door = door;
        let front = {
            let (dx, dz) = house.door_column();
            let (ox, oz) = door.delta();
            (dx + ox, dz + oz)
        };
        if occupied.contains(&front) || !matches!(m.get(front.0, front.1), IN | ROAD | SQUARE | EAVES) {
            continue;
        }
        let before = p.pieces.len();
        if p.try_building_spaced(house, 0) {
            let used = p.pieces[before].rect();
            claim(&mut m, &used, &foot);
            occupied.insert(front);
            if m.get(front.0, front.1) == IN {
                m.set(front.0, front.1, EAVES);
            }
            houses += 1;
        }
    }
    if houses < 12 {
        return None;
    }
    // Lampadaires au bord des rues, de loin en loin
    let seed = dice.next();
    for cz in z - ext..=z + ext {
        for cx in x - ext..=x + ext {
            if m.get(cx, cz) == ROAD && !occupied.contains(&(cx, cz)) && hash2(seed, cx, cz).is_multiple_of(40) && dirs.iter().any(|&(dx, dz)| matches!(m.get(cx + dx, cz + dz), IN | BUILT)) {
                p.pieces.push(Piece::Lamp { x: cx, z: cz, masonry });
            }
        }
    }
    let _ = plazas;
    let mut town = p.finish(SettlementKind::Town, center, biome)?;
    town.size = Some(size);
    Some(town)
}

/// Demi-côté de l'enceinte de la citadelle royale
const CITADEL: i32 = 34;

/// La citadelle royale au cœur d'une capitale : douves, haute enceinte flanquée de tours à
/// flèches, châtelet d'entrée au sud, cour d'honneur, chapelle, caserne et donjon royal à trois
/// étages couronné d'or
fn plan_citadel(p: &mut Planner, center: (i32, i32), masonry: Masonry, street: BlockId, culture: Culture, biome: BiomeType, dice: &mut Dice) -> Option<()> {
    const C: i32 = CITADEL;
    let (x, z) = center;
    let spire = Some(masonry.spire.unwrap_or(FLINT_BRICK));
    let royal = Masonry { height: masonry.height + 4, tower_height: masonry.tower_height + 8, spire, ..masonry };
    let turret = Masonry { tower_height: royal.tower_height + 12, ..royal };
    let gatehouse = Masonry { tower_height: royal.tower_height + 4, ..royal };
    // Donjon et ses quatre tourelles
    let keep = Rect::around(x, z - 7, 10);
    let floor = floor_for(p.generator, p.face, &keep, 10)?;
    for (tx, tz) in [(keep.x0, keep.z0), (keep.x1 - 1, keep.z0), (keep.x0, keep.z1 - 1), (keep.x1 - 1, keep.z1 - 1)] {
        p.take(Piece::Tower { rect: Rect::around(tx, tz, 2), floor, masonry: turret }, 0);
    }
    p.take(Piece::Keep { rect: keep, floor, masonry: royal }, 1);
    // Tours de l'enceinte : angles, milieux des côtés, châtelet d'entrée
    let mut towers: Vec<((i32, i32), Masonry)> = vec![
        ((x - C, z - C), royal),
        ((x + C, z - C), royal),
        ((x - C, z + C), royal),
        ((x + C, z + C), royal),
        ((x - C, z), royal),
        ((x + C, z), royal),
        ((x, z - C), royal),
    ];
    towers.push(((x - 6, z + C), gatehouse));
    towers.push(((x + 6, z + C), gatehouse));
    for ((tx, tz), m) in towers {
        let rect = Rect::around(tx, tz, 3);
        let floor = floor_for(p.generator, p.face, &rect, 8)?;
        p.take(Piece::Tower { rect, floor, masonry: m }, 0);
    }
    // Porte et pont-levis au sud
    p.take(Piece::Gate { rect: Rect::new(x - 2, z + C, x + 3, z + C + 1), masonry: royal, walk: false }, 0);
    p.take(Piece::Gate { rect: Rect::new(x - 2, z + C - 1, x + 3, z + C), masonry: royal, walk: true }, 0);
    p.pieces.push(Piece::Paving { rect: Rect::new(x - 2, z + C + 1, x + 3, z + C + 7), block: PLANK });
    for (lx, lz) in [(x - 3, z + C + 7), (x + 3, z + C + 7), (x - 3, z + C + 1), (x + 3, z + C + 1)] {
        p.pieces.push(Piece::Lamp { x: lx, z: lz, masonry });
    }
    // Enceinte : rangée crénelée et chemin de ronde
    let walls = Rect::new(x - C, z - C, x + C + 1, z + C + 1);
    for (rect, walk) in [
        (Rect::new(walls.x0, walls.z0, walls.x1, walls.z0 + 1), false),
        (Rect::new(walls.x0, walls.z1 - 1, walls.x1, walls.z1), false),
        (Rect::new(walls.x0, walls.z0, walls.x0 + 1, walls.z1), false),
        (Rect::new(walls.x1 - 1, walls.z0, walls.x1, walls.z1), false),
        (Rect::new(walls.x0 + 1, walls.z0 + 1, walls.x1 - 1, walls.z0 + 2), true),
        (Rect::new(walls.x0 + 1, walls.z1 - 2, walls.x1 - 1, walls.z1 - 1), true),
        (Rect::new(walls.x0 + 1, walls.z0 + 1, walls.x0 + 2, walls.z1 - 1), true),
        (Rect::new(walls.x1 - 2, walls.z0 + 1, walls.x1 - 1, walls.z1 - 1), true),
    ] {
        p.pieces.push(Piece::Rampart { rect, masonry: royal, walk });
    }
    // Douves bordées de pierre taillée
    let (o, i) = (Rect::around(x, z, C + 5), Rect::around(x, z, C + 1));
    let ring = |outer: Rect, inner: Rect| {
        [
            Rect::new(outer.x0, outer.z0, outer.x1, inner.z0),
            Rect::new(outer.x0, inner.z1, outer.x1, outer.z1),
            Rect::new(outer.x0, inner.z0, inner.x0, inner.z1),
            Rect::new(inner.x1, inner.z0, outer.x1, inner.z1),
        ]
    };
    for rect in ring(o, i) {
        p.pieces.push(Piece::Moat { rect });
    }
    for rect in ring(Rect::around(x, z, C + 1), Rect::around(x, z, C)).into_iter().chain(ring(Rect::around(x, z, C + 6), o)) {
        p.pieces.push(Piece::Paving { rect, block: CUT_STONE });
    }
    // Cour d'honneur : deux fontaines, l'allée bordée de lanternes, la chapelle et la caserne
    for fx in [x - 13, x + 13] {
        let rect = Rect::around(fx, z + 19, 2);
        if let Some(floor) = floor_for(p.generator, p.face, &rect, 3) {
            p.take(Piece::Fountain { rect, floor }, 0);
        }
    }
    for k in 0..5 {
        for lx in [x - 3, x + 3] {
            p.pieces.push(Piece::Lamp { x: lx, z: z + 7 + 5 * k, masonry });
        }
    }
    let style = Style::town(culture, biome, dice);
    if let Some(chapel) = Special::Church.house(dice, style) {
        p.try_building(House { foot: Rect::sized(x + 19, z - 10, 9, 7), door: Side::West, ..chapel });
    }
    if let Some(hall) = Special::Tavern.house(dice, style) {
        p.try_building(House { foot: Rect::sized(x - 28, z - 10, 9, 8), door: Side::East, porch: false, ..hall });
    }
    p.pieces.push(Piece::Paving { rect: Rect::around(x, z, C - 2), block: street });
    p.taken.push(Rect::around(x, z, C + 6));
    Some(())
}

/// Dessine, pour un aperçu, une ville de taille `size` centrée en `center`, sans tenir compte des
/// cases ni des autres établissements (le jeu ne pose encore que des bourgs)
pub fn preview_town(generator: &PlanetWorldGenerator, face: PlanetFace, center: (i32, i32), size: TownSize, culture: Culture, seed: u64) -> Option<Settlement> {
    let (_, biome) = ground_at(generator, face, center.0, center.1);
    let area = Rect::around(center.0, center.1, size.half() + 8);
    let planner = Planner { generator, face, area, pieces: Vec::new(), taken: Vec::new(), exits: Vec::new(), culture: Some(culture) };
    let mut dice = Dice(mix(seed ^ 0x7031_5A1E));
    plan_town_sized(planner, center, biome, &mut dice, size)
}

fn plan_ruins(mut p: Planner, center: (i32, i32), biome: BiomeType, dice: &mut Dice) -> Option<Settlement> {
    let (x, z) = center;
    let seed = dice.next();
    match dice.range(0, 3) {
        0 => {
            // Temple rond de 12 ou 13 blocs de rayon, son arène et son sanctuaire
            let radius = dice.range(12, 14);
            // L'entrée regarde vers l'intérieur de la case, pour que l'escalier et le parvis y tiennent
            let pick = dice.range(0, 4) as usize;
            let (ax, az) = p.area.center();
            let toward = |side: &Side| {
                let (dx, dz) = side.delta();
                dx * (ax - x) + dz * (az - z)
            };
            let best = Side::ALL.iter().map(toward).max().unwrap_or(0);
            let doors: Vec<Side> = Side::ALL.into_iter().filter(|s| toward(s) == best).collect();
            let door = doors[pick % doors.len()];
            let temple = Temple { center, radius, door, floor: 0, forecourt: 0, seed };
            let floor = floor_for(p.generator, p.face, &temple.enclosure(), 8)? + 1 + Temple::PODIUM;
            // Le sol naturel au milieu du parvis le plus long fixe le nombre de marches
            let (px, pz) = temple.world(0, -radius - Temple::STEPS_MAX - Temple::PARVIS_DEPTH / 2);
            let forecourt = ground_at(p.generator, p.face, px, pz).0 + 1;
            p.take(Piece::Temple(Temple { floor, forecourt, ..temple }), 0);
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
            if p.free(&foot)
                && let Some(floor) = floor_for(p.generator, p.face, &foot, 6) {
                p.take(Piece::RuinHouse(RuinHouse { foot, floor, height: dice.range(2, 5), seed: seed ^ 1 }), 1);
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

    /// Piller une ville ne doit rapporter ni minerai ni métal : aucun bloc posé par les structures
    /// n'est un minerai ou un bloc de métal, ni ne se casse en l'un d'eux, et aucune recette ne
    /// prend un ornement pour ingrédient
    #[test]
    fn test_structures_yield_no_ore_nor_metal() {
        let registry = herocraft_common::blocks();
        let precious = ["crystal", "goldBar", "ironPlate", "bronzePlate", "mithrilPlate", "copperPlate", "steelPlate", "diamondBar", "ironBar", "copperBar", "obsidian"];
        let valuable = |id: &str| precious.contains(&id) || id.ends_with("Ore") || id.contains("Ornate") || id.contains("Plate") || id.ends_with("Bar");
        for (block, id) in block::ALL {
            assert!(!valuable(id), "{id} est un minerai ou un métal");
            if let Some(drop) = registry.drop_of(block) {
                let dropped = registry.get(drop).unwrap();
                assert!(!valuable(&dropped.id), "{id} se casse en {}", dropped.id);
            }
        }
        // Les ornements ne figurent dans aucune fiche d'objet, de bloc ou d'établi, hors la leur
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let ornaments = ["ornementBronze", "ornementMithril", "ornementOr", "ornementFer", "cristalTaille"];
        let mut stack = vec![root];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "json") {
                    let text = std::fs::read_to_string(&path).unwrap();
                    let own = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                    for o in ornaments.iter().filter(|&&o| o != own) {
                        assert!(!text.contains(&format!("\"{o}\"")), "{} cite {o}", path.display());
                    }
                }
            }
        }
    }

    #[test]
    fn test_houses_and_lamps_are_lit() {
        let generator = generator(2026);
        let settlements = all_settlements(&generator);
        let (mut houses, mut lamps) = (0, 0);
        for s in settlements.iter().filter(|s| s.culture.is_some()) {
            for piece in &s.pieces {
                match piece {
                    Piece::House(h) if !matches!(h.usage, Use::Barn) => {
                        // Chaque étage a sa torche, posée sur le plancher, à l'intérieur des murs
                        let torches: Vec<_> = h.torches().collect();
                        assert!(torches.len() >= h.storeys as usize, "{h:?}");
                        for (x, level, z) in torches {
                            assert!(h.foot.contains(x, z) && !h.foot.on_edge(x, z));
                            assert_eq!(h.block(x, h.floor + level, z, h.floor), Some(TORCH), "{h:?}");
                            let below = h.block(x, h.floor + level - 1, z, h.floor).unwrap();
                            assert!(below.is_solid(), "torche posée sur {below:?}");
                        }
                        houses += 1;
                    }
                    Piece::Lamp { x, z, masonry } => {
                        // Le poteau est une barrière, la lumière au-dessus
                        let ground = 100;
                        assert!(piece.block(*x, ground + 1, *z, ground).unwrap().is_fence());
                        assert!(piece.block(*x, ground + LAMP_LIGHT, *z, ground).unwrap().light() > 0, "{masonry:?}");
                        lamps += 1;
                    }
                    _ => {}
                }
            }
            // Les lumières que le plan annonce sont bien celles qu'il pose
            let lights = s.lights(s.bounds, |_, _| 100);
            assert!(!lights.is_empty());
            for (x, y, z, block) in lights {
                assert_eq!(s.block(x, y, z, 100), Some(block));
            }
        }
        assert!(houses > 50 && lamps > 20, "{houses} maisons, {lamps} lampadaires");
    }

    #[test]
    fn test_planet_has_towns_villages_and_ruins() {
        let mut grandes = 0;
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
                // Le plan reste dans sa case (une ville, dans sa région), donc sur sa face
                let (cx, cz) = (s.center.0.div_euclid(CELL), s.center.1.div_euclid(CELL));
                let home = if s.kind == SettlementKind::Town {
                    let (rx, rz) = (cx.div_euclid(REGION_CELLS), cz.div_euclid(REGION_CELLS));
                    Rect::sized(rx * REGION_BLOCKS, rz * REGION_BLOCKS, REGION_BLOCKS, REGION_BLOCKS)
                } else {
                    Rect::sized(cx * CELL, cz * CELL, CELL, CELL)
                };
                assert!(home.contains(s.bounds.x0, s.bounds.z0) && home.contains(s.bounds.x1 - 1, s.bounds.z1 - 1), "{:?} sort de sa case", s.kind);
                assert!(s.top <= crate::edits::WORLD_HEIGHT);
            }
            // Une capitale, une seule, et des villes des deux autres tailles
            let sized = |z| settlements.iter().filter(|s| s.size == Some(z)).count();
            println!("  {} petites villes, {} grandes villes", sized(TownSize::PetiteVille), sized(TownSize::GrandeVille));
            assert_eq!(sized(TownSize::Capitale), 1, "une capitale par planète");
            assert!(sized(TownSize::PetiteVille) >= 1);
            grandes += sized(TownSize::GrandeVille);
        }
        assert!(grandes >= 1, "aucune grande ville");
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
    fn test_no_vegetation_on_buildings_streets_and_fields() {
        let generator = generator(2026);
        let all: Vec<_> = all_settlements(&generator).into_iter().filter(|s| s.kind != SettlementKind::Ruins).collect();
        // La première ville et les cinq premiers villages
        let town = all.iter().copied().find(|s| s.kind == SettlementKind::Town);
        let settlements: Vec<_> = town.into_iter().chain(all.iter().copied().filter(|s| s.kind == SettlementKind::Village).take(5)).collect();
        assert!(settlements.iter().any(|s| s.kind == SettlementKind::Town));
        let mut checked = 0;
        for s in settlements {
            for piece in &s.pieces {
                let rect = piece.rect();
                for z in rect.z0..rect.z1 {
                    for x in rect.x0..rect.x1 {
                        let column = generator.sample_block_column(s.face, x, z);
                        let ground = column.elevation.floor() as i32;
                        for y in ground - 1..ground + crate::vegetation::VEGETATION_SPAN {
                            let with = generator.column_block(&column, s.face, x, y, z);
                            let without = generator.column_block_without_vegetation(&column, s.face, x, y, z);
                            assert_eq!(with, without, "{} : végétation en ({x}, {y}, {z})", s.kind.name());
                        }
                        checked += 1;
                    }
                }
            }
        }
        assert!(checked > 10_000);
    }

    #[test]
    fn test_every_town_and_village_has_a_culture_that_shapes_it() {
        let generator = generator(2026);
        let all = all_settlements(&generator);
        let mut seen = std::collections::HashMap::new();
        for s in &all {
            if s.kind == SettlementKind::Ruins {
                assert_eq!(s.culture, None, "des ruines sans peuple");
                continue;
            }
            let culture = s.culture.expect("une culture par ville et village");
            *seen.entry(culture).or_insert(0) += 1;
            let masonry = Masonry::of(culture);
            for piece in &s.pieces {
                match piece {
                    // La citadelle d'une capitale a des murs plus hauts, de la même pierre
                    Piece::Rampart { masonry: m, .. } | Piece::Gate { masonry: m, .. } | Piece::Tower { masonry: m, .. } | Piece::Lamp { masonry: m, .. } => {
                        assert_eq!((m.wall, m.accent, m.cap, m.palisade), (masonry.wall, masonry.accent, masonry.cap, masonry.palisade), "{} {culture:?} : remparts d'une autre culture", s.kind.name())
                    }
                    Piece::House(h) if h.usage == Use::Home && culture == Culture::ElfesDesBois => {
                        assert!(matches!(h.style.roof, Some(GREEN_LEAF | DARK_LEAF | LIGHT_LEAF)), "toit de feuillage elfe")
                    }
                    Piece::House(h) if h.usage == Use::Home && culture == Culture::HautsElfes => assert_eq!(h.style.wall, WHITE_BRICK),
                    Piece::House(h) if h.usage == Use::Home && culture == Culture::Nains => {
                        assert!([CUT_STONE, FLINT_BRICK, HARD_STONE].contains(&h.style.wall), "maison naine en {:?}", h.style.wall)
                    }
                    _ => {}
                }
            }
        }
        // Les dix peuples se partagent la planète
        assert_eq!(seen.len(), Culture::ALL.len(), "{seen:?}");
        // Le tirage est stable : la même graine redonne les mêmes cultures
        let again = generator2026_cultures();
        assert_eq!(again, all.iter().map(|s| s.culture).collect::<Vec<_>>());
    }

    #[test]
    fn test_every_race_has_a_village_and_only_town_builders_have_towns() {
        for seed in [2026, 42, 7] {
            let generator = generator(seed);
            let all = all_settlements(&generator);
            for culture in Culture::ALL {
                assert!(
                    all.iter().any(|s| s.kind == SettlementKind::Village && s.culture == Some(culture)),
                    "graine {seed} : aucun village {culture:?}"
                );
            }
            for s in all.iter().filter(|s| s.kind == SettlementKind::Town) {
                assert!(s.culture.unwrap().builds_towns(), "graine {seed} : ville {:?}", s.culture);
            }
            // Chaque région réservée a bien son village, de sa race, dans la case choisie
            let pins = generator.structures.pins(&generator);
            assert_eq!(pins.len(), Culture::ALL.len());
            for pin in pins {
                let s = generator.structures.cell(&generator, pin.face, pin.cell.0, pin.cell.1).expect("le village réservé");
                assert_eq!((s.kind, s.culture), (SettlementKind::Village, Some(pin.culture)));
            }
        }
    }

    #[test]
    fn test_race_villages_have_their_own_shape() {
        let generator = generator(2026);
        let all = all_settlements(&generator);
        let village = |c: Culture| all.iter().find(|s| s.kind == SettlementKind::Village && s.culture == Some(c)).unwrap();
        // Camp demi-orque : une arène et une palissade qu'aucun bâtiment ne touche
        let camp = village(Culture::DemiOrques);
        assert!(camp.pieces.iter().any(|p| matches!(p, Piece::Arena { .. })));
        let walls: Vec<Rect> = camp.pieces.iter().filter(|&p| matches!(p, Piece::Rampart { .. } | Piece::Gate { .. })).map(|p| p.rect()).collect();
        assert!(!walls.is_empty(), "une palissade");
        for piece in &camp.pieces {
            if matches!(piece, Piece::House(_) | Piece::Forge(_) | Piece::Arena { .. }) {
                assert!(walls.iter().all(|w| !w.intersects(&piece.rect())), "un bâtiment dans la palissade : {piece:?}");
            }
        }
        // Revenants et Hommes-bêtes sur pilotis ; Gnomes dans des buttes à porte basse
        for (culture, stilts) in [(Culture::Revenants, 2), (Culture::HommesBetes, 2)] {
            let homes: Vec<&House> = village(culture).pieces.iter().filter_map(|p| if let Piece::House(h) = p { (h.usage == Use::Home).then_some(h) } else { None }).collect();
            assert!(!homes.is_empty());
            assert!(homes.iter().all(|h| h.stilts == stilts), "{culture:?}");
        }
        let burrows = village(Culture::Gnomes).pieces.iter().filter(|p| matches!(p, Piece::House(h) if h.form == Form::Burrow && h.usage == Use::Home)).count();
        assert!(burrows >= 2);
        assert!(village(Culture::Revenants).pieces.iter().any(|p| matches!(p, Piece::Ossuary { .. })));
        assert!(village(Culture::Sylphes).pieces.iter().any(|p| matches!(p, Piece::Obelisk { .. })));
        assert!(village(Culture::Aasimars).pieces.iter().any(|p| matches!(p, Piece::Sanctuary { .. })));
        assert!(village(Culture::HommesBetes).pieces.iter().filter(|p| matches!(p, Piece::Totem { .. })).count() >= 4);
    }

    #[test]
    fn test_burrow_has_a_room_a_low_door_and_grass_on_top() {
        let style = Style::village(Culture::Gnomes, BiomeType::Plains, &mut Dice(1));
        let mut house = House::home(Rect::sized(0, 0, 7, 6), 10, 1, Side::North, style);
        house.form = Form::Burrow;
        let (dx, dz) = house.door_column();
        let at = |x, y, z| house.block(x, y, z, 10).unwrap_or(BlockId::AIR);
        assert_eq!(at(dx, 10, dz), BlockId::AIR, "porte ouverte");
        assert_ne!(at(dx, 11, dz), BlockId::AIR, "porte d'un seul bloc");
        assert_eq!(at(dx, 10, dz - 1), BlockId::AIR, "tunnel dans le talus");
        let (cx, cz) = house.foot.center();
        for y in 10..13 {
            assert!(!at(cx, y, cz).is_solid() || at(cx, y, cz) == WORKBENCH || at(cx, y, cz) == FURNACE, "salle libre à {y}");
        }
        let top = (10..30).rev().find(|&y| at(cx, y, cz) != BlockId::AIR).unwrap();
        assert_eq!(at(cx, top, cz), GRASS);
        assert!(top >= 13);
    }

    fn generator2026_cultures() -> Vec<Option<Culture>> {
        let generator = generator(2026);
        all_settlements(&generator).into_iter().map(|s| s.culture).collect()
    }

    #[test]
    fn test_most_residents_share_the_culture_of_their_town() {
        let generator = generator(2026);
        let all = all_settlements(&generator);
        let town = all.iter().find(|s| s.kind != SettlementKind::Ruins).unwrap();
        let home = town.culture.unwrap();
        let residents: Vec<Culture> = (0..400).map(|n| town.resident_culture(n)).collect();
        let natives = residents.iter().filter(|&&c| c == home).count();
        assert!((300..390).contains(&natives), "{natives} natifs sur 400");
        assert!(residents.iter().any(|&c| c != home), "quelques étrangers");
        assert_eq!(residents[17], town.resident_culture(17));
    }

    #[test]
    fn test_spires_and_palisades_stand_on_towers() {
        for culture in Culture::ALL {
            let masonry = Masonry::of(culture);
            let rect = Rect::around(0, 0, 2);
            let tower = Piece::Tower { rect, floor: 50, masonry };
            let span = tower.span(50);
            assert!(span.end >= 50 + masonry.tower_top());
            let block = |x, y, z| tower.block(x, y, z, 50).unwrap_or(BlockId::AIR);
            assert_eq!(block(2, 50, 0), masonry.wall, "{culture:?}");
            assert_eq!(block(0, 50 + masonry.tower_height - 1, 0), masonry.deck);
            match masonry.spire {
                Some(spire) => assert_eq!(block(0, 50 + masonry.tower_height + 3, 0), spire, "{culture:?} : pointe"),
                None => assert!(block(0, 50 + masonry.tower_height + 1, 0).is_air()),
            }
            // Créneaux ou pieux, un sur deux
            let wall = Piece::Rampart { rect: Rect::sized(0, 0, 4, 1), masonry, walk: false };
            let top: Vec<bool> = (0..4).map(|x| wall.block(x, 50 + masonry.height, 0, 50).is_some()).collect();
            assert_eq!(top, [true, false, true, false]);
        }
    }

    #[test]
    fn test_houses_have_walls_roof_and_open_door() {
        let generator = generator(2026);
        let village = all_settlements(&generator).into_iter().find(|s| s.kind == SettlementKind::Village).expect("un village");
        let house = village.pieces.iter().find_map(|p| if let Piece::House(h) = p { (h.usage == Use::Home).then_some(*h) } else { None }).unwrap();
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

    #[test]
    fn test_towns_and_villages_have_varied_buildings() {
        let generator = generator(2026);
        let all = all_settlements(&generator);
        let usage = |s: &Settlement, u: Use| s.pieces.iter().filter(|p| matches!(p, Piece::House(h) if h.usage == u)).count();
        let towns: Vec<_> = all.iter().filter(|s| s.kind == SettlementKind::Town).collect();
        assert!(!towns.is_empty());
        for town in &towns {
            assert!(usage(town, Use::Church) >= 1, "une église par ville");
            assert!(town.pieces.iter().any(|p| matches!(p, Piece::TownHall { .. } | Piece::Keep { .. })), "ni hôtel de ville ni donjon");
            assert!(usage(town, Use::Tavern) >= 1, "pas de taverne");
            let stalls = town.pieces.iter().filter(|p| matches!(p, Piece::Stall(_))).count();
            assert!(stalls >= 4, "pas de marché : {stalls} étals dans {:?} {:?}", town.size, town.center);
        }
        let villages: Vec<_> = all.iter().filter(|s| s.kind == SettlementKind::Village).collect();
        for u in [Use::Church, Use::Tavern, Use::Barn] {
            assert!(villages.iter().any(|v| usage(v, u) > 0), "aucun village n'a de {u:?}");
        }
        assert!(villages.iter().any(|v| v.pieces.iter().any(|p| matches!(p, Piece::Forge(_)))), "aucune forge de village");
        let homes = || all.iter().flat_map(|s| &s.pieces).filter_map(|p| if let Piece::House(h) = p { Some(h) } else { None });
        assert!(homes().any(|h| h.roof == Roof::Hip), "pas de toit à quatre pans");
        assert!(homes().any(|h| h.chimney), "pas de cheminée");
        assert!(homes().any(|h| h.porch), "pas de porche");
    }

    #[test]
    fn test_every_building_has_an_open_door_and_a_roof() {
        let generator = generator(2026);
        let all = all_settlements(&generator);
        let mut checked = 0;
        for s in all.iter().filter(|s| s.kind != SettlementKind::Ruins).take(8) {
            let block = |x: i32, y: i32, z: i32| generator.block_at(s.face, x, y, z);
            for piece in &s.pieces {
                let Piece::House(house) = piece else { continue };
                let (dx, dz) = house.door_column();
                let (ox, oz) = house.door.delta();
                for y in house.floor..house.floor + house.door_height() {
                    assert!(block(dx, y, dz).is_air(), "{:?} : porte bouchée en ({dx}, {y}, {dz})", house.usage);
                }
                assert!(!block(dx, house.floor - 1, dz).is_air(), "{:?} : seuil manquant", house.usage);
                // Devant la porte, le passage est libre (un porche n'a de poteaux qu'aux angles)
                assert!(block(dx + ox, house.floor, dz + oz).is_air(), "{:?} : sortie bouchée", house.usage);
                // Une butte n'a ni angles ni murs droits : sa salle a trois blocs de haut
                let roof_from = match house.form {
                    Form::Box => {
                        assert_eq!(block(house.foot.x0, house.floor, house.foot.z0), house.style.corner);
                        house.floor + house.wall_height()
                    }
                    Form::Burrow => house.floor + 3,
                };
                // Un toit au-dessus d'un coin intérieur
                let (cx, cz) = (house.foot.x0 + 1, house.foot.z0 + 1);
                assert!((roof_from..house.top()).any(|y| !block(cx, y, cz).is_air()), "{:?} : pas de toit", house.usage);
                if house.chimney {
                    let (kx, kz) = house.chimney_column();
                    assert!(!block(kx, house.top() - 1, kz).is_air(), "cheminée tronquée");
                }
                checked += 1;
            }
        }
        assert!(checked > 20, "{checked} bâtiments vérifiés");
    }

    /// Des échelles mènent au chemin de ronde : plat, libre au-dessus, ouvert sous les portes
    #[test]
    fn test_town_ramparts_have_ladders_up_to_a_wall_walk() {
        let generator = generator(2026);
        let towns: Vec<_> = all_settlements(&generator).into_iter().filter(|s| s.kind == SettlementKind::Town).collect();
        assert!(!towns.is_empty());
        let ladder = herocraft_common::blocks().find("ladder").unwrap();
        for town in towns {
            let block = |x: i32, y: i32, z: i32| generator.block_at(town.face, x, y, z);
            let ground = |x: i32, z: i32| generator.sample_block_column(town.face, x, z).elevation.floor() as i32;
            let mut ladders = 0;
            for piece in &town.pieces {
                match *piece {
                    Piece::Ladder { x, z, back, top } => {
                        for y in ground(x, z)..top {
                            assert_eq!(block(x, y, z).kind(), ladder, "échelle interrompue en ({x}, {y}, {z})");
                        }
                        // Le mur derrière elle porte le chemin de ronde, libre jusqu'au ciel
                        let (dx, dz) = back.delta();
                        let (wx, wz) = (x + dx, z + dz);
                        assert!(block(wx, top - 1, wz).is_solid(), "pas de chemin de ronde derrière l'échelle ({x}, {z})");
                        for y in top..top + 2 {
                            assert!(block(wx, y, wz).is_air(), "chemin de ronde encombré en ({wx}, {y}, {wz})");
                        }
                        ladders += 1;
                    }
                    Piece::Gate { rect, masonry, walk: true } => {
                        for (x, z) in [(rect.x0, rect.z0), (rect.x1 - 1, rect.z1 - 1)] {
                            let g = ground(x, z);
                            assert!((g..g + 3).all(|y| block(x, y, z).is_air()), "porte bouchée sous le chemin de ronde");
                            assert!(block(x, g + masonry.height - 1, z).is_solid() && block(x, g + masonry.height, z).is_air());
                        }
                    }
                    _ => {}
                }
            }
            assert!(ladders >= 8, "seulement {ladders} échelles");
        }
    }

    #[test]
    fn test_settlements_keep_their_distance() {
        for seed in [42, 2026] {
            let generator = generator(seed);
            let all = all_settlements(&generator);
            for (i, a) in all.iter().enumerate() {
                for b in &all[i + 1..] {
                    if a.face == b.face {
                        assert!(!a.bounds.expand(20).intersects(&b.bounds.expand(20)), "graine {seed} : {:?} et {:?} trop proches", a.center, b.center);
                    }
                }
            }
        }
    }

    #[test]
    fn test_towns_and_villages_are_minutes_apart() {
        let generator = generator(2026);
        let places: Vec<&Settlement> = all_settlements(&generator).into_iter().filter(|s| s.kind != SettlementKind::Ruins).collect();
        let mut nearest_gaps = Vec::new();
        for a in &places {
            let nearest = places
                .iter()
                .filter(|b| b.face == a.face && b.center != a.center)
                .map(|b| (((a.center.0 - b.center.0) as f64).powi(2) + ((a.center.1 - b.center.1) as f64).powi(2)).sqrt())
                .fold(f64::INFINITY, f64::min);
            if nearest.is_finite() {
                // Deux régions voisines : au moins 2 × REGION_MARGIN cases, moins les bords des plans
                assert!(nearest >= (2 * REGION_MARGIN * CELL - CELL) as f64, "{:?} à {nearest:.0} blocs de son voisin", a.center);
                nearest_gaps.push(nearest);
            }
        }
        nearest_gaps.sort_by(f64::total_cmp);
        let median = nearest_gaps[nearest_gaps.len() / 2];
        println!("{} villes et villages, écart médian au plus proche : {median:.0} blocs", places.len());
        assert!(places.len() >= 20, "seulement {} villes et villages", places.len());
        assert!((1300.0..3000.0).contains(&median), "écart médian {median:.0} blocs");
    }
}
