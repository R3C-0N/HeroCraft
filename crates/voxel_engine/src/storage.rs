use std::fs;
use std::io::{self, Error, ErrorKind};
use std::path::{Path, PathBuf};

use herocraft_common::{PlanetChunkCoord, PlanetFace};

use crate::chunk::ChunkData;

/// Gestionnaire de stockage persistant sur disque pour les chunks d'une planète
#[derive(Debug, Clone)]
pub struct ChunkStorage {
    root_dir: PathBuf,
}

impl ChunkStorage {
    /// Initialise le stockage dans le dossier spécifié (ex: saves/monde_42/)
    pub fn new<P: AsRef<Path>>(root_dir: P) -> io::Result<Self> {
        let root = root_dir.as_ref().to_path_buf();
        fs::create_dir_all(&root)?;
        Ok(Self { root_dir: root })
    }

    /// Construit le chemin relatif unique pour un chunk
    /// Format : root_dir/chunks/{Face}/c_{x}_{y}_{alt}.rle
    pub fn chunk_path(&self, coord: &PlanetChunkCoord) -> PathBuf {
        let face_dir = format!("{:?}", coord.face);
        let file_name = format!("c_{}_{}_{}.rle", coord.x, coord.y, coord.altitude_layer);
        self.root_dir.join("chunks").join(face_dir).join(file_name)
    }

    /// Vérifie si un chunk existe déjà sur le disque
    pub fn has_chunk(&self, coord: &PlanetChunkCoord) -> bool {
        self.chunk_path(coord).exists()
    }

    /// Sauvegarde un chunk sur le disque au format compressé RLE
    pub fn save_chunk(&self, coord: &PlanetChunkCoord, chunk: &ChunkData) -> io::Result<()> {
        herocraft_common::prof_scope!("voxel::storage::save_chunk");
        let path = self.chunk_path(coord);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let compressed_bytes = chunk.compress_rle();
        fs::write(&path, compressed_bytes)?;
        Ok(())
    }

    /// Charge et décompresse un chunk depuis le disque
    pub fn load_chunk(&self, coord: &PlanetChunkCoord) -> io::Result<Option<ChunkData>> {
        herocraft_common::prof_scope!("voxel::storage::load_chunk");
        let path = self.chunk_path(coord);
        if !path.exists() {
            return Ok(None);
        }

        let compressed_bytes = fs::read(&path)?;
        let chunk = ChunkData::decompress_rle(&compressed_bytes).map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

        Ok(Some(chunk))
    }

    /// Liste les chunks enregistrés sur le disque (ceux qu'un joueur a modifiés)
    pub fn list_chunks(&self) -> io::Result<Vec<PlanetChunkCoord>> {
        let mut coords = Vec::new();
        for face in PlanetFace::ALL {
            let face_dir = self.root_dir.join("chunks").join(format!("{:?}", face));
            let entries = match fs::read_dir(&face_dir) {
                Ok(entries) => entries,
                Err(e) if e.kind() == ErrorKind::NotFound => continue,
                Err(e) => return Err(e),
            };
            for entry in entries {
                let name = entry?.file_name();
                let parsed = name.to_str().and_then(|n| n.strip_prefix("c_")).and_then(|n| n.strip_suffix(".rle")).and_then(|n| {
                    let mut parts = n.split('_').map(|p| p.parse::<i32>());
                    match (parts.next(), parts.next(), parts.next(), parts.next()) {
                        (Some(Ok(x)), Some(Ok(y)), Some(Ok(alt)), None) => Some((x, y, alt)),
                        _ => None,
                    }
                });
                if let Some((x, y, alt)) = parsed {
                    coords.push(PlanetChunkCoord::new(face, x, y, alt));
                }
            }
        }
        Ok(coords)
    }

    /// Supprime un chunk du disque si nécessaire
    pub fn delete_chunk(&self, coord: &PlanetChunkCoord) -> io::Result<bool> {
        let path = self.chunk_path(coord);
        if path.exists() {
            fs::remove_file(path)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use herocraft_common::BlockId;

    #[test]
    fn test_chunk_storage_save_load_roundtrip() {
        let temp_dir = std::env::temp_dir().join("herocraft_test_storage");
        let _ = fs::remove_dir_all(&temp_dir);

        let storage = ChunkStorage::new(&temp_dir).expect("Échec de création du stockage");
        let coord = PlanetChunkCoord::new(PlanetFace::PosZ, 12, 18, 0);

        assert!(!storage.has_chunk(&coord));

        let mut original_chunk = ChunkData::new_empty();
        original_chunk.set_voxel(5, 10, 15, BlockId::STONE);
        original_chunk.set_voxel(5, 11, 15, BlockId::GRASS);
        original_chunk.set_voxel(2, 3, 4, BlockId::WATER);

        storage.save_chunk(&coord, &original_chunk).expect("Échec de sauvegarde");
        assert!(storage.has_chunk(&coord));

        let loaded = storage.load_chunk(&coord).expect("Échec de chargement").expect("Chunk introuvable");
        assert_eq!(loaded.get_voxel(5, 10, 15), BlockId::STONE);
        assert_eq!(loaded.get_voxel(5, 11, 15), BlockId::GRASS);
        assert_eq!(loaded.get_voxel(2, 3, 4), BlockId::WATER);
        assert_eq!(loaded.get_voxel(0, 0, 0), BlockId::AIR);
        assert_eq!(storage.list_chunks().unwrap(), vec![coord]);

        let deleted = storage.delete_chunk(&coord).expect("Échec de suppression");
        assert!(deleted);
        assert!(!storage.has_chunk(&coord));

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
