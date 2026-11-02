//! Population fixe des créatures : la planète est découpée en zones de `ZONE_BLOCKS` blocs de côté,
//! et chaque zone reçoit, la première fois qu'un joueur s'en approche, les bêtes que la graine du
//! monde lui destine. Ensuite plus rien n'apparaît : une bête tuée manque pour de bon, une bête qui
//! a erré reste où elle s'est arrêtée.
//!
//! Les zones loin de tout joueur sont « endormies » : leurs bêtes ne sont plus simulées, leur
//! espèce, position, cap et vie sont rangés ici, et repris tels quels quand un joueur revient. Ce
//! rangement est enregistré dans la sauvegarde du monde (`creatures.json`).

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;

use glam::Vec3;
use herocraft_common::PlanetFace;
use serde::{Deserialize, Serialize};

use crate::edits::FACE_BLOCKS;

/// Côté d'une zone de population, en blocs
pub const ZONE_BLOCKS: i32 = 32;
/// Zones par arête de face
pub const FACE_ZONES: i32 = FACE_BLOCKS / ZONE_BLOCKS;

/// Une zone de population : face du cube et rang de la zone le long de x et de z
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ZoneKey {
    pub face: PlanetFace,
    pub x: i32,
    pub z: i32,
}

impl ZoneKey {
    /// La zone qui contient la colonne de blocs (x, z)
    pub fn of_column(face: PlanetFace, x: i32, z: i32) -> Self {
        Self { face, x: x.div_euclid(ZONE_BLOCKS), z: z.div_euclid(ZONE_BLOCKS) }
    }

    /// Rang unique de la zone sur la planète, de 0 à 6 × `FACE_ZONES`²
    pub fn index(&self) -> u64 {
        let face = PlanetFace::ALL.iter().position(|f| *f == self.face).unwrap_or(0) as u64;
        (face * FACE_ZONES as u64 + self.x as u64) * FACE_ZONES as u64 + self.z as u64
    }

    /// Toutes les zones de la planète
    pub fn all() -> impl Iterator<Item = ZoneKey> {
        PlanetFace::ALL
            .into_iter()
            .flat_map(|face| (0..FACE_ZONES).flat_map(move |x| (0..FACE_ZONES).map(move |z| ZoneKey { face, x, z })))
    }

    /// Graine du tirage de la population de cette zone, tirée de la graine du monde
    pub fn seed(&self, world_seed: u64) -> u64 {
        splitmix(world_seed ^ splitmix(self.index().wrapping_add(0x5EED_C2EA_7E25)))
    }
}

fn splitmix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

/// Une bête endormie : de quoi la remettre exactement où elle était
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SleepingCreature {
    pub id: u64,
    /// Identifiant de son espèce (nom du fichier, ex. `vache`)
    pub species: String,
    pub position: Vec3,
    pub yaw: f32,
    pub health: f32,
}

/// Zones dont la population a déjà été tirée, et bêtes des zones endormies
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct PopulationSave {
    /// Prochain identifiant d'une bête posée à la main (commande, test)
    pub next_id: u64,
    /// Chaque zone déjà peuplée, avec ses bêtes (liste vide : toutes mortes ou parties)
    pub zones: Vec<(ZoneKey, Vec<SleepingCreature>)>,
}

impl PopulationSave {
    /// Lit la sauvegarde, ou `None` s'il n'y en a pas encore
    pub fn load(path: &Path) -> io::Result<Option<Self>> {
        match fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Écrit la sauvegarde d'un coup (fichier temporaire renommé) : une coupure ne la tronque pas
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let bytes = serde_json::to_vec(self).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, bytes)?;
        fs::rename(&tmp, path)
    }

    pub fn into_map(self) -> HashMap<ZoneKey, Vec<SleepingCreature>> {
        self.zones.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zone_indices_are_unique_and_seeds_differ() {
        let zones: Vec<ZoneKey> = ZoneKey::all().collect();
        assert_eq!(zones.len(), 6 * (FACE_ZONES * FACE_ZONES) as usize);
        let indices: std::collections::HashSet<u64> = zones.iter().map(|z| z.index()).collect();
        assert_eq!(indices.len(), zones.len());
        assert_ne!(zones[0].seed(1), zones[1].seed(1));
        assert_ne!(zones[0].seed(1), zones[0].seed(2));
        assert_eq!(zones[7].seed(3), zones[7].seed(3));
    }

    #[test]
    fn a_save_round_trips() {
        let dir = std::env::temp_dir().join(format!("herocraft_population_{}", std::process::id()));
        let path = dir.join("creatures.json");
        let zone = ZoneKey { face: PlanetFace::NegY, x: 3, z: 9 };
        let save = PopulationSave {
            next_id: 42,
            zones: vec![(zone, vec![SleepingCreature { id: 7, species: "vache".into(), position: Vec3::new(1.0, 2.0, 3.0), yaw: 0.5, health: 4.0 }])],
        };
        save.save(&path).unwrap();
        let back = PopulationSave::load(&path).unwrap().unwrap();
        assert_eq!(back.next_id, 42);
        assert_eq!(back.zones, save.zones);
        assert!(PopulationSave::load(&dir.join("absent.json")).unwrap().is_none());
        let _ = fs::remove_dir_all(dir);
    }
}
