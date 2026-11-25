//! Artisanat : les recettes et les établis.
//!
//! Une recette est écrite dans la fiche de ce qu'elle fabrique (`recipes`, dans `assets/items/` ou
//! `assets/blocks/`) : ce qu'il faut, combien elle en donne, et l'établi où la faire (avec son
//! niveau). Sans établi, elle se fait à la main, depuis l'écran d'inventaire.
//!
//! Les établis sont décrits dans `assets/stations/` : un fichier par établi, qui nomme le bloc de
//! chacun de ses niveaux. Un établi d'un niveau fait aussi les recettes des niveaux en dessous.
//!
//! Les formats sont décrits dans `assets/stations/README.md`.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer};

use crate::blocks::{blocks, BlockRegistry};
use crate::items::{items, ItemId, ItemRegistry};
use crate::voxel::BlockId;

/// Dossier des fiches d'établis, depuis le dossier de lancement
pub const STATIONS_DIR: &str = "assets/stations";

/// Ingrédients d'une recette, dans l'ordre de la fiche : `{"bronzeIngot": 3, "stick": 2}`. Une clé
/// `"oakTrunk|birchTrunk"` accepte l'un ou l'autre.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IngredientList(pub Vec<(String, u16)>);

impl<'de> Deserialize<'de> for IngredientList {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ListVisitor;
        impl<'de> Visitor<'de> for ListVisitor {
            type Value = IngredientList;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("un objet {\"identifiant\": nombre}")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut list = Vec::new();
                while let Some((id, count)) = map.next_entry::<String, u16>()? {
                    list.push((id, count));
                }
                Ok(IngredientList(list))
            }
        }
        deserializer.deserialize_map(ListVisitor)
    }
}

/// Une recette telle qu'écrite dans une fiche d'objet ou de bloc
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeSheet {
    /// Établi où se fait la recette (identifiant de sa fiche), ou rien : à la main
    #[serde(default)]
    pub station: Option<String>,
    /// Niveau d'établi demandé, à partir de 1
    #[serde(default = "one_u8")]
    pub level: u8,
    pub ingredients: IngredientList,
    /// Exemplaires fabriqués
    #[serde(default = "one_u16")]
    pub count: u16,
    /// Ce qu'il faut, en plus de l'objet usé, pour le remettre à neuf au même établi. Repris de
    /// `develop`, pas encore utilisé : les objets ne s'usent pas encore.
    #[serde(default)]
    pub repair: IngredientList,
}

fn one_u8() -> u8 {
    1
}

fn one_u16() -> u16 {
    1
}

/// Un établi, tel que décrit par sa fiche
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StationSheet {
    /// Identifiant : le nom du fichier sans `.json` (ex. `workbench`)
    #[serde(skip)]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Bloc de chaque niveau, du premier au dernier (identifiants de leurs fiches)
    pub levels: Vec<String>,
}

/// Un établi prêt à servir
#[derive(Debug, Clone)]
pub struct Station {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Bloc de chaque niveau : `levels[0]` est le niveau 1
    pub levels: Vec<BlockId>,
}

impl Station {
    /// Nombre de niveaux
    pub fn max_level(&self) -> u8 {
        self.levels.len() as u8
    }

    /// Nom de l'établi à un niveau : celui du bloc de ce niveau
    pub fn level_name(&self, level: u8) -> &str {
        match self.levels.get(level.saturating_sub(1) as usize) {
            Some(&block) => blocks().name(block),
            None => &self.name,
        }
    }
}

/// Établi devant lequel se tient le joueur : son rang dans `CraftingBook::stations` et son niveau
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StationAt {
    pub station: usize,
    pub level: u8,
}

/// Un ingrédient : `count` exemplaires de l'un des `options`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ingredient {
    pub options: Vec<ItemId>,
    pub count: u16,
}

/// Une recette prête à servir
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recipe {
    pub result: ItemId,
    pub count: u16,
    /// Établi et niveau demandés, ou `None` : à la main
    pub station: Option<StationAt>,
    pub ingredients: Vec<Ingredient>,
}

impl Recipe {
    /// Se fait là où se tient le joueur : à la main toujours, à un établi les recettes de cet
    /// établi jusqu'à son niveau
    pub fn is_available_at(&self, at: Option<StationAt>) -> bool {
        match (self.station, at) {
            (None, _) => true,
            (Some(needed), Some(at)) => needed.station == at.station && needed.level <= at.level,
            (Some(_), None) => false,
        }
    }
}

