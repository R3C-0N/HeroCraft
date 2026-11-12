//! Armes : ce que l'objet tenu en main change au combat. Une fiche d'objet peut porter un bloc
//! `weapon` (famille, intervalle entre deux coups, munition, chargeur) et une fiche de munition un
//! champ `ammo` (`arrow` ou `bolt`). Voir `assets/items/README.md`.
//!
//! Un coup porte les « Dégâts » de l'arme ; à mains nues, ou avec un objet qui n'est pas une arme,
//! `BARE_HAND_DAMAGE`. Un arc ou une arbalète tire au clic gauche une munition de l'inventaire :
//! ses dégâts sont ceux de l'arme plus ceux de la munition.

use serde::{Deserialize, Serialize};

use crate::items::{items, ItemId};

/// Dégâts d'un coup à mains nues (ou d'un objet qui n'est pas une arme)
pub const BARE_HAND_DAMAGE: f32 = crate::creature::PLAYER_HIT_DAMAGE;
/// Secondes entre deux coups à mains nues
pub const BARE_HAND_INTERVAL: f32 = 0.4;
/// Dégâts d'un coup de crosse : un arc ou une arbalète au corps à corps, faute de munition
pub const BUTT_DAMAGE: f32 = 1.0;
/// Portée d'un tir d'arc ou d'arbalète, en blocs
pub const SHOT_RANGE_BLOCKS: f32 = 24.0;
/// Largeur du couloir d'un tir, en blocs
pub const SHOT_WIDTH_BLOCKS: f32 = 1.0;
/// Arc bandé : il faut le tenir au moins tant de secondes pour qu'il tire, il est plein au bout de
/// `BOW_FULL_DRAW`, et sa puissance va de `BOW_MIN_POWER` à 1
pub const BOW_MIN_DRAW: f32 = 0.3;
pub const BOW_FULL_DRAW: f32 = 1.0;
pub const BOW_MIN_POWER: f32 = 0.3;
/// Carreaux que tient une arbalète de poing entre les mains d'un Rôdeur (ses deux arbalètes)
pub const RANGER_HAND_MAGAZINE: u8 = 2;

/// Famille d'arme
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WeaponKind {
    /// Épées et sabres
    Sword,
    /// Dagues et couteaux
    Dagger,
    /// Haches et haches de bataille
    Axe,
    /// Masses, maillets, gourdins
    Mace,
    Spear,
    Gauntlet,
    /// Bâtons d'âme, sceptres, totems
    Staff,
    Shield,
    Bow,
    Crossbow,
    /// Pioches, pelles, houes, trousses d'ingénieur
    Tool,
}

impl WeaponKind {
    /// Nom affiché, avec son article (« Il te faut une arbalète »)
    pub fn name(self) -> &'static str {
        match self {
            WeaponKind::Sword => "une épée",
            WeaponKind::Dagger => "une dague",
            WeaponKind::Axe => "une hache",
            WeaponKind::Mace => "une masse",
            WeaponKind::Spear => "une lance",
            WeaponKind::Gauntlet => "un gantelet",
            WeaponKind::Staff => "un bâton",
            WeaponKind::Shield => "un bouclier",
            WeaponKind::Bow => "un arc",
            WeaponKind::Crossbow => "une arbalète",
            WeaponKind::Tool => "un outil",
        }
    }

    /// Un arc ou une arbalète : tire au lieu de frapper
    pub fn is_ranged(self) -> bool {
        matches!(self, WeaponKind::Bow | WeaponKind::Crossbow)
    }
}

/// Munition d'un arc (`arrow`) ou d'une arbalète (`bolt`)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AmmoKind {
    Arrow,
    Bolt,
}

/// Le bloc `weapon` d'une fiche d'objet
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeaponSheet {
    pub kind: WeaponKind,
    /// Secondes entre deux coups ; pour une arbalète, son rechargement ; pour un arc, le temps
    /// minimal entre deux tirs (il se bande en plus)
    pub interval: f32,
    /// Munition tirée (arc et arbalète)
    #[serde(default)]
    pub ammo: Option<AmmoKind>,
    /// Tirs avant de recharger (arbalète)
    #[serde(default = "one")]
    pub magazine: u8,
    /// Blocs de portée en plus au corps à corps (lance)
    #[serde(default)]
    pub reach: f32,
}

fn one() -> u8 {
    1
}

impl WeaponSheet {
    /// Ce qui cloche dans le bloc
    pub fn check(&self) -> Result<(), String> {
        if !(self.interval > 0.0 && self.interval <= 5.0) {
            return Err("weapon.interval de 0 à 5 secondes".into());
        }
        if self.kind.is_ranged() != self.ammo.is_some() {
            return Err("weapon.ammo pour un arc ou une arbalète, et pour eux seuls".into());
        }
        if self.magazine == 0 || !(0.0..=4.0).contains(&self.reach) {
            return Err("weapon.magazine à partir de 1, weapon.reach de 0 à 4".into());
        }
        Ok(())
    }
}

/// L'objet tenu en main, tel que le combat le voit
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Held {
    pub item: Option<ItemId>,
    /// Famille d'arme (`None` : mains nues ou objet qui n'est pas une arme)
    pub kind: Option<WeaponKind>,
    /// Dégâts d'un coup, ou d'un tir avant la munition
    pub damage: f32,
    pub interval: f32,
    pub ammo: Option<AmmoKind>,
    pub magazine: u8,
    pub reach: f32,
}

