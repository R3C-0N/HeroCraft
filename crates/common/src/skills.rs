//! Roue des compétences : les quinze classes se partagent le cercle, et dans chacune ses douze
//! compétences sont posées sur des anneaux, du niveau 1 (au centre) à l'ultime (au bord), en
//! passant par le niveau 2 et les fusions. Chaque niveau du héros donne un point de compétence ;
//! un point apprend une compétence, de n'importe quelle classe, ou la renforce d'un rang, pourvu
//! que celles qu'elle demande soient déjà apprises.
//!
//! La roue est décrite dans `assets/competences/roue.json`, reprise du carnet Planner par
//! `tools/importer_roue_competences.py`, et embarquée dans le jeu et le serveur pour que les deux
//! jugent les mêmes règles.

use std::collections::HashSet;

use serde::Deserialize;

use crate::CharacterStats;

/// Description de la roue, lue à la compilation
const WHEEL_JSON: &str = include_str!("../../../assets/competences/roue.json");

/// Rang maximal d'une compétence, sauf mention contraire dans la roue
pub const DEFAULT_MAX_RANK: u8 = 3;

/// Une part du cercle : une classe
#[derive(Debug, Clone, Deserialize)]
pub struct SkillBranch {
    pub id: String,
    pub name: String,
    /// Famille de la classe : `magic`, `ranged` ou `melee`
    #[serde(default)]
    pub family: String,
    /// Répartition de départ de la classe entre mêlée, distance et magie (sur 100)
    #[serde(default)]
    pub mix: [u8; 3],
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
    /// Palier : 1 et 2 pour les niveaux 1 et 2, 3 pour les fusions, 4 pour l'ultime
    pub tier: u8,
    /// Distance au centre sur la roue, en anneaux (1 : le plus proche) ; le palier par défaut
    #[serde(default)]
    pub ring: Option<f32>,
    /// Position dans la part de sa branche, de 0 (début) à 1 (fin)
    #[serde(default = "half")]
    pub slot: f32,
    /// Compétences à connaître avant de pouvoir l'apprendre (toutes)
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default = "default_max_rank")]
    pub max_rank: u8,
    /// Numéro de la fiche du carnet Planner qui la décrit
    #[serde(default)]
    pub planner: Option<u32>,
}

impl SkillNode {
    /// Anneau où la dessiner
    pub fn ring(&self) -> f32 {
        self.ring.unwrap_or(self.tier as f32)
    }

    /// Nom du palier, pour sa fiche
    pub fn tier_label(&self) -> &'static str {
        match self.tier {
            1 => "Niveau 1",
            2 => "Niveau 2",
            3 => "Fusion",
            _ => "Ultime",
        }
    }

    pub fn is_ultimate(&self) -> bool {
        self.tier >= 4
    }
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
            if !(1..=4).contains(&node.tier) || node.max_rank == 0 || node.ring() < 1.0 {
                return Err(format!("{} : palier de 1 à 4, anneau et rang maximal à partir de 1", node.id));
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

    /// Anneau le plus éloigné du centre
    pub fn rings(&self) -> f32 {
        self.nodes.iter().map(SkillNode::ring).fold(1.0, f32::max)
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
        // Les 180 compétences des 15 classes, douze chacune
        assert_eq!(wheel.branches.len(), 15);
        assert_eq!(wheel.nodes.len(), 180);
        for branch in &wheel.branches {
            let nodes: Vec<_> = wheel.nodes.iter().filter(|n| n.branch == branch.id).collect();
            assert_eq!(nodes.len(), 12, "{}", branch.name);
            assert_eq!(nodes.iter().filter(|n| n.is_ultimate()).count(), 1, "{} : un ultime", branch.name);
            assert!(["magic", "ranged", "melee"].contains(&branch.family.as_str()), "{}", branch.family);
            assert_eq!(branch.mix.iter().map(|&p| p as u32).sum::<u32>(), 100, "{}", branch.name);
        }
        // Une compétence ne demande que des compétences plus proches du centre
        for node in &wheel.nodes {
            for r in &node.requires {
                assert!(wheel.node(r).unwrap().ring() < node.ring(), "{} demande {}", node.id, r);
            }
        }
        assert_eq!(wheel.rings(), 4.0);
        // Chaque classe a sa couleur
        let colors: std::collections::HashSet<_> = wheel.branches.iter().map(|b| b.color).collect();
        assert_eq!(colors.len(), wheel.branches.len());
    }

    #[test]
    fn test_points_come_from_levels_and_buy_ranks() {
        let wheel = SkillWheel::standard();
        let mut hero = CharacterStats::new(PlayerClass::Warrior);
        assert_eq!(wheel.learn(&mut hero, "frappeLourde"), Err(LearnError::NoPoint));

        // Niveau 4 : trois points
        hero.add_experience(600);
        assert_eq!(hero.skill_points, 3);
        assert_eq!(wheel.learn(&mut hero, "balayageCirculaire"), Err(LearnError::MissingPrerequisite));
        assert_eq!(wheel.learn(&mut hero, "frappeLourde"), Ok(1));
        assert_eq!(wheel.learn(&mut hero, "balayageCirculaire"), Ok(1));
        assert_eq!(wheel.learn(&mut hero, "frappeLourde"), Ok(2));
        assert_eq!(hero.skill_points, 0);
        assert_eq!(wheel.learn(&mut hero, "frappeLourde"), Err(LearnError::NoPoint));
        assert_eq!(wheel.learn(&mut hero, "inconnue"), Err(LearnError::Unknown));

        hero.skill_points = 10;
        assert_eq!(wheel.learn(&mut hero, "frappeLourde"), Ok(3));
        assert_eq!(wheel.learn(&mut hero, "frappeLourde"), Err(LearnError::MaxRank));
        // Une fusion demande les deux formes du niveau 2 qu'elle unit
        assert_eq!(wheel.learn(&mut hero, "tornadeEcrasante"), Err(LearnError::MissingPrerequisite));
        assert_eq!(wheel.learn(&mut hero, "rueeFrontale"), Ok(1));
        assert_eq!(wheel.learn(&mut hero, "frappePlongeante"), Ok(1));
        assert_eq!(wheel.learn(&mut hero, "tornadeEcrasante"), Ok(1));
        // Les compétences des autres classes s'apprennent aussi (#171)
        assert_eq!(wheel.learn(&mut hero, "frappeDuJuste"), Ok(1));
    }

    #[test]
    fn test_parse_rejects_broken_wheels() {
        let branch = r#"{ "id": "a", "name": "A", "color": [0, 0, 0] }"#;
        let bad_branch = format!(r#"{{ "branches": [{branch}], "nodes": [{{ "id": "x", "name": "X", "branch": "b", "tier": 1 }}] }}"#);
        assert!(SkillWheel::parse(&bad_branch).is_err());
        let bad_requires = format!(r#"{{ "branches": [{branch}], "nodes": [{{ "id": "x", "name": "X", "branch": "a", "tier": 1, "requires": ["y"] }}] }}"#);
        assert!(SkillWheel::parse(&bad_requires).is_err());
    }
}
