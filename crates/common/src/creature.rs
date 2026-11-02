//! Créatures de la planète : les espèces, décrites chacune par un fichier JSON du dossier
//! `assets/creatures/` (corps, allure, biomes où elles apparaissent, modèle 3D glTF et sa texture),
//! et l'état d'une créature tel que le serveur le diffuse.
//!
//! Ajouter une bête, c'est ajouter un fichier : la simulation (`herocraft_world::creatures`) et le
//! rendu du client lisent tout dans ces descriptions. Le format est décrit dans
//! `assets/creatures/README.md`.

use std::io;
use std::path::{Path, PathBuf};

use glam::Vec3;
use serde::{Deserialize, Serialize};

/// Dossier des descriptions d'espèces, depuis le dossier de lancement
pub const CREATURES_DIR: &str = "assets/creatures";
/// Points de vie retirés à une créature par un coup de joueur
pub const PLAYER_HIT_DAMAGE: f32 = 2.5;
/// Durée de la teinte rouge d'une créature frappée, en secondes
pub const CREATURE_HURT_TIME: f32 = 0.4;
/// Durée de la chute sur le flanc d'une créature morte, avant qu'elle disparaisse, en secondes
pub const CREATURE_DEATH_TIME: f32 = 0.8;

/// Une espèce de créature, telle que décrite par son fichier
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatureSpecies {
    /// Identifiant : le nom du fichier sans `.json` (ex. `mouton`)
    #[serde(skip)]
    pub id: String,
    /// Nom affiché
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Volume du corps pour les déplacements
    pub body: CreatureBody,
    /// Vitesse de marche, en blocs par seconde
    pub walk_speed: f32,
    /// Points de vie : chaque coup d'un joueur en retire `PLAYER_HIT_DAMAGE`
    #[serde(default = "default_health")]
    pub health: f32,
    /// Taille d'un groupe qui apparaît ensemble : au moins `min_group`, au plus `max_group`
    #[serde(default = "one")]
    pub min_group: u32,
    #[serde(default = "one")]
    pub max_group: u32,
    /// Biomes où l'espèce apparaît (aucun : elle n'apparaît pas d'elle-même)
    #[serde(default)]
    pub spawn: Vec<SpawnRule>,
    /// Réaction à l'approche d'un joueur (indifférente par défaut)
    #[serde(default)]
    pub temperament: Temperament,
    /// Modèle 3D
    pub model: CreatureModel,
}

/// Volume du corps, en blocs
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatureBody {
    /// Demi-largeur (de côté)
    pub half_width: f32,
    /// Demi-longueur (de la tête à la queue)
    pub half_length: f32,
    pub height: f32,
}

/// Réaction d'une espèce à un joueur qui s'approche, tant qu'il ne l'a pas frappée (un coup la fait
/// toujours fuir)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Temperament {
    /// Ne prête pas attention aux joueurs
    #[default]
    Indifferent,
    /// S'enfuit quand un joueur s'approche
    Craintif,
    /// S'arrête et regarde le joueur qui passe près d'elle
    Placide,
    /// Suit le joueur à distance, et le regarde une fois près de lui
    Curieux,
}

/// Apparition dans un biome
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpawnRule {
    /// Nom du biome (`Plains`, `Forest`, `Desert`… : ceux de `herocraft_world::BiomeType`)
    pub biome: String,
    /// Poids face aux autres espèces du même biome
    pub weight: f32,
    /// Chance que le groupe tiré apparaisse vraiment (1 : toujours), pour une espèce rare
    #[serde(default = "always")]
    pub chance: f32,
}

/// Modèle 3D : un maillage glTF à squelette et sa texture, chemins depuis `assets/creatures/`.
/// Le modèle est en blocs, y vers le haut, la tête vers +z ; le point le plus bas de sa pose de repos
/// est posé sur les pieds de la créature.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatureModel {
    /// Fichier `.gltf` (données embarquées), un seul maillage à squelette
    pub mesh: String,
    /// Texture PNG, lue au plus proche texel
    pub texture: String,
    /// Animation jouée pendant la marche, d'autant plus vite que la créature va vite, et figée à
    /// l'arrêt
    #[serde(default = "walk_animation")]
    pub walk_animation: String,
}

fn walk_animation() -> String {
    "repos".into()
}

fn default_health() -> f32 {
    10.0
}

fn one() -> u32 {
    1
}

fn always() -> f32 {
    1.0
}

/// Toutes les espèces connues, rangées par identifiant (le même ordre sur le serveur et le client)
#[derive(Debug, Clone, Default)]
pub struct CreatureRegistry {
    species: Vec<CreatureSpecies>,
    /// Dossier des descriptions, d'où partent les chemins des modèles
    dir: PathBuf,
}

impl CreatureRegistry {
    /// Les espèces de `assets/creatures/` : depuis le dossier de lancement, sinon celles livrées
    /// avec les sources. Un fichier illisible est signalé et ignoré.
    pub fn load_default() -> Self {
        let candidates = [PathBuf::from(CREATURES_DIR), Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(CREATURES_DIR)];
        let Some(dir) = candidates.iter().find(|dir| dir.is_dir()) else {
            log::warn!("Dossier {CREATURES_DIR}/ introuvable : aucune créature.");
            return Self::default();
        };
        match Self::load_dir(dir) {
            Ok((registry, errors)) => {
                for error in errors {
                    log::warn!("{error}");
                }
                log::info!("{} espèce(s) de créatures chargée(s) depuis {}.", registry.len(), dir.display());
                registry
            }
            Err(e) => {
                log::warn!("Lecture de {} impossible : {e}", dir.display());
                Self::default()
            }
        }
    }

