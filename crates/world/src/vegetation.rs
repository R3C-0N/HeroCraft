//! Végétation posée par le générateur : arbres, buissons et cactus, herbes hautes, fleurs et
//! champignons, selon le biome.
//!
//! Rien n'est stocké : tout se déduit de la graine. La face est découpée en cases de
//! `TREE_CELL` blocs ; chaque case tire au plus un arbre, à une place, d'une essence et d'une
//! taille fixées par la graine. Un arbre déborde de sa colonne d'au plus `TREE_REACH` blocs : le
//! bloc d'une colonne ne dépend donc que des quelques cases qui l'entourent, ce qui permet de le
//! retrouver bloc par bloc (collisions, visée, serveur) exactement comme le chunk le pose. Les
//! herbes et les fleurs ne dépendent que de leur colonne.
//!
//! La végétation ne prend jamais la place du relief, de l'eau ni d'une grotte : seulement de l'air
//! au-dessus du sol.

use std::sync::OnceLock;

use herocraft_common::{BlockId, PlanetFace, blocks};

use crate::caves::block_carved;
use crate::edits::FACE_BLOCKS;
use crate::generator::{BiomeType, ColumnSample, PlanetWorldGenerator, SEA_LEVEL_BLOCKS};

/// Côté d'une case d'arbre, en blocs : au plus un arbre par case
pub const TREE_CELL: i32 = 6;
/// Débord d'un arbre autour de son tronc, en blocs
pub const TREE_REACH: i32 = 3;
/// Au-dessus du sol d'une colonne, aucun bloc de végétation n'est posé plus haut que ça (un
/// feuillage qui surplombe une falaise y est coupé) : borne la recherche bloc par bloc
pub const VEGETATION_SPAN: i32 = 24;

/// Les blocs de la flore, par leur fiche (`AIR` si la fiche manque : rien n'est posé)
struct Flora {
    oak: BlockId,
    birch: BlockId,
    pine: BlockId,
    cactus: BlockId,
    green_leaf: BlockId,
    light_leaf: BlockId,
    dark_leaf: BlockId,
    red_leaf: BlockId,
    tall_grass: [BlockId; 3],
    dead_bush: BlockId,
    /// Fleurs des prairies, cueillies par massifs d'une même espèce
    meadow_flowers: [BlockId; 5],
    /// Fleurs des régions froides et des alpages
    cold_flowers: [BlockId; 2],
    shrooms: [BlockId; 2],
    /// Plantes sauvages mûres, par taches d'une même espèce (`WildCrop`)
    wild_crops: [BlockId; WildCrop::COUNT],
}

/// Plantes cultivées qui poussent aussi à l'état sauvage : on les cueille pour leur récolte et
/// leurs graines (voir le champ `loot` de leurs fiches)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WildCrop {
    Wheat,
    Carrot,
    Potato,
    Onion,
    Squash,
    Soy,
    Rice,
    Cotton,
}

impl WildCrop {
    const COUNT: usize = 8;

    /// Espèces sauvages d'un biome
    fn of_biome(biome: BiomeType) -> &'static [WildCrop] {
        use WildCrop::*;
        match biome {
            BiomeType::Plains => &[Wheat, Carrot, Potato, Onion],
            BiomeType::Forest => &[Squash, Potato, Soy],
            BiomeType::Jungle => &[Rice, Soy],
            BiomeType::Savanna => &[Cotton, Soy],
            BiomeType::Taiga | BiomeType::Mountains => &[Potato, Onion],
            _ => &[],
        }
    }
}

/// Taches de plantes sauvages : côté des cases du bruit qui les place, en blocs
const WILD_CROP_CELL: i32 = 24;
/// Valeurs du bruit où une tache commence, puis est pleine (les plus hautes : des taches rares)
const WILD_CROP_PATCH: (f64, f64) = (0.82, 0.90);
/// Part des colonnes d'une tache pleine qui portent une plante
const WILD_CROP_DENSITY: f64 = 0.25;

fn flora() -> &'static Flora {
    static FLORA: OnceLock<Flora> = OnceLock::new();
    FLORA.get_or_init(|| {
        let find = |id: &str| blocks().find(id).unwrap_or(BlockId::AIR);
        Flora {
            oak: find("oakTrunk"),
            birch: find("birchTrunk"),
            pine: find("pineTrunk"),
            cactus: find("cactus"),
            green_leaf: find("greenLeaf"),
            light_leaf: find("lightLeaf"),
            dark_leaf: find("darkLeaf"),
            red_leaf: find("redLeaf"),
            tall_grass: [find("tallGrass1"), find("tallGrass2"), find("tallGrass3")],
            dead_bush: find("deadBush"),
            meadow_flowers: [find("dandelion"), find("yellowFlower"), find("redFlower"), find("tulip"), find("redClover")],
            cold_flowers: [find("iris"), find("lavender")],
            shrooms: [find("brownShroom"), find("redShroom")],
            wild_crops: [
                find("bleStade4"),
                find("carotteStade4"),
                find("pommeDeTerreStade4"),
                find("oignonStade4"),
                find("courgeStade4"),
                find("sojaStade4"),
                find("rizStade4"),
                find("cotton6"),
            ],
        }
    })
}

