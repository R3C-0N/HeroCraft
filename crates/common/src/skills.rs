//! Roue des compétences : des branches qui se partagent le cercle, et dans chacune des compétences
//! posées sur des anneaux, du centre vers le bord. Chaque niveau donne un point de compétence ;
//! un point apprend une compétence, ou la renforce d'un rang, pourvu que celles qu'elle demande
//! soient déjà apprises.
//!
//! La roue est décrite dans `assets/competences/roue.json`, embarquée dans le jeu et le serveur
//! pour que les deux jugent les mêmes règles.

use std::collections::HashSet;

use serde::Deserialize;

use crate::CharacterStats;

/// Description de la roue, lue à la compilation
const WHEEL_JSON: &str = include_str!("../../../assets/competences/roue.json");

/// Rang maximal d'une compétence, sauf mention contraire dans la roue
pub const DEFAULT_MAX_RANK: u8 = 3;

/// Une part du cercle (une famille de compétences)
#[derive(Debug, Clone, Deserialize)]
pub struct SkillBranch {
    pub id: String,
    pub name: String,
    /// Couleur de la part, en RGB
    pub color: [u8; 3],
}

/// Une compétence de la roue
#[derive(Debug, Clone, Deserialize)]
pub struct SkillNode {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Identifiant de la branche qui la porte
    pub branch: String,
    /// Anneau, compté depuis le centre à partir de 1
    pub ring: u8,
    /// Position dans la part de sa branche, de 0 (début) à 1 (fin)
    #[serde(default = "half")]
    pub slot: f32,
    /// Compétences à connaître avant de pouvoir l'apprendre (toutes)
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default = "default_max_rank")]
    pub max_rank: u8,
}

fn half() -> f32 {
    0.5
}

fn default_max_rank() -> u8 {
    DEFAULT_MAX_RANK
}

/// La roue entière
#[derive(Debug, Clone, Deserialize)]
pub struct SkillWheel {
    pub branches: Vec<SkillBranch>,
    pub nodes: Vec<SkillNode>,
}

/// Pourquoi une compétence ne peut pas être apprise
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LearnError {
    Unknown,
    NoPoint,
    MaxRank,
    MissingPrerequisite,
}

impl LearnError {
    pub fn message(self) -> &'static str {
        match self {
            LearnError::Unknown => "compétence inconnue",
            LearnError::NoPoint => "aucun point à dépenser",
            LearnError::MaxRank => "déjà au rang maximal",
            LearnError::MissingPrerequisite => "prérequis manquant",
        }
    }
}

impl SkillWheel {
    /// La roue du jeu
    pub fn standard() -> Self {
        Self::parse(WHEEL_JSON).expect("assets/competences/roue.json invalide")
    }

    /// Lit une roue et vérifie qu'elle se tient : identifiants uniques, branches et prérequis
    /// existants, anneaux à partir de 1
    pub fn parse(json: &str) -> Result<Self, String> {
        let wheel: SkillWheel = serde_json::from_str(json).map_err(|e| e.to_string())?;
        let mut ids = HashSet::new();
        for node in &wheel.nodes {
            if !ids.insert(node.id.as_str()) {
                return Err(format!("compétence en double : {}", node.id));
            }
            if !wheel.branches.iter().any(|b| b.id == node.branch) {
                return Err(format!("{} : branche inconnue {}", node.id, node.branch));
            }
            if node.ring == 0 || node.max_rank == 0 {
                return Err(format!("{} : anneau et rang maximal commencent à 1", node.id));
            }
        }
        for node in &wheel.nodes {
            if let Some(missing) = node.requires.iter().find(|r| !ids.contains(r.as_str())) {
                return Err(format!("{} : prérequis inconnu {}", node.id, missing));
            }
        }
        Ok(wheel)
    }

    pub fn node(&self, id: &str) -> Option<&SkillNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn branch(&self, id: &str) -> Option<&SkillBranch> {
        self.branches.iter().find(|b| b.id == id)
    }

