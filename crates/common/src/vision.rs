//! Le tutoriel de la vision (bible, section 8, « L'éclat ») : le héros n'y est d'abord qu'un
//! soldat, sans compétence ; en ramassant l'éclat de la Pierre du Néant, il s'éveille et les
//! premières compétences de sa voie s'ouvrent. Au retour au présent, il retrouve les siennes.
//!
//! Les mêmes règles servent au solo (le client) et en ligne (le serveur, qui tient les fiches).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::character::{CharacterStats, PlayerClass};
use crate::skills::skills;

/// Bloc de la roche où luit l'éclat du tutoriel : elle ne cède qu'à la pioche
pub const SHARD_ROCK: &str = "rocheEclat";

/// Branche de la roue des compétences d'une voie. L'aventurier, qui n'en a pas encore choisi,
/// vit la vision en soldat.
pub fn skill_branch(class: PlayerClass) -> &'static str {
    match class {
        PlayerClass::Adventurer | PlayerClass::Soldier => "soldier",
        PlayerClass::Sorcerer => "elementalist",
        PlayerClass::Druid => "druid",
        PlayerClass::Necromancer => "necromancer",
        PlayerClass::Chaplain => "chaplain",
        PlayerClass::WarPriest => "warPriest",
        PlayerClass::Rogue => "thief",
        PlayerClass::Ranger => "ranger",
        PlayerClass::Archer => "archer",
        PlayerClass::Engineer => "engineer",
        PlayerClass::MonsterHunter => "monsterHunter",
        PlayerClass::WindBlade => "windBlade",
        PlayerClass::Champion => "champion",
        PlayerClass::Berserker => "berserker",
        PlayerClass::Paladin => "paladin",
    }
}

/// Les compétences du présent, mises de côté le temps de la vision
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PresentSkills {
    pub skills: BTreeMap<String, u8>,
    pub skill_points: u32,
}

impl PresentSkills {
    /// Le héros entre dans la vision en simple soldat : ses compétences sont mises de côté
    pub fn set_aside(stats: &mut CharacterStats) -> Self {
        Self { skills: std::mem::take(&mut stats.skills), skill_points: std::mem::take(&mut stats.skill_points) }
    }

    /// Il revient au présent : il retrouve les siennes, celles de la vision s'effacent
    pub fn restore(self, stats: &mut CharacterStats) {
        stats.skills = self.skills;
        stats.skill_points = self.skill_points;
    }

    /// Il revient au présent après avoir pris la force du cristal : il retrouve les siennes et
    /// garde celles que l'éclat a éveillées (au plus haut rang des deux)
    pub fn keep_awakened(self, stats: &mut CharacterStats) {
        let awakened = std::mem::replace(&mut stats.skills, self.skills);
        for (id, rank) in awakened {
            let kept = stats.skills.entry(id).or_insert(0);
            *kept = (*kept).max(rank);
        }
        stats.skill_points = self.skill_points;
    }
}

/// Les nœuds du premier palier de la voie, sans prérequis : ce que l'éclat éveille
pub fn first_skills(class: PlayerClass) -> Vec<String> {
    let branch = skill_branch(class);
    skills().wheel().nodes.into_iter().filter(|n| n.branch == branch && n.tier == 1 && n.requires.is_empty()).map(|n| n.id).collect()
}

/// L'éclat est ramassé : le soldat devient un héros, les premières compétences de sa voie
/// s'ouvrent (au premier rang). Rend leurs noms, dans l'ordre de la roue.
pub fn awaken(stats: &mut CharacterStats) -> Vec<String> {
    let wheel = skills().wheel();
    let mut names = Vec::new();
    for id in first_skills(stats.class) {
        let rank = stats.skills.entry(id.clone()).or_insert(0);
        if *rank == 0 {
            *rank = 1;
            if let Some(node) = wheel.nodes.iter().find(|n| n.id == id) {
                names.push(node.name.clone());
            }
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_class_has_a_branch_with_first_skills() {
        let wheel = skills().wheel();
        for class in [PlayerClass::Adventurer, PlayerClass::Sorcerer, PlayerClass::Rogue, PlayerClass::Paladin, PlayerClass::Soldier] {
            let branch = skill_branch(class);
            assert!(wheel.branches.iter().any(|b| b.id == branch), "{branch}");
            assert!(!first_skills(class).is_empty(), "{branch}");
        }
    }

    #[test]
    fn the_soldier_awakens_then_gets_his_skills_back() {
        let mut stats = CharacterStats::new(PlayerClass::Soldier);
        stats.skills.insert("seismicBreach".into(), 2);
        stats.skill_points = 3;
        let present = PresentSkills::set_aside(&mut stats);
        assert!(stats.skills.is_empty() && stats.skill_points == 0, "un simple soldat");
        let names = awaken(&mut stats);
        assert_eq!(names.len(), first_skills(PlayerClass::Soldier).len());
        assert!(stats.skills.contains_key("heavyStrike"));
        assert!(skills().castable(&stats).next().is_some(), "des compétences à lancer");
        assert!(awaken(&mut stats).is_empty(), "une seule fois");
        present.restore(&mut stats);
        assert_eq!(stats.skills.get("seismicBreach"), Some(&2));
        assert!(!stats.skills.contains_key("heavyStrike"));
        assert_eq!(stats.skill_points, 3);
    }

    #[test]
    fn the_hero_who_took_the_crystal_keeps_its_power() {
        let mut stats = CharacterStats::new(PlayerClass::Soldier);
        stats.skills.insert("seismicBreach".into(), 2);
        stats.skills.insert("heavyStrike".into(), 3);
        stats.skill_points = 3;
        let present = PresentSkills::set_aside(&mut stats);
        awaken(&mut stats);
        present.keep_awakened(&mut stats);
        assert_eq!(stats.skills.get("seismicBreach"), Some(&2));
        assert_eq!(stats.skills.get("heavyStrike"), Some(&3), "le rang le plus haut");
        for id in first_skills(PlayerClass::Soldier) {
            assert!(stats.skills.get(&id).is_some_and(|&r| r >= 1), "{id} gardée");
        }
        assert_eq!(stats.skill_points, 3);
    }
}