/// Essence d'un arbre (ou d'une plante qui en a la forme)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TreeKind {
    /// Chêne : tronc court, houppier rond
    Oak,
    /// Bouleau : tronc blanc plus haut, houppier étroit et clair
    Birch,
    /// Chêne d'automne, au feuillage roux
    Autumn,
    /// Sapin : tronc élancé, étages de branches en cône
    Spruce,
    /// Arbre de la jungle : très haut, large couronne
    Jungle,
    /// Acacia de la savane : couronne plate et large
    Acacia,
    /// Palmier des plages : palmes en étoile qui retombent
    Palm,
    /// Buisson de feuillage au ras du sol
    Bush,
    /// Cactus des déserts
    Cactus,
}

/// Un arbre : sa colonne, l'altitude du premier bloc d'air au-dessus du sol, son essence et sa taille
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tree {
    pub face: PlanetFace,
    pub x: i32,
    pub z: i32,
    /// Altitude du pied du tronc (le premier bloc d'air de sa colonne)
    pub ground: i32,
    pub kind: TreeKind,
    /// Hauteur du tronc en blocs
    pub height: i32,
    /// Feuillage de l'arbre
    leaf: BlockId,
    /// Tronc (ou cactus)
    trunk: BlockId,
    /// Aléa propre à l'arbre (feuilles manquantes aux coins)
    seed: u64,
}

impl Tree {
    /// Débord horizontal autour du tronc
    pub fn reach(&self) -> i32 {
        match self.kind {
            TreeKind::Cactus => 0,
            TreeKind::Bush | TreeKind::Birch => 1,
            TreeKind::Oak | TreeKind::Autumn | TreeKind::Spruce => 2,
            TreeKind::Jungle | TreeKind::Acacia | TreeKind::Palm => 3,
        }
    }

    /// Altitude juste au-dessus du plus haut bloc de l'arbre
    pub fn top(&self) -> i32 {
        self.ground
            + match self.kind {
                TreeKind::Cactus => self.height,
                TreeKind::Bush => 2,
                TreeKind::Spruce => self.height + 1,
                TreeKind::Oak | TreeKind::Autumn | TreeKind::Birch => self.height + 1,
                TreeKind::Jungle | TreeKind::Acacia | TreeKind::Palm => self.height + 2,
            }
    }

    /// L'arbre couvre la colonne (x, z) de sa face
    pub fn covers(&self, x: i32, z: i32) -> bool {
        let r = self.reach();
        (x - self.x).abs() <= r && (z - self.z).abs() <= r
    }

    /// Bloc de l'arbre en (x, y, z), et s'il s'agit de son tronc (qui l'emporte sur le feuillage
    /// d'un voisin). Ne regarde pas le terrain.
    pub fn block(&self, x: i32, y: i32, z: i32) -> Option<(BlockId, bool)> {
        let (dx, dy, dz) = (x - self.x, y - self.ground, z - self.z);
        let r = self.reach();
        if dx.abs() > r || dz.abs() > r || dy < 0 || y >= self.top() {
            return None;
        }
        let h = self.height;
        let trunk = dx == 0 && dz == 0;
        if trunk && dy < h {
            return Some((self.trunk, true));
        }
        let (ax, az) = (dx.abs(), dz.abs());
        let d2 = dx * dx + dz * dz;
        // Feuille au hasard (coins du houppier), fixée par l'arbre et la place
        let chance = |p: u32| (mix(self.seed ^ ((dx + 8) as u64) ^ (((dy + 8) as u64) << 8) ^ (((dz + 8) as u64) << 16)) % 100) < p as u64;
        let leaf = match self.kind {
            TreeKind::Cactus => false,
            TreeKind::Bush => match dy {
                0 => ax + az <= 1 || chance(45),
                _ => trunk,
            },
            TreeKind::Oak | TreeKind::Autumn => {
                let t = h - dy; // 0 : juste au-dessus du tronc
                match t {
                    0 => ax + az <= 1,
                    1 => ax.max(az) <= 1 && (ax + az < 2 || chance(60)),
                    2 | 3 => ax.max(az) <= 2 && (ax + az < 4 || chance(40)),
                    _ => false,
                }
            }
            TreeKind::Birch => {
                let t = h - dy;
                match t {
                    0 => ax + az <= 1,
                    1 | 2 => ax.max(az) <= 1 && (ax + az < 2 || chance(70)),
                    3 => ax + az <= 1,
                    _ => false,
                }
            }
            TreeKind::Spruce => {
                // Pointe au sommet, puis étages alternés large / étroit jusqu'à deux blocs du sol
                let t = h - dy;
                if dy < 2 || t < 0 {
                    false
                } else {
                    let wide = t >= 3 && t % 2 == 1;
                    match t {
                        0 => trunk,
                        1 | 2 => ax + az <= 1,
                        _ if wide => ax.max(az) <= 2 && (ax + az < 4 || chance(25)) && (t >= 5 || ax + az <= 3),
                        _ => ax + az <= 1 || (ax.max(az) <= 1 && chance(50)),
                    }
                }
            }
            TreeKind::Jungle => {
                let t = dy - h; // 0 : juste au-dessus du tronc
                match t {
                    -2 => d2 <= 5 && (d2 < 5 || chance(50)),
                    -1 => d2 <= 10 && (d2 < 9 || chance(50)),
                    0 => d2 <= 5,
                    1 => ax + az <= 1,
                    _ => false,
                }
            }
            TreeKind::Acacia => {
                let t = dy - h;
                match t {
                    0 => d2 <= 10 && (d2 < 9 || chance(55)),
                    1 => d2 <= 2,
                    _ => false,
                }
            }
            TreeKind::Palm => {
                let t = dy - h;
                match t {
                    // Palmes retombantes au bout des branches
                    -1 => (ax == 3 && az == 0) || (az == 3 && ax == 0) || (ax == 2 && az == 2 && chance(60)),
                    0 => (dx == 0 || dz == 0 || ax == az) && ax.max(az) <= 2,
                    1 => trunk,
                    _ => false,
                }
            }
        };
        leaf.then_some((self.leaf, false))
    }
}

