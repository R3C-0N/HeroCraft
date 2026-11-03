//! Objets de l'inventaire : les blocs, et les objets qui n'en sont pas (outils, armes, armures,
//! nourriture, butin…), décrits chacun par un fichier JSON du dossier `assets/items/` (nom,
//! description, taille de pile, icône, caractéristiques), lu une fois au lancement.
//!
//! Le format est décrit dans `assets/items/README.md`.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::blocks::{blocks, FIRST_OBJECT_ID};
use crate::voxel::BlockId;

/// Dossier des fiches d'objets, depuis le dossier de lancement
pub const ITEMS_DIR: &str = "assets/items";
/// Taille de pile d'un bloc
pub const BLOCK_STACK: u16 = 64;

/// Ce qu'une case d'inventaire contient : un bloc (numéro sous `FIRST_OBJECT_ID`, le même que
/// `BlockId`) ou un objet (numéro de sa fiche, à partir de `FIRST_OBJECT_ID`)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ItemId(pub u16);

impl From<BlockId> for ItemId {
    fn from(block: BlockId) -> Self {
        ItemId(block.kind().0)
    }
}

impl ItemId {
    /// Le bloc, si c'en est un
    pub fn block(self) -> Option<BlockId> {
        (self.0 < FIRST_OBJECT_ID).then_some(BlockId(self.0))
    }

    /// Bloc ou objet décrit par une fiche
    pub fn is_known(self) -> bool {
        match self.block() {
            Some(block) => blocks().is_known(block),
            None => items().get(self).is_some(),
        }
    }

    /// Nom affiché
    pub fn name(self) -> &'static str {
        match self.block() {
            Some(block) => blocks().name(block),
            None => items().get(self).map_or("Objet inconnu", |item| item.name.as_str()),
        }
    }

    /// Nombre au plus par case
    pub fn max_stack(self) -> u16 {
        match self.block() {
            Some(_) => BLOCK_STACK,
            None => items().get(self).map_or(1, |item| item.max_stack),
        }
    }
}

/// Un objet, tel que décrit par sa fiche
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemSheet {
    /// Identifiant : le nom du fichier sans `.json` (ex. `flintSword`)
    #[serde(skip)]
    pub id: String,
    /// Numéro de l'objet dans les inventaires enregistrés : ne change plus une fois choisi
    pub numeric_id: u16,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Rubrique du catalogue (`Armes`, `Plats`…)
    #[serde(default)]
    pub category: String,
    /// Nombre au plus par case
    #[serde(default = "max_stack")]
    pub max_stack: u16,
    /// Icône PNG, chemin depuis `assets/items/`
    pub icon: String,
    /// Caractéristiques affichées (dégâts, durabilité, protection, satiété…)
    #[serde(default)]
    pub stats: BTreeMap<String, f32>,
    /// Recettes qui le fabriquent (voir `crafting`)
    #[serde(default)]
    pub recipes: Vec<crate::crafting::RecipeSheet>,
}

fn max_stack() -> u16 {
    BLOCK_STACK
}

impl ItemSheet {
    /// Lit la fiche JSON d'un objet et vérifie qu'elle tient debout
    pub fn parse(id: &str, json: &str) -> Result<Self, String> {
        let mut sheet: ItemSheet = serde_json::from_str(json).map_err(|e| e.to_string())?;
        sheet.id = id.to_string();
        if sheet.numeric_id < FIRST_OBJECT_ID {
            return Err(format!("numéro {} invalide (à partir de {FIRST_OBJECT_ID} : en dessous, ce sont les blocs)", sheet.numeric_id));
        }
        if !(1..=999).contains(&sheet.max_stack) {
            return Err("taille de pile invalide (de 1 à 999)".into());
        }
        Ok(sheet)
    }

    pub fn item(&self) -> ItemId {
        ItemId(self.numeric_id)
    }
}

/// Tous les objets connus (hors blocs), rangés par numéro
#[derive(Debug, Clone, Default)]
pub struct ItemRegistry {
    sheets: Vec<ItemSheet>,
    /// Dossier des fiches, d'où partent les chemins des icônes
    dir: PathBuf,
}

static ITEMS: OnceLock<ItemRegistry> = OnceLock::new();

/// Les objets du jeu, lus dans `assets/items/` au premier appel
pub fn items() -> &'static ItemRegistry {
    ITEMS.get_or_init(ItemRegistry::load_default)
}

