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

/// Côté d'une case de structures, en blocs : le plus grand plan (une ville) y tient
pub const CELL: i32 = 128;
/// Cases par arête de face
pub const CELLS_PER_FACE: i32 = crate::edits::FACE_BLOCKS / CELL;
/// Cases par côté d'une région, qui accueille au plus une ville ou un village (2 048 blocs : à
/// 5,2 blocs par seconde, de 4 à 9 minutes de marche d'un établissement au suivant)
pub const REGION_CELLS: i32 = 16;
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
    pub const OAK_LOG: BlockId = BlockId::WOOD;
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
    pub const GREEN_LEAF: BlockId = BlockId(9);
    pub const DARK_LEAF: BlockId = BlockId(15);
    pub const LIGHT_LEAF: BlockId = BlockId(16);
    pub const FLINT_BRICK: BlockId = BlockId(40);
    pub const GLOWBELL_BLOOM: BlockId = BlockId(105);
    pub const BASALT: BlockId = BlockId(118);
    // Barrières, dont les poteaux des lampadaires
    pub const OAK_FENCE: BlockId = BlockId(148);
    pub const BIRCH_FENCE: BlockId = BlockId(149);
    pub const CUT_STONE_FENCE: BlockId = BlockId(154);
    pub const SILVER_FENCE: BlockId = BlockId(161);
    // Ornements : l'aspect des métaux et du cristal, sans leur valeur. Cassés, ils ne donnent
    // qu'eux-mêmes et aucune recette ne les refond en lingots ni en minerai : piller une ville ne
    // rapporte ni or, ni mithril, ni cristal.
    pub const BRONZE_TRIM: BlockId = BlockId(142);
    pub const SILVER_TRIM: BlockId = BlockId(143);
    pub const GILDING: BlockId = BlockId(144);
    pub const IRON_TRIM: BlockId = BlockId(145);
    pub const CUT_CRYSTAL: BlockId = BlockId(146);

    /// Tous les blocs que posent les structures (pour vérifier qu'ils ont leur fiche)
    pub const ALL: [(BlockId, &str); 44] = [
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
    /// Chance d'une forge au village
    forge: f64,
}

