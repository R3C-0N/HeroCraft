//! L'arme de chaque joueur au combat : cadence de ses coups, chargeur et rechargement de
//! l'arbalète, arc ou compétence qu'il tient chargé, munitions spéciales des prochains tirs.
//!
//! Le même code tourne sur le serveur (qui juge) et dans le client (qui juge en solo, et prévoit
//! en ligne pour l'affichage). Les durées passent plus vite sous la vitesse d'attaque
//! (`BuffStat::AttackSpeed`), que `SkillRunner` lui donne à chaque instant.

use std::collections::HashMap;

use herocraft_common::skills::EffectEntry;
use herocraft_common::weapons::{Held, WeaponKind, bow_power};
use herocraft_common::{CastError, ItemId, PlayerClass, PlayerId};

/// Secondes entre deux carreaux d'un même chargeur (deux arbalètes de poing)
pub(crate) const MAGAZINE_GAP: f32 = 0.2;

/// Une munition spéciale chargée : les `shots` prochains tirs portent ses effets
#[derive(Debug, Clone)]
pub struct Special {
    /// Compétence qui l'a chargée, et son rang
    pub skill: String,
    pub rank: u8,
    pub shots: u32,
    /// Secondes avant qu'elle se perde
    pub left: f32,
    /// Rayon autour de l'impact, en blocs (`None` : la cible seule)
    pub radius: Option<f32>,
    pub effects: Vec<EffectEntry>,
}

/// Ce que tient la touche ou le bouton maintenu : un tir d'arc (`None`) ou une compétence
#[derive(Debug, Clone, PartialEq)]
struct Charging {
    skill: Option<String>,
    held: f32,
}

#[derive(Debug, Clone)]
struct Gear {
    held: Held,
    /// Secondes avant le prochain coup ou tir
    cooldown: f32,
    /// Carreaux prêts dans le chargeur
    loaded: u8,
    /// Secondes de rechargement restantes (0 : rien à recharger)
    reload: f32,
    charging: Option<Charging>,
    specials: Vec<Special>,
}

impl Gear {
    fn new(held: Held) -> Self {
        let mut gear = Gear { held, cooldown: 0.0, loaded: 0, reload: 0.0, charging: None, specials: Vec::new() };
        gear.start_reload();
        gear
    }

    fn is_crossbow(&self) -> bool {
        self.held.kind == Some(WeaponKind::Crossbow)
    }

    /// Chargeur vide : l'arbalète se recharge
    fn start_reload(&mut self) {
        if self.is_crossbow() && self.loaded == 0 && self.reload <= 0.0 {
            self.reload = self.held.interval;
        }
    }
}

/// Un tir parti : sa puissance (arc bandé), et la munition spéciale qu'il emporte
#[derive(Debug, Clone)]
pub struct Fired {
    pub power: f32,
    pub special: Option<Special>,
}

/// État du chargeur pour l'affichage : carreaux prêts, contenance, rechargement restant et total
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Magazine {
    pub loaded: u8,
    pub size: u8,
    pub reload_left: f32,
    pub reload_total: f32,
}

/// Les armes de tous les joueurs
#[derive(Debug, Default)]
pub struct Arsenal {
    gear: HashMap<PlayerId, Gear>,
}

impl Arsenal {
    /// Le joueur `player` tient `item` (`None` : mains nues). Changer d'objet remet la cadence à
    /// zéro et vide le chargeur de l'arbalète, qui se recharge.
    pub fn hold(&mut self, player: PlayerId, item: Option<ItemId>, class: PlayerClass) {
        let held = Held::of(item, class);
        match self.gear.get_mut(&player) {
            Some(gear) if gear.held == held => {}
            Some(gear) => {
                let specials = std::mem::take(&mut gear.specials);
                let cooldown = gear.cooldown;
                *gear = Gear::new(held);
                gear.specials = specials;
                gear.cooldown = cooldown.max(0.0);
            }
            None => {
                self.gear.insert(player, Gear::new(held));
            }
        }
    }

    /// Ce que tient le joueur `player`
    pub fn held(&self, player: PlayerId) -> Held {
        self.gear.get(&player).map_or(Held::of(None, PlayerClass::Adventurer), |g| g.held)
    }

    fn gear(&mut self, player: PlayerId) -> &mut Gear {
        self.gear.entry(player).or_insert_with(|| Gear::new(Held::of(None, PlayerClass::Adventurer)))
    }

    /// Avance de `dt` secondes ; `speed` : facteur de vitesse d'attaque de chaque joueur (1 : aucun)
    pub fn tick(&mut self, dt: f32, speed: impl Fn(PlayerId) -> f32) {
        for (&player, gear) in &mut self.gear {
            let step = dt * speed(player).max(0.1);
            gear.cooldown = (gear.cooldown - step).max(0.0);
            if gear.reload > 0.0 {
                gear.reload -= step;
                if gear.reload <= 0.0 {
                    gear.reload = 0.0;
                    gear.loaded = gear.held.magazine;
                }
            }
            if let Some(c) = &mut gear.charging {
                c.held += dt;
            }
            for s in &mut gear.specials {
                s.left -= dt;
            }
            gear.specials.retain(|s| s.left > 0.0 && s.shots > 0);
        }
    }

