//! Les quinze voies du râtelier, reprises des classes du module `HeroClasses` de la branche
//! `develop` : leur famille, leur devise, leur armement, la répartition de leur maîtrise (mêlée,
//! distance, magie, sur cent) et le paquetage qu'elles mettent dans le sac au premier matin.
//!
//! Les objets sont nommés par l'identifiant de leur fiche (`assets/items/<id>.json`).

use crate::character::PlayerClass;

/// Famille d'une voie : ce qui décide de ses gains de niveau
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Magic,
    Ranged,
    Melee,
}

impl Family {
    /// Les familles dans l'ordre du râtelier, avec leur nom et leur devise
    pub const ALL: [(Family, &'static str, &'static str); 3] = [
        (Family::Magic, "Magie", "On y plie le monde en le nommant."),
        (Family::Ranged, "Combat à distance", "On y frappe avant d’être vu."),
        (Family::Melee, "Corps à corps", "On y prend les coups de face."),
    ];
}

/// Fiche d'une voie
#[derive(Debug, Clone, Copy)]
pub struct ClassSheet {
    pub class: PlayerClass,
    /// Identifiant de la voie (nom du prefab de la branche `develop`)
    pub id: &'static str,
    pub name: &'static str,
    pub family: Family,
    /// La ligne qu'on dit de ceux qui la suivent
    pub role: &'static str,
    /// Ce que les mains tiennent
    pub armament: &'static str,
    /// Maîtrise de départ, sur cent : mêlée, distance, magie
    pub melee: u32,
    pub ranged: u32,
    pub magic: u32,
    /// Ce que la voie donne au premier matin : objet et nombre
    pub kit: &'static [(&'static str, u16)],
    /// Armes au choix (une seule est donnée) ; vide quand la voie n'en propose pas
    pub arsenal: &'static [&'static str],
}

/// Les voies, dans l'ordre du râtelier (`order` des prefabs)
pub const CLASSES: [ClassSheet; 15] = [
    ClassSheet {
        class: PlayerClass::Sorcerer,
        id: "sorcier",
        name: "Sorcier élémentaire",
        family: Family::Magic,
        role: "Vous avez appris les runes sur un gantelet volé, une ligne par hiver.",
        armament: "Gantelet runique (une main) + main libre (incantation et projection de sorts à distance).",
        melee: 0,
        ranged: 15,
        magic: 85,
        kit: &[("flintGauntlet", 1)],
        arsenal: &[],
    },
    ClassSheet {
        class: PlayerClass::Druid,
        id: "druide",
        name: "Druide",
        family: Family::Magic,
        role: "La forêt vous a laissé partir avec un morceau d’elle-même.",
        armament: "Totem (une main) + main libre (magie de la nature et métamorphoses).",
        melee: 5,
        ranged: 0,
        magic: 95,
        kit: &[("flintTotem", 1)],
        arsenal: &[],
    },
    ClassSheet {
        class: PlayerClass::Necromancer,
        id: "necromancien",
        name: "Nécromancien",
        family: Family::Magic,
        role: "On vous a confié la lanterne parce que personne d’autre n’en voulait.",
        armament: "Bâton-lanterne d’âmes (une main) + main libre (invocations et magie de mort).",
        melee: 0,
        ranged: 0,
        magic: 100,
        kit: &[("flintSoulStaff", 1)],
        arsenal: &[],
    },
    ClassSheet {
        class: PlayerClass::Chaplain,
        id: "chapelain",
        name: "Chapelain",
        family: Family::Magic,
        role: "Vous soignez depuis plus longtemps que vous ne priez.",
        armament: "Sceptre (une main) + main libre (magie divine et sorts de soin).",
        melee: 5,
        ranged: 0,
        magic: 95,
        kit: &[("flintScepter", 1)],
        arsenal: &[],
    },
    ClassSheet {
        class: PlayerClass::WarPriest,
        id: "pretreGuerrier",
        name: "Prêtre-guerrier",
        family: Family::Magic,
        role: "Vous bénissez d’une main et vous corrigez de l’autre.",
        armament: "Sceptre + Maillet de combat / Marteau (les deux mains occupées).",
        melee: 40,
        ranged: 0,
        magic: 60,
        kit: &[("flintScepter", 1)],
        arsenal: &["warMallet", "flintMallet"],
    },
    ClassSheet {
        class: PlayerClass::Rogue,
        id: "voleur",
        name: "Voleur",
        family: Family::Ranged,
        role: "Deux métiers, une seule paire de mains : ouvrir, puis disparaître.",
        armament: "Arbalète de poing (une main) + main libre (magie de l’ombre et disparition).",
        melee: 15,
        ranged: 60,
        magic: 25,
        kit: &[("handCrossbow", 1), ("flintBolt", 32)],
        arsenal: &[],
    },
    ClassSheet {
        class: PlayerClass::Ranger,
        id: "rodeur",
        name: "Rôdeur",
        family: Family::Ranged,
        role: "Vous chassez seul, et vous tirez deux fois.",
        armament: "Deux arbalètes de poing (les deux mains occupées).",
        melee: 0,
        ranged: 75,
        magic: 25,
        kit: &[("handCrossbow", 1), ("handCrossbow", 1), ("flintBolt", 32)],
        arsenal: &[],
    },
    ClassSheet {
        class: PlayerClass::Archer,
        id: "archer",
        name: "Archer",
        family: Family::Ranged,
        role: "On vous reconnaît à la corde, pas au visage.",
        armament: "Arc (les deux mains occupées).",
        melee: 0,
        ranged: 100,
        magic: 0,
        kit: &[("shortBow", 1), ("flintArrow", 32)],
        arsenal: &[],
    },
    ClassSheet {
        class: PlayerClass::Engineer,
        id: "ingenieur",
        name: "Ingénieur",
        family: Family::Ranged,
        role: "Vos pièges travaillent pendant que vous réfléchissez.",
        armament: "Trousse du bricoleur (une main) + main libre (déploiement de tourelles et de pièges).",
        melee: 10,
        ranged: 90,
        magic: 0,
        kit: &[("flintTinkerKit", 1)],
        arsenal: &[],
    },
    ClassSheet {
        class: PlayerClass::MonsterHunter,
        id: "chasseur",
        name: "Chasseur de monstres",
        family: Family::Ranged,
        role: "Vous facturez à la tête, et vous rentrez rarement entier.",
        armament: "Arbalète de poing + Épée à une main (les deux mains occupées).",
        melee: 45,
        ranged: 55,
        magic: 0,
        kit: &[("flintSword", 1), ("handCrossbow", 1), ("flintBolt", 32)],
        arsenal: &[],
    },
    ClassSheet {
        class: PlayerClass::WindBlade,
        id: "lameVent",
        name: "Lame du vent",
        family: Family::Melee,
        role: "Le sabre part avant le vent, le vent finit le geste.",
        armament: "Sabre (une main) + main libre (magie de vent pour frapper à distance).",
        melee: 60,
        ranged: 25,
        magic: 15,
        kit: &[("flintSabre", 1)],
        arsenal: &[],
    },
    ClassSheet {
        class: PlayerClass::Champion,
        id: "champion",
        name: "Champion",
        family: Family::Melee,
        role: "Vous tenez la ligne : c’est déjà un métier.",
        armament: "Épée à une main + Bouclier (les deux mains occupées).",
        melee: 100,
        ranged: 0,
        magic: 0,
        kit: &[("flintSword", 1), ("flintShield", 1)],
        arsenal: &[],
    },
    ClassSheet {
        class: PlayerClass::Soldier,
        id: "soldat",
        name: "Soldat",
        family: Family::Melee,
        role: "Vous savez tout tenir, pourvu qu’un bouclier reste à gauche.",
        armament: "Arsenal au choix + Bouclier (les deux mains occupées).",
        melee: 90,
        ranged: 10,
        magic: 0,
        kit: &[("flintShield", 1)],
        arsenal: &["flintSword", "flintSabre", "flintBattleAxe", "flintMallet", "warMallet"],
    },
    ClassSheet {
        class: PlayerClass::Berserker,
        id: "berserker",
        name: "Berserker",
        family: Family::Melee,
        role: "La rage vous coûte moins cher qu’une armure.",
        armament: "Gourdin ou Hache (une main) + main libre (canalisation de la rage magique).",
        melee: 85,
        ranged: 0,
        magic: 15,
        kit: &[],
        arsenal: &["flintBattleAxe", "club"],
    },
    ClassSheet {
        class: PlayerClass::Paladin,
        id: "paladin",
        name: "Paladin",
        family: Family::Melee,
        role: "Votre serment tient tant que votre bras tient.",
        armament: "Épée ou Marteau + Bouclier (les deux mains occupées, magie défensive et lumineuse canalisée à travers ses armes).",
        melee: 70,
        ranged: 0,
        magic: 30,
        kit: &[("flintShield", 1)],
        arsenal: &["flintSword", "flintMallet"],
    },
];

