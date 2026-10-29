//! Blocs du monde : chacun décrit par un fichier JSON du dossier `assets/blocks/` (nom, couleurs,
//! textures, forme, passage, son, ce qu'il donne une fois cassé), lu une fois au lancement.
//!
//! Ajouter un bloc, c'est ajouter un fichier : le rendu, les collisions, les sons et l'inventaire
//! lisent tout dans ces fiches. Le format est décrit dans `assets/blocks/README.md`.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use serde::Deserialize;

use crate::voxel::BlockId;

/// Dossier des fiches de blocs, depuis le dossier de lancement
pub const BLOCKS_DIR: &str = "assets/blocks";
/// Sous-dossier des formes réutilisables
pub const SHAPES_DIR: &str = "shapes";
/// Texture absente : la face garde sa couleur seule
pub const NO_TEXTURE: u16 = u16::MAX;
/// Premier numéro d'objet : les blocs ont les numéros en dessous (voir `ItemId`)
pub const FIRST_OBJECT_ID: u16 = 0x4000;

/// Un bloc, tel que décrit par sa fiche
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockSheet {
    /// Identifiant : le nom du fichier sans `.json` (ex. `ironOre`)
    #[serde(skip)]
    pub id: String,
    /// Numéro du bloc dans les chunks, le réseau et les sauvegardes : ne change plus une fois choisi
    pub numeric_id: u16,
    /// Nom affiché
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Rubrique du catalogue (`Minéraux`, `Flore`…)
    #[serde(default)]
    pub category: String,
    /// Dureté (celle des modules d'origine : 1 pour une fleur, 14 pour la pierre)
    #[serde(default = "one")]
    pub hardness: f32,
    /// Couleurs des faces dans le terrain
    pub color: BlockColors,
    /// Textures des faces, chemins depuis `assets/blocks/` (icône de l'inventaire)
    #[serde(default)]
    pub textures: BlockTextures,
    /// Teinte appliquée aux textures grises (herbe, feuillages)
    #[serde(default)]
    pub texture_tint: Option<Rgba>,
    /// Forme : `cube`, `plant`, le nom d'une forme de `shapes/` ou des pavés décrits sur place
    #[serde(default)]
    pub shape: ShapeSpec,
    /// Orientation choisie à la pose (`horizontal` : la forme se tourne vers le joueur)
    #[serde(default)]
    pub rotation: BlockRotation,
    /// Arrête le joueur et les créatures
    #[serde(default = "yes")]
    pub solid: bool,
    /// Cache les faces des blocs voisins
    #[serde(default = "yes")]
    pub opaque: bool,
    /// Liquide : ni solide, ni visé, ni ramassé ; on peut poser un bloc dedans
    #[serde(default)]
    pub liquid: bool,
    /// Lumière émise, de 0 à 15 (pas encore rendue)
    #[serde(default)]
    pub light: u8,
    /// Son des pas et des blocs cassés ou posés
    #[serde(default)]
    pub sound: BlockSound,
    /// Ce que donne le bloc cassé : lui-même (champ absent), un autre bloc (son identifiant) ou rien (`null`)
    #[serde(default)]
    pub drop: BlockDrop,
}

/// Couleur `#rrggbb` ou `#rrggbbaa`, en composantes de 0 à 1
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(try_from = "String")]
pub struct Rgba(pub [f32; 4]);

impl TryFrom<String> for Rgba {
    type Error = String;

    fn try_from(text: String) -> Result<Self, String> {
        let hex = text.strip_prefix('#').filter(|h| (h.len() == 6 || h.len() == 8) && h.is_ascii());
        let hex = hex.ok_or_else(|| format!("couleur « {text} » : #rrggbb ou #rrggbbaa attendu"))?;
        let mut rgba = [1.0; 4];
        for (i, value) in rgba.iter_mut().enumerate().take(hex.len() / 2) {
            let byte = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).map_err(|_| format!("couleur « {text} » illisible"))?;
            *value = byte as f32 / 255.0;
        }
        Ok(Rgba(rgba))
    }
}

/// Couleurs des faces : `top` pour le dessus (celle des côtés à défaut), `side` pour les côtés et le dessous
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockColors {
    #[serde(default)]
    pub top: Option<Rgba>,
    pub side: Rgba,
}

/// Textures des faces : `side` sert aussi au dessus et au dessous quand ils n'en ont pas
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockTextures {
    pub top: Option<String>,
    pub side: Option<String>,
    pub bottom: Option<String>,
}