    /// Lit chaque fichier `*.json` du dossier. Renvoie les espèces valides et les erreurs des autres.
    pub fn load_dir(dir: &Path) -> io::Result<(Self, Vec<String>)> {
        let mut registry = Self { species: Vec::new(), dir: dir.to_path_buf() };
        let mut errors = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            let path = entry?.path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let id = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
            let result = std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|text| CreatureSpecies::parse(&id, &text));
            match result {
                Ok(species) => registry.species.push(species),
                Err(e) => errors.push(format!("Créature {} ignorée : {e}", path.display())),
            }
        }
        registry.species.sort_by(|a, b| a.id.cmp(&b.id));
        Ok((registry, errors))
    }

    pub fn from_species(mut species: Vec<CreatureSpecies>, dir: PathBuf) -> Self {
        species.sort_by(|a, b| a.id.cmp(&b.id));
        Self { species, dir }
    }

    /// Chemin d'un fichier cité par une description (modèle, texture)
    pub fn asset_path(&self, relative: &str) -> PathBuf {
        self.dir.join(relative)
    }

    pub fn len(&self) -> usize {
        self.species.len()
    }

    pub fn is_empty(&self) -> bool {
        self.species.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<&CreatureSpecies> {
        self.species.get(index)
    }

    /// Rang de l'espèce d'identifiant `id`
    pub fn find(&self, id: &str) -> Option<usize> {
        self.species.iter().position(|s| s.id == id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &CreatureSpecies> {
        self.species.iter()
    }
}

impl CreatureSpecies {
    /// Lit la description JSON d'une espèce et vérifie qu'elle tient debout
    pub fn parse(id: &str, json: &str) -> Result<Self, String> {
        let mut species: CreatureSpecies = serde_json::from_str(json).map_err(|e| e.to_string())?;
        species.id = id.to_string();
        let body = species.body;
        if !(body.half_width > 0.0 && body.half_length > 0.0 && body.height > 0.0 && species.walk_speed >= 0.0 && species.health > 0.0) {
            return Err("corps, vitesse ou vie invalides (valeurs positives attendues)".into());
        }
        if species.min_group == 0 || species.max_group < species.min_group {
            return Err("taille de groupe invalide (1 ≤ min_group ≤ max_group)".into());
        }
        if species.spawn.iter().any(|rule| !(rule.weight >= 0.0 && (0.0..=1.0).contains(&rule.chance))) {
            return Err("apparition invalide (poids positif, chance entre 0 et 1)".into());
        }
        Ok(species)
    }
}

/// Position d'une créature telle que les joueurs la voient
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CreatureState {
    pub creature_id: u64,
    /// Pieds (repère du centre de la planète)
    pub position: Vec3,
    /// Cap dans le plan tangent (0 = nord, π/2 = est, comme `tangent_frame`)
    pub yaw: f32,
    /// Coup reçu à l'instant : 1 juste après, 0 une fois passé (la bête rougit)
    pub hurt: f32,
    /// Mort : 0 vivante, puis de 0 à 1 pendant qu'elle bascule sur le flanc avant de disparaître
    pub death: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bundled_species_load() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(CREATURES_DIR);
        let (registry, errors) = CreatureRegistry::load_dir(&dir).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        for id in ["lapin", "sanglier", "vache", "loup"] {
            let species = registry.get(registry.find(id).unwrap_or_else(|| panic!("{id} manquant"))).unwrap();
            assert!(!species.spawn.is_empty());
        }
        for species in registry.iter() {
            assert!(registry.asset_path(&species.model.mesh).is_file(), "{} : modèle manquant", species.id);
            assert!(registry.asset_path(&species.model.texture).is_file(), "{} : texture manquante", species.id);
        }
    }

    #[test]
    fn test_invalid_descriptions_are_explained() {
        let ok = r#"{ "name": "Cube", "body": { "half_width": 0.5, "half_length": 0.5, "height": 1 }, "walk_speed": 1,
            "spawn": [ { "biome": "Plains", "weight": 0.5 } ], "model": { "mesh": "cube.gltf", "texture": "cube.png" } }"#;
        let cube = CreatureSpecies::parse("cube", ok).unwrap();
        assert_eq!((cube.min_group, cube.max_group), (1, 1));
        assert_eq!(cube.spawn[0].chance, 1.0);
        assert_eq!(cube.model.walk_animation, "repos");
        assert_eq!(cube.health, 10.0);
        assert_eq!(cube.temperament, Temperament::Indifferent);
        let shy = ok.replace(r#""walk_speed": 1,"#, r#""walk_speed": 1, "temperament": "craintif","#);
        assert_eq!(CreatureSpecies::parse("cube", &shy).unwrap().temperament, Temperament::Craintif);
        let unknown = ok.replace(r#""walk_speed": 1,"#, r#""walk_speed": 1, "temperament": "farouche","#);
        assert!(CreatureSpecies::parse("cube", &unknown).unwrap_err().contains("farouche"));

        let typo = ok.replace("walk_speed", "walk_sped");
        assert!(CreatureSpecies::parse("cube", &typo).unwrap_err().contains("walk_sped"));
        let flat = ok.replace(r#""height": 1"#, r#""height": 0"#);
        assert!(CreatureSpecies::parse("cube", &flat).is_err());
        let groups = ok.replace(r#""walk_speed": 1,"#, r#""walk_speed": 1, "min_group": 3, "max_group": 2,"#);
        assert!(CreatureSpecies::parse("cube", &groups).is_err());
    }
}