    /// Nombre d'anneaux de la roue
    pub fn rings(&self) -> u8 {
        self.nodes.iter().map(|n| n.ring).max().unwrap_or(1)
    }

    /// Ce qui empêche d'apprendre `id` (ou de le renforcer), `None` si c'est possible
    pub fn check(&self, stats: &CharacterStats, id: &str) -> Option<LearnError> {
        let Some(node) = self.node(id) else { return Some(LearnError::Unknown) };
        if rank(stats, id) >= node.max_rank {
            return Some(LearnError::MaxRank);
        }
        if node.requires.iter().any(|r| rank(stats, r) == 0) {
            return Some(LearnError::MissingPrerequisite);
        }
        if stats.skill_points == 0 {
            return Some(LearnError::NoPoint);
        }
        None
    }

    /// Dépense un point pour apprendre `id`, ou le renforcer d'un rang : renvoie le nouveau rang
    pub fn learn(&self, stats: &mut CharacterStats, id: &str) -> Result<u8, LearnError> {
        if let Some(error) = self.check(stats, id) {
            return Err(error);
        }
        stats.skill_points -= 1;
        let rank = stats.skills.entry(id.to_string()).or_insert(0);
        *rank += 1;
        Ok(*rank)
    }
}

/// Rang de `id` chez ce personnage (0 : pas apprise)
pub fn rank(stats: &CharacterStats, id: &str) -> u8 {
    stats.skills.get(id).copied().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PlayerClass;

    #[test]
    fn test_standard_wheel_is_valid() {
        let wheel = SkillWheel::standard();
        assert!(!wheel.nodes.is_empty());
        assert!(wheel.rings() >= 1);
    }

    #[test]
    fn test_points_come_from_levels_and_buy_ranks() {
        let wheel = SkillWheel::standard();
        let mut hero = CharacterStats::new(PlayerClass::Warrior);
        assert_eq!(wheel.learn(&mut hero, "frappeLourde"), Err(LearnError::NoPoint));

        // Niveau 4 : trois points
        hero.add_experience(600);
        assert_eq!(hero.skill_points, 3);
        assert_eq!(wheel.learn(&mut hero, "briseGarde"), Err(LearnError::MissingPrerequisite));
        assert_eq!(wheel.learn(&mut hero, "frappeLourde"), Ok(1));
        assert_eq!(wheel.learn(&mut hero, "briseGarde"), Ok(1));
        assert_eq!(wheel.learn(&mut hero, "frappeLourde"), Ok(2));
        assert_eq!(hero.skill_points, 0);
        assert_eq!(wheel.learn(&mut hero, "frappeLourde"), Err(LearnError::NoPoint));
        assert_eq!(wheel.learn(&mut hero, "inconnue"), Err(LearnError::Unknown));

        hero.skill_points = 5;
        assert_eq!(wheel.learn(&mut hero, "frappeLourde"), Ok(3));
        assert_eq!(wheel.learn(&mut hero, "frappeLourde"), Err(LearnError::MaxRank));
        // Ruée frontale demande les deux compétences de l'anneau 2
        assert_eq!(wheel.learn(&mut hero, "rueeFrontale"), Err(LearnError::MissingPrerequisite));
        assert_eq!(wheel.learn(&mut hero, "ondeSismique"), Ok(1));
        assert_eq!(wheel.learn(&mut hero, "rueeFrontale"), Ok(1));
    }

    #[test]
    fn test_parse_rejects_broken_wheels() {
        let branch = r#"{ "id": "a", "name": "A", "color": [0, 0, 0] }"#;
        let bad_branch = format!(r#"{{ "branches": [{branch}], "nodes": [{{ "id": "x", "name": "X", "branch": "b", "ring": 1 }}] }}"#);
        assert!(SkillWheel::parse(&bad_branch).is_err());
        let bad_requires = format!(r#"{{ "branches": [{branch}], "nodes": [{{ "id": "x", "name": "X", "branch": "a", "ring": 1, "requires": ["y"] }}] }}"#);
        assert!(SkillWheel::parse(&bad_requires).is_err());
    }
}