impl Habits {
    fn of(culture: Culture) -> Habits {
        match culture {
            Culture::Humains => Habits { upstairs: (0.15, 0.55), hip: 0.35, porch: 0.3, forge: 0.6 },
            // Maisons basses et trapues, une forge partout
            Culture::Nains => Habits { upstairs: (0.05, 0.25), hip: 0.8, porch: 0.1, forge: 1.0 },
            Culture::ElfesDesBois => Habits { upstairs: (0.25, 0.5), hip: 0.5, porch: 0.6, forge: 0.3 },
            // Demeures élancées, toits pointus
            Culture::HautsElfes => Habits { upstairs: (0.4, 0.85), hip: 0.75, porch: 0.35, forge: 0.4 },
        }
    }
}

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
    /// Poteau des lampadaires (une barrière)
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
                light: TORCH,
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
                light: TORCH,
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
                light: GLOWBELL_BLOOM,
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
                light: TORCH,
                palisade: false,
                spire: Some(CUT_CRYSTAL),
                height: 5,
                tower_height: 11,
            },
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
        }
    }

    fn wall_height(&self) -> i32 {
        self.height
    }

    fn door_height(&self) -> i32 {
        if matches!(self.usage, Use::Barn | Use::Church) { 3 } else { 2 }
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
    fn from_door(&self, x: i32, z: i32) -> Option<i32> {
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
            return Some(if y == self.top() - 1 { COBBLE_SLAB } else { COBBLE });
        }
        if let Some(tower) = self.tower_rect() {
            if tower.contains(x, z) && y >= f {
                return self.tower_block(&tower, x, y, z, ground);
            }
        }
        let h = self.wall_height();
        let foot = &self.foot;
        let pitched = self.style.roof.is_some();

        if !foot.contains(x, z) {
            if let Some(porch) = self.porch_rect() {
                if porch.contains(x, z) && y < f + 3 {
                    return self.porch_block(&porch, x, y, z, ground);
                }
            }
            // Devant la porte : un seuil, et de l'air sur la hauteur de la porte même si le terrain monte
            let (ox, oz) = self.door.delta();
            if self.from_door(x - ox, z - oz).is_some_and(|d| d <= self.door_width / 2) {
                if y == f - 1 {
                    return Some(self.style.foundation);
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
                let from_door = self.from_door(x, z);
                if from_door.is_some_and(|d| d <= self.door_width / 2) && level < self.door_height() {
                    return Some(BlockId::AIR);
                }
                let along = if z == foot.z0 || z == foot.z1 - 1 { x - foot.x0 } else { z - foot.z0 };
                let door_near = from_door.is_some_and(|d| d <= self.door_width / 2 + 1);
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
    pub fn torches(&self) -> impl Iterator<Item = (i32, i32, i32)> {
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
                    return if depth > 1 && h % 4 == 0 { WHEAT } else { BlockId::AIR };
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

    /// Porche : plancher, deux poteaux devant, un auvent de dalles
    fn porch_block(&self, porch: &Rect, x: i32, y: i32, z: i32, ground: i32) -> Option<BlockId> {
        let f = self.floor;
        if y < f - 1 {
            return (y >= ground).then_some(self.style.foundation);
        }
        if y == f - 1 {
            return Some(self.style.floor);
        }
        let outer = match self.door {
            Side::North => z == porch.z0,
            Side::South => z == porch.z1 - 1,
            Side::West => x == porch.x0,
            Side::East => x == porch.x1 - 1,
        };
        let level = y - f;
        if level < 2 {
            return Some(if outer && porch.on_corner(x, z) { OAK_LOG } else { BlockId::AIR });
        }
        Some(PLANK_SLAB)
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
            if level == 0 && foot.on_edge(x, z) && !on_back && hash2(0xF0, x, z) % 3 == 0 {
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
                1 => return Some(CUT_CRYSTAL),
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
            Piece::Ladder { top, .. } => ground..(*top).max(ground),
            Piece::Rampart { masonry, .. } | Piece::Gate { masonry, .. } => ground - 1..ground + masonry.height + 1,
            Piece::House(h) => span(h.floor, 1, h.top()),
            Piece::Forge(forge) => span(forge.floor, 1, forge.top()),
            Piece::Stall(_) => ground - 1..ground + 3,
            Piece::Well { floor, .. } => span(*floor, 5, floor + 4),
            Piece::Fountain { floor, .. } => span(*floor, 3, floor + 3),
            Piece::Tower { floor, masonry, .. } => span(*floor, 1, floor + masonry.tower_top()),
            Piece::RuinHouse(r) => span(r.floor, 1, r.top()),
            Piece::Temple(t) => span(t.floor, 3, t.top()),
            Piece::RuinTower(t) => span(t.floor, 1, t.top()),
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
                0 | 1 => Some(masonry.post),
                2 => Some(masonry.light),
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

    /// Blocs qui éclairent (torches, lampadaires, fontaines) posés par le plan dans les colonnes
    /// `area` : position et bloc. `ground(x, z)` donne l'altitude du premier bloc d'air au-dessus
    /// du relief naturel de la colonne (le pied d'un lampadaire). Seuls ceux que le plan pose
    /// vraiment à cet endroit sont gardés.
    pub fn lights(&self, area: Rect, ground: impl Fn(i32, i32) -> i32) -> Vec<(i32, i32, i32, BlockId)> {
        let mut found = Vec::new();
        if !self.bounds.intersects(&area) {
            return found;
        }
        for piece in self.pieces.iter().filter(|p| p.rect().intersects(&area)) {
            let mut spots: Vec<(i32, i32, i32)> = Vec::new();
            match piece {
                Piece::Lamp { x, z, .. } => spots.push((*x, ground(*x, *z) + 2, *z)),
                Piece::Fountain { rect, floor } => {
                    let (cx, cz) = rect.center();
                    spots.push((cx, floor + 2, cz));
                }
                Piece::House(house) => spots.extend(house.torches().map(|(x, level, z)| (x, house.floor + level, z))),
                _ => continue,
            }
            for (x, y, z) in spots {
                if !area.contains(x, z) {
                    continue;
                }
                if let Some(block) = self.block(x, y, z, ground(x, z)) {
                    if block.light() > 0 {
                        found.push((x, y, z, block));
                    }
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
        self.bounds.expand(margin).contains(x, z) && self.pieces.iter().any(|p| p.rect().expand(margin).contains(x, z))
    }

    /// Culture du `n`-ième habitant : celle de la ville le plus souvent (`culture::NATIVE_SHARE`),
    /// parfois une autre. Ne dépend que de l'établissement et de `n`. Des ruines n'ont pas
    /// d'habitants : on y prend des Humains.
    pub fn resident_culture(&self, n: u64) -> Culture {
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

impl StructureMap {
    /// La ville ou le village de la région (rx, rz) d'une face (régions de `REGION_CELLS` cases de
    /// côté), s'il y en a un
    pub fn region_settlement(&self, generator: &PlanetWorldGenerator, face: PlanetFace, rx: i32, rz: i32) -> Option<&Settlement> {
        if !(0..REGIONS_PER_FACE).contains(&rx) || !(0..REGIONS_PER_FACE).contains(&rz) {
            return None;
        }
        region_candidates(generator, face, rx, rz).into_iter().find_map(|(cx, cz)| self.cell(generator, face, cx, cz))
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
    fn try_building(&mut self, mut house: House) -> bool {
        // Les avant-toits peuvent toucher la rue, pas les murs
        let room = |h: &House| h.rect().union(&h.foot.expand(1));
        if house.porch && !self.free(&room(&house)) {
            house.porch = false;
        }
        if !self.free(&room(&house)) {
            return false;
        }
        let Some(floor) = floor_for(self.generator, self.face, &house.foot, 5) else {
            return false;
        };
        house.floor = floor;
        self.take(Piece::House(house), 1);
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
        Some(Settlement { kind, face: self.face, center, biome, culture: self.culture, bounds, top, pieces: self.pieces, exits: self.exits })
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

/// Ce que la graine destine à une case : le rang de ville ou de village de sa région qu'elle
/// essaie (`Some(rang)`), des ruines (`None`), ou rien (`Err`)
fn cell_role(generator: &PlanetWorldGenerator, face: PlanetFace, cx: i32, cz: i32) -> Result<Option<usize>, ()> {
    let candidates = region_candidates(generator, face, cx.div_euclid(REGION_CELLS), cz.div_euclid(REGION_CELLS));
    if let Some(rank) = candidates.iter().position(|&c| c == (cx, cz)) {
        return Ok(Some(rank));
    }
    // Rien autour des sites de villes et villages : ils ne dépendent ainsi d'aucune case voisine
    if candidates.iter().any(|c| (c.0 - cx).abs() <= 1 && (c.1 - cz).abs() <= 1) {
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
    let mut dice = Dice(cell_seed(generator, face, cx, cz));
    let wanted = match cell_role(generator, face, cx, cz) {
        Err(()) => return None,
        Ok(None) => SettlementKind::Ruins,
        Ok(Some(rank)) => {
            // La région a déjà sa ville ou son village dans une case candidate précédente
            let earlier = region_candidates(generator, face, cx.div_euclid(REGION_CELLS), cz.div_euclid(REGION_CELLS));
            if earlier[..rank].iter().any(|&(px, pz)| generator.structures.cell(generator, face, px, pz).is_some()) {
                return None;
            }
            if unit(dice.next()) < 0.46 {
                SettlementKind::Town
            } else {
                SettlementKind::Village
            }
        }
    };
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
    // Un site refusé à une ville peut accueillir un village, puis des ruines
    let kinds: &[SettlementKind] = match wanted {
        SettlementKind::Town => &[SettlementKind::Town, SettlementKind::Village],
        SettlementKind::Village => &[SettlementKind::Village],
        SettlementKind::Ruins => &[SettlementKind::Ruins],
    };
    for &kind in kinds {
        let margin = kind.radius() + 4;
        for _ in 0..10 {
            let x = dice.range(cell.x0 + margin, cell.x1 - margin);
            let z = dice.range(cell.z0 + margin, cell.z1 - margin);
            let reach = Rect::around(x, z, kind.radius() + MIN_GAP);
            if neighbours.iter().any(|n| n.intersects(&reach)) {
                continue;
            }
            let (g, biome) = ground_at(generator, face, x, z);
            if !kind.accepts(biome) || (g as f64) <= SEA_LEVEL_BLOCKS + 1.0 {
                continue;
            }
            let step = if kind == SettlementKind::Ruins { 7 } else { 8 };
            if !site_fits(generator, face, x, z, kind.radius(), step, kind) {
                continue;
            }
            let culture = (kind != SettlementKind::Ruins).then(|| settlement_culture(generator, face, (x, z), biome));
            let planner = Planner { generator, face, area: cell.expand(-2), pieces: Vec::new(), taken: Vec::new(), exits: Vec::new(), culture };
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

/// Culture d'une ville ou d'un village dont le centre est en `center`, au climat `biome` : tirée
/// de sa propre graine (le plan n'en dépend pas), penchant vers celle de son royaume, un carré de
/// `REALM_REGIONS` × `REALM_REGIONS` régions
fn settlement_culture(generator: &PlanetWorldGenerator, face: PlanetFace, center: (i32, i32), biome: BiomeType) -> Culture {
    let seed = generator.planet.seed ^ 0xC017_0BE5_0000_0000 ^ ((face_index(face) as u64) << 40);
    let realm_size = REALM_REGIONS * REGION_BLOCKS;
    let realm = hash2(seed ^ 0x8EA1, center.0.div_euclid(realm_size), center.1.div_euclid(realm_size));
    let realm = Culture::ALL[(realm % Culture::ALL.len() as u64) as usize];
    Culture::choose(biome, realm, unit(hash2(seed ^ 0xC1, center.0, center.1)))
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

fn plan_village(mut p: Planner, center: (i32, i32), biome: BiomeType, dice: &mut Dice) -> Option<Settlement> {
    let (x, z) = center;
    let well = Rect::around(x, z, 1);
    let floor = floor_for(p.generator, p.face, &well, 4)?;
    p.take(Piece::Well { rect: well, floor }, 0);
    let culture = p.culture.unwrap_or(Culture::Humains);
    let (habits, masonry) = (Habits::of(culture), Masonry::of(culture));
    let path = street_block(culture, biome, false);
    p.take(Piece::Paving { rect: Rect::around(x, z, 3), block: path }, 0);

    // Deux à quatre chemins partent de la place
    let mut arms: Vec<Side> = Side::ALL.into_iter().filter(|_| dice.chance(0.8)).collect();
    if arms.len() < 2 {
        arms = vec![Side::West, Side::East];
    }
    // Une chapelle, une taverne et une forge (chacune pas toujours), posées en premier le long des chemins
    let mut specials: Vec<Special> = [(Special::Church, 0.75), (Special::Tavern, 0.6), (Special::Forge, habits.forge)]
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
        let (a, b) = ((x + dx * 4, z + dz * 4), (x + dx * length, z + dz * length));
        let road = Rect::new(a.0.min(b.0) - (!along_x) as i32, a.1.min(b.1) - along_x as i32, a.0.max(b.0) + 1 + (!along_x) as i32, a.1.max(b.1) + 1 + along_x as i32);
        p.take(Piece::Paving { rect: road, block: path }, 0);
        p.exits.push(((x + dx * (length + 1), z + dz * (length + 1)), arm));
        // Bâtiments des deux côtés, porte sur le chemin
        let mut t = 7;
        while t + 3 <= length {
            for side in [-1, 1] {
                let at = (x + dx * t, z + dz * t);
                if let Some(&special) = specials.first() {
                    let (w, d) = special.size(dice, false);
                    let style = Style::village(culture, biome, dice);
                    let placed = match special.house(dice, style) {
                        Some(house) => p.house_along(at, along_x, side, 1, w, d, house),
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
                let (w, d) = (dice.range(5, 9), dice.range(5, 8));
                let style = Style::village(culture, biome, dice);
                let storeys = if dice.chance(habits.upstairs.0) { 2 } else { 1 };
                let house = random_home(dice, storeys, style, habits.porch, habits.hip);
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
    if placed > 0 && dice.chance(0.7) {
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

fn plan_town(mut p: Planner, center: (i32, i32), biome: BiomeType, dice: &mut Dice) -> Option<Settlement> {
    let (x, z) = center;
    // Le damier : rues tous les 14 blocs, remparts à 42 blocs du centre
    const STEP: i32 = 14;
    const HALF: i32 = 42;
    let walls = Rect::new(x - HALF, z - HALF, x + HALF + 1, z + HALF + 1);
    let culture = p.culture.unwrap_or(Culture::Humains);
    let (habits, masonry) = (Habits::of(culture), Masonry::of(culture));
    let street = street_block(culture, biome, true);

    // Fontaine au milieu de la grand-place (posée en premier : elle passe avant le pavage)
    let fountain = Rect::around(x, z, 2);
    let floor = floor_for(p.generator, p.face, &fountain, 3)?;
    p.take(Piece::Fountain { rect: fountain, floor }, 0);
    // Tours d'angle, puis portes et remparts
    for (tx, tz) in [(walls.x0, walls.z0), (walls.x1 - 1, walls.z0), (walls.x0, walls.z1 - 1), (walls.x1 - 1, walls.z1 - 1)] {
        let rect = Rect::around(tx, tz, 2);
        let floor = floor_for(p.generator, p.face, &rect, 6)?;
        p.take(Piece::Tower { rect, floor, masonry }, 0);
    }
    let gates = [
        Rect::new(x - 1, walls.z0, x + 2, walls.z0 + 1),
        Rect::new(x - 1, walls.z1 - 1, x + 2, walls.z1),
        Rect::new(walls.x0, z - 1, walls.x0 + 1, z + 2),
        Rect::new(walls.x1 - 1, z - 1, walls.x1, z + 2),
    ];
    for gate in gates {
        p.take(Piece::Gate { rect: gate, masonry, walk: false }, 0);
    }
    p.exits.extend([
        ((x, walls.z0 - 1), Side::North),
        ((x, walls.z1), Side::South),
        ((walls.x0 - 1, z), Side::West),
        ((walls.x1, z), Side::East),
    ]);
    for rect in [
        Rect::new(walls.x0, walls.z0, walls.x1, walls.z0 + 1),
        Rect::new(walls.x0, walls.z1 - 1, walls.x1, walls.z1),
        Rect::new(walls.x0, walls.z0, walls.x0 + 1, walls.z1),
        Rect::new(walls.x1 - 1, walls.z0, walls.x1, walls.z1),
    ] {
        p.pieces.push(Piece::Rampart { rect, masonry, walk: false });
    }
    // Chemin de ronde : une seconde rangée contre le mur, en dedans, sans créneaux (le passage des
    // portes reste ouvert dessous)
    for gate in [
        Rect::new(x - 1, walls.z0 + 1, x + 2, walls.z0 + 2),
        Rect::new(x - 1, walls.z1 - 2, x + 2, walls.z1 - 1),
        Rect::new(walls.x0 + 1, z - 1, walls.x0 + 2, z + 2),
        Rect::new(walls.x1 - 2, z - 1, walls.x1 - 1, z + 2),
    ] {
        p.take(Piece::Gate { rect: gate, masonry, walk: true }, 0);
    }
    for rect in [
        Rect::new(walls.x0 + 1, walls.z0 + 1, walls.x1 - 1, walls.z0 + 2),
        Rect::new(walls.x0 + 1, walls.z1 - 2, walls.x1 - 1, walls.z1 - 1),
        Rect::new(walls.x0 + 1, walls.z0 + 1, walls.x0 + 2, walls.z1 - 1),
        Rect::new(walls.x1 - 2, walls.z0 + 1, walls.x1 - 1, walls.z1 - 1),
    ] {
        p.pieces.push(Piece::Rampart { rect, masonry, walk: true });
    }
    // Échelles vers le chemin de ronde, au bout des rues qui butent contre le rempart (sur le bord
    // de la rue, qui garde deux blocs de passage)
    for offset in [-2 * STEP, -STEP, STEP, 2 * STEP] {
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
    // Les remparts et leurs abords sont pris, l'intérieur reste à bâtir
    for side in [
        Rect::new(walls.x0 - 2, walls.z0 - 2, walls.x1 + 2, walls.z0 + 2),
        Rect::new(walls.x0 - 2, walls.z1 - 2, walls.x1 + 2, walls.z1 + 2),
        Rect::new(walls.x0 - 2, walls.z0 - 2, walls.x0 + 2, walls.z1 + 2),
        Rect::new(walls.x1 - 2, walls.z0 - 2, walls.x1 + 2, walls.z1 + 2),
    ] {
        p.taken.push(side);
    }

    // Étals de marché autour de la fontaine, tournés vers elle (posés avant le pavage de la place)
    let mut spots: [(i32, i32); 8] = [(-7, -7), (7, -7), (-7, 7), (7, 7), (-4, -9), (4, 9), (9, -4), (-9, 4)];
    shuffle(dice, &mut spots);
    let goods = [WHEAT, CARROT, POTATO, SQUASH, PLANK, CUT_CRYSTAL];
    for &(ox, oz) in &spots[..dice.range(4, 7) as usize] {
        let (rect, front) = if ox.abs() > oz.abs() {
            (Rect::sized(x + ox - (ox > 0) as i32, z + oz - 1, 2, 3), if ox > 0 { Side::West } else { Side::East })
        } else {
            (Rect::sized(x + ox - 1, z + oz - (oz > 0) as i32, 3, 2), if oz > 0 { Side::North } else { Side::South })
        };
        p.pieces.push(Piece::Stall(Stall { rect, front, goods: dice.pick(&goods) }));
    }
    // Grand-place pavée autour de la fontaine
    let plaza = Rect::around(x, z, STEP - 2);
    p.pieces.push(Piece::Paving { rect: plaza, block: street });
    p.taken.push(plaza.expand(1));
    // Rues pavées
    let mut offset = -(HALF / STEP) * STEP;
    while offset <= HALF {
        if offset.abs() < HALF {
            let street_x = Rect::new(walls.x0 + 1, z + offset - 1, walls.x1 - 1, z + offset + 2);
            let street_z = Rect::new(x + offset - 1, walls.z0 + 1, x + offset + 2, walls.z1 - 1);
            for lane in [street_x, street_z] {
                p.pieces.push(Piece::Paving { rect: lane, block: street });
                p.taken.push(lane);
            }
        }
        offset += STEP;
    }
    // Un bâtiment par pâté de maisons : l'église sur la place, une ou deux tavernes et une forge
    // ailleurs, des maisons partout autour
    let mut blocks = Vec::new();
    let mut bz = -HALF;
    while bz < HALF {
        let mut bx = -HALF;
        while bx < HALF {
            // Pâté de 11 × 11 entre deux rues de 3 de large
            let block = Rect::new(x + bx + 2, z + bz + 2, x + bx + STEP - 1, z + bz + STEP - 1);
            if !block.intersects(&plaza) {
                blocks.push(block);
            }
            bx += STEP;
        }
        bz += STEP;
    }
    // Les pâtés qui donnent sur la place d'abord : l'église prend l'un d'eux
    let near_plaza = |b: &Rect| {
        let (bcx, bcz) = b.center();
        b.expand(4).intersects(&plaza) && ((bcx - x).abs() < STEP || (bcz - z).abs() < STEP)
    };
    let (mut first, mut rest): (Vec<Rect>, Vec<Rect>) = blocks.into_iter().partition(near_plaza);
    shuffle(dice, &mut first);
    shuffle(dice, &mut rest);
    let mut specials = vec![Special::Church, Special::Tavern, Special::Forge];
    if dice.chance(0.5) {
        specials.push(Special::Tavern);
    }
    let mut houses = 0;
    for block in first.into_iter().chain(rest) {
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
    // Lampadaires aux carrefours, au coin des rues
    let mut lx = -(HALF / STEP) * STEP + STEP;
    while lx < HALF {
        let mut lz = -(HALF / STEP) * STEP + STEP;
        while lz < HALF {
            if (lx, lz) != (0, 0) && dice.chance(0.7) {
                p.pieces.push(Piece::Lamp { x: x + lx + 1, z: z + lz + 1, masonry });
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
                        assert!(piece.block(*x, ground, *z, ground).unwrap().is_fence());
                        assert!(piece.block(*x, ground + 2, *z, ground).unwrap().light() > 0, "{masonry:?}");
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
                    Piece::Rampart { masonry: m, .. } | Piece::Gate { masonry: m, .. } | Piece::Tower { masonry: m, .. } | Piece::Lamp { masonry: m, .. } => {
                        assert_eq!(*m, masonry, "{} {culture:?} : remparts d'une autre culture", s.kind.name())
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
        // Les quatre peuples se partagent la planète
        assert_eq!(seen.len(), 4, "{seen:?}");
        // Le tirage est stable : la même graine redonne les mêmes cultures
        let again = generator2026_cultures();
        assert_eq!(again, all.iter().map(|s| s.culture).collect::<Vec<_>>());
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
            assert_eq!(usage(town, Use::Church), 1, "une église par ville");
            assert!(usage(town, Use::Tavern) >= 1, "pas de taverne");
            assert!(town.pieces.iter().filter(|p| matches!(p, Piece::Stall(_))).count() >= 4, "pas de marché");
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
                for y in house.floor..house.floor + 2 {
                    assert!(block(dx, y, dz).is_air(), "{:?} : porte bouchée en ({dx}, {y}, {dz})", house.usage);
                }
                assert!(!block(dx, house.floor - 1, dz).is_air(), "{:?} : seuil manquant", house.usage);
                // Devant la porte, le passage est libre (un porche n'a de poteaux qu'aux angles)
                assert!(block(dx + ox, house.floor, dz + oz).is_air(), "{:?} : sortie bouchée", house.usage);
                assert_eq!(block(house.foot.x0, house.floor, house.foot.z0), house.style.corner);
                // Un toit au-dessus d'un coin intérieur
                let (cx, cz) = (house.foot.x0 + 1, house.foot.z0 + 1);
                assert!((house.floor + house.wall_height()..house.top()).any(|y| !block(cx, y, cz).is_air()), "{:?} : pas de toit", house.usage);
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
