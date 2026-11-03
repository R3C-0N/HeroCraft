//! Inventaire du joueur : 9 cases de barre d'objets (touches 1 à 9, molette) et 27 cases de sac.
//! Les blocs ramassés au sol y entrent, comme ce qui est fabriqué ; le bloc posé au clic droit est
//! pris dans la case choisie de la barre. Enregistré en JSON avec le monde.

use std::path::{Path, PathBuf};

use herocraft_common::{blocks, BlockId, Ingredient, ItemId, Recipe};
use serde::{Deserialize, Serialize};

/// Cases de la barre d'objets, en bas de l'écran
pub const HOTBAR_SLOTS: usize = 9;
/// Cases du sac, visibles dans l'écran d'inventaire
pub const BAG_SLOTS: usize = 27;
pub const TOTAL_SLOTS: usize = HOTBAR_SLOTS + BAG_SLOTS;
/// Pile de blocs ou d'objets identiques dans une case
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemStack {
    /// `block` dans les inventaires enregistrés avant les objets : les numéros sont les mêmes
    #[serde(alias = "block")]
    pub item: ItemId,
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
            if slot.is_some_and(|s| s.count == 0 || !can_hold(s.item)) {
                *slot = None;
            } else if let Some(stack) = slot {
                stack.count = stack.count.min(stack.item.max_stack());
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

    /// Bloc de la case choisie, à poser au clic droit (aucun si elle tient un objet)
    pub fn selected_block(&self) -> Option<BlockId> {
        self.slot(self.selected).and_then(|s| s.item.block())
    }

    /// Aliment de la case choisie et la satiété qu'il rend, à manger au clic droit
    pub fn selected_food(&self) -> Option<(ItemId, f32)> {
        let item = self.slot(self.selected)?.item;
        Some((item, item.satiety()?))
    }

    /// Retire un objet de la case choisie (aliment mangé)
    pub fn consume_selected(&mut self) -> Option<ItemId> {
        let slot = &mut self.slots[self.selected];
        let stack = slot.as_mut()?;
        let item = stack.item;
        stack.count -= 1;
        if stack.count == 0 {
            *slot = None;
        }
        Some(item)
    }

    /// Retire un bloc de la case choisie (bloc posé)
    pub fn take_selected(&mut self) -> Option<BlockId> {
        let slot = &mut self.slots[self.selected];
        let stack = slot.as_mut()?;
        let block = stack.item.block()?;
        stack.count -= 1;
        if stack.count == 0 {
            *slot = None;
        }
        Some(block)
    }

    /// Range un bloc ou un objet : d'abord sur une pile du même, sinon dans la première case vide
    /// (barre d'objets puis sac). Renvoie faux si l'inventaire est plein.
    pub fn add(&mut self, item: impl Into<ItemId>) -> bool {
        let item = item.into();
        let stackable = self.slots.iter().position(|s| s.is_some_and(|s| s.item == item && s.count < item.max_stack()));
        if let Some(stack) = stackable.and_then(|i| self.slots[i].as_mut()) {
            stack.count += 1;
            return true;
        }
        match self.slots.iter().position(Option::is_none) {
            Some(i) => {
                self.slots[i] = Some(ItemStack { item, count: 1 });
                true
            }
            None => false,
        }
    }

    /// Range jusqu'à `count` exemplaires. Renvoie le nombre rangé.
    pub fn add_many(&mut self, item: impl Into<ItemId>, count: u16) -> u16 {
        let item = item.into();
        (0..count).take_while(|_| self.add(item)).count() as u16
    }

    /// Un exemplaire de plus trouverait sa place
    pub fn has_room_for(&self, item: impl Into<ItemId>) -> bool {
        let item = item.into();
        self.slots.iter().any(|s| s.is_none_or(|s| s.item == item && s.count < item.max_stack()))
    }

    /// Nombre total d'un bloc ou d'un objet dans l'inventaire
    pub fn count(&self, item: impl Into<ItemId>) -> u32 {
        let item = item.into();
        self.slots.iter().flatten().filter(|s| s.item == item).map(|s| s.count as u32).sum()
    }

    /// Exemplaires d'un ingrédient, toutes ses variantes comprises
    pub fn count_ingredient(&self, ingredient: &Ingredient) -> u32 {
        ingredient.options.iter().map(|&item| self.count(item)).sum()
    }

    /// Nombre de fois que la recette peut se faire avec ce que contient l'inventaire (sans
    /// compter la place pour ce qu'elle donne)
    pub fn craftable_times(&self, recipe: &Recipe) -> u32 {
        recipe.ingredients.iter().map(|i| self.count_ingredient(i) / i.count as u32).min().unwrap_or(0)
    }

    /// Fabrique une fois la recette : retire ses ingrédients (le sac d'abord, la barre d'objets
    /// ensuite) et range ce qu'elle donne. Renvoie faux, sans rien toucher, s'il manque un
    /// ingrédient ou de la place. La pile tenue à la souris est d'abord rangée.
    pub fn craft(&mut self, recipe: &Recipe) -> bool {
        self.return_held();
        let mut slots = self.slots.clone();
        for ingredient in &recipe.ingredients {
            let mut missing = ingredient.count;
            for &option in &ingredient.options {
                for slot in slots.iter_mut().rev() {
                    if missing == 0 {
                        break;
                    }
                    if let Some(stack) = slot.as_mut().filter(|s| s.item == option) {
                        let taken = stack.count.min(missing);
                        stack.count -= taken;
                        missing -= taken;
                        if stack.count == 0 {
                            *slot = None;
                        }
                    }
                }
            }
            if missing > 0 {
                return false;
            }
        }
        let mut after = Inventory { slots, ..Inventory::default() };
        if after.add_many(recipe.result, recipe.count) < recipe.count {
            return false;
        }
        self.slots = after.slots;
        true
    }

    /// Fabrique jusqu'à `times` fois. Renvoie le nombre de fois fabriqué.
    pub fn craft_many(&mut self, recipe: &Recipe, times: u32) -> u32 {
        (0..times).take_while(|_| self.craft(recipe)).count() as u32
    }

    /// Pile tenue à la souris dans l'écran d'inventaire
    pub fn held(&self) -> Option<ItemStack> {
        self.held.map(|(stack, _)| stack)
    }

    /// Clic gauche sur une case de l'écran d'inventaire : prend la pile, la pose dans une case
    /// vide, la fusionne avec une pile du même objet ou l'échange avec la pile qui s'y trouve
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
            Some(stack) if stack.item == held.item => {
                let moved = held.count.min(held.item.max_stack().saturating_sub(stack.count));
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

    /// Clic droit : prend la moitié d'une pile, ou pose un seul exemplaire de la pile tenue
    pub fn click_secondary(&mut self, index: usize) {
        if index >= TOTAL_SLOTS {
            return;
        }
        match (self.held.as_mut(), self.slots[index].as_mut()) {
            (None, Some(stack)) => {
                let half = stack.count.div_ceil(2);
                stack.count -= half;
                self.held = Some((ItemStack { item: stack.item, count: half }, index));
                if stack.count == 0 {
                    self.slots[index] = None;
                }
            }
            (Some((held, _)), slot) => {
                match slot {
                    None => self.slots[index] = Some(ItemStack { item: held.item, count: 1 }),
                    Some(stack) if stack.item == held.item && stack.count < held.item.max_stack() => stack.count += 1,
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
        if let Some(stack) = self.slots[origin].as_mut().filter(|s| s.item == held.item) {
            let moved = remaining.min(held.item.max_stack().saturating_sub(stack.count));
            stack.count += moved;
            remaining -= moved;
        } else if self.slots[origin].is_none() {
            self.slots[origin] = Some(held);
            remaining = 0;
        }
        while remaining > 0 {
            if !self.add(held.item) {
                log::warn!("Inventaire plein : {} × {} perdus.", remaining, held.item.name());
                break;
            }
            remaining -= 1;
        }
    }
}

/// Bloc gagné en cassant `block`, selon sa fiche (l'herbe donne de la terre, comme dans
/// Minecraft ; les liquides ne se ramassent pas)
pub fn drop_for(block: BlockId) -> Option<BlockId> {
    blocks().drop_of(block)
}

/// Ce qu'une case peut garder : un objet connu, ou un bloc qui se ramasse tel quel
pub fn can_hold(item: ItemId) -> bool {
    match item.block() {
        Some(block) => drop_for(block) == Some(block),
        None => item.is_known(),
    }
}

/// Teintes du dessus et des côtés d'un bloc (icône, bloc tombé au sol), celles de sa fiche
pub fn item_colors(block: BlockId) -> ([f32; 4], [f32; 4]) {
    match blocks().get(block) {
        Some(sheet) => (sheet.top_color(), sheet.side_color()),
        None => ([0.6, 0.6, 0.6, 1.0], [0.6, 0.6, 0.6, 1.0]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use herocraft_common::BLOCK_STACK as MAX_STACK;

    #[test]
    fn test_food_in_hand_is_eaten_one_at_a_time() {
        let bread = herocraft_common::items().find("pain").unwrap();
        let mut inv = Inventory::default();
        inv.add(BlockId::STONE);
        assert_eq!(inv.selected_food(), None, "un bloc ne se mange pas");
        inv.add_many(bread, 2);
        inv.select(1);
        assert_eq!(inv.selected_food(), Some((bread, 12.0)));
        assert_eq!(inv.consume_selected(), Some(bread));
        assert_eq!(inv.count(bread), 1);
        inv.consume_selected();
        assert_eq!(inv.slot(1), None);
        assert_eq!(inv.selected_food(), None);
        assert_eq!(inv.consume_selected(), None);
    }

    #[test]
    fn test_add_stacks_then_fills_hotbar_then_bag() {
        let mut inv = Inventory::default();
        for _ in 0..MAX_STACK + 1 {
            assert!(inv.add(BlockId::STONE));
        }
        assert_eq!(inv.slot(0), Some(ItemStack { item: BlockId::STONE.into(), count: MAX_STACK }));
        assert_eq!(inv.slot(1), Some(ItemStack { item: BlockId::STONE.into(), count: 1 }));
        assert!(inv.add(BlockId::DIRT));
        assert_eq!(inv.slot(2).and_then(|s| s.item.block()), Some(BlockId::DIRT));
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
        assert_eq!(inv.held().and_then(|s| s.item.block()), Some(BlockId::STONE));
        assert_eq!(inv.slot(0), None);
        inv.click(20);
        assert_eq!(inv.slot(20).and_then(|s| s.item.block()), Some(BlockId::STONE));
        assert_eq!(inv.held(), None);
        // Échange : la terre prend la place de la pierre, qui reste en main
        inv.click(1);
        inv.click(20);
        assert_eq!(inv.slot(20).and_then(|s| s.item.block()), Some(BlockId::DIRT));
        assert_eq!(inv.held().and_then(|s| s.item.block()), Some(BlockId::STONE));
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
        assert_eq!(loaded.slot(0), Some(ItemStack { item: BlockId::SAND.into(), count: 2 }));
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

    fn recipe(result: ItemId, count: u16, ingredients: &[(&[ItemId], u16)]) -> Recipe {
        let ingredients = ingredients.iter().map(|(options, count)| Ingredient { options: options.to_vec(), count: *count }).collect();
        Recipe { result, count, station: None, ingredients }
    }

    #[test]
    fn test_crafting_takes_ingredients_and_gives_the_result() {
        let oak = ItemId::from(BlockId::WOOD);
        let birch = ItemId::from(blocks().find("birchTrunk").unwrap());
        let plank = ItemId::from(blocks().find("plank").unwrap());
        let planks = recipe(plank, 4, &[(&[oak, birch], 1)]);
        let mut inv = Inventory::default();
        assert_eq!(inv.craftable_times(&planks), 0);
        assert!(!inv.craft(&planks), "rien à couper");
        inv.add(birch);
        inv.add_many(oak, 2);
        assert_eq!(inv.craftable_times(&planks), 3, "chêne ou bouleau");
        assert_eq!(inv.craft_many(&planks, 10), 3);
        assert_eq!(inv.count(plank), 12);
        assert_eq!(inv.count(oak) + inv.count(birch), 0);

        // Plusieurs ingrédients : tous ou rien
        let stick = herocraft_common::items().find("stick").unwrap();
        let rope = herocraft_common::items().find("rope").unwrap();
        let bench = ItemId::from(blocks().find("workbench").unwrap());
        let workbench = recipe(bench, 1, &[(&[plank], 4), (&[rope], 2)]);
        inv.add(rope);
        assert!(!inv.craft(&workbench), "il manque une corde");
        assert_eq!((inv.count(plank), inv.count(rope)), (12, 1), "rien n'a été pris");
        inv.add(rope);
        assert!(inv.craft(&workbench));
        assert_eq!((inv.count(plank), inv.count(rope), inv.count(bench)), (8, 0, 1));
        assert_eq!(inv.count(stick), 0);
    }

    #[test]
    fn test_crafting_needs_room_for_the_result() {
        let stone = ItemId::from(BlockId::STONE);
        let sword = herocraft_common::items().find("flintSword").unwrap();
        let mut full = Inventory::default();
        for _ in 0..TOTAL_SLOTS {
            full.add(sword);
        }
        let too_many = recipe(stone, 65, &[(&[sword], 1)]);
        assert!(!full.craft(&too_many), "65 pierres ne tiennent pas dans la case libérée");
        assert_eq!(full.count(sword), TOTAL_SLOTS as u32, "rien n'a été pris");
        let swap = recipe(stone, 1, &[(&[sword], 1)]);
        assert!(full.craft(&swap), "la case de l'épée prise se libère pour la pierre");
        assert_eq!((full.count(sword), full.count(stone)), (TOTAL_SLOTS as u32 - 1, 1));
    }

    #[test]
    fn test_objects_stack_by_their_sheet_and_are_not_placed() {
        let sword = herocraft_common::items().find("flintSword").unwrap();
        let bread = herocraft_common::items().find("pain").unwrap();
        let mut inv = Inventory::default();
        assert_eq!(inv.add_many(sword, 2), 2);
        assert_eq!(inv.slot(1).map(|s| s.count), Some(1), "une épée par case");
        assert_eq!(inv.selected_block(), None);
        assert_eq!(inv.take_selected(), None, "un objet ne se pose pas");
        assert_eq!(inv.count(sword), 2);
        assert_eq!(inv.add_many(bread, 99), 99);
        assert_eq!(inv.slot(2).map(|s| s.count), Some(99));

        // Un objet inconnu (fiche retirée) disparaît au chargement
        let json = format!(r#"{{ "slots": [ {{ "item": {}, "count": 1 }}, {{ "item": 65000, "count": 1 }} ] }}"#, sword.0);
        let mut loaded: Inventory = serde_json::from_str(&json).unwrap();
        loaded.repair();
        assert_eq!(loaded.slot(0).map(|s| s.item), Some(sword));
        assert_eq!(loaded.slot(1), None);
    }
}