/// Pavé d'une forme, en seizièmes de bloc : `[x0, y0, z0, x1, y1, z1]`, de 0 à 16. L'avant de la
/// forme est le côté z = 0, tourné vers le joueur quand le bloc s'oriente à la pose.
pub type ShapeBox = [u8; 6];

/// Forme écrite dans une fiche : un nom (`cube`, `plant` ou une forme de `shapes/`) ou des pavés
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum ShapeSpec {
    Named(String),
    Boxes {
        boxes: Vec<ShapeBox>,
        #[serde(default)]
        texture: ShapeTexture,
    },
}

/// Comment une face de pavé montre la texture
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShapeTexture {
    /// Le morceau de texture à sa place dans le bloc : une dalle montre le bas des côtés
    #[default]
    Crop,
    /// Toute la texture sur chaque face, quelle que soit sa taille (torche)
    Stretch,
}

impl Default for ShapeSpec {
    fn default() -> Self {
        ShapeSpec::Named("cube".into())
    }
}

impl ShapeSpec {
    pub fn is_named(&self, name: &str) -> bool {
        matches!(self, ShapeSpec::Named(n) if n == name)
    }
}

/// Fiche d'une forme réutilisable (`assets/blocks/shapes/*.json`)
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShapeSheet {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub boxes: Vec<ShapeBox>,
    #[serde(default)]
    pub texture: ShapeTexture,
}

/// Orientation d'un bloc posé
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BlockRotation {
    /// Toujours posé de la même façon
    #[default]
    None,
    /// Tourné d'un quart de tour en quart de tour pour que son avant regarde le joueur
    Horizontal,
}

/// Les six faces d'un bloc, dans l'ordre des voisins : +X, -X, +Y (dessus), -Y, +Z, -Z
pub const FACE_DIRS: [(i32, i32, i32); 6] = [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)];

/// Face opposée à la face `dir` (index de `FACE_DIRS`)
pub const fn opposite_face(dir: usize) -> usize {
    dir ^ 1
}

/// Forme faite de pavés, tournée dans ses quatre orientations
#[derive(Debug, Clone, PartialEq)]
pub struct BlockShape {
    /// Pavés de chaque orientation (quart de tour 0 à 3)
    pub rotations: [Vec<ShapeBox>; 4],
    /// Faces entièrement couvertes, par orientation (bit `dir` de `FACE_DIRS`)
    full_faces: [u8; 4],
    pub texture: ShapeTexture,
}

impl BlockShape {
    pub fn new(boxes: &[ShapeBox], texture: ShapeTexture) -> Result<Self, String> {
        if boxes.is_empty() {
            return Err("forme sans pavé".into());
        }
        for b in boxes {
            if b.iter().any(|&v| v > 16) || b[0] >= b[3] || b[1] >= b[4] || b[2] >= b[5] {
                return Err(format!("pavé {b:?} invalide : [x0, y0, z0, x1, y1, z1] de 0 à 16, chaque début avant sa fin"));
            }
        }
        let rotations: [Vec<ShapeBox>; 4] = std::array::from_fn(|r| boxes.iter().map(|b| rotate_box(*b, r as u8)).collect());
        let full_faces = std::array::from_fn(|r| (0..6).filter(|&dir| covers_face(&rotations[r], dir)).fold(0, |m, dir| m | (1 << dir)));
        Ok(Self { rotations, full_faces, texture })
    }

    /// Pavés du bloc tourné de `rotation` quarts de tour
    pub fn boxes(&self, rotation: u8) -> &[ShapeBox] {
        &self.rotations[(rotation & 3) as usize]
    }

    /// La face `dir` (index de `FACE_DIRS`) du bloc tourné est entièrement couverte
    pub fn covers(&self, rotation: u8, dir: usize) -> bool {
        self.full_faces[(rotation & 3) as usize] & (1 << dir) != 0
    }

    /// Le point local (de 0 à 1 dans chaque axe) est dans un pavé
    pub fn contains(&self, rotation: u8, local: [f64; 3]) -> bool {
        let p = local.map(|v| v * 16.0);
        self.boxes(rotation).iter().any(|b| (0..3).all(|i| p[i] >= b[i] as f64 && p[i] < b[i + 3] as f64))
    }

    /// Hauteur (de 0 à 1) du dessus du plus haut pavé au-dessus du point local (x, z), au plus `max_y`
    pub fn top_at(&self, rotation: u8, x: f64, z: f64, max_y: f64) -> Option<f64> {
        let (x, z) = (x * 16.0, z * 16.0);
        self.boxes(rotation)
            .iter()
            .filter(|b| x >= b[0] as f64 && x < b[3] as f64 && z >= b[2] as f64 && z < b[5] as f64)
            .map(|b| b[4] as f64 / 16.0)
            .filter(|&top| top <= max_y)
            .fold(None, |best: Option<f64>, top| Some(best.map_or(top, |b| b.max(top))))
    }

