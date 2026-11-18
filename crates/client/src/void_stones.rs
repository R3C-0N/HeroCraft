//! Pierres du Néant côté client : la file des événements de l'image et l'unique point où ils
//! sont distribués (`dispatch`). Un système qui veut réagir à une Pierre obtenue, touchée ou
//! absorbée (cinématique de la vision, quête, temple) se branche dans `dispatch`.
//!
//! Les règles et les types sont dans `herocraft_common::void_stones`.

use herocraft_common::{ItemId, StoneEvent, StoneEvents, StoneKind};

use crate::chat::Chat;
use crate::inventory::Inventory;

/// La Pierre entière tenue en main, si c'en est une (un fragment ne se touche pas)
pub fn held_stone(inventory: &Inventory) -> Option<ItemId> {
    inventory.slot(inventory.selected()).map(|s| s.item).filter(|&item| StoneKind::of(item) == Some(StoneKind::Whole))
}

/// Le joueur touche la Pierre qu'il tient (clic droit) : signale `Touched`
pub fn touch(item: ItemId, events: &mut StoneEvents) {
    events.push(StoneEvent::Touched { item });
}

/// La Pierre `item` donne ses dernières forces : elle quitte l'inventaire, détruite, et
/// `Absorbed` est signalé. Faux si l'inventaire ne la contient pas.
#[allow(dead_code)] // appelée par la fin de la vision tutoriel (fil des cinématiques)
pub fn absorb(item: ItemId, inventory: &mut Inventory, events: &mut StoneEvents) -> bool {
    if StoneKind::of(item) != Some(StoneKind::Whole) || !inventory.take_one(item) {
        return false;
    }
    events.push(StoneEvent::Absorbed { item });
    true
}

/// Distribue les événements de l'image. C'est ici que se branchent les autres systèmes.
pub fn dispatch(events: &mut StoneEvents, chat: &mut Chat) {
    for event in events.drain() {
        log::info!("Pierre du Néant : {event:?}");
        match event {
            StoneEvent::Obtained { kind: StoneKind::Whole, .. } => chat.info("Vous avez obtenu une Pierre du Néant. Une mémoire ancienne y dort."),
            StoneEvent::Obtained { kind: StoneKind::Fragment, .. } => chat.info("Vous avez obtenu un fragment de Pierre du Néant."),
            StoneEvent::Touched { .. } => chat.info("La Pierre du Néant vibre sous vos doigts…"),
            StoneEvent::Absorbed { .. } => chat.info("La Pierre du Néant s'éteint et se brise : sa force est en vous."),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absorbing_destroys_the_held_stone() {
        let stone = StoneKind::Whole.item().unwrap();
        let fragment = StoneKind::Fragment.item().unwrap();
        let mut inventory = Inventory::default();
        let mut events = StoneEvents::new();
        assert!(!absorb(stone, &mut inventory, &mut events), "pas de Pierre, rien à absorber");
        assert!(inventory.add(stone));
        assert!(inventory.add(fragment));
        assert!(!absorb(fragment, &mut inventory, &mut events), "un fragment ne s'absorbe pas");
        assert!(absorb(stone, &mut inventory, &mut events));
        assert_eq!(inventory.count(stone), 0);
        assert_eq!(inventory.count(fragment), 1);
        assert_eq!(events.drain(), vec![StoneEvent::Absorbed { item: stone }]);
    }
}
