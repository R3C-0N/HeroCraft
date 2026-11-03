//! Champs : les plantes semées par les joueurs, et le temps qu'il leur reste avant le stade suivant.
//!
//! Chaque bloc posé ou cassé passe par `Fields::observe` : un semis ou une plante pas encore mûre
//! entre au registre, tout autre bloc l'en retire. `Fields::tick` fait avancer leur pousse et renvoie
//! les blocs à changer (le stade suivant), que la partie solo ou le serveur applique et diffuse comme
//! n'importe quel bloc. Une plante pousse deux fois moins vite sur une terre sèche, et plus du tout
//! si elle n'est plus sur une terre labourée (ou si la plante a été remplacée entre-temps).
//!
//! Les plantes des champs des villages ne sont pas au registre : elles restent telles que le
//! générateur les a posées, jusqu'à ce qu'un joueur les récolte et ressème.
//!
//! Le registre est enregistré dans `cultures.json`, dans la sauvegarde du monde.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use herocraft_common::{cultures, BlockAddress, BlockId, CropRegistry, WATER_REACH_BLOCKS};
use serde::{Deserialize, Serialize};

use crate::edits::{BlockPos, TerrainEdits, TerrainQuery};
use crate::generator::PlanetWorldGenerator;

/// Fichier du registre, dans le dossier de sauvegarde du monde
pub const FIELDS_FILE: &str = "cultures.json";

/// Une plante qui pousse
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
struct Growing {
    block: BlockAddress,
    /// Avancement vers le stade suivant, de 0 à 1
    progress: f32,
}

#[derive(Serialize, Deserialize, Default)]
struct FieldsSave {
    plants: Vec<Growing>,
}

/// Les plantes semées qui poussent
#[derive(Debug, Default)]
pub struct Fields {
    /// Avancement vers le stade suivant de chaque plante
    plants: HashMap<BlockPos, f32>,
    save_path: Option<PathBuf>,
}

impl Fields {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registre enregistré dans le dossier de sauvegarde `dir`, repris s'il existe
    pub fn with_save_dir(dir: impl AsRef<Path>) -> Self {
        let path = dir.as_ref().join(FIELDS_FILE);
        let mut fields = Self::new();
        match std::fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<FieldsSave>(&text) {
                Ok(save) => {
                    fields.plants = save.plants.into_iter().map(|g| (BlockPos::from(g.block), g.progress.clamp(0.0, 1.0))).collect();
                    fields.save_path = Some(path);
                }
                // Fichier illisible : on ne l'écrase pas
                Err(e) => log::warn!("Cultures de {} illisibles ({e}) : elles ne seront pas enregistrées.", path.display()),
            },
            Err(_) => fields.save_path = Some(path),
        }
        fields
    }

    pub fn len(&self) -> usize {
        self.plants.len()
    }

    pub fn is_empty(&self) -> bool {
        self.plants.is_empty()
    }

    /// La plante en `pos` pousse
    pub fn is_growing(&self, pos: BlockPos) -> bool {
        self.plants.contains_key(&pos)
    }

    /// Avancement vers le stade suivant de la plante en `pos`
    pub fn progress(&self, pos: BlockPos) -> Option<f32> {
        self.plants.get(&pos).copied()
    }

    pub fn save(&self) -> io::Result<()> {
        let Some(path) = &self.save_path else { return Ok(()) };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut plants: Vec<Growing> = self.plants.iter().map(|(&pos, &progress)| Growing { block: pos.into(), progress }).collect();
        // Ordre fixe : le fichier ne change pas d'un enregistrement à l'autre sans raison
        plants.sort_by_key(|g| (g.block.face as u8, g.block.x, g.block.z, g.block.y));
        let text = serde_json::to_string(&FieldsSave { plants }).map_err(io::Error::other)?;
        std::fs::write(path, text)
    }

    /// Le bloc en `pos` vient de changer : un semis (ou une plante pas encore mûre) commence à
    /// pousser, tout autre bloc arrête ce qui poussait là. La plante qui passe au stade suivant
    /// repart de zéro.
    pub fn observe(&mut self, pos: BlockPos, block: BlockId) {
        if cultures().next_stage(block).is_some() {
            self.plants.insert(pos, 0.0);
        } else {
            self.plants.remove(&pos);
        }
    }

    /// Fait pousser les plantes de `dt` secondes. `growth(pos)` multiplie la pousse de chacune
    /// (saison, météo ; 1 par défaut). Renvoie les plantes qui passent au stade suivant, avec ce
    /// stade : à poser dans le monde (sans repasser par `observe`, déjà fait ici). Une plante qui
    /// n'est plus là est oubliée.
    pub fn tick(&mut self, dt: f32, generator: &PlanetWorldGenerator, edits: &TerrainEdits, mut growth: impl FnMut(BlockPos) -> f32) -> Vec<(BlockPos, BlockId)> {
        if self.plants.is_empty() || dt <= 0.0 {
            return Vec::new();
        }
        let registry = cultures();
        let mut grown = Vec::new();
        let mut gone = Vec::new();
        for (&pos, progress) in self.plants.iter_mut() {
            let block = edits.block(generator, pos);
            let (Some((index, _)), Some(next)) = (registry.stage_of(block), registry.next_stage(block)) else {
                gone.push(pos);
                continue;
            };
            let crop = registry.get(index).expect("rang d'une culture connue");
            let soil = edits.block(generator, BlockPos { y: pos.y - 1, ..pos });
            let rate = registry.growth_factor(crop, soil) * growth(pos).max(0.0);
            *progress += dt * rate / crop.stage_seconds;
            if *progress >= 1.0 {
                grown.push((pos, next));
            }
        }
        for pos in gone {
            self.plants.remove(&pos);
        }
        for &(pos, next) in &grown {
            self.observe(pos, next);
        }
        grown
    }
}