/// Toutes les recettes et tous les établis
#[derive(Debug, Clone, Default)]
pub struct CraftingBook {
    pub stations: Vec<Station>,
    pub recipes: Vec<Recipe>,
}

static CRAFTING: OnceLock<CraftingBook> = OnceLock::new();

/// Les recettes et les établis du jeu, lus au premier appel
pub fn crafting() -> &'static CraftingBook {
    CRAFTING.get_or_init(CraftingBook::load_default)
}

impl CraftingBook {
    /// Les établis de `assets/stations/` (depuis le dossier de lancement, sinon ceux livrés avec les
    /// sources) et les recettes des fiches. Une fiche ou une recette invalide est signalée et ignorée.
    pub fn load_default() -> Self {
        let candidates = [PathBuf::from(STATIONS_DIR), Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(STATIONS_DIR)];
        let mut errors = Vec::new();
        let stations = match candidates.iter().find(|dir| dir.is_dir()) {
            Some(dir) => match load_station_sheets(dir) {
                Ok((sheets, more)) => {
                    errors.extend(more);
                    sheets
                }
                Err(e) => {
                    log::warn!("Lecture de {} impossible : {e}", dir.display());
                    Vec::new()
                }
            },
            None => {
                log::warn!("Dossier {STATIONS_DIR}/ introuvable : aucun établi.");
                Vec::new()
            }
        };
        let (book, more) = Self::build(&stations, blocks(), items());
        errors.extend(more);
        for error in errors {
            log::warn!("{error}");
        }
        log::info!("{} établi(s) et {} recette(s) chargés.", book.stations.len(), book.recipes.len());
        book
    }

    /// Assemble les établis et les recettes écrites dans les fiches des blocs et des objets.
    /// Renvoie aussi les erreurs de ce qui a été écarté.
    pub fn build(station_sheets: &[StationSheet], blocks: &BlockRegistry, items: &ItemRegistry) -> (Self, Vec<String>) {
        let mut errors = Vec::new();
        let mut stations: Vec<Station> = Vec::new();
        for sheet in station_sheets {
            let mut levels = Vec::new();
            for id in &sheet.levels {
                match blocks.find(id) {
                    Some(block) if !stations.iter().any(|s| s.levels.contains(&block)) && !levels.contains(&block) => levels.push(block),
                    Some(_) => errors.push(format!("Établi {} : le bloc « {id} » sert déjà à un autre niveau ou établi", sheet.id)),
                    None => errors.push(format!("Établi {} : le bloc « {id} » n'existe pas", sheet.id)),
                }
            }
            if levels.len() != sheet.levels.len() || levels.is_empty() {
                errors.push(format!("Établi {} ignoré : chaque niveau doit nommer un bloc", sheet.id));
                continue;
            }
            stations.push(Station { id: sheet.id.clone(), name: sheet.name.clone(), description: sheet.description.clone(), levels });
        }

        let find = |id: &str| -> Option<ItemId> { blocks.find(id).map(ItemId::from).or_else(|| items.find(id)) };
        let mut recipes = Vec::new();
        let written = blocks
            .iter()
            .map(|s| (s.id.as_str(), ItemId::from(BlockId(s.numeric_id)), &s.recipes))
            .chain(items.iter().map(|s| (s.id.as_str(), s.item(), &s.recipes)));
        for (owner, result, sheets) in written {
            for (n, sheet) in sheets.iter().enumerate() {
                let label = format!("Recette {} de {owner}", n + 1);
                match resolve(sheet, result, &stations, &find, blocks) {
                    Ok(recipe) => recipes.push(recipe),
                    Err(e) => errors.push(format!("{label} ignorée : {e}")),
                }
            }
        }
        (Self { stations, recipes }, errors)
    }

    /// Établi que forme ce bloc posé, et son niveau
    pub fn station_of(&self, block: BlockId) -> Option<StationAt> {
        let block = block.kind();
        self.stations.iter().enumerate().find_map(|(station, s)| {
            s.levels.iter().position(|&b| b == block).map(|i| StationAt { station, level: i as u8 + 1 })
        })
    }

    /// Établi d'identifiant `id`
    pub fn station(&self, id: &str) -> Option<usize> {
        self.stations.iter().position(|s| s.id == id)
    }

