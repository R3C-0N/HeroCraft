//! Cultures : chaque plante cultivée est décrite par un fichier JSON du dossier `assets/cultures/`
//! (graine, blocs de ses stades de pousse, durée d'un stade, récolte), lu une fois au lancement.
//!
//! La houe change l'herbe ou la terre en terre labourée (humide près de l'eau) ; une graine se sème
//! sur la terre labourée et y pose le premier stade de sa plante, qui passe ensuite d'un stade à
//! l'autre (`herocraft_world::farming` tient le compte du temps). Ce que donne la plante cassée est
//! décrit dans les fiches de ses blocs. Le format est décrit dans `assets/cultures/README.md`.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::Deserialize;

use crate::blocks::blocks;
use crate::items::{ItemId, items};
use crate::voxel::BlockId;

/// Dossier des fiches de cultures, depuis le dossier de lancement
pub const CROPS_DIR: &str = "assets/cultures";
/// Bloc de la terre labourée sèche, et de la terre labourée humide (près de l'eau)
pub const FARMLAND: &str = "terreLabouree";
pub const WET_FARMLAND: &str = "terreLaboureeHumide";
/// Une terre labourée est humide s'il y a de l'eau à moins de tant de blocs (à sa hauteur ou juste
/// en dessous)
pub const WATER_REACH_BLOCKS: i32 = 4;
/// Sur une terre sèche, une plante pousse d'autant moins vite
pub const DRY_GROWTH: f32 = 0.5;

/// Une culture, telle que décrite par sa fiche
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CropSheet {
    /// Identifiant : le nom du fichier sans `.json` (ex. `ble`)
    #[serde(skip)]
    pub id: String,
    /// Nom affiché
    pub name: String,
    /// Objet qui la sème (identifiant de sa fiche)
    pub seed: String,
    /// Blocs de ses stades, du semis à la plante mûre (au moins deux)
    pub stages: Vec<String>,
    /// Durée d'un stade sur une terre humide, en minutes (deux fois plus sur une terre sèche)
    pub stage_minutes: f32,
    /// Ne pousse que sur une terre labourée humide (riz)
    #[serde(default)]
    pub needs_water: bool,
}

/// Fiche de culture impossible à lire ou qui ne tient pas debout
#[derive(Debug, thiserror::Error)]
pub enum CropSheetError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("il faut au moins deux stades (le semis et la plante mûre)")]
    TooFewStages,
    #[error("durée d'un stade invalide (positive, en minutes)")]
    StageMinutes,
}

impl CropSheet {
    /// Lit la fiche JSON d'une culture et vérifie qu'elle tient debout
    pub fn parse(id: &str, json: &str) -> Result<Self, CropSheetError> {
        let mut sheet: CropSheet = serde_json::from_str(json)?;
        sheet.id = id.to_string();
        if sheet.stages.len() < 2 {
            return Err(CropSheetError::TooFewStages);
        }
        if sheet.stage_minutes.is_nan() || sheet.stage_minutes <= 0.0 {
            return Err(CropSheetError::StageMinutes);
        }
        Ok(sheet)
    }
}

/// Une culture, ses blocs et objets résolus en numéros
#[derive(Debug, Clone)]
pub struct Crop {
    pub id: String,
    pub name: String,
    pub seed: ItemId,
    /// Blocs des stades, du semis à la plante mûre
    pub stages: Vec<BlockId>,
    /// Durée d'un stade sur une terre humide, en secondes
    pub stage_seconds: f32,
    pub needs_water: bool,
}

impl Crop {
    /// Bloc mûr
    pub fn mature(&self) -> BlockId {
        *self.stages.last().expect("au moins deux stades")
    }

    /// Pousse-t-elle sur ce bloc ?
    pub fn grows_on(&self, soil: BlockId, registry: &CropRegistry) -> bool {
        match registry.soil_kind(soil) {
            Some(wet) => wet || !self.needs_water,
            None => false,
        }
    }
}

/// Toutes les cultures connues
#[derive(Debug, Clone, Default)]
pub struct CropRegistry {
    crops: Vec<Crop>,
    /// Culture et stade de chaque bloc de stade
    stages: HashMap<u16, (usize, u8)>,
    /// Culture semée par chaque graine
    seeds: HashMap<ItemId, usize>,
    farmland: Option<BlockId>,
    wet_farmland: Option<BlockId>,
}

static CROPS: OnceLock<CropRegistry> = OnceLock::new();

/// Les cultures du jeu, lues dans `assets/cultures/` au premier appel
pub fn cultures() -> &'static CropRegistry {
    CROPS.get_or_init(CropRegistry::load_default)
}