/// Les arbres d'un chunk et de sa bordure, pour le poser d'un coup et pour lire ses voisins
#[derive(Debug, Clone, Default)]
pub struct VegetationField {
    /// Dans l'ordre des cases (v puis u croissants) : en cas de chevauchement, le premier feuillage l'emporte
    trees: Vec<Tree>,
    top: i32,
}

impl VegetationField {
    pub fn trees(&self) -> &[Tree] {
        &self.trees
    }

    pub fn is_empty(&self) -> bool {
        self.trees.is_empty()
    }

    /// Altitude juste au-dessus du plus haut bloc d'arbre (0 sans arbre)
    pub fn top(&self) -> i32 {
        self.top
    }

    /// Bloc d'arbre en (x, y, z), `ground` étant l'altitude du premier bloc d'air de la colonne
    pub fn tree_block(&self, ground: i32, x: i32, y: i32, z: i32) -> Option<BlockId> {
        if y < ground || y >= ground + VEGETATION_SPAN || y >= self.top {
            return None;
        }
        first_tree_block(self.trees.iter().copied(), x, y, z)
    }
}

/// Tronc d'un des arbres en (x, y, z), sinon le premier feuillage
fn first_tree_block(trees: impl Iterator<Item = Tree> + Clone, x: i32, y: i32, z: i32) -> Option<BlockId> {
    let mut leaf = None;
    for tree in trees {
        match tree.block(x, y, z) {
            Some((block, true)) => return Some(block),
            Some((block, false)) if leaf.is_none() => leaf = Some(block),
            _ => {}
        }
    }
    leaf
}

/// Altitude du premier bloc d'air au-dessus du sol d'une colonne
#[inline]
pub(crate) fn column_ground(column: &ColumnSample) -> i32 {
    column.elevation.floor() as i32
}

/// La colonne a de l'air au-dessus de son sol, hors de l'eau
#[inline]
fn dry_ground(column: &ColumnSample) -> bool {
    column.elevation.floor() >= SEA_LEVEL_BLOCKS
}

impl PlanetWorldGenerator {
    /// Matière du dessus du sol de la colonne (x, z), ou `None` si une grotte l'a percée ou s'il
    /// est sous l'eau
    pub fn surface_block(&self, column: &ColumnSample, face: PlanetFace, x: i32, z: i32) -> Option<BlockId> {
        if !dry_ground(column) {
            return None;
        }
        let ground = column_ground(column);
        if block_carved(self.planet.seed, face, column, x, ground - 1, z) {
            return None;
        }
        Some(self.solid_block(column.dir, column.biome, (ground - 1) as f64, column.elevation.floor()))
    }

