use serde::{Deserialize, Serialize};

/// Hauteur de chute sans dommage, en blocs (un saut depuis le bord d'une marche n'en fait pas)
pub const SAFE_FALL_BLOCKS: f32 = 5.0;
/// Points de vie perdus par bloc de chute au-delà de `SAFE_FALL_BLOCKS`
pub const FALL_DAMAGE_PER_BLOCK: f32 = 8.0;
/// Endurance consommée par seconde de sprint
pub const SPRINT_STAMINA_COST: f32 = 15.0;

/// Satiété d'un ventre plein (la jauge de faim)
pub const MAX_SATIETY: f32 = 100.0;
/// Satiété perdue par seconde, au repos comme en marchant : un ventre plein tient 25 minutes
pub const SATIETY_DRAIN: f32 = MAX_SATIETY / (25.0 * 60.0);
/// Satiété perdue en plus par seconde de sprint (un ventre plein tient 6 minutes de course)
pub const SPRINT_SATIETY_COST: f32 = 0.2;
/// Sous cette fraction de la satiété, le joueur a faim : sa vie ne remonte plus
pub const HUNGRY_FRACTION: f32 = 0.25;
/// Le joueur affamé (satiété à zéro) perd autant de points de vie par seconde…
pub const STARVATION_DAMAGE: f32 = 0.5;
/// … jusqu'à cette fraction de sa vie : la faim affaiblit, mais ne tue pas
pub const STARVATION_FLOOR: f32 = 0.1;

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
    if blocks.is_finite() { (blocks - SAFE_FALL_BLOCKS).max(0.0) * FALL_DAMAGE_PER_BLOCK } else { 0.0 }
}

/// Ce que la faim fait au joueur, selon sa satiété
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hunger {
    /// Rassasié : la vie et l'endurance remontent normalement
    Fed,
    /// Sous `HUNGRY_FRACTION` : la vie ne remonte plus, l'endurance deux fois moins vite
    Hungry,
    /// Satiété à zéro : en plus, la vie baisse jusqu'à `STARVATION_FLOOR`
    Starving,
}

