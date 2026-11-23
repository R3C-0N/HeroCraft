//! Inventaire du joueur : 9 cases de barre d'objets (touches 1 à 9, molette), 27 cases de sac et 8
//! cases d'équipement (tête, torse, jambes, pieds, cape, amulette et deux anneaux). Les blocs
//! ramassés au sol y entrent, comme ce qui est fabriqué ; le bloc posé au clic droit est pris dans
//! la case choisie de la barre. Enregistré en JSON avec le monde.
//!
//! L'équipement n'est pas un second inventaire : ses cases suivent celles du sac, si bien que
//! prendre, poser et échanger une pile y marchent comme ailleurs. Elles ne prennent que ce qui s'y
//! porte, et ni le ramassage, ni la fabrication n'y touchent.

use std::path::{Path, PathBuf};

use herocraft_common::{blocks, items, BlockId, EquipSlot, Ingredient, ItemId, Recipe};
use serde::{Deserialize, Serialize};

/// Cases de la barre d'objets, en bas de l'écran
pub const HOTBAR_SLOTS: usize = 9;
/// Cases du sac, visibles dans l'écran d'inventaire
pub const BAG_SLOTS: usize = 27;
/// Cases où vont les objets ramassés ou fabriqués : la barre d'objets puis le sac
pub const STORAGE_SLOTS: usize = HOTBAR_SLOTS + BAG_SLOTS;
/// Ce que porte chaque case d'équipement, dans l'ordre des cases (comme sur la branche `develop`)
pub const EQUIPMENT: [EquipSlot; 8] = [
    EquipSlot::Head,
    EquipSlot::Chest,
    EquipSlot::Legs,
    EquipSlot::Feet,
    EquipSlot::Cape,
    EquipSlot::Amulet,
    EquipSlot::Ring,
    EquipSlot::Ring,
];
/// Première case d'équipement, juste après le sac
pub const FIRST_EQUIPMENT: usize = STORAGE_SLOTS;
pub const TOTAL_SLOTS: usize = STORAGE_SLOTS + EQUIPMENT.len();

/// Emplacement d'équipement de la case `index`, si c'en est une
pub fn equipment_kind(index: usize) -> Option<EquipSlot> {
    index.checked_sub(FIRST_EQUIPMENT).and_then(|i| EQUIPMENT.get(i)).copied()
}

/// La case `index` peut recevoir `item` : toute case de rangement, ou la case d'équipement de son
/// emplacement
pub fn fits(index: usize, item: ItemId) -> bool {
    match equipment_kind(index) {
        Some(kind) => item.equip_slot() == Some(kind),
        None => index < STORAGE_SLOTS,
    }
}

/// La pioche du paquetage du soldat
pub const SOLDIER_PICKAXE: &str = "ironPickaxe";