    /// Un coup au corps à corps : refusé si le précédent est trop récent ; sinon ses dégâts de base
    /// (ceux de l'arme), et la cadence repart
    pub fn strike(&mut self, player: PlayerId) -> Result<f32, CastError> {
        let gear = self.gear(player);
        if gear.cooldown > 0.0 {
            return Err(CastError::Cooldown);
        }
        gear.cooldown = gear.held.interval;
        Ok(gear.held.melee_damage())
    }

    /// Le joueur commence à tenir chargé un tir d'arc (`skill` : `None`) ou une compétence
    pub fn begin_charge(&mut self, player: PlayerId, skill: Option<&str>) {
        self.gear(player).charging = Some(Charging { skill: skill.map(str::to_string), held: 0.0 });
    }

    /// Secondes que le joueur tient chargé ce tir ou cette compétence (0 : il ne le charge pas)
    pub fn charge_held(&self, player: PlayerId, skill: Option<&str>) -> f32 {
        self.gear.get(&player).and_then(|g| g.charging.as_ref()).filter(|c| c.skill.as_deref() == skill).map_or(0.0, |c| c.held)
    }

    /// Le joueur tient chargé ce tir ou cette compétence
    pub fn is_charging(&self, player: PlayerId, skill: Option<&str>) -> bool {
        self.gear.get(&player).and_then(|g| g.charging.as_ref()).is_some_and(|c| c.skill.as_deref() == skill)
    }

    /// Fin de la charge (relâchée) : ce qu'elle a duré
    pub fn release_charge(&mut self, player: PlayerId, skill: Option<&str>) -> f32 {
        let held = self.charge_held(player, skill);
        if let Some(gear) = self.gear.get_mut(&player).filter(|g| g.charging.as_ref().is_some_and(|c| c.skill.as_deref() == skill)) {
            gear.charging = None;
        }
        held
    }

    /// Un tir d'arc ou d'arbalète part : refusé sans arme à distance, trop tôt, l'arbalète vide
    /// ou l'arc pas assez bandé. Il prend un carreau du chargeur (qui se recharge une fois vide)
    /// et la prochaine munition spéciale.
    pub fn fire(&mut self, player: PlayerId) -> Result<Fired, CastError> {
        let drawn = self.release_charge(player, None);
        let gear = self.gear(player);
        if !gear.held.is_ranged() {
            return Err(CastError::NoWeapon);
        }
        if gear.cooldown > 0.0 {
            return Err(CastError::Cooldown);
        }
        let power = match gear.held.kind {
            Some(WeaponKind::Bow) => bow_power(drawn).ok_or(CastError::NotCharged)?,
            _ => {
                if gear.loaded == 0 {
                    return Err(CastError::Reloading);
                }
                gear.loaded -= 1;
                gear.start_reload();
                1.0
            }
        };
        gear.cooldown = if gear.is_crossbow() { MAGAZINE_GAP } else { gear.held.interval };
        Ok(Fired { power, special: self.take_special(player) })
    }

    /// Une compétence qui tire un carreau (`Cast::uses_round`) : refusée tant que l'arbalète recharge
    pub fn use_round(&mut self, player: PlayerId) -> Result<(), CastError> {
        let gear = self.gear(player);
        if !gear.is_crossbow() {
            return Err(CastError::NoWeapon);
        }
        if gear.loaded == 0 {
            return Err(CastError::Reloading);
        }
        gear.loaded -= 1;
        gear.start_reload();
        Ok(())
    }

    /// Le chargeur se remplit d'un coup (Esquive tactique)
    pub fn reload_now(&mut self, player: PlayerId) {
        let gear = self.gear(player);
        gear.loaded = gear.held.magazine;
        gear.reload = 0.0;
    }

    /// Une munition spéciale chargée par une compétence remplace la précédente
    pub fn load(&mut self, player: PlayerId, special: Special) {
        let gear = self.gear(player);
        gear.specials.clear();
        gear.specials.push(special);
    }

    /// Prend un tir de la munition spéciale chargée, s'il en reste
    pub(crate) fn take_special(&mut self, player: PlayerId) -> Option<Special> {
        let gear = self.gear.get_mut(&player)?;
        let special = gear.specials.first_mut()?;
        special.shots -= 1;
        let one = Special { shots: 1, ..special.clone() };
        gear.specials.retain(|s| s.shots > 0);
        Some(one)
    }

    /// La munition spéciale chargée : compétence et tirs restants
    pub fn special(&self, player: PlayerId) -> Option<(&str, u32)> {
        self.gear.get(&player)?.specials.first().map(|s| (s.skill.as_str(), s.shots))
    }

    /// Le chargeur de l'arbalète tenue, `None` pour une autre arme
    pub fn magazine(&self, player: PlayerId) -> Option<Magazine> {
        let gear = self.gear.get(&player).filter(|g| g.is_crossbow())?;
        Some(Magazine { loaded: gear.loaded, size: gear.held.magazine, reload_left: gear.reload, reload_total: gear.held.interval })
    }

