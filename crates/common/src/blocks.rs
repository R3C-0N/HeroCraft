//! Blocs du monde : chacun décrit par un fichier JSON du dossier `assets/blocks/` (nom, couleurs,
//! textures, forme, passage, son, ce qu'il donne une fois cassé), lu une fois au lancement.
//!
//! Ajouter un bloc, c'est ajouter un fichier : le rendu, les collisions, les sons et l'inventaire
//! lisent tout dans ces fiches. Le format est décrit dans `assets/blocks/README.md`.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::Deserialize;

use crate::voxel::BlockId;

/// Dossier des fiches de blocs, depuis le dossier de lancement
pub const BLOCKS_DIR: &str = "assets/blocks";
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
    #[serde(default)]
    pub shape: BlockShape,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BlockShape {
    /// Cube plein
    #[default]
    Cube,
    /// Deux panneaux croisés (fleurs, herbes, pousses, torche)
    Plant,
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
        if sheet.shape == BlockShape::Plant {
            sheet.opaque = false;
        }
        Ok(sheet)
    }

    pub fn top_color(&self) -> [f32; 4] {
        self.color.top.unwrap_or(self.color.side).0
    }

    pub fn side_color(&self) -> [f32; 4] {
        self.color.side.0
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
}

/// Bloc sans fiche (fichier retiré d'un monde existant) : un cube plein
const UNKNOWN: BlockFlags = BlockFlags { known: false, solid: true, opaque: true, liquid: false, plant: false };
const AIR: BlockFlags = BlockFlags { known: false, solid: false, opaque: false, liquid: false, plant: false };

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

    /// Range les fiches par numéro. Un numéro déjà pris écarte la seconde fiche ; un bloc donné
    /// inconnu fait que le bloc se donne lui-même.
    pub fn from_sheets(mut sheets: Vec<BlockSheet>, dir: PathBuf) -> (Self, Vec<String>) {
        let mut errors = Vec::new();
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
        flags[0] = AIR;
        for (i, sheet) in sheets.iter().enumerate() {
            index[sheet.numeric_id as usize] = Some(i);
            flags[sheet.numeric_id as usize] = BlockFlags {
                known: true,
                solid: sheet.solid,
                opaque: sheet.opaque,
                liquid: sheet.liquid,
                plant: sheet.shape == BlockShape::Plant,
            };
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
        (Self { sheets, index, flags, drops, dir }, errors)
    }

    fn flags(&self, block: BlockId) -> BlockFlags {
        self.flags.get(block.0 as usize).copied().unwrap_or(UNKNOWN)
    }

    /// Fiche du bloc, s'il en a une
    pub fn get(&self, block: BlockId) -> Option<&BlockSheet> {
        self.index.get(block.0 as usize).copied().flatten().map(|i| &self.sheets[i])
    }

    /// Bloc d'identifiant `id` (le nom de sa fiche)
    pub fn find(&self, id: &str) -> Option<BlockId> {
        self.sheets.iter().find(|s| s.id == id).map(|s| BlockId(s.numeric_id))
    }

    pub fn is_known(&self, block: BlockId) -> bool {
        self.flags(block).known
    }

    /// Ce que donne le bloc cassé
    pub fn drop_of(&self, block: BlockId) -> Option<BlockId> {
        self.drops.get(block.0 as usize).copied().flatten()
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
    #[inline(always)]
    pub fn is_air(&self) -> bool {
        self.0 == 0
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