/// Terre labourée que donne la houe en `pos` : humide s'il y a de l'eau à moins de
/// `WATER_REACH_BLOCKS` blocs, à sa hauteur ou juste en dessous
pub fn tilled_block(query: &mut TerrainQuery, pos: BlockPos, registry: &CropRegistry) -> Option<BlockId> {
    let r = WATER_REACH_BLOCKS;
    let wet = (-r..=r).any(|dx| {
        (-r..=r).any(|dz| {
            [0, -1].iter().any(|&dy| {
                let p = BlockPos { x: pos.x + dx, y: pos.y + dy, z: pos.z + dz, ..pos };
                p.is_valid() && query.block(p) == BlockId::WATER
            })
        })
    });
    registry.farmland(wet)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PlanetDescriptor;
    use herocraft_common::{items, PlanetFace};

    #[test]
    fn test_sown_crop_grows_stage_by_stage_on_farmland() {
        let generator = PlanetWorldGenerator::new(PlanetDescriptor { seed: 2026, ..Default::default() });
        let edits = TerrainEdits::new(None);
        let registry = cultures();
        let wheat = registry.crop_of_seed(items().find("grainesBle").unwrap()).unwrap().clone();
        let soil = BlockPos::new(PlanetFace::PosX, 100, 300, 100);
        let pos = BlockPos { y: soil.y + 1, ..soil };
        edits.set_block(&generator, soil, registry.farmland(true).unwrap());
        edits.set_block(&generator, pos, wheat.stages[0]);
        let mut fields = Fields::new();
        fields.observe(pos, wheat.stages[0]);
        assert!(fields.is_growing(pos));

        // Rien avant la fin du stade ; la saison peut l'arrêter
        assert!(fields.tick(wheat.stage_seconds * 0.5, &generator, &edits, |_| 1.0).is_empty());
        assert!(fields.tick(wheat.stage_seconds, &generator, &edits, |_| 0.0).is_empty());
        let grown = fields.tick(wheat.stage_seconds * 0.6, &generator, &edits, |_| 1.0);
        assert_eq!(grown, vec![(pos, wheat.stages[1])]);
        for (p, b) in grown {
            edits.set_block(&generator, p, b);
        }
        assert_eq!(fields.progress(pos), Some(0.0), "le stade suivant repart de zéro");

        // Sur une terre sèche, deux fois moins vite
        edits.set_block(&generator, soil, registry.farmland(false).unwrap());
        assert!(fields.tick(wheat.stage_seconds * 1.5, &generator, &edits, |_| 1.0).is_empty());
        let grown = fields.tick(wheat.stage_seconds * 0.6, &generator, &edits, |_| 1.0);
        assert_eq!(grown, vec![(pos, wheat.stages[2])]);
        edits.set_block(&generator, pos, wheat.stages[2]);
        let grown = fields.tick(wheat.stage_seconds * 2.1, &generator, &edits, |_| 1.0);
        assert_eq!(grown, vec![(pos, wheat.mature())]);
        assert!(!fields.is_growing(pos), "mûre, elle ne pousse plus");

        // Plante cassée : oubliée
        fields.observe(pos, wheat.stages[0]);
        edits.set_block(&generator, pos, BlockId::AIR);
        assert!(fields.tick(1.0, &generator, &edits, |_| 1.0).is_empty());
        assert!(fields.is_empty());
    }

    #[test]
    fn test_fields_are_saved_with_the_world() {
        let dir = std::env::temp_dir().join(format!("herocraft_champs_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let wheat = cultures().crop_of_seed(items().find("grainesBle").unwrap()).unwrap().stages[0];
        let pos = BlockPos::new(PlanetFace::NegZ, 5, 200, 7);
        let mut fields = Fields::with_save_dir(&dir);
        fields.observe(pos, wheat);
        fields.save().unwrap();
        let again = Fields::with_save_dir(&dir);
        assert_eq!(again.progress(pos), Some(0.0));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_farmland_is_wet_near_water() {
        let generator = PlanetWorldGenerator::new(PlanetDescriptor { seed: 2026, ..Default::default() });
        let edits = TerrainEdits::new(None);
        let registry = cultures();
        let pos = BlockPos::new(PlanetFace::PosY, 400, 300, 400);
        let mut query = TerrainQuery::new(&generator, &edits);
        // Très haut dans le ciel : pas d'eau autour
        assert_eq!(tilled_block(&mut query, pos, registry), registry.farmland(false));
        edits.set_block(&generator, BlockPos { x: pos.x + 3, ..pos }, BlockId::WATER);
        let mut query = TerrainQuery::new(&generator, &edits);
        assert_eq!(tilled_block(&mut query, pos, registry), registry.farmland(true));
    }
}