    /// Secondes avant le prochain coup, et l'intervalle de l'arme
    pub fn cooldown(&self, player: PlayerId) -> (f32, f32) {
        self.gear.get(&player).map_or((0.0, 0.0), |g| (g.cooldown, g.held.interval))
    }

    pub(crate) fn forget(&mut self, player: PlayerId) {
        self.gear.remove(&player);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use herocraft_common::items;

    fn item(id: &str) -> Option<ItemId> {
        Some(items().find(id).unwrap())
    }

    #[test]
    fn hits_follow_the_weapon_pace_and_attack_speed() {
        let mut arsenal = Arsenal::default();
        arsenal.hold(PlayerId(1), item("ironDagger"), PlayerClass::Adventurer);
        assert_eq!(arsenal.strike(PlayerId(1)), Ok(10.0));
        assert_eq!(arsenal.strike(PlayerId(1)), Err(CastError::Cooldown));
        arsenal.tick(0.26, |_| 1.0);
        assert!(arsenal.strike(PlayerId(1)).is_ok(), "une dague frappe toutes les 0,25 s");
        arsenal.hold(PlayerId(1), item("ironBattleAxe"), PlayerClass::Adventurer);
        arsenal.tick(0.3, |_| 1.0);
        assert!(arsenal.strike(PlayerId(1)).is_ok());
        arsenal.tick(0.5, |_| 1.0);
        assert_eq!(arsenal.strike(PlayerId(1)), Err(CastError::Cooldown), "une hache de bataille : 0,8 s");
        // 60 % plus vite : 0,5 s suffisent
        arsenal.tick(0.5, |_| 1.6);
        assert!(arsenal.strike(PlayerId(1)).is_ok());
    }

    #[test]
    fn a_crossbow_reloads_after_each_bolt() {
        let mut arsenal = Arsenal::default();
        arsenal.hold(PlayerId(1), item("handCrossbow"), PlayerClass::Adventurer);
        assert_eq!(arsenal.fire(PlayerId(1)).unwrap_err(), CastError::Reloading, "prise en main : elle se charge d'abord");
        arsenal.tick(0.85, |_| 1.0);
        assert_eq!(arsenal.magazine(PlayerId(1)).unwrap().loaded, 1);
        assert!(arsenal.fire(PlayerId(1)).is_ok());
        arsenal.tick(0.3, |_| 1.0);
        assert_eq!(arsenal.fire(PlayerId(1)).unwrap_err(), CastError::Reloading);
        arsenal.tick(0.6, |_| 1.0);
        assert!(arsenal.fire(PlayerId(1)).is_ok());
        // Esquive tactique : rechargée d'un coup
        arsenal.tick(0.25, |_| 1.0);
        arsenal.reload_now(PlayerId(1));
        assert!(arsenal.fire(PlayerId(1)).is_ok());
    }

    #[test]
    fn a_ranger_shoots_twice_before_reloading() {
        let mut arsenal = Arsenal::default();
        arsenal.hold(PlayerId(1), item("handCrossbow"), PlayerClass::Ranger);
        arsenal.reload_now(PlayerId(1));
        assert!(arsenal.fire(PlayerId(1)).is_ok());
        arsenal.tick(MAGAZINE_GAP + 0.01, |_| 1.0);
        assert!(arsenal.fire(PlayerId(1)).is_ok());
        arsenal.tick(MAGAZINE_GAP + 0.01, |_| 1.0);
        assert_eq!(arsenal.fire(PlayerId(1)).unwrap_err(), CastError::Reloading);
    }

    #[test]
    fn a_bow_must_be_drawn() {
        let mut arsenal = Arsenal::default();
        arsenal.hold(PlayerId(1), item("shortBow"), PlayerClass::Adventurer);
        arsenal.begin_charge(PlayerId(1), None);
        arsenal.tick(0.1, |_| 1.0);
        assert_eq!(arsenal.fire(PlayerId(1)).unwrap_err(), CastError::NotCharged);
        arsenal.begin_charge(PlayerId(1), None);
        arsenal.tick(1.2, |_| 1.0);
        assert_eq!(arsenal.fire(PlayerId(1)).unwrap().power, 1.0);
        arsenal.hold(PlayerId(1), item("ironSword"), PlayerClass::Adventurer);
        assert_eq!(arsenal.fire(PlayerId(1)).unwrap_err(), CastError::NoWeapon);
    }

    #[test]
    fn special_ammo_rides_the_next_shots() {
        let mut arsenal = Arsenal::default();
        arsenal.hold(PlayerId(1), item("shortBow"), PlayerClass::Adventurer);
        arsenal.load(PlayerId(1), Special { skill: "poisonBolt".into(), rank: 1, shots: 2, left: 10.0, radius: None, effects: Vec::new() });
        for expected in [Some("poisonBolt"), Some("poisonBolt"), None] {
            arsenal.begin_charge(PlayerId(1), None);
            arsenal.tick(1.0, |_| 1.0);
            assert_eq!(arsenal.fire(PlayerId(1)).unwrap().special.map(|s| s.skill).as_deref(), expected);
        }
    }
}
