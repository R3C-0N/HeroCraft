use std::collections::HashMap;
use std::io;

use herocraft_common::{BlockId, PlanetChunkCoord};
use log::debug;

use crate::chunk::ChunkData;
use crate::storage::ChunkStorage;

/// Chunk en mémoire avec métadonnées de suivi
#[derive(Clone)]
pub struct CachedChunk {
    pub data: ChunkData,
    pub dirty: bool,
    pub last_accessed: u64,
}

/// Cache mémoire LRU pour les chunks de terrain avec support de persistance
pub struct WorldChunkCache {
    storage: Option<ChunkStorage>,
    chunks: HashMap<PlanetChunkCoord, CachedChunk>,
    capacity: usize,
    access_counter: u64,
}

impl WorldChunkCache {
    /// Crée un nouveau cache avec une capacité maximale et un stockage disque optionnel
    pub fn new(storage: Option<ChunkStorage>, capacity: usize) -> Self {
        Self { storage, chunks: HashMap::with_capacity(capacity.min(512)), capacity: capacity.max(1), access_counter: 0 }
    }

    /// Nombre de chunks actuellement en mémoire vive
    pub fn len(&self) -> usize {
        self.chunks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    /// Nombre de chunks modifiés en attente d'écriture sur disque
    pub fn dirty_count(&self) -> usize {
        self.chunks.values().filter(|c| c.dirty).count()
    }

    /// Récupère un chunk en lecture seule s'il est déjà en mémoire
    pub fn get(&mut self, coord: &PlanetChunkCoord) -> Option<&ChunkData> {
        self.access_counter = self.access_counter.wrapping_add(1);
        let counter = self.access_counter;

        if let Some(cached) = self.chunks.get_mut(coord) {
            cached.last_accessed = counter;
            Some(&cached.data)
        } else {
            None
        }
    }

    /// Récupère un chunk s'il est en cache/disque, ou le génère via le générateur fourni
    pub fn get_or_load<F>(&mut self, coord: &PlanetChunkCoord, generator: F) -> &ChunkData
    where
        F: FnOnce() -> ChunkData,
    {
        herocraft_common::prof_scope!("voxel::cache::get_or_load");
        self.access_counter = self.access_counter.wrapping_add(1);
        let counter = self.access_counter;

        // 1. Déjà présent en mémoire ?
        if self.chunks.contains_key(coord) {
            let cached = self.chunks.get_mut(coord).unwrap();
            cached.last_accessed = counter;
            return &self.chunks.get(coord).unwrap().data;
        }

        // 2. Présent sur le disque ?
        let loaded_from_disk = if let Some(ref storage) = self.storage { storage.load_chunk(coord).ok().flatten() } else { None };

        let (data, is_dirty) = if let Some(disk_chunk) = loaded_from_disk {
            (disk_chunk, false)
        } else {
            // 3. Sinon : calcul procédural
            (generator(), false)
        };

        // Si la capacité maximale est atteinte, évincer le chunk le plus ancien
        if self.chunks.len() >= self.capacity {
            self.evict_oldest();
        }

        self.chunks.insert(*coord, CachedChunk { data, dirty: is_dirty, last_accessed: counter });

        &self.chunks.get(coord).unwrap().data
    }

    /// Modifie un voxel dans un chunk (marque le chunk comme `dirty`)
    pub fn set_voxel<F>(&mut self, coord: &PlanetChunkCoord, x: u8, y: u8, z: u8, block: BlockId, generator: F)
    where
        F: FnOnce() -> ChunkData,
    {
        self.get_or_load(coord, generator);
        if let Some(cached) = self.chunks.get_mut(coord) {
            cached.data.set_voxel(x, y, z, block);
            cached.dirty = true;
            cached.last_accessed = self.access_counter;
        }
    }

    /// Sauvegarde tous les chunks modifiés (`dirty == true`) sur disque
    pub fn flush_dirty(&mut self) -> io::Result<usize> {
        herocraft_common::prof_scope!("voxel::cache::flush_dirty");
        let storage = match self.storage {
            Some(ref s) => s,
            None => return Ok(0),
        };

        let mut saved_count = 0;
        for (coord, cached) in self.chunks.iter_mut() {
            if cached.dirty {
                storage.save_chunk(coord, &cached.data)?;
                cached.dirty = false;
                saved_count += 1;
            }
        }

        if saved_count > 0 {
            debug!("WorldChunkCache : {} chunks sauvegardés sur disque.", saved_count);
        }

        Ok(saved_count)
    }

    /// Évince le chunk le moins récemment accédé (LRU), en le sauvegardant si dirty
    fn evict_oldest(&mut self) {
        herocraft_common::prof_scope!("voxel::cache::evict_oldest");
        if self.chunks.is_empty() {
            return;
        }

        let mut oldest_coord: Option<PlanetChunkCoord> = None;
        let mut min_access = u64::MAX;

        for (coord, cached) in self.chunks.iter() {
            if cached.last_accessed < min_access {
                min_access = cached.last_accessed;
                oldest_coord = Some(*coord);
            }
        }

        if let Some(coord) = oldest_coord
            && let Some(cached) = self.chunks.remove(&coord)
        {
            // Sauvegarder sur disque avant suppression si le chunk a été modifié
            if cached.dirty
                && let Some(ref storage) = self.storage
            {
                let _ = storage.save_chunk(&coord, &cached.data);
            }
        }
    }

    /// Vide le cache en sauvegardant tous les chunks modifiés
    pub fn clear(&mut self) -> io::Result<()> {
        self.flush_dirty()?;
        self.chunks.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use herocraft_common::PlanetFace;

    #[test]
    fn test_cache_lru_and_dirty_flush() {
        let temp_dir = std::env::temp_dir().join("herocraft_test_cache");
        let _ = std::fs::remove_dir_all(&temp_dir);

        let storage = ChunkStorage::new(&temp_dir).unwrap();
        let mut cache = WorldChunkCache::new(Some(storage.clone()), 3);

        let c1 = PlanetChunkCoord::new(PlanetFace::PosX, 0, 0, 0);
        let c2 = PlanetChunkCoord::new(PlanetFace::PosX, 1, 0, 0);
        let c3 = PlanetChunkCoord::new(PlanetFace::PosX, 2, 0, 0);
        let c4 = PlanetChunkCoord::new(PlanetFace::PosX, 3, 0, 0);

        // 1. Insertion de 3 chunks
        cache.get_or_load(&c1, ChunkData::new_empty);
        cache.get_or_load(&c2, ChunkData::new_empty);
        cache.get_or_load(&c3, ChunkData::new_empty);
        assert_eq!(cache.len(), 3);

        // 2. Modification de c1 (il devient dirty)
        cache.set_voxel(&c1, 10, 10, 10, BlockId::STONE, ChunkData::new_empty);
        assert_eq!(cache.dirty_count(), 1);

        // 3. Accès à c2 et c3, puis insertion de c4 : c1 étant le plus ancien (ou c2 selon access),
        // let's access c1 so c2 is the oldest
        let _ = cache.get(&c1);
        let _ = cache.get(&c3);
        // Maintenant c2 est le moins récemment accédé
        cache.get_or_load(&c4, ChunkData::new_empty);
        assert_eq!(cache.len(), 3);
        assert!(cache.get(&c2).is_none(), "c2 aurait dû être évincé par la politique LRU");
        assert!(cache.get(&c1).is_some());

        // 4. Flush dirty sur disque
        let flushed = cache.flush_dirty().unwrap();
        assert_eq!(flushed, 1);
        assert_eq!(cache.dirty_count(), 0);
        assert!(storage.has_chunk(&c1));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