impl CropRegistry {
    /// Les cultures de `assets/cultures/` : depuis le dossier de lancement, sinon celles livrées
    /// avec les sources. Un fichier illisible est signalé et ignoré.
    pub fn load_default() -> Self {
        let candidates = [PathBuf::from(CROPS_DIR), Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(CROPS_DIR)];
        let Some(dir) = candidates.iter().find(|dir| dir.is_dir()) else {
            log::warn!("Dossier {CROPS_DIR}/ introuvable : aucune culture.");
            return Self::from_sheets(Vec::new()).0;
        };
        match Self::load_dir(dir) {
            Ok((registry, errors)) => {
                for error in errors {
                    log::warn!("{error}");
                }
                log::info!("{} culture(s) chargée(s) depuis {}.", registry.len(), dir.display());
                registry
            }
            Err(e) => {
                log::warn!("Lecture de {} impossible : {e}", dir.display());
                Self::from_sheets(Vec::new()).0
            }
        }
    }

    /// Lit chaque fichier `*.json` du dossier. Renvoie les cultures valides et les erreurs des autres.
    pub fn load_dir(dir: &Path) -> io::Result<(Self, Vec<String>)> {
        let mut sheets = Vec::new();
        let mut errors = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            let path = entry?.path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let id = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
            match std::fs::read_to_string(&path).map_err(CropSheetError::from).and_then(|text| CropSheet::parse(&id, &text)) {
                Ok(sheet) => sheets.push(sheet),
                Err(e) => errors.push(format!("Culture {} ignorée : {e}", path.display())),
            }
        }
        let (registry, more) = Self::from_sheets(sheets);
        errors.extend(more);
        Ok((registry, errors))
    }

    /// Résout les fiches d'après les blocs et les objets du jeu. Une fiche qui cite un bloc ou un
    /// objet inconnu est écartée.
    pub fn from_sheets(mut sheets: Vec<CropSheet>) -> (Self, Vec<String>) {
        sheets.sort_by(|a, b| a.id.cmp(&b.id));
        let mut errors = Vec::new();
        let mut registry = Self { farmland: blocks().find(FARMLAND), wet_farmland: blocks().find(WET_FARMLAND), ..Self::default() };
        let thing = |id: &str| items().find(id).or_else(|| blocks().find(id).map(ItemId::from));
        for sheet in sheets {
            let Some(seed) = thing(&sheet.seed) else {
                errors.push(format!("Culture {} ignorée : graine « {} » inconnue", sheet.id, sheet.seed));
                continue;
            };
            let stages: Vec<BlockId> = sheet.stages.iter().map_while(|s| blocks().find(s)).collect();
            if let Some(unknown) = sheet.stages.get(stages.len()) {
                errors.push(format!("Culture {} ignorée : bloc « {unknown} » inconnu", sheet.id));
                continue;
            }
            if let Some(taken) = stages.iter().find(|s| registry.stages.contains_key(&s.0)) {
                errors.push(format!("Culture {} ignorée : le bloc {} sert déjà à une autre culture", sheet.id, blocks().name(*taken)));
                continue;
            }
            let index = registry.crops.len();
            for (stage, block) in stages.iter().enumerate() {
                registry.stages.insert(block.0, (index, stage as u8));
            }
            registry.seeds.insert(seed, index);
            registry.crops.push(Crop {
                id: sheet.id,
                name: sheet.name,
                seed,
                stages,
                stage_seconds: sheet.stage_minutes * 60.0,
                needs_water: sheet.needs_water,
            });
        }
        (registry, errors)
    }

    pub fn len(&self) -> usize {
        self.crops.len()
    }

    pub fn is_empty(&self) -> bool {
        self.crops.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<&Crop> {
        self.crops.get(index)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Crop> {
        self.crops.iter()
    }

    /// Culture (son rang) et stade du bloc, s'il est le stade d'une culture
    pub fn stage_of(&self, block: BlockId) -> Option<(usize, u8)> {
        self.stages.get(&block.kind().0).copied()
    }

    /// Le bloc est une plante cultivée, à n'importe quel stade
    pub fn is_crop(&self, block: BlockId) -> bool {
        self.stage_of(block).is_some()
    }

    /// Stade suivant du bloc, s'il est une plante qui n'est pas encore mûre
    pub fn next_stage(&self, block: BlockId) -> Option<BlockId> {
        let (crop, stage) = self.stage_of(block)?;
        self.crops[crop].stages.get(stage as usize + 1).copied()
    }

    /// Culture que sème cet objet
    pub fn crop_of_seed(&self, item: ItemId) -> Option<&Crop> {
        self.seeds.get(&item).map(|&i| &self.crops[i])
    }

    /// Terre labourée sèche ou humide
    pub fn farmland(&self, wet: bool) -> Option<BlockId> {
        if wet { self.wet_farmland.or(self.farmland) } else { self.farmland }
    }

    /// `Some(humide)` si le bloc est une terre labourée
    pub fn soil_kind(&self, block: BlockId) -> Option<bool> {
        let kind = block.kind();
        if Some(kind) == self.wet_farmland {
            Some(true)
        } else if Some(kind) == self.farmland {
            Some(false)
        } else {
            None
        }
    }

    pub fn is_farmland(&self, block: BlockId) -> bool {
        self.soil_kind(block).is_some()
    }

    /// La houe laboure ce bloc (herbe ou terre)
    pub fn is_tillable(&self, block: BlockId) -> bool {
        let kind = block.kind();
        kind == BlockId::GRASS || kind == BlockId::DIRT
    }

    /// L'objet est une houe (fiche dont l'identifiant finit par `Hoe` : `flintHoe`, `ironHoe`…)
    pub fn is_hoe(&self, item: ItemId) -> bool {
        items().get(item).is_some_and(|sheet| sheet.id.ends_with("Hoe"))
    }

    /// Un joueur peut changer `current` en `next` sans le casser d'abord : labourer (herbe ou
    /// terre en terre labourée), ou faire passer une plante au stade suivant. Le serveur s'en sert
    /// pour valider ce que les clients lui envoient.
    pub fn allows_change(&self, current: BlockId, next: BlockId) -> bool {
        (self.is_tillable(current) && self.is_farmland(next)) || self.next_stage(current) == Some(next)
    }

    /// Facteur de pousse sur ce sol : 1 sur une terre humide, `DRY_GROWTH` sur une sèche, 0 ailleurs
    /// (ou sur une terre sèche pour une plante qui veut de l'eau)
    pub fn growth_factor(&self, crop: &Crop, soil: BlockId) -> f32 {
        match self.soil_kind(soil) {
            Some(true) => 1.0,
            Some(false) if !crop.needs_water => DRY_GROWTH,
            _ => 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bundled_crops_load() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(CROPS_DIR);
        let (registry, errors) = CropRegistry::load_dir(&dir).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(registry.len(), 7);
        let seeds = items().find("grainesBle").unwrap();
        let wheat = registry.crop_of_seed(seeds).expect("les graines de blé sèment le blé");
        assert_eq!(wheat.stages.len(), 4);
        assert_eq!(registry.next_stage(wheat.stages[0]), Some(wheat.stages[1]));
        assert_eq!(registry.next_stage(wheat.mature()), None);
        assert_eq!(registry.stage_of(wheat.stages[2]).map(|(_, s)| s), Some(2));
        assert!(!registry.is_crop(BlockId::STONE));
        // Le riz ne pousse que les pieds dans l'eau
        let rice = registry.crop_of_seed(items().find("grainesRiz").unwrap()).unwrap();
        let (dry, wet) = (registry.farmland(false).unwrap(), registry.farmland(true).unwrap());
        assert!(rice.grows_on(wet, &registry) && !rice.grows_on(dry, &registry));
        assert!(wheat.grows_on(dry, &registry) && !wheat.grows_on(BlockId::DIRT, &registry));
        assert_eq!(registry.growth_factor(rice, dry), 0.0);
        assert_eq!(registry.growth_factor(wheat, dry), DRY_GROWTH);
    }

    #[test]
    fn test_hoe_tills_and_server_allows_it() {
        let registry = cultures();
        assert!(registry.is_hoe(items().find("flintHoe").unwrap()));
        assert!(!registry.is_hoe(items().find("flintSword").unwrap()));
        let farmland = registry.farmland(false).unwrap();
        assert!(registry.allows_change(BlockId::GRASS, farmland));
        assert!(registry.allows_change(BlockId::DIRT, registry.farmland(true).unwrap()));
        assert!(!registry.allows_change(BlockId::STONE, farmland));
        assert!(!registry.allows_change(BlockId::GRASS, BlockId::STONE));
        let wheat = registry.crop_of_seed(items().find("grainesBle").unwrap()).unwrap();
        assert!(registry.allows_change(wheat.stages[0], wheat.stages[1]));
        assert!(!registry.allows_change(wheat.stages[0], wheat.mature()));
    }

    #[test]
    fn test_invalid_sheets_are_explained() {
        let ok = r#"{ "name": "Blé", "seed": "grainesBle", "stages": ["bleStade1", "bleStade4"], "stage_minutes": 1 }"#;
        assert!(CropSheet::parse("ble", ok).is_ok());
        assert!(CropSheet::parse("ble", &ok.replace(r#""bleStade1", "#, "")).unwrap_err().to_string().contains("deux stades"));
        assert!(matches!(
            CropSheet::parse("ble", &ok.replace(r#""stage_minutes": 1"#, r#""stage_minutes": 0"#)),
            Err(CropSheetError::StageMinutes)
        ));
        assert!(matches!(CropSheet::parse("ble", &ok.replace("seed", "graine")), Err(CropSheetError::Json(_))));
        let unknown = CropSheet::parse("x", &ok.replace("bleStade4", "inconnu")).unwrap();
        let (registry, errors) = CropRegistry::from_sheets(vec![unknown]);
        assert!(registry.is_empty() && errors[0].contains("inconnu"));
    }
}
