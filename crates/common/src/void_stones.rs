//! Les Pierres du Néant : les artefacts tombés du ciel qui ont créé la vie, puis donné leur
//! pouvoir aux premiers héros (voir la bible de l'univers, sections 5 à 9).
//!
//! Dans le jeu, ce sont deux objets de `assets/items/` :
//! - la **Pierre** entière (`voidStone`), unique dans sa case ;
//! - le **fragment** (`voidStoneFragment`), un éclat d'une Pierre brisée, comme ceux de la Pierre
//!   du roi que les élus lui rapportent.
//!
//! Ce module dit ce qu'est une Pierre et ce qui lui arrive, par des événements que d'autres
//! systèmes écoutent (cinématiques, temples, quêtes) sans connaître le ramassage ni l'inventaire :
//! - [`StoneEvent::Obtained`] : une Pierre ou un fragment vient d'entrer dans l'inventaire ;
//! - [`StoneEvent::Touched`] : le joueur touche la Pierre qu'il tient (clic droit) ;
//! - [`StoneEvent::Absorbed`] : la Pierre a donné ses dernières forces et s'est détruite.
//!
//! Les règles du lore qui ont un effet en jeu sont ici aussi : une Pierre ne disparaît jamais
//! d'elle-même au sol, toute Pierre est liée à un héros, et seul un descendant de ce héros
//! l'absorbe (elle est alors détruite) ; un autre héros peut s'en servir sans la détruire.

use serde::{Deserialize, Serialize};

use crate::items::{ItemId, items};

/// Identifiant de la fiche de la Pierre entière
pub(crate) const VOID_STONE: &str = "voidStone";
/// Identifiant de la fiche d'un fragment de Pierre
pub(crate) const VOID_STONE_FRAGMENT: &str = "voidStoneFragment";

/// Ce qu'est un objet du Néant
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StoneKind {
    /// Une Pierre entière, liée à la mémoire d'un héros
    Whole,
    /// Un éclat d'une Pierre brisée
    Fragment,
}

impl StoneKind {
    /// L'objet de la fiche, s'il est chargé
    pub fn item(self) -> Option<ItemId> {
        items().find(match self {
            StoneKind::Whole => VOID_STONE,
            StoneKind::Fragment => VOID_STONE_FRAGMENT,
        })
    }

    /// Ce qu'est `item`, si c'est une Pierre ou un fragment
    pub fn of(item: ItemId) -> Option<Self> {
        if item.block().is_some() {
            return None;
        }
        match items().get(item)?.id.as_str() {
            VOID_STONE => Some(StoneKind::Whole),
            VOID_STONE_FRAGMENT => Some(StoneKind::Fragment),
            _ => None,
        }
    }
}

impl ItemId {
    /// Pierre du Néant ou fragment de Pierre
    pub fn is_void_stone(self) -> bool {
        StoneKind::of(self).is_some()
    }
}

/// Ce qui arrive quand quelqu'un touche une Pierre entière (bible, section 9)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TouchOutcome {
    /// La mémoire du héros de la Pierre se lie au porteur, son descendant : il en absorbe les
    /// dernières forces, reçoit le statut de héros, et la Pierre est détruite
    Absorbed,
    /// Un héros qui n'en descend pas puise dans la Pierre sans la détruire
    Used,
    /// Rien : la mémoire ne se lie pas à qui n'est ni héros ni descendant
    Silent,
}

/// Règle du toucher : seul un descendant du héros de la Pierre l'absorbe ; n'importe quel héros
/// peut s'en servir sans la détruire ; sinon, la Pierre reste muette.
pub fn touch_outcome(is_descendant: bool, is_hero: bool) -> TouchOutcome {
    if is_descendant {
        TouchOutcome::Absorbed
    } else if is_hero {
        TouchOutcome::Used
    } else {
        TouchOutcome::Silent
    }
}

/// Un événement de Pierre du Néant, à écouter par les autres systèmes
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum StoneEvent {
    /// « Pierre obtenue » : `item` vient d'entrer dans l'inventaire, ramassé en `position`
    /// (repère du monde)
    Obtained { kind: StoneKind, item: ItemId, position: [f32; 3] },
    /// Le joueur touche la Pierre qu'il tient (clic droit). Ce qui s'ensuit dépend du contexte
    /// (vision, descendance : voir [`touch_outcome`]) et revient à qui écoute l'événement.
    Touched { item: ItemId },
    /// La Pierre a été absorbée : elle a quitté l'inventaire, détruite
    Absorbed { item: ItemId },
}

/// File des événements de Pierres de l'image en cours : les systèmes qui les produisent les
/// poussent, la boucle du jeu les vide une fois par image et les distribue.
#[derive(Debug, Clone, Default)]
pub struct StoneEvents {
    pending: Vec<StoneEvent>,
}

impl StoneEvents {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, event: StoneEvent) {
        self.pending.push(event);
    }

    /// Les événements en attente, dans l'ordre où ils sont arrivés ; la file est vidée
    pub fn drain(&mut self) -> Vec<StoneEvent> {
        std::mem::take(&mut self.pending)
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voxel::BlockId;

    #[test]
    fn stones_are_bundled_items() {
        let stone = StoneKind::Whole.item().expect("fiche voidStone");
        let fragment = StoneKind::Fragment.item().expect("fiche voidStoneFragment");
        assert_eq!(stone.name(), "Pierre du Néant");
        assert_eq!(stone.max_stack(), 1, "une Pierre est unique dans sa case");
        assert!(fragment.max_stack() > 1, "les fragments s'empilent");
        assert_eq!(StoneKind::of(stone), Some(StoneKind::Whole));
        assert_eq!(StoneKind::of(fragment), Some(StoneKind::Fragment));
        assert!(stone.is_void_stone() && fragment.is_void_stone());
        assert!(!items().find("flintSword").unwrap().is_void_stone());
        assert!(!ItemId::from(BlockId::STONE).is_void_stone(), "le bloc de pierre n'est pas une Pierre du Néant");
        assert_eq!(stone.equip_slot(), None);
        assert_eq!(stone.satiety(), None);
    }

    #[test]
    fn only_a_descendant_absorbs_the_stone() {
        assert_eq!(touch_outcome(true, false), TouchOutcome::Absorbed);
        assert_eq!(touch_outcome(true, true), TouchOutcome::Absorbed);
        assert_eq!(touch_outcome(false, true), TouchOutcome::Used);
        assert_eq!(touch_outcome(false, false), TouchOutcome::Silent);
    }

    #[test]
    fn events_drain_in_order() {
        let stone = StoneKind::Whole.item().unwrap();
        let mut events = StoneEvents::new();
        assert!(events.is_empty());
        events.push(StoneEvent::Obtained { kind: StoneKind::Whole, item: stone, position: [1.0, 2.0, 3.0] });
        events.push(StoneEvent::Absorbed { item: stone });
        let drained = events.drain();
        assert_eq!(drained.len(), 2);
        assert!(matches!(drained[0], StoneEvent::Obtained { kind: StoneKind::Whole, .. }));
        assert!(events.is_empty());
    }
}