/// Une pioche, quel qu'en soit le métal
pub fn is_pickaxe(item: ItemId) -> bool {
    item.block().is_none() && herocraft_common::items().get(item).is_some_and(|sheet| sheet.id.to_ascii_lowercase().contains("pickaxe"))
}

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
    /// Barre d'objets (cases 0 à 8), sac (9 à 35) puis équipement (36 à 43)
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

    /// Remet d'aplomb un fichier ancien ou édité à la main. Un inventaire d'avant l'équipement
    /// gagne ses cases à la fin, sans que rien ne bouge ; ce qui ne va pas dans une case
    /// d'équipement (fiche changée) rejoint le sac.
    fn repair(&mut self) {
        self.slots.resize(TOTAL_SLOTS, None);
        let mut misplaced = Vec::new();
        for (index, slot) in self.slots.iter_mut().enumerate() {
            if slot.is_some_and(|s| s.count == 0 || !can_hold(s.item)) {
                *slot = None;
            } else if let Some(stack) = slot {
                stack.count = stack.count.min(stack.item.max_stack());
                if !fits(index, stack.item) {
                    misplaced.extend(slot.take());
                } else if equipment_kind(index).is_some() {
                    stack.count = 1;
                }
            }
        }
        for stack in misplaced {
            let kept = self.add_many(stack.item, stack.count);
            if kept < stack.count {
                log::warn!("Inventaire plein : {} × {} perdus.", stack.count - kept, stack.item.name());
            }
        }
        self.selected = self.selected.min(HOTBAR_SLOTS - 1);
    }

    /// Le paquetage du soldat de la vision (bible, section 8) : maille de fer portée, épée en
    /// main, et une pioche rangée dans le sac (le tutoriel la fait prendre). Il ne s'enregistre
    /// nulle part : l'inventaire du présent revient au retour.
    pub fn soldier_kit() -> Self {
        let mut kit = Self::default();
        let find = |id: &str| herocraft_common::items().find(id);
        for (i, id) in ["ironMailHelmet", "ironMailChestplate", "ironMailLeggings", "ironMailBoots"].into_iter().enumerate() {
            if let Some(item) = find(id).filter(|&item| fits(FIRST_EQUIPMENT + i, item)) {
                kit.slots[FIRST_EQUIPMENT + i] = Some(ItemStack { item, count: 1 });
            }
        }
        if let Some(sword) = find("ironSword") {
            kit.slots[0] = Some(ItemStack { item: sword, count: 1 });
        }
        if let Some(pickaxe) = find(SOLDIER_PICKAXE) {
            kit.slots[HOTBAR_SLOTS + 4] = Some(ItemStack { item: pickaxe, count: 1 });
        }
        kit
    }

    /// Une pioche est dans la barre d'objets
    pub fn pickaxe_in_hotbar(&self) -> bool {
        self.slots[..HOTBAR_SLOTS].iter().flatten().any(|s| is_pickaxe(s.item))
    }

    /// Le joueur tient une pioche
    pub fn holds_pickaxe(&self) -> bool {
        self.selected_item().is_some_and(is_pickaxe)
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

    /// Bloc ou objet de la case choisie
    pub fn selected_item(&self) -> Option<ItemId> {
        self.slot(self.selected).map(|s| s.item)
    }

    /// Aliment de la case choisie et la satiété qu'il rend, à manger au clic droit
    pub fn selected_food(&self) -> Option<(ItemId, f32)> {
        let item = self.slot(self.selected)?.item;
        Some((item, item.satiety()?))
    }

    /// Retire un exemplaire de la case choisie, bloc ou objet (graine semée, aliment mangé ou donné)
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

    /// Retire un bloc de la case choisie (rien si elle tient un objet)
    #[cfg(test)]
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

    /// Une munition de ce type dans la barre ou le sac (la barre d'abord), sans la retirer
    pub fn find_ammo(&self, kind: herocraft_common::AmmoKind) -> Option<ItemId> {
        self.slots[..STORAGE_SLOTS].iter().flatten().find(|s| s.item.block().is_none() && items().get(s.item).is_some_and(|sheet| sheet.ammo == Some(kind))).map(|s| s.item)
    }

    /// Retire un exemplaire de `item` (la barre d'abord) ; faux s'il n'y en a pas
    pub fn take_one(&mut self, item: ItemId) -> bool {
        let Some(slot) = self.slots[..STORAGE_SLOTS].iter_mut().find(|s| s.is_some_and(|s| s.item == item)) else { return false };
        if let Some(stack) = slot.as_mut() {
            stack.count -= 1;
            if stack.count == 0 {
                *slot = None;
            }
        }
        true
    }

    /// Range un bloc ou un objet : d'abord sur une pile du même, sinon dans la première case vide
    /// (barre d'objets puis sac, jamais l'équipement). Renvoie faux si l'inventaire est plein.
    pub fn add(&mut self, item: impl Into<ItemId>) -> bool {
        let item = item.into();
        let stackable = self.slots[..STORAGE_SLOTS].iter().position(|s| s.is_some_and(|s| s.item == item && s.count < item.max_stack()));
        if let Some(stack) = stackable.and_then(|i| self.slots[i].as_mut()) {
            stack.count += 1;
            return true;
        }
        match self.slots[..STORAGE_SLOTS].iter().position(Option::is_none) {
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
        self.slots[..STORAGE_SLOTS].iter().any(|s| s.is_none_or(|s| s.item == item && s.count < item.max_stack()))
    }

    /// Nombre total d'un bloc ou d'un objet dans la barre et le sac (ce qui est porté ne compte
    /// pas : la fabrication n'y prend rien)
    pub fn count(&self, item: impl Into<ItemId>) -> u32 {
        let item = item.into();
        self.slots[..STORAGE_SLOTS].iter().flatten().filter(|s| s.item == item).map(|s| s.count as u32).sum()
    }

    /// Ce que porte la case d'équipement `i` (de 0 à 7, dans l'ordre de `EQUIPMENT`)
    pub fn equipped(&self, i: usize) -> Option<ItemStack> {
        self.slot(FIRST_EQUIPMENT + i)
    }

    /// Pièces portées, dans l'ordre des cases d'équipement
    pub fn worn(&self) -> Vec<ItemId> {
        (0..EQUIPMENT.len()).filter_map(|i| self.equipped(i)).map(|stack| stack.item).collect()
    }

    /// Protection totale de ce qui est porté : la somme de la caractéristique `Protection` des
    /// pièces
    pub fn protection(&self) -> u32 {
        items().protection(self.worn()) as u32
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
    /// ensuite, jamais ce qui est porté) et range ce qu'elle donne. Renvoie faux, sans rien toucher, s'il manque un
    /// ingrédient ou de la place. La pile tenue à la souris est d'abord rangée.
    pub fn craft(&mut self, recipe: &Recipe) -> bool {
        self.return_held();
        let mut slots = self.slots.clone();
        for ingredient in &recipe.ingredients {
            let mut missing = ingredient.count;
            for &option in &ingredient.options {
                for slot in slots[..STORAGE_SLOTS].iter_mut().rev() {
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

    /// L'inventaire contient tout ce que demande un échange
    pub fn can_trade(&self, trade: &herocraft_common::Trade) -> bool {
        trade.give.iter().all(|&(item, count)| self.count(item) >= count as u32)
    }

    /// Fait un échange avec un habitant : donne ce qu'il demande (le sac d'abord) et range ce qu'il
    /// donne. Renvoie faux, sans rien toucher, s'il manque un objet ou de la place.
    pub fn trade(&mut self, trade: &herocraft_common::Trade) -> bool {
        self.return_held();
        let mut slots = self.slots.clone();
        for &(item, count) in &trade.give {
            let mut missing = count;
            for slot in slots.iter_mut().rev() {
                if missing == 0 {
                    break;
                }
                if let Some(stack) = slot.as_mut().filter(|s| s.item == item) {
                    let taken = stack.count.min(missing);
                    stack.count -= taken;
                    missing -= taken;
                    if stack.count == 0 {
                        *slot = None;
                    }
                }
            }
            if missing > 0 {
                return false;
            }
        }
        let mut after = Inventory { slots, ..Inventory::default() };
        let (item, count) = trade.get;
        if after.add_many(item, count) < count {
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
    /// vide, la fusionne avec une pile du même objet ou l'échange avec la pile qui s'y trouve. Une
    /// case d'équipement refuse ce qui ne s'y porte pas.
    pub fn click(&mut self, index: usize) {
        if index >= TOTAL_SLOTS {
            return;
        }
        let Some((mut held, origin)) = self.held.take() else {
            self.held = self.slots[index].take().map(|stack| (stack, index));
            return;
        };
        if !fits(index, held.item) {
            self.held = Some((held, origin));
            return;
        }
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
        if index >= TOTAL_SLOTS || self.held.is_some_and(|(held, _)| !fits(index, held.item)) {
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

    /// Maj + clic : enfile ce qui se porte dans sa case d'équipement (une case libre d'abord, sinon
    /// à la place de la pièce portée, qui prend sa place), ou range dans la barre ou le sac ce qui
    /// était porté. Renvoie vrai si quelque chose a bougé.
    pub fn quick_move(&mut self, index: usize) -> bool {
        if self.held.is_some() {
            return false;
        }
        let Some(stack) = self.slot(index) else { return false };
        if equipment_kind(index).is_some() {
            if !self.add(stack.item) {
                return false;
            }
            self.slots[index] = None;
            return true;
        }
        let Some(kind) = stack.item.equip_slot() else { return false };
        let targets: Vec<usize> = (FIRST_EQUIPMENT..TOTAL_SLOTS).filter(|&i| equipment_kind(i) == Some(kind)).collect();
        let Some(&target) = targets.iter().find(|&&i| self.slots[i].is_none()).or(targets.first()) else { return false };
        self.slots.swap(index, target);
        true
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
mod trade_tests {
    use super::*;

    #[test]
    fn a_trade_takes_what_is_asked_and_gives_the_goods() {
        let mut inventory = Inventory::default();
        let (ingot, sword) = (ItemId(20000), ItemId(20001));
        let trade = herocraft_common::Trade { give: vec![(ingot, 3)], get: (sword, 1) };
        inventory.add_many(ingot, 2);
        assert!(!inventory.can_trade(&trade) && !inventory.trade(&trade));
        assert_eq!(inventory.count(ingot), 2);
        inventory.add(ingot);
        assert!(inventory.can_trade(&trade) && inventory.trade(&trade));
        assert_eq!((inventory.count(ingot), inventory.count(sword)), (0, 1));
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
        for _ in 0..STORAGE_SLOTS * MAX_STACK as usize {
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
        for _ in 0..STORAGE_SLOTS {
            full.add(sword);
        }
        let too_many = recipe(stone, 65, &[(&[sword], 1)]);
        assert!(!full.craft(&too_many), "65 pierres ne tiennent pas dans la case libérée");
        assert_eq!(full.count(sword), STORAGE_SLOTS as u32, "rien n'a été pris");
        let swap = recipe(stone, 1, &[(&[sword], 1)]);
        assert!(full.craft(&swap), "la case de l'épée prise se libère pour la pierre");
        assert_eq!((full.count(sword), full.count(stone)), (STORAGE_SLOTS as u32 - 1, 1));
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

    fn item(id: &str) -> ItemId {
        herocraft_common::items().find(id).unwrap()
    }

    /// Case d'équipement de l'emplacement `kind` (la première, pour les anneaux)
    fn equipment(kind: EquipSlot) -> usize {
        FIRST_EQUIPMENT + EQUIPMENT.iter().position(|&k| k == kind).unwrap()
    }

    #[test]
    fn test_equipment_slots_only_take_what_is_worn_there() {
        let helmet = item("boneHelmet");
        let boots = item("hideBoots");
        let mut inv = Inventory::default();
        inv.add(BlockId::STONE);
        inv.add(helmet);
        inv.add(boots);
        let head = equipment(EquipSlot::Head);
        assert_eq!(head, FIRST_EQUIPMENT, "la tête d'abord, après le sac");

        // La pierre n'entre pas dans la case de la tête
        inv.click(0);
        inv.click(head);
        assert_eq!(inv.slot(head), None);
        assert_eq!(inv.held().and_then(|s| s.item.block()), Some(BlockId::STONE), "elle reste en main");
        inv.click(0);
        // Les bottes non plus, le heaume si
        inv.click(2);
        inv.click(head);
        assert_eq!(inv.slot(head), None);
        inv.click(2);
        inv.click(1);
        inv.click(head);
        assert_eq!(inv.slot(head).map(|s| s.item), Some(helmet));
        assert_eq!(inv.held(), None);
        assert_eq!(inv.equipped(0).map(|s| s.item), Some(helmet));
        // Clic droit en tenant les bottes : refusé aussi
        inv.click(2);
        inv.click_secondary(head);
        assert_eq!(inv.slot(head).map(|s| s.item), Some(helmet));
        inv.click(equipment(EquipSlot::Feet));
        assert_eq!(inv.slot(equipment(EquipSlot::Feet)).map(|s| s.item), Some(boots));
        assert_eq!(inv.protection(), 4 + 1, "heaume d'os et bottes de cuir");
    }

    #[test]
    fn test_worn_items_are_kept_out_of_pickup_and_crafting() {
        let helmet = item("boneHelmet");
        let mut inv = Inventory::default();
        inv.add(helmet);
        assert!(inv.quick_move(0), "Maj + clic : le heaume se porte");
        assert_eq!(inv.slot(0), None);
        assert_eq!(inv.equipped(0).map(|s| s.item), Some(helmet));
        assert_eq!(inv.count(helmet), 0, "ce qui est porté ne compte pas pour la fabrication");
        let melt = recipe(ItemId::from(BlockId::STONE), 1, &[(&[helmet], 1)]);
        assert!(!inv.craft(&melt), "la fabrication ne prend pas ce qui est porté");
        assert_eq!(inv.equipped(0).map(|s| s.item), Some(helmet));

        // Barre et sac pleins : rien ne tombe dans l'équipement
        while inv.add(BlockId::SAND) {}
        assert!(inv.slots[FIRST_EQUIPMENT + 1..].iter().all(Option::is_none));
        assert!(!inv.has_room_for(BlockId::SAND));
        assert!(!inv.quick_move(FIRST_EQUIPMENT), "plus de place pour l'ôter");
        assert_eq!(inv.equipped(0).map(|s| s.item), Some(helmet));
    }

    #[test]
    fn test_quick_move_swaps_and_takes_off() {
        let bone = item("boneHelmet");
        let iron = item("ironPlateHelmet");
        let ring_slots = (FIRST_EQUIPMENT..TOTAL_SLOTS).filter(|&i| equipment_kind(i) == Some(EquipSlot::Ring)).count();
        assert_eq!(ring_slots, 2, "deux anneaux");
        let mut inv = Inventory::default();
        inv.add(bone);
        inv.add(iron);
        inv.add(BlockId::DIRT);
        assert!(inv.quick_move(0));
        // Un second heaume prend la place du premier, qui revient dans sa case
        assert!(inv.quick_move(1));
        assert_eq!(inv.equipped(0).map(|s| s.item), Some(iron));
        assert_eq!(inv.slot(1).map(|s| s.item), Some(bone));
        assert!(!inv.quick_move(2), "la terre ne se porte pas");
        // Ôter : retour dans la première case libre
        assert!(inv.quick_move(FIRST_EQUIPMENT));
        assert_eq!(inv.equipped(0), None);
        assert_eq!(inv.slot(0).map(|s| s.item), Some(iron));
    }

    #[test]
    fn test_old_saves_gain_equipment_slots_and_misplaced_items_move_out() {
        let helmet = item("boneHelmet");
        // Inventaire d'avant l'équipement : 36 cases
        let mut slots = vec![serde_json::Value::Null; STORAGE_SLOTS];
        slots[3] = serde_json::json!({ "item": 1, "count": 5 });
        let json = serde_json::json!({ "slots": slots, "selected": 2 }).to_string();
        let mut inv: Inventory = serde_json::from_str(&json).unwrap();
        inv.repair();
        assert_eq!(inv.slots.len(), TOTAL_SLOTS);
        assert_eq!(inv.slot(3).map(|s| s.count), Some(5), "rien n'a bougé");
        assert!((0..EQUIPMENT.len()).all(|i| inv.equipped(i).is_none()));

        // Une pierre glissée à la main dans la case des pieds rejoint le sac ; le heaume reste
        inv.slots[FIRST_EQUIPMENT] = Some(ItemStack { item: helmet, count: 1 });
        inv.slots[equipment(EquipSlot::Feet)] = Some(ItemStack { item: BlockId::STONE.into(), count: 7 });
        inv.repair();
        assert_eq!(inv.equipped(0).map(|s| s.item), Some(helmet));
        assert_eq!(inv.slot(equipment(EquipSlot::Feet)), None);
        assert_eq!(inv.count(BlockId::STONE), 12);
    }
}
