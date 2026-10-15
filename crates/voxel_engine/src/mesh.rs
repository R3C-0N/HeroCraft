use crate::chunk::{ChunkData, CHUNK_SIZE};

/// Sommet de maillage voxel pour le pipeline WGPU / GPU
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VoxelVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub block_id: u32,
}

/// Maillage indexé prêt pour l'envoi vers un VertexBuffer et IndexBuffer GPU
#[derive(Debug, Default, Clone)]
pub struct VoxelMesh {
    pub vertices: Vec<VoxelVertex>,
    pub indices: Vec<u32>,
}

impl VoxelMesh {
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// Génère le maillage du chunk avec masquage des faces internes (Culled Meshing)
    pub fn build_from_chunk(chunk: &ChunkData) -> Self {
        let mut mesh = Self::default();

        for y in 0..CHUNK_SIZE as u8 {
            for z in 0..CHUNK_SIZE as u8 {
                for x in 0..CHUNK_SIZE as u8 {
                    let block = chunk.get_voxel(x, y, z);
                    if block.is_air() {
                        continue;
                    }

                    // Face Haut (+Y)
                    if y + 1 >= CHUNK_SIZE as u8 || !chunk.get_voxel(x, y + 1, z).is_opaque() {
                        mesh.add_quad_top(x as f32, y as f32, z as f32, block.0 as u32);
                    }
                    // Face Bas (-Y)
                    if y == 0 || !chunk.get_voxel(x, y - 1, z).is_opaque() {
                        mesh.add_quad_bottom(x as f32, y as f32, z as f32, block.0 as u32);
                    }
                    // Face Gauche (-X)
                    if x == 0 || !chunk.get_voxel(x - 1, y, z).is_opaque() {
                        mesh.add_quad_left(x as f32, y as f32, z as f32, block.0 as u32);
                    }
                    // Face Droite (+X)
                    if x + 1 >= CHUNK_SIZE as u8 || !chunk.get_voxel(x + 1, y, z).is_opaque() {
                        mesh.add_quad_right(x as f32, y as f32, z as f32, block.0 as u32);
                    }
                    // Face Avant (+Z)
                    if z + 1 >= CHUNK_SIZE as u8 || !chunk.get_voxel(x, y, z + 1).is_opaque() {
                        mesh.add_quad_front(x as f32, y as f32, z as f32, block.0 as u32);
                    }
                    // Face Arrière (-Z)
                    if z == 0 || !chunk.get_voxel(x, y, z - 1).is_opaque() {
                        mesh.add_quad_back(x as f32, y as f32, z as f32, block.0 as u32);
                    }
                }
            }
        }

        mesh
    }

    fn add_quad(
        &mut self,
        v0: [f32; 3],
        v1: [f32; 3],
        v2: [f32; 3],
        v3: [f32; 3],
        normal: [f32; 3],
        block_id: u32,
    ) {
        let base_index = self.vertices.len() as u32;

        self.vertices.push(VoxelVertex { position: v0, normal, uv: [0.0, 0.0], block_id });
        self.vertices.push(VoxelVertex { position: v1, normal, uv: [1.0, 0.0], block_id });
        self.vertices.push(VoxelVertex { position: v2, normal, uv: [1.0, 1.0], block_id });
        self.vertices.push(VoxelVertex { position: v3, normal, uv: [0.0, 1.0], block_id });

        // Deux triangles par quad
        self.indices.extend_from_slice(&[
            base_index,
            base_index + 1,
            base_index + 2,
            base_index,
            base_index + 2,
            base_index + 3,
        ]);
    }

    fn add_quad_top(&mut self, x: f32, y: f32, z: f32, id: u32) {
        self.add_quad(
            [x, y + 1.0, z + 1.0],
            [x + 1.0, y + 1.0, z + 1.0],
            [x + 1.0, y + 1.0, z],
            [x, y + 1.0, z],
            [0.0, 1.0, 0.0],
            id,
        );
    }

    fn add_quad_bottom(&mut self, x: f32, y: f32, z: f32, id: u32) {
        self.add_quad(
            [x, y, z],
            [x + 1.0, y, z],
            [x + 1.0, y, z + 1.0],
            [x, y, z + 1.0],
            [0.0, -1.0, 0.0],
            id,
        );
    }

    fn add_quad_left(&mut self, x: f32, y: f32, z: f32, id: u32) {
        self.add_quad(
            [x, y, z + 1.0],
            [x, y + 1.0, z + 1.0],
            [x, y + 1.0, z],
            [x, y, z],
            [-1.0, 0.0, 0.0],
            id,
        );
    }

    fn add_quad_right(&mut self, x: f32, y: f32, z: f32, id: u32) {
        self.add_quad(
            [x + 1.0, y, z],
            [x + 1.0, y + 1.0, z],
            [x + 1.0, y + 1.0, z + 1.0],
            [x + 1.0, y, z + 1.0],
            [1.0, 0.0, 0.0],
            id,
        );
    }

    fn add_quad_front(&mut self, x: f32, y: f32, z: f32, id: u32) {
        self.add_quad(
            [x, y, z + 1.0],
            [x + 1.0, y, z + 1.0],
            [x + 1.0, y + 1.0, z + 1.0],
            [x, y + 1.0, z + 1.0],
            [0.0, 0.0, 1.0],
            id,
        );
    }

    fn add_quad_back(&mut self, x: f32, y: f32, z: f32, id: u32) {
        self.add_quad(
            [x + 1.0, y, z],
            [x, y, z],
            [x, y + 1.0, z],
            [x + 1.0, y + 1.0, z],
            [0.0, 0.0, -1.0],
            id,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use herocraft_common::BlockId;

    #[test]
    fn test_single_block_meshing() {
        let mut chunk = ChunkData::new_empty();
        chunk.set_voxel(5, 5, 5, BlockId::STONE);

        let mesh = VoxelMesh::build_from_chunk(&chunk);
        // Un bloc isolé au milieu de l'air a 6 faces = 24 sommets, 36 indices (12 triangles)
        assert_eq!(mesh.vertices.len(), 24);
        assert_eq!(mesh.indices.len(), 36);
    }

    #[test]
    fn test_internal_face_culling() {
        let mut chunk = ChunkData::new_empty();
        // Deux blocs adjacents en X
        chunk.set_voxel(5, 5, 5, BlockId::STONE);
        chunk.set_voxel(6, 5, 5, BlockId::STONE);

        let mesh = VoxelMesh::build_from_chunk(&chunk);
        // 2 blocs = 12 faces au total, mais 2 faces communes masquées = 10 faces visibles = 40 sommets, 60 indices
        assert_eq!(mesh.vertices.len(), 40);
        assert_eq!(mesh.indices.len(), 60);
    }
}
