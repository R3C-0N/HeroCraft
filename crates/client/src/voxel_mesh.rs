use glam::Vec3;
use herocraft_common::{BlockId, PlanetChunkCoord, PlanetFace};
use herocraft_voxel_engine::chunk::{ChunkData, CHUNK_SIZE};
use herocraft_world::generator::PlanetWorldGenerator;

use crate::planet_mesh::{PlanetMesh, PlanetVertex};

pub struct VoxelMeshBuilder;

impl VoxelMeshBuilder {
    /// Construit le maillage de voxels incurvés pour une grille de chunks autour d'un centre
    pub fn build_incurved_grid(
        generator: &PlanetWorldGenerator,
        face: PlanetFace,
        center_x: i32,
        center_y: i32,
        radius: i32,
        base_radius: f32,
    ) -> PlanetMesh {
        let mut vertices: Vec<PlanetVertex> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();

        // 32 chunks par face de cube
        let face_chunks_count = 32.0;
        let chunk_span_uv = 2.0 / face_chunks_count;
        let voxel_span_uv = chunk_span_uv / (CHUNK_SIZE as f64);
        let altitude_scale = 0.22_f32;

        for dy in -radius..=radius {
            for dx in -radius..=radius {
                let cx = (center_x + dx).clamp(0, 31);
                let cy = (center_y + dy).clamp(0, 31);
                let coord = PlanetChunkCoord::new(face, cx, cy, 0);

                let chunk = generator.generate_chunk(&coord);
                Self::mesh_single_chunk(
                    &chunk,
                    &coord,
                    chunk_span_uv,
                    voxel_span_uv,
                    base_radius,
                    altitude_scale,
                    &mut vertices,
                    &mut indices,
                );
            }
        }

        PlanetMesh { vertices, indices }
    }

    fn mesh_single_chunk(
        chunk: &ChunkData,
        coord: &PlanetChunkCoord,
        chunk_span_uv: f64,
        voxel_span_uv: f64,
        base_radius: f32,
        altitude_scale: f32,
        vertices: &mut Vec<PlanetVertex>,
        indices: &mut Vec<u32>,
    ) {
        let base_u = -1.0 + (coord.x as f64) * chunk_span_uv;
        let base_v = -1.0 + (coord.y as f64) * chunk_span_uv;
        let face = coord.face;

        let get_corner = |vx: f64, vy: f64, vz: f64| -> Vec3 {
            let u = base_u + vx * voxel_span_uv;
            let v = base_v + vz * voxel_span_uv;
            let dir = face.uv_to_sphere_direction(u, v);
            let r = base_radius + (vy as f32) * altitude_scale;
            Vec3::new(dir.x as f32, dir.y as f32, dir.z as f32) * r
        };

        for y in 0..CHUNK_SIZE as u8 {
            for z in 0..CHUNK_SIZE as u8 {
                for x in 0..CHUNK_SIZE as u8 {
                    let block = chunk.get_voxel(x, y, z);
                    if block.is_air() {
                        continue;
                    }

                    let fx = x as f64;
                    let fy = (coord.altitude_layer * (CHUNK_SIZE as i32)) as f64 + y as f64;
                    let fz = z as f64;

                    // 1. Face Haut (+Y)
                    if y + 1 >= CHUNK_SIZE as u8 || !chunk.get_voxel(x, y + 1, z).is_opaque() {
                        let c0 = get_corner(fx, fy + 1.0, fz);
                        let c1 = get_corner(fx + 1.0, fy + 1.0, fz);
                        let c2 = get_corner(fx + 1.0, fy + 1.0, fz + 1.0);
                        let c3 = get_corner(fx, fy + 1.0, fz + 1.0);
                        let color = Self::block_color(block, true);
                        Self::add_quad(vertices, indices, c0, c1, c2, c3, color);
                    }

                    // 2. Face Bas (-Y)
                    if y == 0 || !chunk.get_voxel(x, y - 1, z).is_opaque() {
                        let c0 = get_corner(fx, fy, fz + 1.0);
                        let c1 = get_corner(fx + 1.0, fy, fz + 1.0);
                        let c2 = get_corner(fx + 1.0, fy, fz);
                        let c3 = get_corner(fx, fy, fz);
                        let color = Self::block_color(block, false);
                        Self::add_quad(vertices, indices, c0, c1, c2, c3, color);
                    }

                    // 3. Face Gauche (-X)
                    if x == 0 || !chunk.get_voxel(x - 1, y, z).is_opaque() {
                        let c0 = get_corner(fx, fy, fz);
                        let c1 = get_corner(fx, fy, fz + 1.0);
                        let c2 = get_corner(fx, fy + 1.0, fz + 1.0);
                        let c3 = get_corner(fx, fy + 1.0, fz);
                        let color = Self::block_color(block, false);
                        Self::add_quad(vertices, indices, c0, c1, c2, c3, color);
                    }

                    // 4. Face Droite (+X)
                    if x + 1 >= CHUNK_SIZE as u8 || !chunk.get_voxel(x + 1, y, z).is_opaque() {
                        let c0 = get_corner(fx + 1.0, fy, fz + 1.0);
                        let c1 = get_corner(fx + 1.0, fy, fz);
                        let c2 = get_corner(fx + 1.0, fy + 1.0, fz);
                        let c3 = get_corner(fx + 1.0, fy + 1.0, fz + 1.0);
                        let color = Self::block_color(block, false);
                        Self::add_quad(vertices, indices, c0, c1, c2, c3, color);
                    }

                    // 5. Face Avant (+Z)
                    if z + 1 >= CHUNK_SIZE as u8 || !chunk.get_voxel(x, y, z + 1).is_opaque() {
                        let c0 = get_corner(fx, fy, fz + 1.0);
                        let c1 = get_corner(fx + 1.0, fy, fz + 1.0);
                        let c2 = get_corner(fx + 1.0, fy + 1.0, fz + 1.0);
                        let c3 = get_corner(fx, fy + 1.0, fz + 1.0);
                        let color = Self::block_color(block, false);
                        Self::add_quad(vertices, indices, c0, c1, c2, c3, color);
                    }

                    // 6. Face Arrière (-Z)
                    if z == 0 || !chunk.get_voxel(x, y, z - 1).is_opaque() {
                        let c0 = get_corner(fx + 1.0, fy, fz);
                        let c1 = get_corner(fx, fy, fz);
                        let c2 = get_corner(fx, fy + 1.0, fz);
                        let c3 = get_corner(fx + 1.0, fy + 1.0, fz);
                        let color = Self::block_color(block, false);
                        Self::add_quad(vertices, indices, c0, c1, c2, c3, color);
                    }
                }
            }
        }
    }

