//! Butin : ce qu'un bloc cassé ou une bête tuée laisse tomber en plus (fibre des herbes hautes,
//! silex du gravier, récoltes des plantes sauvages, viande et peaux des bêtes…).
//!
//! Une fiche de bloc (`assets/blocks/`) ou de créature (`assets/creatures/`) l'écrit dans son champ
//! `loot` : une liste d'objets, chacun avec son nombre et sa chance de tomber.
//!
//! ```json
//! "loot": [
//!   {"item": "fibre", "chance": 0.5},
//!   {"item": "grainesBle", "count": [1, 2], "chance": 0.1}
//! ]
//! ```

use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Deserializer};

use crate::blocks::blocks;
use crate::items::{items, ItemId};
use crate::voxel::BlockId;
use crate::CreatureRegistry;

/// Une ligne de butin telle qu'écrite dans une fiche
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LootEntry {
    /// Objet ou bloc donné (identifiant de sa fiche)
    pub item: String,
    /// Nombre donné : un nombre fixe, ou `[min, max]` tiré au hasard (1 par défaut)
    #[serde(default)]
    pub count: LootCount,
    /// Probabilité de tomber, de 0 (exclu) à 1 (par défaut)
    #[serde(default = "always")]
    pub chance: f32,
}

fn always() -> f32 {
    1.0
}

/// Nombre d'exemplaires d'une ligne de butin, de `min` à `max` compris
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LootCount {
    pub min: u16,
    pub max: u16,
}

impl Default for LootCount {
    fn default() -> Self {
        Self { min: 1, max: 1 }
    }
}

impl<'de> Deserialize<'de> for LootCount {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Written {
            Fixed(u16),
            Range([u16; 2]),
        }
        Ok(match Written::deserialize(deserializer)? {
            Written::Fixed(n) => LootCount { min: n, max: n },
            Written::Range([min, max]) => LootCount { min, max },
        })
    }
}

/// Ligne de butin qui ne tient pas debout (le nom de l'objet en paramètre)
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum LootError {
    #[error("butin « {0} » : chance invalide (de 0 exclu à 1)")]
    Chance(String),
    #[error("butin « {0} » : nombre invalide (au moins 1, min ≤ max)")]
    Count(String),
}

/// Vérifie qu'une liste de butin tient debout (sans regarder si les objets existent)
pub(crate) fn check_entries(entries: &[LootEntry]) -> Result<(), LootError> {
    for entry in entries {
        if !(entry.chance > 0.0 && entry.chance <= 1.0) {
            return Err(LootError::Chance(entry.item.clone()));
        }
        if entry.count.max == 0 || entry.count.min > entry.count.max {
            return Err(LootError::Count(entry.item.clone()));
        }
    }
    Ok(())
}

/// Une ligne de butin prête à tirer
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LootRoll {
    pub item: ItemId,
    pub count: LootCount,
    pub chance: f32,
}

/// Le butin d'un bloc ou d'une espèce
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LootTable(pub Vec<LootRoll>);

/// Objet ou bloc d'identifiant `id`
pub fn find_item(id: &str) -> Option<ItemId> {
    blocks().find(id).map(ItemId::from).or_else(|| items().find(id))
}

impl LootTable {
    /// Résout les identifiants des lignes. Une ligne qui cite un objet inconnu est signalée et
    /// laissée de côté.
    pub fn resolve(entries: &[LootEntry], owner: &str) -> (Self, Vec<String>) {
        let mut rolls = Vec::new();
        let mut errors = Vec::new();
        for entry in entries {
            match find_item(&entry.item) {
                Some(item) => rolls.push(LootRoll { item, count: entry.count, chance: entry.chance }),
                None => errors.push(format!("{owner} : l'objet de butin « {} » n'existe pas", entry.item)),
            }
        }
        (Self(rolls), errors)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Tire le butin : chaque ligne tombe selon sa chance, avec un nombre tiré entre son min et
    /// son max. Renvoie les objets tombés et leur nombre.
    pub fn roll(&self, rng: &mut LootRng) -> Vec<(ItemId, u16)> {
        let mut out = Vec::new();
        for roll in &self.0 {
            if roll.chance < 1.0 && rng.next_f32() >= roll.chance {
                continue;
            }
            let span = (roll.count.max - roll.count.min) as u32 + 1;
            let count = roll.count.min + (rng.next_u64() % span as u64) as u16;
            if count > 0 {
                out.push((roll.item, count));
            }
        }
        out
    }
}

/// Petit générateur pseudo-aléatoire (SplitMix64) pour tirer le butin
#[derive(Debug, Clone)]
pub struct LootRng(u64);

impl LootRng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    /// Graine tirée de l'horloge : un tirage différent à chaque partie
    pub fn from_clock() -> Self {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
        Self(nanos ^ 0xD1B5_4A32_D192_ED03)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Nombre dans [0, 1)
    pub(crate) fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }
}

/// Butin de chaque bloc, rangé par numéro de fiche
fn block_tables() -> &'static Vec<LootTable> {
    static TABLES: OnceLock<Vec<LootTable>> = OnceLock::new();
    TABLES.get_or_init(|| {
        let registry = blocks();
        let mut tables: Vec<LootTable> = Vec::new();
        for sheet in registry.iter().filter(|s| !s.loot.is_empty()) {
            let (table, errors) = LootTable::resolve(&sheet.loot, &format!("Bloc {}", sheet.id));
            for error in errors {
                log::warn!("{error}");
            }
            let index = sheet.numeric_id as usize;
            if tables.len() <= index {
                tables.resize(index + 1, LootTable::default());
            }
            tables[index] = table;
        }
        tables
    })
}