    /// L'arbre de la case (cx, cz) d'une face, s'il y en a un. `column` échantillonne une colonne
    /// (x, z) de la face (pour reprendre des colonnes déjà calculées).
    pub(crate) fn tree_in_cell_with(
        &self,
        face: PlanetFace,
        cx: i32,
        cz: i32,
        column: impl FnOnce(i32, i32) -> ColumnSample,
    ) -> Option<Tree> {
        let seed = self.planet.seed;
        let h = cell_hash(seed, face, cx, cz, 0x7EE5);
        let (x, z) = (cx * TREE_CELL + (h % TREE_CELL as u64) as i32, cz * TREE_CELL + ((h >> 8) % TREE_CELL as u64) as i32);
        // Pas d'arbre à cheval sur deux faces du cube
        if x < TREE_REACH || z < TREE_REACH || x >= FACE_BLOCKS - TREE_REACH || z >= FACE_BLOCKS - TREE_REACH {
            return None;
        }
        let roll = unit(mix(h ^ 0x0011));
        // Au-delà, aucun biome n'a d'arbre : la colonne n'est pas échantillonnée
        if roll >= 0.92 {
            return None;
        }
        // Ni dans une rue, ni contre une maison, un champ ou un rempart (feuillage compris)
        if self.is_built(face, x, z, TREE_REACH) {
            return None;
        }
        let column = column(x, z);
        if !dry_ground(&column) || column_ground(&column) <= SEA_LEVEL_BLOCKS as i32 {
            return None;
        }
        let pick = unit(mix(h ^ 0xA5A5));
        let size = mix(h ^ 0x5A5A);
        let groves = patch(seed, face, x, z, 40, 0x6A0E);
        let (density, kind) = match column.biome {
            BiomeType::Forest => {
                // Clairières là où le bosquet s'éclaircit, roux par grandes taches
                let density = 0.30 + 0.62 * smooth(0.25, 0.6, groves);
                let autumn = patch(seed, face, x, z, 64, 0xA07) > 0.68;
                let kind = if pick < 0.14 {
                    TreeKind::Bush
                } else if pick < 0.42 {
                    TreeKind::Birch
                } else if autumn {
                    TreeKind::Autumn
                } else {
                    TreeKind::Oak
                };
                (density, kind)
            }
            BiomeType::Jungle => (
                0.90,
                if pick < 0.30 {
                    TreeKind::Bush
                } else if pick < 0.52 {
                    TreeKind::Oak
                } else {
                    TreeKind::Jungle
                },
            ),
            BiomeType::Taiga => (0.80, if pick < 0.10 { TreeKind::Bush } else { TreeKind::Spruce }),
            BiomeType::Tundra => (0.07, if pick < 0.45 { TreeKind::Bush } else { TreeKind::Spruce }),
            BiomeType::Mountains => (0.22, if pick < 0.2 { TreeKind::Bush } else { TreeKind::Spruce }),
            BiomeType::Plains => {
                // Prairie ouverte, quelques bosquets
                let density = 0.03 + 0.45 * smooth(0.7, 0.85, groves);
                (
                    density,
                    if pick < 0.40 {
                        TreeKind::Bush
                    } else if pick < 0.55 {
                        TreeKind::Birch
                    } else {
                        TreeKind::Oak
                    },
                )
            }
            BiomeType::Savanna => (0.12, if pick < 0.45 { TreeKind::Bush } else { TreeKind::Acacia }),
            BiomeType::Desert => (0.07, TreeKind::Cactus),
            BiomeType::Beach if column.lat.abs() < 0.6 => (0.05, TreeKind::Palm),
            _ => return None,
        };
        if roll >= density {
            return None;
        }
        let surface = self.surface_block(&column, face, x, z)?;
        let on_sand = matches!(kind, TreeKind::Cactus | TreeKind::Palm);
        if surface != if on_sand { BlockId::SAND } else { BlockId::GRASS } {
            return None;
        }
        let f = flora();
        let span = |lo: i32, hi: i32| lo + (size % (hi - lo + 1) as u64) as i32;
        let (height, trunk, leaf) = match kind {
            TreeKind::Oak => (span(4, 6), f.oak, f.green_leaf),
            TreeKind::Autumn => (span(4, 6), f.oak, f.red_leaf),
            TreeKind::Birch => (span(5, 7), f.birch, f.light_leaf),
            TreeKind::Spruce if column.biome == BiomeType::Tundra => (span(4, 6), f.pine, f.dark_leaf),
            TreeKind::Spruce => (span(7, 11), f.pine, f.dark_leaf),
            TreeKind::Jungle => (span(9, 13), f.oak, f.green_leaf),
            TreeKind::Acacia => (span(4, 6), f.oak, f.light_leaf),
            TreeKind::Palm => (span(5, 7), f.birch, f.green_leaf),
            TreeKind::Bush => {
                let leaf = match column.biome {
                    BiomeType::Taiga | BiomeType::Tundra | BiomeType::Mountains => f.dark_leaf,
                    BiomeType::Savanna => f.light_leaf,
                    _ => f.green_leaf,
                };
                (0, BlockId::AIR, leaf)
            }
            TreeKind::Cactus => (span(1, 3), f.cactus, BlockId::AIR),
        };
        let tree = Tree { face, x, z, ground: column_ground(&column), kind, height, leaf, trunk, seed: mix(h ^ 0x7333) };
        if (tree.trunk.is_air() && kind != TreeKind::Bush) || (tree.leaf.is_air() && kind != TreeKind::Cactus) {
            return None;
        }
        Some(tree)
    }