    /// Boîte englobante (seizièmes)
    pub fn bounds(&self, rotation: u8) -> ShapeBox {
        self.boxes(rotation).iter().fold([16, 16, 16, 0, 0, 0], |a, b| {
            [a[0].min(b[0]), a[1].min(b[1]), a[2].min(b[2]), a[3].max(b[3]), a[4].max(b[4]), a[5].max(b[5])]
        })
    }
}

/// Tourne un pavé de `r` quarts de tour autour de la verticale : l'avant (z = 0) passe à +X, puis
/// +Z, puis -X
pub fn rotate_box(b: ShapeBox, r: u8) -> ShapeBox {
    let turn = |x: u8, z: u8| -> (u8, u8) {
        match r & 3 {
            0 => (x, z),
            1 => (16 - z, x),
            2 => (16 - x, 16 - z),
            _ => (z, 16 - x),
        }
    };
    let (ax, az) = turn(b[0], b[2]);
    let (bx, bz) = turn(b[3], b[5]);
    [ax.min(bx), b[1], az.min(bz), ax.max(bx), b[4], az.max(bz)]
}

/// Les pavés couvrent entièrement la face `dir` du bloc
fn covers_face(boxes: &[ShapeBox], dir: usize) -> bool {
    let axis = match dir {
        0 | 1 => 0,
        2 | 3 => 1,
        _ => 2,
    };
    let plane = if dir % 2 == 0 { 16 } else { 0 };
    let (a, b) = ((axis + 1) % 3, (axis + 2) % 3);
    let mut covered = [[false; 16]; 16];
    for bx in boxes {
        let touches = if plane == 16 { bx[axis + 3] == 16 } else { bx[axis] == 0 };
        if touches {
            for i in bx[a]..bx[a + 3] {
                for j in bx[b]..bx[b + 3] {
                    covered[i as usize][j as usize] = true;
                }
            }
        }
    }
    covered.iter().all(|row| row.iter().all(|&c| c))
}

/// Matière des sons
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BlockSound {
    #[default]
    Stone,
    Dirt,
    Grass,
    Sand,
    Snow,
    Wood,
    Water,
}

/// Ce que donne un bloc cassé
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(from = "Option<String>")]
pub enum BlockDrop {
    #[default]
    Itself,
    Nothing,
    Block(String),
}

impl From<Option<String>> for BlockDrop {
    fn from(value: Option<String>) -> Self {
        value.map_or(BlockDrop::Nothing, BlockDrop::Block)
    }
}

fn one() -> f32 {
    1.0
}

fn yes() -> bool {
    true
}

impl BlockSheet {
    /// Lit la fiche JSON d'un bloc et vérifie qu'elle tient debout
    pub fn parse(id: &str, json: &str) -> Result<Self, String> {
        let mut sheet: BlockSheet = serde_json::from_str(json).map_err(|e| e.to_string())?;
        sheet.id = id.to_string();
        if sheet.numeric_id == 0 || sheet.numeric_id >= FIRST_OBJECT_ID {
            return Err(format!("numéro {} invalide (de 1 à {} : 0 est l'air)", sheet.numeric_id, FIRST_OBJECT_ID - 1));
        }
        if sheet.light > 15 {
            return Err("lumière invalide (de 0 à 15)".into());
        }
        if sheet.liquid {
            sheet.solid = false;
            sheet.opaque = false;
            sheet.drop = BlockDrop::Nothing;
        }
        if sheet.shape.is_named("plant") {
            sheet.opaque = false;
        }
        if let ShapeSpec::Boxes { boxes, texture } = &sheet.shape {
            BlockShape::new(boxes, *texture)?;
        }
        Ok(sheet)
    }

    pub fn top_color(&self) -> [f32; 4] {
        self.color.top.unwrap_or(self.color.side).0
    }

    pub fn side_color(&self) -> [f32; 4] {
        self.color.side.0
    }

    /// Texture d'une face (`FACE_DIRS`) : le dessus, le dessous, ou les côtés, chacun à défaut des côtés
    pub fn face_texture(&self, dir: usize) -> Option<&str> {
        let t = &self.textures;
        match dir {
            2 => t.top.as_ref().or(t.side.as_ref()),
            3 => t.bottom.as_ref().or(t.side.as_ref()),
            _ => t.side.as_ref(),
        }
        .map(String::as_str)
    }