    /// Recettes faisables là où se tient le joueur (`None` : à la main)
    pub fn recipes_at(&self, at: Option<StationAt>) -> impl Iterator<Item = &Recipe> {
        self.recipes.iter().filter(move |r| r.is_available_at(at))
    }

    /// Recettes qui fabriquent `item`
    pub fn recipes_for(&self, item: ItemId) -> impl Iterator<Item = &Recipe> {
        self.recipes.iter().filter(move |r| r.result == item)
    }

    /// Où se fait une recette, dit en entier : « À la main », « Établi », « Forge astrale ultime »…
    pub fn place_name(&self, at: Option<StationAt>) -> &str {
        match at.and_then(|at| self.stations.get(at.station).map(|s| s.level_name(at.level))) {
            Some(name) => name,
            None => "À la main",
        }
    }
}

/// Recette qui ne tient pas debout
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RecipeError {
    #[error("elle ne donne rien (count à 0)")]
    NoCount,
    #[error("aucun ingrédient")]
    NoIngredient,
    #[error("ce bloc ne se garde pas dans l'inventaire")]
    BlockNotKept,
    #[error("l'établi « {0} » n'existe pas")]
    UnknownStation(String),
    #[error("niveau {level} invalide pour {station} (de 1 à {max})")]
    Level { level: u8, station: String, max: u8 },
    #[error("un niveau sans établi")]
    LevelWithoutStation,
    #[error("« {0} » : il en faut au moins un")]
    ZeroCount(String),
    #[error("l'ingrédient « {0} » n'existe pas")]
    UnknownIngredient(String),
}

/// Fiche d'établi impossible à lire
#[derive(Debug, thiserror::Error)]
enum StationSheetError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

fn resolve(sheet: &RecipeSheet, result: ItemId, stations: &[Station], find: &impl Fn(&str) -> Option<ItemId>, blocks: &BlockRegistry) -> Result<Recipe, RecipeError> {
    if sheet.count == 0 {
        return Err(RecipeError::NoCount);
    }
    if sheet.ingredients.0.is_empty() {
        return Err(RecipeError::NoIngredient);
    }
    if let Some(block) = result.block() {
        if blocks.drop_of(block) != Some(block) {
            return Err(RecipeError::BlockNotKept);
        }
    }
    let station = match &sheet.station {
        None => None,
        Some(id) => {
            let index = stations.iter().position(|s| &s.id == id).ok_or_else(|| RecipeError::UnknownStation(id.clone()))?;
            let max = stations[index].max_level();
            if !(1..=max).contains(&sheet.level) {
                return Err(RecipeError::Level { level: sheet.level, station: id.clone(), max });
            }
            Some(StationAt { station: index, level: sheet.level })
        }
    };
    if station.is_none() && sheet.level != 1 {
        return Err(RecipeError::LevelWithoutStation);
    }
    let mut ingredients = Vec::new();
    for (key, count) in &sheet.ingredients.0 {
        if *count == 0 {
            return Err(RecipeError::ZeroCount(key.clone()));
        }
        let options = key
            .split('|')
            .map(|id| find(id.trim()).ok_or_else(|| RecipeError::UnknownIngredient(id.trim().to_string())))
            .collect::<Result<Vec<_>, _>>()?;
        ingredients.push(Ingredient { options, count: *count });
    }
    Ok(Recipe { result, count: sheet.count, station, ingredients })
}