impl ItemRegistry {
    /// Les objets de `assets/items/` : depuis le dossier de lancement, sinon ceux livrés avec les
    /// sources. Un fichier illisible est signalé et ignoré.
    pub fn load_default() -> Self {
        let candidates = [PathBuf::from(ITEMS_DIR), Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(ITEMS_DIR)];
        let Some(dir) = candidates.iter().find(|dir| dir.is_dir()) else {
            log::warn!("Dossier {ITEMS_DIR}/ introuvable : aucun objet.");
            return Self::default();
        };
        match Self::load_dir(dir) {
            Ok((registry, errors)) => {
                for error in errors {
                    log::warn!("{error}");
                }
                log::info!("{} objet(s) chargé(s) depuis {}.", registry.len(), dir.display());
                registry
            }
            Err(e) => {
                log::warn!("Lecture de {} impossible : {e}", dir.display());
                Self::default()
            }
        }
    }

    /// Lit chaque fichier `*.json` du dossier. Renvoie les objets valides et les erreurs des autres.
    pub fn load_dir(dir: &Path) -> io::Result<(Self, Vec<String>)> {
        let mut sheets: Vec<ItemSheet> = Vec::new();
        let mut errors = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            let path = entry?.path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let id = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
            let result = std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|text| ItemSheet::parse(&id, &text));
            match result {
                Ok(sheet) => sheets.push(sheet),
                Err(e) => errors.push(format!("Objet {} ignoré : {e}", path.display())),
            }
        }
        sheets.sort_by(|a, b| (a.numeric_id, &a.id).cmp(&(b.numeric_id, &b.id)));
        sheets.dedup_by(|b, a| {
            let duplicate = a.numeric_id == b.numeric_id;
            if duplicate {
                errors.push(format!("Objet {} ignoré : numéro {} déjà pris par {}", b.id, b.numeric_id, a.id));
            }
            duplicate
        });
        Ok((Self { sheets, dir: dir.to_path_buf() }, errors))
    }

    pub fn get(&self, item: ItemId) -> Option<&ItemSheet> {
        self.sheets.binary_search_by_key(&item.0, |s| s.numeric_id).ok().map(|i| &self.sheets[i])
    }

    /// Objet d'identifiant `id` (le nom de sa fiche)
    pub fn find(&self, id: &str) -> Option<ItemId> {
        self.sheets.iter().find(|s| s.id == id).map(ItemSheet::item)
    }

    /// Chemin d'un fichier cité par une fiche (icône)
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
    pub fn iter(&self) -> impl Iterator<Item = &ItemSheet> {
        self.sheets.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bundled_items_load() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(ITEMS_DIR);
        let (registry, errors) = ItemRegistry::load_dir(&dir).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        assert!(registry.len() > 500);
        for sheet in registry.iter() {
            assert!(registry.asset_path(&sheet.icon).is_file(), "{} : icône manquante", sheet.id);
        }
        let sword = items().find("flintSword").unwrap();
        assert_eq!(sword.name(), "Glaive en silex");
        assert_eq!(sword.max_stack(), 1);
        assert_eq!(sword.block(), None);
        assert_eq!(items().get(sword).unwrap().stats["Dégâts"], 4.0);
    }

    #[test]
    fn test_blocks_are_items() {
        let stone = ItemId::from(BlockId::STONE);
        assert_eq!(stone.block(), Some(BlockId::STONE));
        assert_eq!(stone.name(), "Pierre");
        assert_eq!(stone.max_stack(), BLOCK_STACK);
        assert!(stone.is_known());
        assert!(!ItemId(u16::MAX).is_known());
    }

    #[test]
    fn test_invalid_sheets_are_explained() {
        let ok = r#"{ "numeric_id": 20000, "name": "Caillou", "icon": "caillou.png" }"#;
        let pebble = ItemSheet::parse("caillou", ok).unwrap();
        assert_eq!(pebble.max_stack, BLOCK_STACK);
        assert!(ItemSheet::parse("caillou", &ok.replace("20000", "12")).unwrap_err().contains("blocs"));
        assert!(ItemSheet::parse("caillou", &ok.replace("icon", "icone")).unwrap_err().contains("icone"));
        let empty = ok.replace(r#""icon""#, r#""max_stack": 0, "icon""#);
        assert!(ItemSheet::parse("caillou", &empty).is_err());
    }
}