    /// L'arbre de la case (cx, cz) d'une face, s'il y en a un
    pub fn tree_in_cell(&self, face: PlanetFace, cx: i32, cz: i32) -> Option<Tree> {
        self.tree_in_cell_with(face, cx, cz, |x, z| self.sample_block_column(face, x, z))
    }

    /// Arbres qui touchent les colonnes `xs` x `zs` d'une face. `columns` (grille de côté `side`
    /// commençant en (x0, z0)) évite de rééchantillonner les colonnes déjà connues.
    pub fn vegetation_field(
        &self,
        face: PlanetFace,
        xs: std::ops::Range<i32>,
        zs: std::ops::Range<i32>,
        columns: Option<(&[ColumnSample], i32, i32, i32)>,
    ) -> VegetationField {
        herocraft_common::prof_scope!("world::vegetation_field");
        let mut trees = Vec::new();
        for cz in cell_span(zs.clone()) {
            for cx in cell_span(xs.clone()) {
                let tree = self.tree_in_cell_with(face, cx, cz, |x, z| {
                    if let Some((grid, x0, z0, side)) = columns {
                        let (gx, gz) = (x - x0, z - z0);
                        if (0..side).contains(&gx) && (0..side).contains(&gz) {
                            return grid[(gz * side + gx) as usize];
                        }
                    }
                    self.sample_block_column(face, x, z)
                });
                if let Some(tree) = tree {
                    let r = tree.reach();
                    if tree.x + r >= xs.start && tree.x - r < xs.end && tree.z + r >= zs.start && tree.z - r < zs.end {
                        trees.push(tree);
                    }
                }
            }
        }
        let top = trees.iter().map(Tree::top).max().unwrap_or(0);
        VegetationField { trees, top }
    }

    /// Bloc de végétation en (x, y, z) d'une face, `column` étant sa colonne : l'air s'il n'y en a
    /// pas. `tree_of` donne l'arbre d'une case (pour garder les cases déjà calculées).
    pub fn vegetation_block(
        &self,
        column: &ColumnSample,
        face: PlanetFace,
        x: i32,
        y: i32,
        z: i32,
        mut tree_of: impl FnMut(i32, i32) -> Option<Tree>,
    ) -> BlockId {
        let ground = column_ground(column);
        if !dry_ground(column) || y < ground || y >= ground + VEGETATION_SPAN {
            return BlockId::AIR;
        }
        if let Some(block) = self.column_tree_block(x, y, z, &mut tree_of) {
            return block;
        }
        if y == ground
            && let Some(plant) = self.ground_plant(column, face, x, z)
        {
            return plant;
        }
        BlockId::AIR
    }

    /// Bloc d'arbre en (x, y, z), en cherchant les arbres des cases voisines
    pub(crate) fn column_tree_block(&self, x: i32, y: i32, z: i32, tree_of: &mut impl FnMut(i32, i32) -> Option<Tree>) -> Option<BlockId> {
        let mut trees: [Option<Tree>; 4] = [None; 4];
        let mut n = 0;
        for cz in cell_span(z..z + 1) {
            for cx in cell_span(x..x + 1) {
                if let Some(tree) = tree_of(cx, cz)
                    && n < trees.len()
                {
                    trees[n] = Some(tree);
                    n += 1;
                }
            }
        }
        first_tree_block(trees[..n].iter().flatten().copied(), x, y, z)
    }

    /// Altitude juste au-dessus du plus haut bloc d'arbre de la colonne (x, z) (0 sans arbre)
    pub(crate) fn column_tree_top(&self, x: i32, z: i32, tree_of: &mut impl FnMut(i32, i32) -> Option<Tree>) -> i32 {
        let mut top = 0;
        for cz in cell_span(z..z + 1) {
            for cx in cell_span(x..x + 1) {
                if let Some(tree) = tree_of(cx, cz)
                    && tree.covers(x, z)
                {
                    top = top.max(tree.top());
                }
            }
        }
        top
    }