/// Lit chaque fiche `*.json` du dossier des établis
pub fn load_station_sheets(dir: &Path) -> io::Result<(Vec<StationSheet>, Vec<String>)> {
    let mut sheets = Vec::new();
    let mut errors = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }
        let id = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
        let parsed = std::fs::read_to_string(&path).map_err(StationSheetError::from).and_then(|text| Ok(serde_json::from_str::<StationSheet>(&text)?));
        match parsed {
            Ok(mut sheet) => {
                sheet.id = id;
                sheets.push(sheet);
            }
            Err(e) => errors.push(format!("Établi {} ignoré : {e}", path.display())),
        }
    }
    sheets.sort_by(|a, b| a.id.cmp(&b.id));
    Ok((sheets, errors))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bundled() -> (CraftingBook, Vec<String>) {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(STATIONS_DIR);
        let (sheets, mut errors) = load_station_sheets(&dir).unwrap();
        let (book, more) = CraftingBook::build(&sheets, blocks(), items());
        errors.extend(more);
        (book, errors)
    }

    #[test]
    fn test_bundled_recipes_and_stations_load() {
        let (book, errors) = bundled();
        assert!(errors.is_empty(), "{errors:#?}");
        assert!(book.recipes.len() > 400, "{} recettes", book.recipes.len());
        let forge = book.station("astralForge").unwrap();
        assert_eq!(book.stations[forge].max_level(), 2);
        let ultimate = blocks().find("ultimateAstralForge").unwrap();
        assert_eq!(book.station_of(ultimate), Some(StationAt { station: forge, level: 2 }));
        assert_eq!(book.place_name(book.station_of(ultimate)), "Forge astrale ultime");
        assert_eq!(book.place_name(None), "À la main");
        assert_eq!(book.station_of(BlockId::STONE), None);
    }

    #[test]
    fn test_recipes_follow_station_and_level() {
        let (book, _) = bundled();
        let planks = book.recipes_for(blocks().find("plank").unwrap().into()).next().unwrap();
        assert_eq!(planks.station, None, "les planches se font à la main");
        assert_eq!(planks.count, 4);
        assert_eq!(planks.ingredients[0].options.len(), 3, "chêne, bouleau ou pin");

        let pickaxe = book.recipes_for(items().find("bronzePickaxe").unwrap()).next().unwrap();
        let workbench = book.station_of(blocks().find("workbench").unwrap());
        let furnace = book.station_of(blocks().find("furnace").unwrap());
        assert!(pickaxe.is_available_at(workbench));
        assert!(!pickaxe.is_available_at(None));
        assert!(!pickaxe.is_available_at(furnace));
        assert!(planks.is_available_at(workbench), "la main suit partout");

        // Une recette de la forge ultime ne se fait pas à la forge astrale, l'inverse oui
        let forge = book.station_of(blocks().find("astralForge").unwrap()).unwrap();
        let ultimate = StationAt { level: 2, ..forge };
        let level2 = book.recipes.iter().find(|r| r.station == Some(ultimate)).unwrap();
        let level1 = book.recipes.iter().find(|r| r.station == Some(forge)).unwrap();
        assert!(!level2.is_available_at(Some(forge)));
        assert!(level2.is_available_at(Some(ultimate)));
        assert!(level1.is_available_at(Some(ultimate)));
    }

    #[test]
    fn test_invalid_recipes_are_explained() {
        let stations = [Station { id: "workbench".into(), name: "Établi".into(), description: String::new(), levels: vec![BlockId(23)] }];
        let find = |id: &str| (id == "stick").then_some(ItemId(20000));
        let sheet = |json: &str| serde_json::from_str::<RecipeSheet>(json).unwrap();
        let ok = sheet(r#"{"station": "workbench", "ingredients": {"stick": 2}}"#);
        let recipe = resolve(&ok, ItemId(20001), &stations, &find, blocks()).unwrap();
        assert_eq!(recipe.ingredients, vec![Ingredient { options: vec![ItemId(20000)], count: 2 }]);
        assert_eq!(recipe.station, Some(StationAt { station: 0, level: 1 }));
        let err = |json: &str| resolve(&sheet(json), ItemId(20001), &stations, &find, blocks()).unwrap_err();
        assert_eq!(err(r#"{"station": "four", "ingredients": {"stick": 2}}"#), RecipeError::UnknownStation("four".into()));
        assert!(matches!(err(r#"{"station": "workbench", "level": 2, "ingredients": {"stick": 2}}"#), RecipeError::Level { level: 2, .. }));
        assert_eq!(err(r#"{"ingredients": {"caillou": 2}}"#), RecipeError::UnknownIngredient("caillou".into()));
        assert_eq!(err(r#"{"ingredients": {}}"#), RecipeError::NoIngredient);
        assert!(serde_json::from_str::<RecipeSheet>(r#"{"ingredients": {"stick": 2}, "etabli": "x"}"#).is_err());
        // L'ordre de la fiche est gardé
        let order = sheet(r#"{"ingredients": {"zinc": 1, "argent": 2}}"#);
        assert_eq!(order.ingredients.0, vec![("zinc".to_string(), 1), ("argent".to_string(), 2)]);
    }
}
