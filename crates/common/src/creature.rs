//! Créatures de la planète : les espèces, décrites chacune par un fichier JSON du dossier
//! `assets/creatures/` (corps, allure, biomes où elles apparaissent, modèle 3D), et l'état d'une
//! créature tel que le serveur le diffuse.
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

/// Une espèce de créature, telle que décrite par son fichier
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatureSpecies {
    /// Identifiant : le nom du fichier sans `.json` (ex. `mouton`)
    #[serde(skip)]
    pub id: String,
    /// Nom affiché
    pub name: String,
    /// Volume du corps pour les déplacements
    pub body: CreatureBody,
    /// Vitesse de marche, en blocs par seconde
    pub walk_speed: f32,
    /// Taille maximale d'un groupe qui apparaît ensemble
    #[serde(default = "one")]
    pub max_group: u32,
    /// Biomes où l'espèce apparaît (aucun : elle n'apparaît pas d'elle-même)
    #[serde(default)]
    pub spawn: Vec<SpawnRule>,
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

/// Apparition dans un biome
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpawnRule {
    /// Nom du biome (`Plains`, `Forest`, `Desert`… : ceux de `herocraft_world::BiomeType`)
    pub biome: String,
    /// Poids face aux autres espèces du même biome
    pub weight: u32,
    /// Chance que le groupe tiré apparaisse vraiment (1 : toujours), pour une espèce rare
    #[serde(default = "always")]
    pub chance: f32,
}

/// Modèle 3D : des pavés colorés, posés dans le repère de la créature. L'origine est sous ses
/// pieds, au centre ; x va vers sa gauche, y vers le haut, z vers l'avant (la tête). Les mesures
/// sont en unités de `units_per_block` par bloc (16 par défaut, comme dans Blockbench).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatureModel {
    #[serde(default = "sixteen")]
    pub units_per_block: f32,
    pub parts: Vec<ModelPart>,
}

/// Un pavé du modèle
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelPart {
    /// Nom de la pièce (pour s'y retrouver dans le fichier)
    pub name: String,
    /// Coin minimal et coin maximal
    pub from: [f32; 3],
    pub to: [f32; 3],
    pub color: Color,
    /// Point autour duquel la pièce pivote quand elle s'anime (par défaut : le haut de son centre)
    #[serde(default)]
    pub pivot: Option<[f32; 3]>,
    /// Balancement pendant la marche (pattes, tête, queue)
    #[serde(default)]
    pub swing: Option<Swing>,
}

impl ModelPart {
    /// Point de pivot, en unités du modèle
    pub fn pivot_point(&self) -> [f32; 3] {
        self.pivot.unwrap_or([
            0.5 * (self.from[0] + self.to[0]),
            self.from[1].max(self.to[1]),
            0.5 * (self.from[2] + self.to[2]),
        ])
    }
}

/// Balancement d'avant en arrière (rotation autour de l'axe gauche-droite), rythmé par le pas
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Swing {
    /// Angle maximal, en degrés
    pub amplitude: f32,
    /// Décalage dans le cycle du pas, en fraction de cycle (0.5 : à contretemps)
    #[serde(default)]
    pub phase: f32,
}

/// Couleur `#rrggbb`, lue en composantes de 0 à 1
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(try_from = "String")]
pub struct Color(pub [f32; 3]);

impl TryFrom<String> for Color {
    type Error = String;

    fn try_from(text: String) -> Result<Self, String> {
        let hex = text.strip_prefix('#').unwrap_or(&text);
        let value = (hex.len() == 6).then(|| u32::from_str_radix(hex, 16).ok()).flatten();
        let Some(value) = value else { return Err(format!("couleur « {text} » invalide (attendu : #rrggbb)")) };
        Ok(Color([(value >> 16) as f32 / 255.0, ((value >> 8) & 0xff) as f32 / 255.0, (value & 0xff) as f32 / 255.0]))
    }
}

fn one() -> u32 {
    1
}

fn always() -> f32 {
    1.0
}

fn sixteen() -> f32 {
    16.0
}

/// Toutes les espèces connues, rangées par identifiant (le même ordre sur le serveur et le client)
#[derive(Debug, Clone, Default)]
pub struct CreatureRegistry {
    species: Vec<CreatureSpecies>,
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
        let mut registry = Self::default();
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

    pub fn from_species(mut species: Vec<CreatureSpecies>) -> Self {
        species.sort_by(|a, b| a.id.cmp(&b.id));
        Self { species }
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
        if !(body.half_width > 0.0 && body.half_length > 0.0 && body.height > 0.0 && species.walk_speed >= 0.0) {
            return Err("corps ou vitesse invalides (tailles positives attendues)".into());
        }
        if species.model.parts.is_empty() || species.model.units_per_block <= 0.0 {
            return Err("modèle vide".into());
        }
        if species.max_group == 0 {
            return Err("max_group doit valoir au moins 1".into());
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bundled_species_load() {
        let (registry, errors) = CreatureRegistry::load_dir(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(CREATURES_DIR)).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        for id in ["lapin", "mouton", "sanglier"] {
            let species = registry.get(registry.find(id).unwrap_or_else(|| panic!("{id} manquant"))).unwrap();
            assert!(!species.spawn.is_empty());
        }
    }

    #[test]
    fn test_invalid_descriptions_are_explained() {
        let model = r##""model": { "parts": [ { "name": "corps", "from": [0, 0, 0], "to": [1, 1, 1], "color": "#ff8000" } ] }"##;
        let ok = format!(r#"{{ "name": "Cube", "body": {{ "half_width": 0.5, "half_length": 0.5, "height": 1 }}, "walk_speed": 1, {model} }}"#);
        let cube = CreatureSpecies::parse("cube", &ok).unwrap();
        assert_eq!(cube.max_group, 1);
        assert_eq!(cube.model.units_per_block, 16.0);
        assert_eq!(cube.model.parts[0].color, Color([1.0, 128.0 / 255.0, 0.0]));

        let bad_color = ok.replace("#ff8000", "orange");
        assert!(CreatureSpecies::parse("cube", &bad_color).unwrap_err().contains("orange"));
        let typo = ok.replace("walk_speed", "walk_sped");
        assert!(CreatureSpecies::parse("cube", &typo).unwrap_err().contains("walk_sped"));
        let flat = ok.replace(r#""height": 1"#, r#""height": 0"#);
        assert!(CreatureSpecies::parse("cube", &flat).is_err());
    }
}