/// Butin d'un bloc cassé, en plus de ce que donne son champ `drop`
pub fn block_loot(block: BlockId) -> &'static LootTable {
    static EMPTY: LootTable = LootTable(Vec::new());
    block_tables().get(block.kind().0 as usize).unwrap_or(&EMPTY)
}

/// Butin d'une espèce de créature (rang dans le registre)
pub fn creature_loot(registry: &CreatureRegistry, species: usize) -> LootTable {
    let Some(sheet) = registry.get(species) else { return LootTable::default() };
    let (table, errors) = LootTable::resolve(&sheet.loot, &format!("Créature {}", sheet.id));
    for error in errors {
        log::warn!("{error}");
    }
    table
}

/// Tous les objets (hors blocs) qui peuvent tomber au sol : butin des blocs et des créatures. Le
/// rendu des objets au sol charge leurs icônes.
pub fn droppable_items() -> &'static [ItemId] {
    static ITEMS: OnceLock<Vec<ItemId>> = OnceLock::new();
    ITEMS.get_or_init(|| {
        let mut out: Vec<ItemId> = block_tables().iter().flat_map(|t| t.0.iter().map(|r| r.item)).collect();
        let creatures = CreatureRegistry::load_quiet();
        for species in 0..creatures.len() {
            let (table, _) = LootTable::resolve(creatures.get(species).map_or(&[][..], |s| &s.loot), "");
            out.extend(table.0.iter().map(|r| r.item));
        }
        out.retain(|item| item.block().is_none());
        out.sort_by_key(|item| item.0);
        out.dedup();
        out
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_is_fixed_or_a_range() {
        let entry: LootEntry = serde_json::from_str(r#"{"item": "fibre"}"#).unwrap();
        assert_eq!((entry.count, entry.chance), (LootCount { min: 1, max: 1 }, 1.0));
        let entry: LootEntry = serde_json::from_str(r#"{"item": "fibre", "count": 3}"#).unwrap();
        assert_eq!(entry.count, LootCount { min: 3, max: 3 });
        let entry: LootEntry = serde_json::from_str(r#"{"item": "fibre", "count": [1, 3], "chance": 0.5}"#).unwrap();
        assert_eq!((entry.count, entry.chance), (LootCount { min: 1, max: 3 }, 0.5));
        assert!(check_entries(std::slice::from_ref(&entry)).is_ok());
        assert!(matches!(check_entries(&[LootEntry { chance: 0.0, ..entry.clone() }]), Err(LootError::Chance(_))));
        assert!(matches!(check_entries(&[LootEntry { count: LootCount { min: 3, max: 1 }, ..entry }]), Err(LootError::Count(_))));
    }

    #[test]
    fn test_roll_follows_chances_and_counts() {
        let fibre = items().find("fibre").unwrap();
        let flint = items().find("flint").unwrap();
        let table = LootTable(vec![
            LootRoll { item: fibre, count: LootCount { min: 1, max: 3 }, chance: 1.0 },
            LootRoll { item: flint, count: LootCount { min: 1, max: 1 }, chance: 0.25 },
        ]);
        let mut rng = LootRng::new(42);
        let (mut fibres, mut flints, mut seen) = (0u32, 0u32, [false; 4]);
        for _ in 0..4000 {
            for (item, count) in table.roll(&mut rng) {
                if item == fibre {
                    assert!((1..=3).contains(&count));
                    seen[count as usize] = true;
                    fibres += 1;
                } else {
                    assert_eq!((item, count), (flint, 1));
                    flints += 1;
                }
            }
        }
        assert_eq!(fibres, 4000, "une chance de 1 tombe toujours");
        assert!(seen[1] && seen[2] && seen[3], "tout le compte est tiré");
        assert!((800..1200).contains(&flints), "un quart des fois : {flints}");
    }

    /// Les fiches citent des objets qui existent, et les matières premières des recettes
    /// s'obtiennent toutes dans le monde
    #[test]
    fn test_loot_of_sheets_resolves_and_covers_raw_materials() {
        let mut found = Vec::new();
        for sheet in blocks().iter() {
            let (table, errors) = LootTable::resolve(&sheet.loot, &sheet.id);
            assert!(errors.is_empty(), "{errors:?}");
            found.extend(table.0.iter().map(|r| r.item));
        }
        let creatures = CreatureRegistry::load_quiet();
        for sheet in creatures.iter() {
            let (table, errors) = LootTable::resolve(&sheet.loot, &sheet.id);
            assert!(errors.is_empty(), "{errors:?}");
            found.extend(table.0.iter().map(|r| r.item));
        }
        for id in ["fibre", "flint", "hide", "bone", "ble", "carotte", "pommeDeTerre", "oignon", "riz", "soja", "courge", "viandeVache", "laine", "plumes"] {
            let item = items().find(id).unwrap();
            assert!(found.contains(&item), "{id} ne tombe de rien");
        }
        assert!(droppable_items().contains(&items().find("fibre").unwrap()));
        assert_eq!(block_loot(blocks().find("tallGrass1").unwrap()).0[0].item, items().find("fibre").unwrap());
    }
}
