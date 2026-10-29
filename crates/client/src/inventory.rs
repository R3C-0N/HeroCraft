//! Inventaire du joueur : 9 cases de barre d'objets (touches 1 à 9, molette) et 27 cases de sac.
//! Les blocs cassés y entrent, le bloc posé au clic droit est pris dans la case choisie de la
//! barre. Enregistré en JSON avec le monde.

use std::path::{Path, PathBuf};

use herocraft_common::BlockId;
use serde::{Deserialize, Serialize};

/// Cases de la barre d'objets, en bas de l'écran
pub const HOTBAR_SLOTS: usize = 9;
/// Cases du sac, visibles dans l'écran d'inventaire
pub const BAG_SLOTS: usize = 27;
pub const TOTAL_SLOTS: usize = HOTBAR_SLOTS + BAG_SLOTS;
/// Blocs au plus par case
pub const MAX_STACK: u16 = 64;

/// Pile de blocs identiques dans une case
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemStack {
    pub block: BlockId,
    pub count: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Inventory {
    /// Barre d'objets (cases 0 à 8) puis sac
    slots: Vec<Option<ItemStack>>,
    /// Case choisie de la barre d'objets
    selected: usize,
    /// Pile prise à la souris dans l'écran d'inventaire, et sa case d'origine
    #[serde(skip)]
    held: Option<(ItemStack, usize)>,
    #[serde(skip)]
    path: Option<PathBuf>,
}

impl Default for Inventory {
    fn default() -> Self {
        Self { slots: vec![None; TOTAL_SLOTS], selected: 0, held: None, path: None }
    }
}

impl Inventory {
    /// Charge l'inventaire depuis `path`, ou un inventaire vide si le fichier est absent ou
    /// illisible. Les sauvegardes suivantes écrivent dans `path`.
    pub fn load_or_default(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();
        let mut inventory = match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str::<Inventory>(&text).unwrap_or_else(|e| {
                log::warn!("Inventaire illisible dans {} ({}), inventaire vide.", path.display(), e);
                Inventory::default()
            }),
            Err(_) => Inventory::default(),
        };
        inventory.repair();
        inventory.path = Some(path.to_path_buf());
        inventory
    }

    /// Remet d'aplomb un fichier ancien ou édité à la main
    fn repair(&mut self) {
        self.slots.resize(TOTAL_SLOTS, None);
        for slot in &mut self.slots {
            if slot.is_some_and(|s| s.count == 0 || drop_for(s.block) != Some(s.block)) {
                *slot = None;
            } else if let Some(stack) = slot {
                stack.count = stack.count.min(MAX_STACK);
            }
        }
        self.selected = self.selected.min(HOTBAR_SLOTS - 1);
    }

    pub fn save(&self) {
        let Some(path) = &self.path else { return };
        let result = (|| -> std::io::Result<()> {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
            std::fs::write(path, text)
        })();
        if let Err(e) = result {
            log::warn!("Impossible d'enregistrer l'inventaire dans {} : {}", path.display(), e);
        }
    }

    pub fn slot(&self, index: usize) -> Option<ItemStack> {
        self.slots.get(index).copied().flatten()
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn select(&mut self, index: usize) {
        if index < HOTBAR_SLOTS {
            self.selected = index;
        }
    }

    /// Molette : `steps` positif avance vers la droite de la barre, en bouclant
    pub fn scroll(&mut self, steps: i32) {
        self.selected = (self.selected as i32 + steps).rem_euclid(HOTBAR_SLOTS as i32) as usize;
    }

    /// Bloc de la case choisie, à poser au clic droit
    pub fn selected_block(&self) -> Option<BlockId> {
        self.slot(self.selected).map(|s| s.block)
    }

    /// Retire un bloc de la case choisie (bloc posé)
    pub fn take_selected(&mut self) -> Option<BlockId> {
        let slot = &mut self.slots[self.selected];
        let stack = slot.as_mut()?;
        let block = stack.block;
        stack.count -= 1;
        if stack.count == 0 {
            *slot = None;
        }
        Some(block)
    }

    /// Range un bloc : d'abord sur une pile du même bloc, sinon dans la première case vide (barre
    /// d'objets puis sac). Renvoie faux si l'inventaire est plein.
    pub fn add(&mut self, block: BlockId) -> bool {
        let stackable = self.slots.iter().position(|s| s.is_some_and(|s| s.block == block && s.count < MAX_STACK));
        if let Some(stack) = stackable.and_then(|i| self.slots[i].as_mut()) {
            stack.count += 1;
            return true;
        }
        match self.slots.iter().position(Option::is_none) {
            Some(i) => {
                self.slots[i] = Some(ItemStack { block, count: 1 });
                true
            }
            None => false,
        }
    }

    /// Nombre total d'un bloc dans l'inventaire
    #[allow(dead_code)]
    pub fn count(&self, block: BlockId) -> u32 {
        self.slots.iter().flatten().filter(|s| s.block == block).map(|s| s.count as u32).sum()
    }

    /// Pile tenue à la souris dans l'écran d'inventaire
    pub fn held(&self) -> Option<ItemStack> {
        self.held.map(|(stack, _)| stack)
    }

    /// Clic gauche sur une case de l'écran d'inventaire : prend la pile, la pose dans une case
    /// vide, la fusionne avec une pile du même bloc ou l'échange avec la pile qui s'y trouve
    pub fn click(&mut self, index: usize) {
        if index >= TOTAL_SLOTS {
            return;
        }
        let Some((mut held, origin)) = self.held.take() else {
            self.held = self.slots[index].take().map(|stack| (stack, index));
            return;
        };
        match self.slots[index].as_mut() {
            None => self.slots[index] = Some(held),
            Some(stack) if stack.block == held.block => {
                let moved = held.count.min(MAX_STACK - stack.count);
                stack.count += moved;
                held.count -= moved;
                if held.count > 0 {
                    self.held = Some((held, origin));
                }
            }
            Some(stack) => {
                let swapped = std::mem::replace(stack, held);
                self.held = Some((swapped, index));
            }
        }
    }

    /// Clic droit : prend la moitié d'une pile, ou pose un seul bloc de la pile tenue
    pub fn click_secondary(&mut self, index: usize) {
        if index >= TOTAL_SLOTS {
            return;
        }
        match (self.held.as_mut(), self.slots[index].as_mut()) {
            (None, Some(stack)) => {
                let half = stack.count.div_ceil(2);
                stack.count -= half;
                self.held = Some((ItemStack { block: stack.block, count: half }, index));
                if stack.count == 0 {
                    self.slots[index] = None;
                }
            }
            (Some((held, _)), slot) => {
                match slot {
                    None => self.slots[index] = Some(ItemStack { block: held.block, count: 1 }),
                    Some(stack) if stack.block == held.block && stack.count < MAX_STACK => stack.count += 1,
                    Some(_) => return,
                }
                held.count -= 1;
                if held.count == 0 {
                    self.held = None;
                }
            }
            (None, None) => {}
        }
    }

    /// Écran d'inventaire refermé : la pile tenue revient à sa case d'origine, ou à défaut
    /// ailleurs dans l'inventaire
    pub fn return_held(&mut self) {
        let Some((held, origin)) = self.held.take() else { return };
        let mut remaining = held.count;
        if let Some(stack) = self.slots[origin].as_mut().filter(|s| s.block == held.block) {
            let moved = remaining.min(MAX_STACK - stack.count);
            stack.count += moved;
            remaining -= moved;
        } else if self.slots[origin].is_none() {
            self.slots[origin] = Some(held);
            remaining = 0;
        }
        while remaining > 0 {
            if !self.add(held.block) {
                log::warn!("Inventaire plein : {} × {} perdus.", remaining, block_name(held.block));
                break;
            }
            remaining -= 1;
        }
    }
}

/// Bloc gagné en cassant `block` (l'herbe donne de la terre, comme dans Minecraft). Les liquides
/// ne se ramassent pas.
pub fn drop_for(block: BlockId) -> Option<BlockId> {
    match block {
        BlockId::AIR | BlockId::WATER | BlockId::LAVA => None,
        BlockId::GRASS => Some(BlockId::DIRT),
        other => Some(other),
    }
}

/// Nom affiché d'un bloc
pub fn block_name(block: BlockId) -> &'static str {
    match block {
        BlockId::AIR => "Air",
        BlockId::STONE => "Pierre",
        BlockId::DIRT => "Terre",
        BlockId::GRASS => "Herbe",
        BlockId::SAND => "Sable",
        BlockId::SNOW => "Neige",
        BlockId::WATER => "Eau",
        BlockId::LAVA => "Lave",
        BlockId::WOOD => "Bois",
        BlockId::LEAVES => "Feuilles",
        BlockId::IRON_ORE => "Minerai de fer",
        BlockId::GOLD_ORE => "Minerai d'or",
        BlockId::CRYSTAL => "Cristal",
        BlockId::OBSIDIAN => "Obsidienne",
        _ => "Bloc inconnu",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_stacks_then_fills_hotbar_then_bag() {
        let mut inv = Inventory::default();
        for _ in 0..MAX_STACK + 1 {
            assert!(inv.add(BlockId::STONE));
        }
        assert_eq!(inv.slot(0), Some(ItemStack { block: BlockId::STONE, count: MAX_STACK }));
        assert_eq!(inv.slot(1), Some(ItemStack { block: BlockId::STONE, count: 1 }));
        assert!(inv.add(BlockId::DIRT));
        assert_eq!(inv.slot(2).map(|s| s.block), Some(BlockId::DIRT));
        assert_eq!(inv.count(BlockId::STONE), MAX_STACK as u32 + 1);

        // Toutes les cases prises par des piles pleines : plus rien n'entre
        let mut full = Inventory::default();
        for _ in 0..TOTAL_SLOTS * MAX_STACK as usize {
            assert!(full.add(BlockId::SAND));
        }
        assert!(full.slot(HOTBAR_SLOTS).is_some(), "le sac prend la suite de la barre");
        assert!(!full.add(BlockId::SAND));
    }

    #[test]
    fn test_selection_and_placing_from_hotbar() {
        let mut inv = Inventory::default();
        inv.add(BlockId::STONE);
        inv.add(BlockId::STONE);
        inv.add(BlockId::DIRT);
        assert_eq!(inv.selected_block(), Some(BlockId::STONE));
        inv.scroll(1);
        assert_eq!(inv.selected_block(), Some(BlockId::DIRT));
        inv.scroll(-2);
        assert_eq!(inv.selected(), HOTBAR_SLOTS - 1, "la molette boucle");
        assert_eq!(inv.take_selected(), None, "case vide : rien à poser");
        inv.select(0);
        assert_eq!(inv.take_selected(), Some(BlockId::STONE));
        assert_eq!(inv.take_selected(), Some(BlockId::STONE));
        assert_eq!(inv.selected_block(), None, "la case se vide");
        inv.select(42);
        assert_eq!(inv.selected(), 0);
    }

    #[test]
    fn test_drops() {
        assert_eq!(drop_for(BlockId::STONE), Some(BlockId::STONE));
        assert_eq!(drop_for(BlockId::GRASS), Some(BlockId::DIRT));
        assert_eq!(drop_for(BlockId::WATER), None);
        assert_eq!(drop_for(BlockId::LAVA), None);
    }

    #[test]
    fn test_moving_stacks_with_the_mouse() {
        let mut inv = Inventory::default();
        inv.add(BlockId::STONE);
        inv.add(BlockId::DIRT);
        // Prendre la pierre et la poser dans le sac
        inv.click(0);
        assert_eq!(inv.held().map(|s| s.block), Some(BlockId::STONE));
        assert_eq!(inv.slot(0), None);
        inv.click(20);
        assert_eq!(inv.slot(20).map(|s| s.block), Some(BlockId::STONE));
        assert_eq!(inv.held(), None);
        // Échange : la terre prend la place de la pierre, qui reste en main
        inv.click(1);
        inv.click(20);
        assert_eq!(inv.slot(20).map(|s| s.block), Some(BlockId::DIRT));
        assert_eq!(inv.held().map(|s| s.block), Some(BlockId::STONE));
        // Refermer l'écran remet la pile tenue dans l'inventaire
        inv.return_held();
        assert_eq!(inv.held(), None);
        assert_eq!(inv.count(BlockId::STONE), 1);
        assert_eq!(inv.count(BlockId::DIRT), 1);
    }

    #[test]
    fn test_merge_and_split_with_the_mouse() {
        let mut inv = Inventory::default();
        for _ in 0..10 {
            inv.add(BlockId::STONE);
        }
        // Clic droit : la moitié en main
        inv.click_secondary(0);
        assert_eq!(inv.held().map(|s| s.count), Some(5));
        assert_eq!(inv.slot(0).map(|s| s.count), Some(5));
        // Clic droit en tenant : un seul bloc posé
        inv.click_secondary(3);
        assert_eq!(inv.slot(3).map(|s| s.count), Some(1));
        assert_eq!(inv.held().map(|s| s.count), Some(4));
        // Clic gauche sur une pile du même bloc : fusion
        inv.click(0);
        assert_eq!(inv.slot(0).map(|s| s.count), Some(9));
        assert_eq!(inv.held(), None);
        assert_eq!(inv.count(BlockId::STONE), 10);
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let dir = std::env::temp_dir().join(format!("herocraft_inventory_{}", std::process::id()));
        let path = dir.join("inventaire.json");
        let mut inv = Inventory::load_or_default(&path);
        assert_eq!(inv.slot(0), None);
        inv.add(BlockId::SAND);
        inv.add(BlockId::SAND);
        inv.select(4);
        inv.save();

        let loaded = Inventory::load_or_default(&path);
        assert_eq!(loaded.slot(0), Some(ItemStack { block: BlockId::SAND, count: 2 }));
        assert_eq!(loaded.selected(), 4);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_repair_drops_invalid_slots() {
        let json = r#"{ "slots": [ { "block": 6, "count": 3 }, { "block": 1, "count": 500 } ], "selected": 99 }"#;
        let mut inv: Inventory = serde_json::from_str(json).unwrap();
        inv.repair();
        assert_eq!(inv.slot(0), None, "l'eau ne se garde pas");
        assert_eq!(inv.slot(1).map(|s| s.count), Some(MAX_STACK));
        assert_eq!(inv.selected(), HOTBAR_SLOTS - 1);
        assert_eq!(inv.slots.len(), TOTAL_SLOTS);
    }
}
