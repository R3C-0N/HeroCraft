use herocraft_common::{BlockId, LocalVoxelCoord};

pub const CHUNK_SIZE: usize = 32;
pub const CHUNK_VOLUME: usize = CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE;

/// Stockage dense et rapide d'un chunk de 32x32x32 blocs (32 768 voxels)
#[derive(Clone)]
pub struct ChunkData {
    pub voxels: Box<[BlockId; CHUNK_VOLUME]>,
}

impl Default for ChunkData {
    fn default() -> Self {
        Self::new_empty()
    }
}

impl ChunkData {
    pub fn new_empty() -> Self {
        Self {
            voxels: vec![BlockId::AIR; CHUNK_VOLUME]
                .into_boxed_slice()
                .try_into()
                .unwrap(),
        }
    }

    #[inline(always)]
    pub fn get_voxel(&self, x: u8, y: u8, z: u8) -> BlockId {
        let coord = LocalVoxelCoord::new(x, y, z);
        self.voxels[coord.to_linear_index()]
    }

    #[inline(always)]
    pub fn set_voxel(&mut self, x: u8, y: u8, z: u8, id: BlockId) {
        let coord = LocalVoxelCoord::new(x, y, z);
        let idx = coord.to_linear_index();
        self.voxels[idx] = id;
    }

    /// Compression RLE (Run-Length Encoding) ultra-rapide pour l'envoi réseau
    pub fn compress_rle(&self) -> Vec<u8> {
        herocraft_common::prof_scope!("voxel::chunk::compress_rle");
        let mut compressed = Vec::with_capacity(1024);
        let mut current_id = self.voxels[0];
        let mut run_length: u16 = 0;

        for &block in self.voxels.iter() {
            if block == current_id && run_length < u16::MAX {
                run_length += 1;
            } else {
                compressed.extend_from_slice(&current_id.0.to_le_bytes());
                compressed.extend_from_slice(&run_length.to_le_bytes());
                current_id = block;
                run_length = 1;
            }
        }
        compressed.extend_from_slice(&current_id.0.to_le_bytes());
        compressed.extend_from_slice(&run_length.to_le_bytes());
        compressed
    }

    /// Décompression RLE
    pub fn decompress_rle(bytes: &[u8]) -> Result<Self, &'static str> {
        herocraft_common::prof_scope!("voxel::chunk::decompress_rle");
        let mut chunk = Self::new_empty();
        let mut idx = 0;
        let mut byte_cursor = 0;

        while byte_cursor + 4 <= bytes.len() {
            let id = u16::from_le_bytes([bytes[byte_cursor], bytes[byte_cursor + 1]]);
            let count = u16::from_le_bytes([bytes[byte_cursor + 2], bytes[byte_cursor + 3]]) as usize;
            byte_cursor += 4;

            let end = (idx + count).min(CHUNK_VOLUME);
            for i in idx..end {
                chunk.voxels[i] = BlockId(id);
            }
            idx = end;
        }

        Ok(chunk)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_set_get() {
        let mut chunk = ChunkData::new_empty();
        assert_eq!(chunk.get_voxel(10, 15, 20), BlockId::AIR);

        chunk.set_voxel(10, 15, 20, BlockId::STONE);
        assert_eq!(chunk.get_voxel(10, 15, 20), BlockId::STONE);
    }

    #[test]
    fn test_chunk_rle_compression_roundtrip() {
        let mut chunk = ChunkData::new_empty();
        // Remplir une couche de pierre
        for x in 0..CHUNK_SIZE as u8 {
            for z in 0..CHUNK_SIZE as u8 {
                chunk.set_voxel(x, 0, z, BlockId::STONE);
            }
        }

        let compressed = chunk.compress_rle();
        // Le RLE sur un chunk principalement vide doit être minuscule (quelques dizaines d'octets au lieu de 65Ko)
        assert!(compressed.len() < 500);

        let decompressed = ChunkData::decompress_rle(&compressed).expect("Décompression failed");
        assert_eq!(decompressed.get_voxel(0, 0, 0), BlockId::STONE);
        assert_eq!(decompressed.get_voxel(0, 1, 0), BlockId::AIR);
    }
}