impl Held {
    /// Ce que vaut au combat l'objet `item` (`None` : mains nues), entre les mains d'un joueur de
    /// voie `class` (une arbalète de poing tient deux carreaux pour un Rôdeur)
    pub fn of(item: Option<ItemId>, class: crate::PlayerClass) -> Self {
        let sheet = item.filter(|i| i.block().is_none()).and_then(|i| items().get(i));
        let Some((sheet, weapon)) = sheet.and_then(|s| s.weapon.as_ref().map(|w| (s, w))) else {
            return Held { item, kind: None, damage: BARE_HAND_DAMAGE, interval: BARE_HAND_INTERVAL, ammo: None, magazine: 1, reach: 0.0 };
        };
        let damage = sheet.stats.get(DAMAGE_STAT).copied().filter(|d| *d > 0.0).unwrap_or(BARE_HAND_DAMAGE);
        let mut magazine = weapon.magazine;
        if class == crate::PlayerClass::Ranger && sheet.id == "handCrossbow" {
            magazine = magazine.max(RANGER_HAND_MAGAZINE);
        }
        Held { item, kind: Some(weapon.kind), damage, interval: weapon.interval, ammo: weapon.ammo, magazine, reach: weapon.reach }
    }

    /// Un arc ou une arbalète
    pub fn is_ranged(&self) -> bool {
        self.kind.is_some_and(WeaponKind::is_ranged)
    }

    /// Dégâts d'un coup au corps à corps : ceux de l'arme, un coup de crosse pour un arc ou une arbalète
    pub fn melee_damage(&self) -> f32 {
        if self.is_ranged() {
            BUTT_DAMAGE
        } else {
            self.damage
        }
    }
}

/// Caractéristique des fiches qui donne les dégâts d'une arme ou d'une munition
pub const DAMAGE_STAT: &str = "Dégâts";

/// Dégâts qu'ajoute la munition `item` à un tir
pub fn ammo_damage(item: ItemId) -> f32 {
    items().get(item).and_then(|s| s.stats.get(DAMAGE_STAT)).copied().unwrap_or(0.0)
}

/// Puissance d'un arc tenu bandé `held` secondes (`None` : pas assez pour tirer)
pub fn bow_power(held: f32) -> Option<f32> {
    if held + 1e-3 < BOW_MIN_DRAW {
        return None;
    }
    let t = ((held - BOW_MIN_DRAW) / (BOW_FULL_DRAW - BOW_MIN_DRAW)).clamp(0.0, 1.0);
    Some(BOW_MIN_POWER + (1.0 - BOW_MIN_POWER) * t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PlayerClass;

    fn held(id: &str) -> Held {
        Held::of(Some(items().find(id).unwrap()), PlayerClass::Adventurer)
    }

    #[test]
    fn every_weapon_has_its_family() {
        for sheet in items().iter().filter(|s| s.category == "Armes" || s.category == "Outils") {
            assert!(sheet.weapon.is_some() || sheet.ammo.is_some(), "{} : ni arme ni munition", sheet.id);
        }
        assert_eq!(held("flintSword").kind, Some(WeaponKind::Sword));
        assert_eq!(held("ironDagger").kind, Some(WeaponKind::Dagger));
        assert_eq!(held("steelBattleAxe").kind, Some(WeaponKind::Axe));
        assert_eq!(held("shortBow").kind, Some(WeaponKind::Bow));
        assert_eq!(held("handCrossbow").ammo, Some(AmmoKind::Bolt));
        assert_eq!(items().get(items().find("ironArrow").unwrap()).unwrap().ammo, Some(AmmoKind::Arrow));
    }

    #[test]
    fn a_hit_carries_the_weapon_damage_and_pace() {
        let bare = Held::of(None, PlayerClass::Adventurer);
        assert_eq!((bare.damage, bare.interval, bare.kind), (BARE_HAND_DAMAGE, BARE_HAND_INTERVAL, None));
        let stone = Held::of(Some(ItemId::from(crate::BlockId::STONE)), PlayerClass::Adventurer);
        assert_eq!(stone.damage, BARE_HAND_DAMAGE, "un bloc n'est pas une arme");
        assert_eq!(held("flintSword").damage, 4.0);
        assert_eq!(held("adamantineSword").damage, 18.0);
        assert!(held("ironDagger").interval < held("ironSword").interval);
        assert!(held("ironSword").interval < held("ironBattleAxe").interval);
        assert!(held("ironSpear").reach > 0.0);
        assert_eq!(held("shortBow").melee_damage(), BUTT_DAMAGE, "un arc au corps à corps : coup de crosse");
    }

    #[test]
    fn a_ranger_holds_two_hand_crossbows() {
        let crossbow = Some(items().find("handCrossbow").unwrap());
        assert_eq!(Held::of(crossbow, PlayerClass::Adventurer).magazine, 1);
        assert_eq!(Held::of(crossbow, PlayerClass::Ranger).magazine, 2);
    }

    #[test]
    fn a_bow_draws_from_weak_to_full() {
        assert_eq!(bow_power(0.1), None);
        assert_eq!(bow_power(BOW_MIN_DRAW), Some(BOW_MIN_POWER));
        assert_eq!(bow_power(BOW_FULL_DRAW), Some(1.0));
        assert_eq!(bow_power(5.0), Some(1.0));
    }

    #[test]
    fn invalid_weapon_blocks_are_explained() {
        let parse = |w: &str| serde_json::from_str::<WeaponSheet>(w).unwrap().check();
        assert!(parse(r#"{"kind":"sword","interval":0.4}"#).is_ok());
        assert!(parse(r#"{"kind":"sword","interval":0}"#).is_err());
        assert!(parse(r#"{"kind":"bow","interval":0.3}"#).is_err(), "un arc sans munition");
        assert!(parse(r#"{"kind":"sword","interval":0.4,"ammo":"arrow"}"#).is_err());
        assert!(serde_json::from_str::<WeaponSheet>(r#"{"kind":"fusil","interval":1}"#).is_err());
    }
}