fn full_satiety() -> f32 {
    MAX_SATIETY
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
        Self { strength: 10, agility: 10, intelligence: 10, constitution: 10 }
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
    /// Jauge de faim : pleine, le joueur est rassasié ; elle baisse avec le temps et l'effort et se
    /// remplit en mangeant
    #[serde(default = "full_satiety")]
    pub satiety: f32,
    #[serde(default = "full_satiety")]
    pub max_satiety: f32,

    /// Points de compétence gagnés et pas encore investis dans la roue (un par niveau)
    #[serde(default)]
    pub skill_points: u32,
    /// Compétences apprises dans la roue, par identifiant, et leur rang (1 à `SkillNode::max_rank`)
    #[serde(default)]
    pub skills: std::collections::BTreeMap<String, u8>,
    /// Bouclier d'une compétence : points de dégâts qu'il absorbe encore, et temps qu'il dure
    #[serde(default)]
    pub shield: f32,
    #[serde(default)]
    pub shield_time: f32,
    /// Part des dégâts absorbés par le bouclier qu'il rend en mana (0 : rien)
    #[serde(default)]
    pub shield_mana: f32,
    /// Type des coups que le bouclier absorbe (aucun : tous)
    #[serde(default)]
    pub shield_kind: Option<crate::DamageKind>,
    /// Ralenti : part de la vitesse perdue (0 à 1), et pour combien de secondes encore
    #[serde(default)]
    pub slow: f32,
    #[serde(default)]
    pub slow_time: f32,
    /// Étourdi : ni pas, ni saut, ni compétence, pour tant de secondes encore
    #[serde(default)]
    pub stun_time: f32,
    /// Invisible aux yeux des bêtes pour tant de secondes encore (une attaque le dévoile)
    #[serde(default)]
    pub invisible_time: f32,
    /// Tombé au combat et pas encore réapparu : une résurrection peut le relever sur place
    #[serde(default)]
    pub fallen: bool,
    /// Nombre de fois qu'une compétence l'a relevé : son client voit ainsi qu'il revient à la vie
    #[serde(default)]
    pub revived: u32,
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
            satiety: MAX_SATIETY,
            max_satiety: MAX_SATIETY,
            skill_points: 0,
            skills: std::collections::BTreeMap::new(),
            shield: 0.0,
            shield_time: 0.0,
            shield_mana: 0.0,
            shield_kind: None,
            slow: 0.0,
            slow_time: 0.0,
            stun_time: 0.0,
            invisible_time: 0.0,
            fallen: false,
            revived: 0,
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
        self.take_hit(amount, None)
    }

    /// Comme `take_damage`, pour un coup de type `kind` : un bouclier qui ne vaut que contre un
    /// type (`shield_kind`) laisse passer les autres, et ce qui n'a pas de type (une chute)
    pub fn take_hit(&mut self, amount: f32, kind: Option<crate::DamageKind>) -> bool {
        // Le bouclier d'une compétence encaisse d'abord, s'il vaut contre ce coup
        let shielded = self.shield_kind.is_none() || self.shield_kind == kind;
        let absorbed = if shielded { amount.min(self.shield).max(0.0) } else { 0.0 };
        self.shield -= absorbed;
        self.mana = (self.mana + absorbed * self.shield_mana).min(self.max_mana);
        let amount = amount - absorbed;
        self.hp = (self.hp - amount).max(0.0);
        self.hp == 0.0 // True si mort
    }

    /// Avance les jauges de `dt` secondes : la satiété baisse (plus vite au sprint), le sprint
    /// consomme l'endurance, qui se régénère (+10 par sec) sinon, le mana remonte (+2 par sec).
    /// La vie remonte (+1 par sec) tant que le joueur n'a pas faim ; affamé, il en perd.
    pub fn regenerate(&mut self, dt: f32, sprinting: bool) {
        self.shield_time = (self.shield_time - dt).max(0.0);
        if self.shield_time <= 0.0 {
            self.shield = 0.0;
            self.shield_mana = 0.0;
            self.shield_kind = None;
        }
        self.tick_status(dt);
        let drain = SATIETY_DRAIN + if sprinting { SPRINT_SATIETY_COST } else { 0.0 };
        self.satiety = (self.satiety - drain * dt).clamp(0.0, self.max_satiety);
        let hunger = self.hunger();

        let stamina = match (sprinting, hunger) {
            (true, _) => -SPRINT_STAMINA_COST,
            (false, Hunger::Fed) => 10.0,
            (false, _) => 5.0,
        };
        self.stamina = (self.stamina + stamina * dt).clamp(0.0, self.max_stamina);
        self.mana = (self.mana + 2.0 * dt).min(self.max_mana);
        if self.hp > 0.0 {
            match hunger {
                Hunger::Fed => self.hp = (self.hp + 1.0 * dt).min(self.max_hp),
                Hunger::Hungry => {}
                Hunger::Starving => {
                    let floor = STARVATION_FLOOR * self.max_hp;
                    if self.hp > floor {
                        self.hp = (self.hp - STARVATION_DAMAGE * dt).max(floor);
                    }
                }
            }
        }
    }

    /// Le temps passe sur le ralenti, l'étourdissement et l'invisibilité
    pub fn tick_status(&mut self, dt: f32) {
        self.slow_time = (self.slow_time - dt).max(0.0);
        if self.slow_time <= 0.0 {
            self.slow = 0.0;
        }
        self.stun_time = (self.stun_time - dt).max(0.0);
        self.invisible_time = (self.invisible_time - dt).max(0.0);
    }

    /// Ralenti de `strength` (0 à 1) pendant `seconds` : le plus fort et le plus long l'emportent
    pub fn apply_slow(&mut self, strength: f32, seconds: f32) {
        self.slow = self.slow.max(strength.clamp(0.0, 1.0));
        self.slow_time = self.slow_time.max(seconds);
    }

    /// Retire le ralenti et l'étourdissement (Purification)
    pub fn cleanse(&mut self) {
        self.slow = 0.0;
        self.slow_time = 0.0;
        self.stun_time = 0.0;
    }

    pub fn apply_stun(&mut self, seconds: f32) {
        self.stun_time = self.stun_time.max(seconds);
    }

    pub fn is_stunned(&self) -> bool {
        self.stun_time > 0.0
    }

    pub fn is_invisible(&self) -> bool {
        self.invisible_time > 0.0
    }

    /// Allure que laissent le ralenti et l'étourdissement (1 : pleine, 0 : immobile)
    pub fn status_speed(&self) -> f32 {
        if self.is_stunned() { 0.0 } else { 1.0 - self.slow }
    }

    /// Relevé par une compétence avec `fraction` de sa vie, s'il est tombé
    pub fn resurrect(&mut self, fraction: f32) -> bool {
        if !self.fallen {
            return false;
        }
        self.fallen = false;
        self.hp = (self.max_hp * fraction.clamp(0.05, 1.0)).max(1.0);
        self.revived += 1;
        true
    }

    /// Ce que la faim fait au joueur en ce moment
    pub fn hunger(&self) -> Hunger {
        if self.satiety <= 0.0 {
            Hunger::Starving
        } else if self.satiety < HUNGRY_FRACTION * self.max_satiety {
            Hunger::Hungry
        } else {
            Hunger::Fed
        }
    }

    /// Mange un aliment qui rend `satiety` points : renvoie faux (rien n'est mangé) si ce n'est pas
    /// un aliment ou si le joueur n'a plus faim
    pub fn eat(&mut self, satiety: f32) -> bool {
        if (satiety.is_nan() || satiety <= 0.0) || self.satiety >= self.max_satiety - 0.5 {
            return false;
        }
        self.satiety = (self.satiety + satiety).min(self.max_satiety);
        true
    }

    /// Retour à la vie après une mort : jauges pleines, niveau et expérience conservés
    pub fn revive(&mut self) {
        self.hp = self.max_hp;
        self.mana = self.max_mana;
        self.stamina = self.max_stamina;
        self.satiety = self.max_satiety;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hunger_drains_slowly_and_faster_when_sprinting() {
        let mut hero = CharacterStats::new(PlayerClass::Soldier);
        assert_eq!(hero.hunger(), Hunger::Fed);
        // Dix minutes de marche : il reste plus de la moitié
        hero.regenerate(600.0, false);
        assert!((55.0..65.0).contains(&hero.satiety), "{}", hero.satiety);
        let mut runner = CharacterStats::new(PlayerClass::Soldier);
        for _ in 0..600 {
            runner.regenerate(1.0, true);
        }
        assert_eq!(runner.satiety, 0.0, "dix minutes de course vident le ventre");
        assert_eq!(runner.hunger(), Hunger::Starving);
    }

    #[test]
    fn test_hungry_player_stops_healing_then_weakens_without_dying() {
        let mut hero = CharacterStats::new(PlayerClass::Soldier);
        hero.take_damage(50.0);
        let hurt = hero.hp;
        hero.satiety = 0.2 * hero.max_satiety;
        assert_eq!(hero.hunger(), Hunger::Hungry);
        hero.regenerate(1.0, false);
        assert_eq!(hero.hp, hurt, "affamé, la vie ne remonte plus");

        hero.satiety = 0.0;
        hero.regenerate(10.0, false);
        assert!(hero.hp < hurt, "le ventre vide affaiblit");
        for _ in 0..3600 {
            hero.regenerate(1.0, false);
        }
        assert!((hero.hp - STARVATION_FLOOR * hero.max_hp).abs() < 1e-3, "{}", hero.hp);

        // Un repas, et la vie remonte
        assert!(hero.eat(40.0));
        assert_eq!(hero.hunger(), Hunger::Fed);
        let weak = hero.hp;
        hero.regenerate(1.0, false);
        assert!(hero.hp > weak);
    }

    #[test]
    fn test_eating_fills_up_to_full_only() {
        let mut hero = CharacterStats::new(PlayerClass::Soldier);
        assert!(!hero.eat(10.0), "ventre plein : on ne mange pas");
        hero.satiety = 90.0;
        assert!(hero.eat(40.0));
        assert_eq!(hero.satiety, hero.max_satiety);
        hero.satiety = 10.0;
        assert!(!hero.eat(0.0) && !hero.eat(f32::NAN));
        hero.revive();
        assert_eq!(hero.satiety, hero.max_satiety);
    }

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