    /// La teinte s'applique à cette face : le dessus, et les côtés qui ont la texture du dessus
    /// (feuillages, herbes). Les côtés de l'herbe, terre bordée de vert, gardent leurs couleurs.
    pub fn tinted_face(&self, dir: usize) -> bool {
        self.texture_tint.is_some() && (dir == 2 || (dir != 3 && self.face_texture(dir) == self.face_texture(2)))
    }
}

/// Propriétés lues à chaque bloc par le maillage et les collisions, rangées par numéro
#[derive(Debug, Clone, Copy)]
struct BlockFlags {
    known: bool,
    solid: bool,
    opaque: bool,
    liquid: bool,
    plant: bool,
    /// Fait de pavés (dalle, escalier…)
    shaped: bool,
    /// Matière qui ne laisse pas voir à travers (pour les faces couvertes d'une forme)
    material_opaque: bool,
    rotatable: bool,
}

/// Bloc sans fiche (fichier retiré d'un monde existant) : un cube plein
const UNKNOWN: BlockFlags =
    BlockFlags { known: false, solid: true, opaque: true, liquid: false, plant: false, shaped: false, material_opaque: true, rotatable: false };
const AIR: BlockFlags =
    BlockFlags { known: false, solid: false, opaque: false, liquid: false, plant: false, shaped: false, material_opaque: false, rotatable: false };

/// Tous les blocs connus
#[derive(Debug, Clone)]
pub struct BlockRegistry {
    /// Fiches rangées par numéro
    sheets: Vec<BlockSheet>,
    /// Rang de la fiche de chaque numéro, et ses propriétés
    index: Vec<Option<usize>>,
    flags: Vec<BlockFlags>,
    /// Ce que donne chaque bloc cassé, résolu en numéros
    drops: Vec<Option<BlockId>>,
    /// Forme de chaque numéro fait de pavés
    shapes: Vec<Option<Arc<BlockShape>>>,
    /// Textures utilisées par les faces, chacune une fois (chemins depuis le dossier des fiches)
    textures: Vec<String>,
    /// Rang dans `textures` de chaque face (`FACE_DIRS`) de chaque numéro, ou `NO_TEXTURE`
    face_textures: Vec<[u16; 6]>,
    /// Dossier des fiches, d'où partent les chemins des textures
    dir: PathBuf,
}

impl Default for BlockRegistry {
    fn default() -> Self {
        Self::from_sheets(Vec::new(), PathBuf::new()).0
    }
}

static BLOCKS: OnceLock<BlockRegistry> = OnceLock::new();

/// Les blocs du jeu, lus dans `assets/blocks/` au premier appel
pub fn blocks() -> &'static BlockRegistry {
    BLOCKS.get_or_init(BlockRegistry::load_default)
}

impl BlockRegistry {
    /// Les blocs de `assets/blocks/` : depuis le dossier de lancement, sinon ceux livrés avec les
    /// sources. Un fichier illisible est signalé et ignoré.
    pub fn load_default() -> Self {
        let candidates = [PathBuf::from(BLOCKS_DIR), Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(BLOCKS_DIR)];
        let Some(dir) = candidates.iter().find(|dir| dir.is_dir()) else {
            log::warn!("Dossier {BLOCKS_DIR}/ introuvable : aucun bloc décrit.");
            return Self::default();
        };
        match Self::load_dir(dir) {
            Ok((registry, errors)) => {
                for error in errors {
                    log::warn!("{error}");
                }
                log::info!("{} bloc(s) chargé(s) depuis {}.", registry.len(), dir.display());
                registry
            }
            Err(e) => {
                log::warn!("Lecture de {} impossible : {e}", dir.display());
                Self::default()
            }
        }
    }