    fn add_quad(
        vertices: &mut Vec<PlanetVertex>,
        indices: &mut Vec<u32>,
        c0: Vec3,
        c1: Vec3,
        c2: Vec3,
        c3: Vec3,
        color: [f32; 4],
    ) {
        let base_idx = vertices.len() as u32;

        // Calcul de la normale via le produit vectoriel des arêtes
        let e1 = c1 - c0;
        let e2 = c2 - c0;
        let normal = e1.cross(e2).normalize_or_zero().to_array();

        vertices.push(PlanetVertex {
            position: c0.to_array(),
            normal,
            color,
        });
        vertices.push(PlanetVertex {
            position: c1.to_array(),
            normal,
            color,
        });
        vertices.push(PlanetVertex {
            position: c2.to_array(),
            normal,
            color,
        });
        vertices.push(PlanetVertex {
            position: c3.to_array(),
            normal,
            color,
        });

        // Premier triangle (c0, c1, c2)
        indices.push(base_idx);
        indices.push(base_idx + 1);
        indices.push(base_idx + 2);

        // Deuxième triangle (c0, c2, c3)
        indices.push(base_idx);
        indices.push(base_idx + 2);
        indices.push(base_idx + 3);
    }

    /// Palette de couleurs fidèle aux blocs HeroCraft
    fn block_color(block: BlockId, is_top: bool) -> [f32; 4] {
        match block {
            BlockId::GRASS => {
                if is_top {
                    [0.28, 0.65, 0.22, 1.0] // Sommet herbeux vert frais
                } else {
                    [0.46, 0.33, 0.18, 1.0] // Tranche de terre brune
                }
            }
            BlockId::DIRT => [0.46, 0.33, 0.18, 1.0],
            BlockId::STONE => [0.52, 0.50, 0.48, 1.0],
            BlockId::SAND => [0.88, 0.82, 0.56, 1.0],
            BlockId::SNOW => [0.96, 0.97, 1.00, 1.0],
            BlockId::WATER => [0.08, 0.48, 0.78, 1.0],
            BlockId::WOOD => [0.42, 0.28, 0.16, 1.0],
            BlockId::LEAVES => [0.14, 0.44, 0.16, 1.0],
            _ => [0.60, 0.60, 0.60, 1.0],
        }
    }
}
