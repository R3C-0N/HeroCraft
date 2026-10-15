use serde::{Deserialize, Serialize};

/// Classe de personnage RPG
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlayerClass {
    Adventurer,
    Warrior,
    Mage,
    Ranger,
    Paladin,
    Rogue,
}

/// Attributs primaires du RPG
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Attributes {
    pub strength: u32,
    pub agility: u32,
    pub intelligence: u32,
    pub constitution: u32,
}

impl Default for Attributes {
    fn default() -> Self {
        Self {
            strength: 10,
            agility: 10,
            intelligence: 10,
            constitution: 10,
        }
    }
}

/// Statistiques vitales et progression d'un joueur ou d'un mob
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterStats {
    pub level: u32,
    pub experience: u64,
    pub attributes: Attributes,
    pub class: PlayerClass,

    // Jauges de vie, mana et endurance
    pub hp: f32,
    pub max_hp: f32,
    pub mana: f32,
    pub max_mana: f32,
    pub stamina: f32,
    pub max_stamina: f32,
}

impl CharacterStats {
    pub fn new(class: PlayerClass) -> Self {
        let mut stats = Self {
            level: 1,
            experience: 0,
            attributes: Attributes::default(),
            class,
            hp: 100.0,
            max_hp: 100.0,
            mana: 50.0,
            max_mana: 50.0,
            stamina: 100.0,
            max_stamina: 100.0,
        };
        stats.recalculate();
        stats.hp = stats.max_hp;
        stats.mana = stats.max_mana;
        stats.stamina = stats.max_stamina;
        stats
    }

    /// Recalcule les jauges maximales à partir des attributs et du niveau
    pub fn recalculate(&mut self) {
        self.max_hp = 50.0 + (self.attributes.constitution as f32 * 10.0) + (self.level as f32 * 5.0);
        self.max_mana = 20.0 + (self.attributes.intelligence as f32 * 8.0) + (self.level as f32 * 4.0);
        self.max_stamina = 50.0 + (self.attributes.agility as f32 * 5.0);

        self.hp = self.hp.min(self.max_hp);
        self.mana = self.mana.min(self.max_mana);
        self.stamina = self.stamina.min(self.max_stamina);
    }

    /// Expérience requise pour passer au niveau suivant
    pub fn exp_for_next_level(&self) -> u64 {
        let lvl = self.level as u64;
        100 * lvl * lvl
    }

    /// Ajoute de l'expérience et gère le level up
    pub fn add_experience(&mut self, amount: u64) -> bool {
        self.experience += amount;
        let mut leveled_up = false;

        while self.experience >= self.exp_for_next_level() {
            self.experience -= self.exp_for_next_level();
            self.level += 1;
            leveled_up = true;

            // Gain automatique d'attributs selon la classe
            match self.class {
                PlayerClass::Warrior => {
                    self.attributes.strength += 3;
                    self.attributes.constitution += 2;
                }
                PlayerClass::Mage => {
                    self.attributes.intelligence += 4;
                    self.attributes.constitution += 1;
                }
                PlayerClass::Ranger => {
                    self.attributes.agility += 3;
                    self.attributes.strength += 1;
                    self.attributes.constitution += 1;
                }
                _ => {
                    self.attributes.strength += 1;
                    self.attributes.agility += 1;
                    self.attributes.intelligence += 1;
                    self.attributes.constitution += 1;
                }
            }
        }

        if leveled_up {
            self.recalculate();
            // Restauration complète lors d'un level up
            self.hp = self.max_hp;
            self.mana = self.max_mana;
        }

        leveled_up
    }

    pub fn take_damage(&mut self, amount: f32) -> bool {
        self.hp = (self.hp - amount).max(0.0);
        self.hp == 0.0 // True si mort
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_level_up_progression() {
        let mut hero = CharacterStats::new(PlayerClass::Warrior);
        assert_eq!(hero.level, 1);
        let initial_max_hp = hero.max_hp;

        // 100 XP suffisent pour passer niveau 2
        let leveled = hero.add_experience(100);
        assert!(leveled);
        assert_eq!(hero.level, 2);
        assert!(hero.max_hp > initial_max_hp);
    }
}