    /// Lit chaque fichier `*.json` du dossier. Renvoie les blocs valides et les erreurs des autres.
    pub fn load_dir(dir: &Path) -> io::Result<(Self, Vec<String>)> {
        let mut sheets = Vec::new();
        let mut errors = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            let path = entry?.path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let id = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
            let result = std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|text| BlockSheet::parse(&id, &text));
            match result {
                Ok(sheet) => sheets.push(sheet),
                Err(e) => errors.push(format!("Bloc {} ignoré : {e}", path.display())),
            }
        }
        let (registry, more) = Self::from_sheets(sheets, dir.to_path_buf());
        errors.extend(more);
        Ok((registry, errors))
    }

    /// Formes réutilisables du sous-dossier `shapes/`, par nom de fichier
    fn load_shapes(dir: &Path, errors: &mut Vec<String>) -> HashMap<String, Arc<BlockShape>> {
        let mut shapes = HashMap::new();
        let Ok(entries) = std::fs::read_dir(dir.join(SHAPES_DIR)) else {
            return shapes;
        };
        for path in entries.flatten().map(|e| e.path()) {
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let id = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
            let result = std::fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|text| serde_json::from_str::<ShapeSheet>(&text).map_err(|e| e.to_string()))
                .and_then(|sheet| BlockShape::new(&sheet.boxes, sheet.texture));
            match result {
                Ok(shape) => {
                    shapes.insert(id, Arc::new(shape));
                }
                Err(e) => errors.push(format!("Forme {} ignorée : {e}", path.display())),
            }
        }
        shapes
    }

    /// Range les fiches par numéro. Un numéro déjà pris écarte la seconde fiche ; un bloc donné
    /// inconnu fait que le bloc se donne lui-même ; une forme inconnue en fait un cube.
    pub fn from_sheets(mut sheets: Vec<BlockSheet>, dir: PathBuf) -> (Self, Vec<String>) {
        let mut errors = Vec::new();
        let named_shapes = Self::load_shapes(&dir, &mut errors);
        sheets.sort_by(|a, b| (a.numeric_id, &a.id).cmp(&(b.numeric_id, &b.id)));
        sheets.dedup_by(|b, a| {
            let duplicate = a.numeric_id == b.numeric_id;
            if duplicate {
                errors.push(format!("Bloc {} ignoré : numéro {} déjà pris par {}", b.id, b.numeric_id, a.id));
            }
            duplicate
        });
        let size = sheets.last().map_or(1, |s| s.numeric_id as usize + 1);
        let mut index = vec![None; size];
        let mut flags = vec![UNKNOWN; size];
        let mut shapes = vec![None; size];
        let mut textures: Vec<String> = Vec::new();
        let mut face_textures = vec![[NO_TEXTURE; 6]; size];
        flags[0] = AIR;
        for (i, sheet) in sheets.iter().enumerate() {
            let n = sheet.numeric_id as usize;
            index[n] = Some(i);
            let shape = match &sheet.shape {
                ShapeSpec::Named(name) if name == "cube" || name == "plant" => None,
                ShapeSpec::Named(name) => match named_shapes.get(name) {
                    Some(shape) => Some(shape.clone()),
                    None => {
                        errors.push(format!("Bloc {} : forme « {name} » inconnue (ni cube, ni plant, ni {SHAPES_DIR}/{name}.json), dessiné en cube", sheet.id));
                        None
                    }
                },
                // Vérifiée à la lecture de la fiche
                ShapeSpec::Boxes { boxes, texture } => BlockShape::new(boxes, *texture).ok().map(Arc::new),
            };
            flags[n] = BlockFlags {
                known: true,
                solid: sheet.solid,
                // Une forme laisse voir ce qui l'entoure : ses faces couvertes cachent seules leurs voisines
                opaque: sheet.opaque && shape.is_none(),
                liquid: sheet.liquid,
                plant: sheet.shape.is_named("plant"),
                shaped: shape.is_some(),
                material_opaque: sheet.opaque,
                rotatable: sheet.rotation == BlockRotation::Horizontal && shape.is_some(),
            };
            shapes[n] = shape;
            // Les liquides gardent leur couleur : la mer garde ses dégradés de profondeur
            if !sheet.liquid {
                for (dir, layer) in face_textures[n].iter_mut().enumerate() {
                    if let Some(file) = sheet.face_texture(dir) {
                        let rank = textures.iter().position(|t| t == file).unwrap_or_else(|| {
                            textures.push(file.to_string());
                            textures.len() - 1
                        });
                        *layer = rank as u16;
                    }
                }
            }
        }
        let mut drops = vec![None; size];
        for sheet in &sheets {
            let block = BlockId(sheet.numeric_id);
            drops[block.0 as usize] = match &sheet.drop {
                BlockDrop::Itself => Some(block),
                BlockDrop::Nothing => None,
                BlockDrop::Block(id) => match sheets.iter().find(|s| &s.id == id) {
                    Some(other) => Some(BlockId(other.numeric_id)),
                    None => {
                        errors.push(format!("Bloc {} : le bloc donné « {id} » n'existe pas, il se donne lui-même", sheet.id));
                        Some(block)
                    }
                },
            };
        }
        (Self { sheets, index, flags, drops, shapes, textures, face_textures, dir }, errors)
    }

    fn flags(&self, block: BlockId) -> BlockFlags {
        self.flags.get(block.kind().0 as usize).copied().unwrap_or(UNKNOWN)
    }

    /// Fiche du bloc, s'il en a une
    pub fn get(&self, block: BlockId) -> Option<&BlockSheet> {
        self.index.get(block.kind().0 as usize).copied().flatten().map(|i| &self.sheets[i])
    }

    /// Forme du bloc s'il est fait de pavés
    pub fn shape(&self, block: BlockId) -> Option<&BlockShape> {
        self.shapes.get(block.kind().0 as usize).and_then(|s| s.as_deref())
    }

    /// Textures des faces de tous les blocs, chacune une fois : la liste qu'envoie le rendu
    pub fn texture_files(&self) -> &[String] {
        &self.textures
    }

    /// Rang dans `texture_files` de la texture de la face `dir` (`FACE_DIRS`), ou `NO_TEXTURE`
    pub fn face_texture(&self, block: BlockId, dir: usize) -> u16 {
        self.face_textures.get(block.kind().0 as usize).map_or(NO_TEXTURE, |faces| faces[dir])
    }

    /// Bloc d'identifiant `id` (le nom de sa fiche)
    pub fn find(&self, id: &str) -> Option<BlockId> {
        self.sheets.iter().find(|s| s.id == id).map(|s| BlockId(s.numeric_id))
    }

    pub fn is_known(&self, block: BlockId) -> bool {
        let flags = self.flags(block);
        flags.known && (block.rotation() == 0 || flags.rotatable)
    }

    /// Ce que donne le bloc cassé
    pub fn drop_of(&self, block: BlockId) -> Option<BlockId> {
        self.drops.get(block.kind().0 as usize).copied().flatten()
    }

    /// Nom affiché
    pub fn name(&self, block: BlockId) -> &str {
        match self.get(block) {
            Some(sheet) => &sheet.name,
            None if block.is_air() => "Air",
            None => "Bloc inconnu",
        }
    }

    /// Chemin d'un fichier cité par une fiche (texture)
    pub fn asset_path(&self, relative: &str) -> PathBuf {
        self.dir.join(relative)
    }

    pub fn len(&self) -> usize {
        self.sheets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sheets.is_empty()
    }

    /// Les fiches, par numéro croissant
    pub fn iter(&self) -> impl Iterator<Item = &BlockSheet> {
        self.sheets.iter()
    }
}