/// La fiche d'une voie ; `None` pour l'aventurier, qui n'en a pas encore choisi
pub fn class_sheet(class: PlayerClass) -> Option<&'static ClassSheet> {
    CLASSES.iter().find(|c| c.class == class)
}

impl ClassSheet {
    /// L'arme que donne la voie : celle choisie si elle est dans l'arsenal, sinon la première ;
    /// `None` sans arsenal
    pub fn weapon(&self, wanted: Option<&str>) -> Option<&'static str> {
        let first = self.arsenal.first().copied()?;
        Some(wanted.and_then(|w| self.arsenal.iter().copied().find(|a| *a == w)).unwrap_or(first))
    }

    /// Ce que la voie met dans le sac au premier matin : l'arme choisie, puis le paquetage
    pub fn starting_items(&self, weapon: Option<&str>) -> Vec<(&'static str, u16)> {
        let mut items: Vec<(&'static str, u16)> = self.weapon(weapon).map(|w| (w, 1)).into_iter().collect();
        items.extend_from_slice(self.kit);
        items
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_class_has_a_sheet_and_a_full_split() {
        for c in &CLASSES {
            assert_eq!(c.melee + c.ranged + c.magic, 100, "{}", c.id);
            assert_eq!(class_sheet(c.class).map(|s| s.id), Some(c.id));
        }
        assert!(class_sheet(PlayerClass::Adventurer).is_none());
    }

    #[test]
    fn every_starting_item_exists() {
        let registry = crate::items::items();
        assert!(!registry.is_empty());
        for c in &CLASSES {
            for item in c.kit.iter().map(|(item, _)| *item).chain(c.arsenal.iter().copied()) {
                assert!(registry.find(item).is_some(), "{} : objet inconnu {item}", c.id);
            }
        }
    }

    #[test]
    fn arsenal_pick_falls_back_to_the_first_weapon() {
        let soldat = CLASSES.iter().find(|c| c.id == "soldat").unwrap();
        assert_eq!(soldat.weapon(Some("warMallet")), Some("warMallet"));
        assert_eq!(soldat.weapon(Some("shortBow")), Some("flintSword"));
        assert_eq!(soldat.starting_items(None), vec![("flintSword", 1), ("flintShield", 1)]);
        let archer = CLASSES.iter().find(|c| c.id == "archer").unwrap();
        assert_eq!(archer.weapon(Some("flintSword")), None);
    }
}
