use serde::{Deserialize, Serialize};

/// Hauteur de chute sans dommage, en blocs (un saut depuis le bord d'une marche n'en fait pas)
pub const SAFE_FALL_BLOCKS: f32 = 5.0;
/// Points de vie perdus par bloc de chute au-delà de `SAFE_FALL_BLOCKS`
pub const FALL_DAMAGE_PER_BLOCK: f32 = 8.0;
/// Endurance consommée par seconde de sprint
pub const SPRINT_STAMINA_COST: f32 = 15.0;

/// Expérience du passage au niveau 2 ; chaque niveau suivant en coûte autant de plus
pub const EXPERIENCE_STEP: u64 = 100;
/// Points de compétence qu'apporte chaque niveau
pub const SKILL_POINTS_PER_LEVEL: u32 = 1;

/// Voie du personnage : l'aventurier n'en a pas encore choisi, les quinze autres sont celles du
/// râtelier du créateur de légende (voir `hero::CLASSES`, qui donne leur fiche)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PlayerClass {
    Adventurer,
    Sorcerer,
    Druid,
    Necromancer,
    Chaplain,
    WarPriest,
    Rogue,
    Ranger,
    Archer,
    Engineer,
    MonsterHunter,
    WindBlade,
    Champion,
    Soldier,
    Berserker,
    Paladin,
}

impl PlayerClass {
    /// Nom affiché de la voie
    pub fn label(self) -> &'static str {
        crate::hero::class_sheet(self).map_or("Aventurier", |sheet| sheet.name)
    }

    /// Famille de la voie ; `None` pour l'aventurier
    pub fn family(self) -> Option<crate::hero::Family> {
        crate::hero::class_sheet(self).map(|sheet| sheet.family)
    }
}

/// Points de vie perdus pour une chute de `blocks` blocs
pub fn fall_damage(blocks: f32) -> f32 {
    if blocks.is_finite() {
        (blocks - SAFE_FALL_BLOCKS).max(0.0) * FALL_DAMAGE_PER_BLOCK
    } else {
        0.0
    }
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

    /// Points de compétence gagnés et pas encore investis dans la roue (un par niveau)
    #[serde(default)]
    pub skill_points: u32,
    /// Compétences apprises dans la roue, par identifiant, et leur rang (1 à `SkillNode::max_rank`)
    #[serde(default)]
    pub skills: std::collections::BTreeMap<String, u8>,
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
            skill_points: 0,
            skills: std::collections::BTreeMap::new(),
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

    /// Expérience requise pour passer au niveau suivant : 100 pour le niveau 2, et chaque palier
    /// coûte 100 de plus que le précédent (la progression de la branche Java)
    pub fn exp_for_next_level(&self) -> u64 {
        EXPERIENCE_STEP * self.level.max(1) as u64
    }

    /// Ajoute de l'expérience et gère le level up : chaque niveau gagné donne
    /// `SKILL_POINTS_PER_LEVEL` point(s) de compétence
    pub fn add_experience(&mut self, amount: u64) -> bool {
        self.experience += amount;
        let mut leveled_up = false;

        while self.experience >= self.exp_for_next_level() {
            self.experience -= self.exp_for_next_level();
            self.level += 1;
            self.skill_points += SKILL_POINTS_PER_LEVEL;
            leveled_up = true;

            // Gain automatique d'attributs selon la famille de la voie
            match self.class.family() {
                Some(crate::hero::Family::Melee) => {
                    self.attributes.strength += 3;
                    self.attributes.constitution += 2;
                }
                Some(crate::hero::Family::Magic) => {
                    self.attributes.intelligence += 4;
                    self.attributes.constitution += 1;
                }
                Some(crate::hero::Family::Ranged) => {
                    self.attributes.agility += 3;
                    self.attributes.strength += 1;
                    self.attributes.constitution += 1;
                }
                None => {
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

    /// Avance les jauges de `dt` secondes : le sprint consomme l'endurance, qui se régénère
    /// (+10 par sec) sinon ; la vie (+1 par sec) et le mana (+2 par sec) remontent toujours
    pub fn regenerate(&mut self, dt: f32, sprinting: bool) {
        let stamina = if sprinting { -SPRINT_STAMINA_COST } else { 10.0 };
        self.stamina = (self.stamina + stamina * dt).clamp(0.0, self.max_stamina);
        self.mana = (self.mana + 2.0 * dt).min(self.max_mana);
        if self.hp > 0.0 {
            self.hp = (self.hp + 1.0 * dt).min(self.max_hp);
        }
    }

    /// Retour à la vie après une mort : jauges pleines, niveau et expérience conservés
    pub fn revive(&mut self) {
        self.hp = self.max_hp;
        self.mana = self.max_mana;
        self.stamina = self.max_stamina;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_level_up_progression() {
        let mut hero = CharacterStats::new(PlayerClass::Soldier);
        assert_eq!(hero.level, 1);
        let initial_max_hp = hero.max_hp;

        // 100 XP suffisent pour passer niveau 2
        let leveled = hero.add_experience(100);
        assert!(leveled);
        assert_eq!(hero.level, 2);
        assert!(hero.max_hp > initial_max_hp);
        assert_eq!(hero.skill_points, 1);
    }

    #[test]
    fn test_each_level_costs_one_step_more_and_brings_a_point() {
        let mut hero = CharacterStats::new(PlayerClass::Soldier);
        // Niveaux 2, 3 et 4 : 100 + 200 + 300
        hero.add_experience(599);
        assert_eq!((hero.level, hero.experience, hero.skill_points), (3, 299, 2));
        hero.add_experience(1);
        assert_eq!((hero.level, hero.experience, hero.skill_points), (4, 0, 3));
        assert_eq!(hero.exp_for_next_level(), 400);
    }

    #[test]
    fn test_fall_damage_starts_past_safe_height() {
        assert_eq!(fall_damage(0.0), 0.0);
        assert_eq!(fall_damage(SAFE_FALL_BLOCKS), 0.0);
        assert_eq!(fall_damage(SAFE_FALL_BLOCKS + 2.0), 2.0 * FALL_DAMAGE_PER_BLOCK);
        assert_eq!(fall_damage(f32::NAN), 0.0);
        // Une très grande chute tue un guerrier débutant
        let mut hero = CharacterStats::new(PlayerClass::Soldier);
        assert!(hero.take_damage(fall_damage(40.0)));
    }

    #[test]
    fn test_sprint_drains_stamina_and_rest_restores_it() {
        let mut hero = CharacterStats::new(PlayerClass::Soldier);
        hero.regenerate(2.0, true);
        assert!((hero.stamina - (hero.max_stamina - 2.0 * SPRINT_STAMINA_COST)).abs() < 1e-3);
        hero.regenerate(100.0, true);
        assert_eq!(hero.stamina, 0.0);
        hero.regenerate(100.0, false);
        assert_eq!(hero.stamina, hero.max_stamina);
    }
}