impl BlockId {
    /// Les deux bits hauts du numéro posé dans le monde : l'orientation (quarts de tour) d'un bloc
    /// qui s'oriente. Les fiches ont des numéros en dessous (`FIRST_OBJECT_ID`).
    pub const ROTATION_SHIFT: u16 = 14;

    #[inline(always)]
    pub fn is_air(&self) -> bool {
        self.0 == 0
    }

    /// Le bloc sans son orientation : le numéro de sa fiche
    #[inline(always)]
    pub fn kind(self) -> BlockId {
        BlockId(self.0 & (FIRST_OBJECT_ID - 1))
    }

    /// Quarts de tour du bloc posé (0 à 3)
    #[inline(always)]
    pub fn rotation(self) -> u8 {
        (self.0 >> Self::ROTATION_SHIFT) as u8
    }

    /// Le bloc tourné de `rotation` quarts de tour, s'il s'oriente (sinon tel quel)
    pub fn with_rotation(self, rotation: u8) -> BlockId {
        if blocks().flags(self).rotatable {
            BlockId(self.kind().0 | ((rotation as u16 & 3) << Self::ROTATION_SHIFT))
        } else {
            self.kind()
        }
    }

    /// Forme du bloc s'il est fait de pavés (dalle, escalier…)
    #[inline(always)]
    pub fn shape(&self) -> Option<&'static BlockShape> {
        blocks().shape(*self)
    }

    /// Le bloc cache entièrement la face de son voisin collée à sa face `dir` (`FACE_DIRS`)
    #[inline(always)]
    pub fn hides_face(&self, dir: usize) -> bool {
        let flags = blocks().flags(*self);
        flags.opaque || (flags.shaped && flags.material_opaque && self.shape().is_some_and(|s| s.covers(self.rotation(), dir)))
    }

    /// Le point local (de 0 à 1 dans chaque axe du bloc) est dans la matière du bloc : tout le
    /// bloc, ou les pavés de sa forme
    #[inline(always)]
    pub fn contains(&self, local: [f64; 3]) -> bool {
        match self.shape() {
            Some(shape) => shape.contains(self.rotation(), local),
            None => true,
        }
    }

    /// Cache les faces de ses voisins
    #[inline(always)]
    pub fn is_opaque(&self) -> bool {
        blocks().flags(*self).opaque
    }

    /// Arrête le joueur et les créatures
    #[inline(always)]
    pub fn is_solid(&self) -> bool {
        blocks().flags(*self).solid
    }

    /// Eau, lave : ni visé, ni ramassé ; un bloc posé le remplace
    #[inline(always)]
    pub fn is_liquid(&self) -> bool {
        blocks().flags(*self).liquid
    }

    /// Dessiné en panneaux croisés plutôt qu'en cube
    #[inline(always)]
    pub fn is_plant(&self) -> bool {
        blocks().flags(*self).plant
    }

    /// Visé par le joueur : ni l'air, ni un liquide
    #[inline(always)]
    pub fn is_targetable(&self) -> bool {
        !self.is_air() && !self.is_liquid()
    }

    /// Un bloc posé peut prendre sa place : l'air ou un liquide
    #[inline(always)]
    pub fn is_replaceable(&self) -> bool {
        self.is_air() || self.is_liquid()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bundled() -> BlockRegistry {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(BLOCKS_DIR);
        let (registry, errors) = BlockRegistry::load_dir(&dir).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        registry
    }

    #[test]
    fn test_bundled_blocks_load() {
        let registry = bundled();
        assert!(registry.len() > 100);
        // Les blocs que pose le générateur de terrain ont tous leur fiche
        let terrain = [
            BlockId::STONE, BlockId::DIRT, BlockId::GRASS, BlockId::SAND, BlockId::SNOW, BlockId::WATER, BlockId::LAVA,
            BlockId::WOOD, BlockId::LEAVES, BlockId::IRON_ORE, BlockId::GOLD_ORE, BlockId::CRYSTAL, BlockId::OBSIDIAN,
        ];
        for block in terrain {
            assert!(registry.is_known(block), "{block:?} sans fiche");
        }
        assert_eq!(registry.name(BlockId::STONE), "Pierre");
        assert_eq!(registry.drop_of(BlockId::GRASS), Some(BlockId::DIRT));
        assert_eq!(registry.drop_of(BlockId::WATER), None);
        assert_eq!(registry.drop_of(BlockId::STONE), Some(BlockId::STONE));
        for sheet in registry.iter() {
            for texture in [&sheet.textures.top, &sheet.textures.side, &sheet.textures.bottom].into_iter().flatten() {
                assert!(registry.asset_path(texture).is_file(), "{} : texture {texture} manquante", sheet.id);
            }
        }
    }

    #[test]
    fn test_block_properties() {
        assert!(BlockId::AIR.is_air());
        assert!(!BlockId::AIR.is_solid());
        assert!(!BlockId::STONE.is_air());
        assert!(BlockId::STONE.is_solid());
        assert!(!BlockId::WATER.is_solid());
        assert!(BlockId::WATER.is_liquid() && BlockId::WATER.is_replaceable() && !BlockId::WATER.is_targetable());
        assert!(BlockId::STONE.is_opaque());
        assert!(!BlockId::WATER.is_opaque());
        assert!(!BlockId::LEAVES.is_opaque());
        let flower = blocks().find("dandelion").unwrap();
        assert!(flower.is_plant() && !flower.is_solid() && flower.is_targetable());
        // Un numéro sans fiche reste un cube plein
        assert!(BlockId(FIRST_OBJECT_ID - 1).is_solid());
    }

    #[test]
    fn test_shapes() {
        let slab = BlockShape::new(&[[0, 0, 0, 16, 8, 16]], ShapeTexture::Crop).unwrap();
        // Le dessous est couvert, le dessus et les côtés non
        assert!(slab.covers(0, 3) && !slab.covers(0, 2) && !slab.covers(0, 0));
        assert!(slab.contains(0, [0.5, 0.2, 0.5]) && !slab.contains(0, [0.5, 0.7, 0.5]));
        assert_eq!(slab.top_at(0, 0.3, 0.9, 1.0), Some(0.5));
        assert_eq!(slab.top_at(0, 0.3, 0.9, 0.4), None);

        let stairs = BlockShape::new(&[[0, 0, 0, 16, 8, 16], [0, 8, 8, 16, 16, 16]], ShapeTexture::Crop).unwrap();
        // Dos (+Z) couvert sans rotation ; tourné d'un quart, le dos passe à -X
        assert!(stairs.covers(0, 4) && !stairs.covers(0, 5));
        assert!(stairs.covers(1, 1) && !stairs.covers(1, 0));
        assert!(stairs.covers(2, 5) && stairs.covers(3, 0));
        // Avant : marche basse ; arrière : marche haute
        assert_eq!(stairs.top_at(0, 0.5, 0.2, 1.0), Some(0.5));
        assert_eq!(stairs.top_at(0, 0.5, 0.8, 1.0), Some(1.0));
        assert_eq!(stairs.top_at(2, 0.5, 0.2, 1.0), Some(1.0));
        assert_eq!(stairs.bounds(1), [0, 0, 0, 16, 16, 16]);
        // Quatre quarts de tour ramènent le pavé
        let b = [1, 2, 3, 9, 10, 12];
        assert_eq!((0..4).fold(b, |b, _| rotate_box(b, 1)), b);

        assert!(BlockShape::new(&[[0, 0, 0, 16, 17, 16]], ShapeTexture::Crop).is_err());
        assert!(BlockShape::new(&[[4, 0, 0, 4, 8, 16]], ShapeTexture::Crop).is_err());
        let inline = r##"{ "numeric_id": 500, "name": "Plaque", "color": { "side": "#808080" }, "shape": { "boxes": [[0, 0, 0, 16, 2, 16]] } }"##;
        assert!(BlockSheet::parse("plaque", inline).is_ok());
        assert!(BlockSheet::parse("plaque", &inline.replace("16, 2, 16", "16, 2, 99")).unwrap_err().contains("invalide"));
    }

    #[test]
    fn test_bundled_shaped_blocks() {
        let registry = bundled();
        let stairs = registry.find("plankStairs").unwrap();
        let shape = registry.shape(stairs).expect("escalier fait de pavés");
        assert_eq!(shape.boxes(0).len(), 2);
        assert!(!stairs.is_opaque() && stairs.is_solid());
        // Orienté : le numéro posé porte ses quarts de tour, la fiche et ce qu'il donne restent les mêmes
        let turned = stairs.with_rotation(3);
        assert_eq!((turned.kind(), turned.rotation()), (stairs, 3));
        assert!(registry.is_known(turned));
        assert_eq!(registry.drop_of(turned), Some(stairs));
        assert_eq!(registry.name(turned), "Escalier de planches");
        assert!(turned.hides_face(0) && !turned.hides_face(1));
        // Une dalle ne s'oriente pas : la rotation est ignorée, et refusée d'un autre joueur
        let slab = registry.find("plankSlab").unwrap();
        assert_eq!(slab.with_rotation(2), slab);
        assert!(!registry.is_known(BlockId(slab.0 | 2 << BlockId::ROTATION_SHIFT)));
        assert!(slab.hides_face(3) && !slab.hides_face(2));
        // L'échelle, aux texels découpés, ne cache rien
        let ladder = registry.find("ladder").unwrap();
        assert!(registry.shape(ladder).is_some() && !ladder.hides_face(4));
        // Textures des faces : chacune une fois dans la liste
        let files = registry.texture_files();
        assert_eq!(files[registry.face_texture(slab, 0) as usize], "textures/Plank.png");
        assert_eq!(registry.face_texture(slab, 0), registry.face_texture(registry.find("plank").unwrap(), 2));
        assert_eq!(registry.face_texture(BlockId::WATER, 0), NO_TEXTURE);
        let mut unique = files.to_vec();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), files.len());
    }

    #[test]
    fn test_invalid_sheets_are_explained() {
        let ok = r##"{ "numeric_id": 500, "name": "Cube", "color": { "side": "#808080" } }"##;
        let cube = BlockSheet::parse("cube", ok).unwrap();
        assert!(cube.solid && cube.opaque && cube.drop == BlockDrop::Itself);
        assert_eq!(cube.top_color(), cube.side_color());

        let typo = ok.replace("color", "colour");
        assert!(BlockSheet::parse("cube", &typo).unwrap_err().contains("colour"));
        assert!(BlockSheet::parse("cube", &ok.replace("500", "0")).is_err());
        assert!(BlockSheet::parse("cube", &ok.replace("#808080", "gris")).unwrap_err().contains("gris"));

        let nothing = ok.replace(r#""name""#, r#""drop": null, "name""#);
        assert_eq!(BlockSheet::parse("cube", &nothing).unwrap().drop, BlockDrop::Nothing);
        let water = ok.replace(r#""name""#, r#""liquid": true, "name""#);
        let water = BlockSheet::parse("eau", &water).unwrap();
        assert!(!water.solid && !water.opaque && water.drop == BlockDrop::Nothing);

        let twin = BlockSheet::parse("jumeau", ok).unwrap();
        let (registry, errors) = BlockRegistry::from_sheets(vec![cube, twin], PathBuf::new());
        assert_eq!(registry.len(), 1);
        assert!(errors[0].contains("déjà pris"));
    }
}