    /// Plante posée sur le sol de la colonne (herbe haute, fleur, champignon, buisson mort), selon
    /// le biome, si son dessus est de l'herbe (ou du sable au désert)
    pub(crate) fn ground_plant(&self, column: &ColumnSample, face: PlanetFace, x: i32, z: i32) -> Option<BlockId> {
        // Rien ne pousse sur une rue, un champ ou dans une maison
        if self.is_built(face, x, z, 0) {
            return None;
        }
        let plant = self.plant_choice(column, face, x, z)?;
        if plant.is_air() {
            return None;
        }
        let wanted = if plant == flora().dead_bush && column.biome == BiomeType::Desert { BlockId::SAND } else { BlockId::GRASS };
        (self.surface_block(column, face, x, z)? == wanted).then_some(plant)
    }

    /// Plante tirée pour la colonne, sans regarder son sol
    fn plant_choice(&self, column: &ColumnSample, face: PlanetFace, x: i32, z: i32) -> Option<BlockId> {
        let seed = self.planet.seed;
        let h = cell_hash(seed, face, x, z, 0x9A55);
        let roll = unit(h);
        // Aucun biome n'en pose plus : la plupart des colonnes s'arrêtent là
        if roll >= 0.50 {
            return None;
        }
        let f = flora();
        let pick = (h >> 40) as usize;
        let grass = |weights: [usize; 3]| {
            let total: usize = weights.iter().sum();
            let mut k = pick % total;
            for (i, w) in weights.iter().enumerate() {
                if k < *w {
                    return f.tall_grass[i];
                }
                k -= w;
            }
            f.tall_grass[0]
        };
        // Massifs de fleurs : une espèce par tache
        let meadow = || {
            let species = (patch(seed, face, x, z, 14, 0xF10E) * f.meadow_flowers.len() as f64) as usize;
            f.meadow_flowers[species.min(f.meadow_flowers.len() - 1)]
        };
        let cold = || f.cold_flowers[(patch(seed, face, x, z, 12, 0xC01D) * 2.0) as usize % 2];
        let shroom = || f.shrooms[pick % 2];
        // Taches de plantes sauvages mûres, une espèce par tache
        let wild = WildCrop::of_biome(column.biome);
        if !wild.is_empty() && roll < WILD_CROP_DENSITY {
            let zone = smooth(WILD_CROP_PATCH.0, WILD_CROP_PATCH.1, patch(seed, face, x, z, WILD_CROP_CELL, 0x5A1D));
            if roll < WILD_CROP_DENSITY * zone {
                let species = (patch(seed, face, x, z, WILD_CROP_CELL, 0x5EED) * wild.len() as f64) as usize;
                return Some(f.wild_crops[wild[species.min(wild.len() - 1)] as usize]);
            }
        }
        let plant = match column.biome {
            BiomeType::Plains => {
                let bloom = smooth(0.58, 0.72, patch(seed, face, x, z, 22, 0xB100));
                let flowers = 0.012 + 0.20 * bloom;
                if roll < flowers {
                    meadow()
                } else if roll < flowers + 0.20 {
                    grass([5, 3, 2])
                } else {
                    return None;
                }
            }
            BiomeType::Forest => match roll {
                r if r < 0.015 => shroom(),
                r if r < 0.035 => meadow(),
                r if r < 0.16 => grass([3, 4, 3]),
                _ => return None,
            },
            BiomeType::Jungle => match roll {
                r if r < 0.26 => grass([1, 4, 5]),
                _ => return None,
            },
            BiomeType::Taiga => match roll {
                r if r < 0.025 => shroom(),
                r if r < 0.10 => grass([5, 3, 1]),
                _ => return None,
            },
            BiomeType::Tundra => match roll {
                r if r < 0.02 => cold(),
                r if r < 0.10 => grass([8, 2, 0]),
                _ => return None,
            },
            BiomeType::Mountains => match roll {
                r if r < 0.04 => cold(),
                r if r < 0.14 => grass([6, 3, 1]),
                _ => return None,
            },
            BiomeType::Savanna => match roll {
                r if r < 0.02 => f.dead_bush,
                r if r < 0.24 => grass([6, 3, 1]),
                _ => return None,
            },
            BiomeType::Desert => match roll {
                r if r < 0.012 => f.dead_bush,
                _ => return None,
            },
            _ => return None,
        };
        Some(plant)
    }
}

/// Cases d'arbre dont un arbre peut toucher les colonnes `range`
fn cell_span(range: std::ops::Range<i32>) -> std::ops::RangeInclusive<i32> {
    (range.start - TREE_REACH).div_euclid(TREE_CELL)..=(range.end - 1 + TREE_REACH).div_euclid(TREE_CELL)
}

fn mix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

fn cell_hash(seed: u64, face: PlanetFace, x: i32, z: i32, salt: u64) -> u64 {
    let key = ((face as u64) << 56) ^ ((x as u32 as u64) << 28) ^ (z as u32 as u64);
    mix(seed ^ mix(key ^ salt.wrapping_mul(0x2545_F491_4F6C_DD1D)))
}

/// Nombre dans [0, 1) tiré des bits hauts de `h`
fn unit(h: u64) -> f64 {
    (h >> 11) as f64 / (1u64 << 53) as f64
}

/// Bruit de valeur lissé sur la face, dans [0, 1], de cases de `cell` blocs
fn patch(seed: u64, face: PlanetFace, x: i32, z: i32, cell: i32, salt: u64) -> f64 {
    let (gx, gz) = (x.div_euclid(cell), z.div_euclid(cell));
    let (fx, fz) = ((x.rem_euclid(cell) as f64 + 0.5) / cell as f64, (z.rem_euclid(cell) as f64 + 0.5) / cell as f64);
    let (sx, sz) = (fx * fx * (3.0 - 2.0 * fx), fz * fz * (3.0 - 2.0 * fz));
    let v = |i: i32, j: i32| unit(cell_hash(seed, face, gx + i, gz + j, salt));
    let top = v(0, 0) + (v(1, 0) - v(0, 0)) * sx;
    let bottom = v(0, 1) + (v(1, 1) - v(0, 1)) * sx;
    top + (bottom - top) * sz
}

fn smooth(edge0: f64, edge1: f64, x: f64) -> f64 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cubed_sphere::PlanetDescriptor;
    use crate::edits::{BlockPos, TerrainEdits, TerrainQuery};
    use herocraft_common::PlanetChunkCoord;
    use herocraft_voxel_engine::CHUNK_SIZE;
    use std::collections::HashMap;

    fn generator() -> PlanetWorldGenerator {
        PlanetWorldGenerator::new(PlanetDescriptor { seed: 42, ..Default::default() })
    }

    /// Un chunk de chaque biome terrestre trouvé sur les faces équatoriales : (biome, face, x, y)
    fn chunks_by_biome(generator: &PlanetWorldGenerator) -> HashMap<BiomeType, (PlanetFace, i32, i32)> {
        let mut found = HashMap::new();
        for face in [PlanetFace::PosX, PlanetFace::NegX, PlanetFace::PosZ, PlanetFace::NegZ, PlanetFace::PosY] {
            for cy in (1..31).step_by(2) {
                for cx in (1..31).step_by(2) {
                    let c = generator.sample_block_column(face, cx * 32 + 16, cy * 32 + 16);
                    found.entry(c.biome).or_insert((face, cx, cy));
                }
            }
        }
        found
    }

    #[test]
    fn chunks_and_block_queries_agree_on_vegetation() {
        let generator = generator();
        let edits = TerrainEdits::new(None);
        let biomes = chunks_by_biome(&generator);
        let mut trunks = 0;
        let mut plants = 0;
        for biome in [BiomeType::Forest, BiomeType::Plains, BiomeType::Taiga, BiomeType::Jungle, BiomeType::Desert, BiomeType::Savanna] {
            let Some(&(face, cx, cy)) = biomes.get(&biome) else { continue };
            let columns = generator.sample_chunk_columns(face, cx, cy, 0);
            let ground = column_ground(&columns[16 * 32 + 16]);
            let mut query = TerrainQuery::new(&generator, &edits);
            for layer in [ground / 32, ground / 32 + 1] {
                let coord = PlanetChunkCoord::new(face, cx, cy, layer);
                let chunk = generator.generate_chunk(&coord);
                for (y, z, x) in itertools(CHUNK_SIZE as u8) {
                    let block = chunk.get_voxel(x, y, z);
                    let (bx, by, bz) = (cx * 32 + x as i32, layer * 32 + y as i32, cy * 32 + z as i32);
                    let column = &columns[z as usize * 32 + x as usize];
                    // Sans cache, chaque bloc rééchantillonne les cases voisines : un sur sept suffit
                    if (bx + 3 * bz + by) % 7 == 0 {
                        assert_eq!(generator.column_block(column, face, bx, by, bz), block, "{biome:?} {bx} {by} {bz}");
                    }
                    assert_eq!(query.block(BlockPos::new(face, bx, by, bz)), block, "{biome:?} {bx} {by} {bz}");
                    trunks += [flora().oak, flora().birch, flora().pine, flora().cactus].contains(&block) as usize;
                    plants += block.is_plant() as usize;
                }
            }
        }
        assert!(trunks > 20, "{trunks} blocs de tronc");
        assert!(plants > 200, "{plants} plantes");
    }

    fn itertools(n: u8) -> impl Iterator<Item = (u8, u8, u8)> {
        (0..n).flat_map(move |y| (0..n).flat_map(move |z| (0..n).map(move |x| (y, z, x))))
    }

    #[test]
    fn forests_are_dense_and_plains_open() {
        let generator = generator();
        let mut cells: HashMap<BiomeType, (usize, usize)> = HashMap::new();
        for cz in (20..150).step_by(3) {
            for cx in (20..150).step_by(3) {
                for face in [PlanetFace::PosX, PlanetFace::NegZ, PlanetFace::PosZ] {
                    let (x, z) = (cx * TREE_CELL + 3, cz * TREE_CELL + 3);
                    let biome = generator.sample_block_column(face, x, z).biome;
                    let entry = cells.entry(biome).or_default();
                    entry.0 += 1;
                    entry.1 += generator.tree_in_cell(face, cx, cz).is_some_and(|t| t.kind != TreeKind::Bush) as usize;
                }
            }
        }
        let share = |b: BiomeType| cells.get(&b).map_or(0.0, |&(n, t)| t as f64 / n.max(1) as f64);
        for (biome, (n, t)) in &cells {
            println!("{biome:?} : {t} arbres sur {n} cases ({:.0} %)", 100.0 * *t as f64 / *n as f64);
        }
        assert!(share(BiomeType::Forest) > 0.35, "forêt : {:.2}", share(BiomeType::Forest));
        assert!(share(BiomeType::Plains) < 0.15, "plaine : {:.2}", share(BiomeType::Plains));
        assert!(share(BiomeType::Ocean) < 0.01, "océan : {:.3}", share(BiomeType::Ocean));
    }

    /// Des plantes sauvages mûres poussent par taches dans les prairies, assez pour en trouver en
    /// se promenant, sans couvrir la plaine
    #[test]
    fn wild_crops_grow_in_rare_patches() {
        let generator = generator();
        let crops = flora().wild_crops;
        assert!(crops.iter().all(|c| !c.is_air()), "chaque plante sauvage a sa fiche");
        let (mut plains, mut wild, mut kinds) = (0usize, 0usize, std::collections::HashSet::new());
        for face in [PlanetFace::PosX, PlanetFace::NegZ, PlanetFace::PosZ] {
            for z in (2000..2600).step_by(3) {
                for x in (2000..2600).step_by(3) {
                    let column = generator.sample_block_column(face, x, z);
                    if column.biome != BiomeType::Plains {
                        continue;
                    }
                    plains += 1;
                    if let Some(plant) = generator.ground_plant(&column, face, x, z).filter(|p| crops.contains(p)) {
                        wild += 1;
                        kinds.insert(plant);
                    }
                }
            }
        }
        let share = wild as f64 / plains.max(1) as f64;
        println!("{wild} plantes sauvages sur {plains} colonnes de prairie ({:.2} %), {} espèces", 100.0 * share, kinds.len());
        assert!(plains > 1000, "pas assez de prairie échantillonnée : {plains}");
        assert!(share > 0.001 && share < 0.03, "part de plantes sauvages : {share}");
        assert!(kinds.len() >= 2, "plusieurs espèces : {kinds:?}");
    }

    #[test]
    fn trees_stay_on_their_face_and_off_water() {
        let generator = generator();
        for face in PlanetFace::ALL {
            for cz in 0..FACE_BLOCKS / TREE_CELL {
                for cx in [0, 1, FACE_BLOCKS / TREE_CELL - 1] {
                    if let Some(tree) = generator.tree_in_cell(face, cx, cz) {
                        assert!(tree.x - tree.reach() >= 0 && tree.x + tree.reach() < FACE_BLOCKS);
                        assert!(tree.ground > SEA_LEVEL_BLOCKS as i32);
                    }
                }
            }
        }
    }

    #[test]
    fn every_tree_kind_has_a_trunk_or_leaves() {
        for kind in [
            TreeKind::Oak,
            TreeKind::Birch,
            TreeKind::Autumn,
            TreeKind::Spruce,
            TreeKind::Jungle,
            TreeKind::Acacia,
            TreeKind::Palm,
            TreeKind::Bush,
            TreeKind::Cactus,
        ] {
            let f = flora();
            let tree = Tree { face: PlanetFace::PosX, x: 0, z: 0, ground: 120, kind, height: 6, leaf: f.green_leaf, trunk: f.oak, seed: 7 };
            let r = tree.reach();
            let mut leaves = 0;
            let mut highest = 0;
            for y in tree.ground - 2..tree.top() + 3 {
                for z in -4..=4 {
                    for x in -4..=4 {
                        if let Some((_, trunk)) = tree.block(x, y, z) {
                            assert!(x.abs() <= r && z.abs() <= r && y < tree.top(), "{kind:?} déborde en {x} {y} {z}");
                            leaves += !trunk as usize;
                            highest = highest.max(y + 1);
                        }
                    }
                }
            }
            assert_eq!(highest, tree.top(), "{kind:?} : sommet");
            assert!(tree.top() - tree.ground <= VEGETATION_SPAN && r <= TREE_REACH);
            if kind != TreeKind::Cactus {
                assert!(leaves >= 4, "{kind:?} : {leaves} feuilles");
            }
        }
    }
}
