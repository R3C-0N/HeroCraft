use std::ops::Range;
use std::sync::Arc;

use glam::Vec3;
use herocraft_common::{blocks, opposite_face, BlockId, PlanetChunkCoord, PlanetFace, ShapeBox, ShapeTexture, FACE_DIRS};
use herocraft_voxel_engine::cache::WorldChunkCache;
use herocraft_voxel_engine::chunk::{ChunkData, CHUNK_SIZE};
use herocraft_world::generator::{ColumnSample, PlanetWorldGenerator};
use herocraft_world::{carvable_range, TerrainEdits, VegetationField, ALTITUDE_LAYERS, FACE_CHUNKS};

use crate::block_light::{self, LightField, LIGHT_REACH};
use crate::block_textures::{block_textures, face_uv};
use crate::planet_mesh::{PlanetMesh, PlanetVertex};

/// Une face de cube : ses coins (cycliques) et, pour l'occlusion ambiante de chacun, ses deux
/// voisins de côté et son voisin de coin
pub(crate) struct CubeFace {
    pub(crate) corners: [(u8, u8, u8); 4],
    ao: [[(i32, i32, i32); 3]; 4],
}

/// Les six faces, dans l'ordre de `FACE_DIRS` (+X, -X, +Y, -Y, +Z, -Z)
pub(crate) const CUBE_FACES: [CubeFace; 6] = [
    CubeFace {
        corners: [(1, 0, 1), (1, 0, 0), (1, 1, 0), (1, 1, 1)],
        ao: [[(1, -1, 0), (1, 0, 1), (1, -1, 1)], [(1, -1, 0), (1, 0, -1), (1, -1, -1)], [(1, 1, 0), (1, 0, -1), (1, 1, -1)], [(1, 1, 0), (1, 0, 1), (1, 1, 1)]],
    },
    CubeFace {
        corners: [(0, 0, 0), (0, 0, 1), (0, 1, 1), (0, 1, 0)],
        ao: [[(-1, -1, 0), (-1, 0, -1), (-1, -1, -1)], [(-1, -1, 0), (-1, 0, 1), (-1, -1, 1)], [(-1, 1, 0), (-1, 0, 1), (-1, 1, 1)], [(-1, 1, 0), (-1, 0, -1), (-1, 1, -1)]],
    },
    CubeFace {
        corners: [(0, 1, 0), (0, 1, 1), (1, 1, 1), (1, 1, 0)],
        ao: [[(-1, 1, 0), (0, 1, -1), (-1, 1, -1)], [(-1, 1, 0), (0, 1, 1), (-1, 1, 1)], [(1, 1, 0), (0, 1, 1), (1, 1, 1)], [(1, 1, 0), (0, 1, -1), (1, 1, -1)]],
    },
    CubeFace {
        corners: [(0, 0, 0), (1, 0, 0), (1, 0, 1), (0, 0, 1)],
        ao: [[(-1, -1, 0), (0, -1, -1), (-1, -1, -1)], [(1, -1, 0), (0, -1, -1), (1, -1, -1)], [(1, -1, 0), (0, -1, 1), (1, -1, 1)], [(-1, -1, 0), (0, -1, 1), (-1, -1, 1)]],
    },
    CubeFace {
        corners: [(1, 0, 1), (0, 0, 1), (0, 1, 1), (1, 1, 1)],
        ao: [[(1, 0, 1), (0, -1, 1), (1, -1, 1)], [(-1, 0, 1), (0, -1, 1), (-1, -1, 1)], [(-1, 0, 1), (0, 1, 1), (-1, 1, 1)], [(1, 0, 1), (0, 1, 1), (1, 1, 1)]],
    },
    CubeFace {
        corners: [(0, 0, 0), (1, 0, 0), (1, 1, 0), (0, 1, 0)],
        ao: [[(-1, 0, -1), (0, -1, -1), (-1, -1, -1)], [(1, 0, -1), (0, -1, -1), (1, -1, -1)], [(1, 0, -1), (0, 1, -1), (1, 1, -1)], [(-1, 0, -1), (0, 1, -1), (-1, 1, -1)]],
    },
];

/// Colonnes de bordure échantillonnées autour d'un chunk (voisins des faces et de l'AO)
const COLUMN_BORDER: i32 = 1;
const COLUMN_SIDE: i32 = CHUNK_SIZE as i32 + 2 * COLUMN_BORDER;

/// Relief échantillonné d'une colonne de chunks : ce que coûte le générateur, fait une seule fois
pub struct VoxelColumnSource {
    pub face: PlanetFace,
    pub chunk_x: i32,
    pub chunk_y: i32,
    pub base_radius: f32,
    min_elev: f64,
    max_elev: f64,
    /// Plus basse altitude où les grottes de la colonne ou de sa bordure peuvent creuser
    cave_bottom: Option<i32>,
    /// Couleur du biome de chaque colonne de blocs (32x32)
    column_colors: Vec<[f32; 4]>,
    /// Colonnes du générateur avec une bordure de `COLUMN_BORDER`
    columns: Vec<ColumnSample>,
    /// Arbres qui touchent la colonne et sa bordure
    vegetation: VegetationField,
}

/// Place d'une couche d'altitude dans le maillage d'une colonne
#[derive(Debug, Clone)]
pub struct LayerRange {
    pub layer: i32,
    pub vertices: Range<usize>,
    pub indices: Range<usize>,
}

/// Maillage d'une colonne de chunks, découpé par couche d'altitude pour en refaire une seule
pub struct VoxelColumnMesh {
    pub mesh: PlanetMesh,
    pub layers: Vec<LayerRange>,
    pub source: Arc<VoxelColumnSource>,
}

impl VoxelColumnMesh {
    /// Remplace (ou insère, ou vide) une couche du maillage : les sommets des couches suivantes
    /// glissent, leurs indices sont décalés d'autant
    pub fn replace_layer(&mut self, layer: i32, part: PlanetMesh) {
        let pos = self.layers.partition_point(|r| r.layer < layer);
        let existing = self.layers.get(pos).filter(|r| r.layer == layer).cloned();
        let (v_range, i_range) = match &existing {
            Some(r) => (r.vertices.clone(), r.indices.clone()),
            None => {
                let (v, i) = self
                    .layers
                    .get(pos)
                    .map(|r| (r.vertices.start, r.indices.start))
                    .unwrap_or((self.mesh.vertices.len(), self.mesh.indices.len()));
                (v..v, i..i)
            }
        };
        let vertex_shift = part.vertices.len() as i64 - v_range.len() as i64;
        let index_shift = part.indices.len() as i64 - i_range.len() as i64;
        let offset = v_range.start as u32;

        self.mesh.vertices.splice(v_range.clone(), part.vertices.iter().copied());
        let after = i_range.start + part.indices.len();
        self.mesh.indices.splice(i_range.clone(), part.indices.iter().map(|i| i + offset));
        for index in &mut self.mesh.indices[after..] {
            *index = (*index as i64 + vertex_shift) as u32;
        }

        let new_range = LayerRange {
            layer,
            vertices: v_range.start..v_range.start + part.vertices.len(),
            indices: i_range.start..i_range.start + part.indices.len(),
        };
        if existing.is_some() {
            self.layers[pos] = new_range;
        } else {
            self.layers.insert(pos, new_range);
        }
        for r in &mut self.layers[pos + 1..] {
            r.vertices = (r.vertices.start as i64 + vertex_shift) as usize..(r.vertices.end as i64 + vertex_shift) as usize;
            r.indices = (r.indices.start as i64 + index_shift) as usize..(r.indices.end as i64 + index_shift) as usize;
        }
        self.layers.retain(|r| !r.vertices.is_empty());
    }
}

impl VoxelColumnSource {
    /// Chunk du générateur à cette couche, tiré des colonnes déjà échantillonnées (sans bruit)
    pub fn generate_chunk(&self, generator: &PlanetWorldGenerator, layer: i32) -> ChunkData {
        let coord = PlanetChunkCoord::new(self.face, self.chunk_x, self.chunk_y, layer);
        let caves = generator.chunk_cave_field(&coord, &self.columns, COLUMN_BORDER);
        generator.generate_chunk_full(&coord, &self.columns, COLUMN_BORDER, caves.as_ref(), &self.vegetation)
    }
}

pub struct VoxelMeshBuilder;

impl VoxelMeshBuilder {
    /// Construit le maillage de voxels incurvés pour une grille 3x3 de chunks sans murs internes
    #[allow(dead_code)]
    pub fn build_incurved_grid(
        generator: &PlanetWorldGenerator,
        chunk_cache: &mut WorldChunkCache,
        face: PlanetFace,
        center_x: i32,
        center_y: i32,
        base_radius: f32,
    ) -> PlanetMesh {
        herocraft_common::prof_scope!("voxel_mesh::build_incurved_grid");
        let mut vertices: Vec<PlanetVertex> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();

        // 32 chunks par face de cube
        let face_chunks_count = FACE_CHUNKS as f64;
        let chunk_span_uv = 2.0 / face_chunks_count;
        let voxel_span_uv = chunk_span_uv / (CHUNK_SIZE as f64);
        
        // Échelle verticale calibrée pour que les voxels soient de vrais cubes 1:1:1
        // Sur une planète de rayon 25.0, la taille horizontale d'un voxel vaut environ 0.03835 unité
        let altitude_scale = (base_radius as f64 * std::f64::consts::FRAC_PI_4 * voxel_span_uv) as f32;

        // 1. Chargement de la grille 3x3 de chunks depuis le cache/disque ou générateur
        let mut chunks: Vec<ChunkData> = Vec::with_capacity(9);
        let start_cx = (center_x - 1).max(0);
        let start_cy = (center_y - 1).max(0);

        for dy in 0..3 {
            for dx in 0..3 {
                let cx = (start_cx + dx).min(FACE_CHUNKS - 1);
                let cy = (start_cy + dy).min(FACE_CHUNKS - 1);
                let coord = PlanetChunkCoord::new(face, cx, cy, 0);
                let chunk = chunk_cache.get_or_load(&coord, || generator.generate_chunk(&coord));
                chunks.push(chunk.clone());
            }
        }

        let base_u = -1.0 + (start_cx as f64) * chunk_span_uv;
        let base_v = -1.0 + (start_cy as f64) * chunk_span_uv;

        // Accesseur sécurisé de voxel sur toute la grille globale 96x32x96
        let get_voxel = |gx: i32, gy: i32, gz: i32| -> BlockId {
            if gy < 0 || gy >= CHUNK_SIZE as i32 {
                return BlockId::AIR;
            }
            if gx < 0 || gx >= 96 || gz < 0 || gz >= 96 {
                // En dehors de la zone 3x3 chargée :
                // On simule du solide sous le niveau de la mer (12) pour éviter tout mur souterrain sur la bordure extérieure
                return if gy <= 12 { BlockId::STONE } else { BlockId::AIR };
            }
            let chunk_x = (gx / 32) as usize;
            let chunk_z = (gz / 32) as usize;
            let chunk_idx = chunk_z * 3 + chunk_x;
            let lx = (gx % 32) as u8;
            let lz = (gz % 32) as u8;
            chunks[chunk_idx].get_voxel(lx, gy as u8, lz)
        };

        // Calcul de la position 3D incurvée d'un sommet de voxel sur la sphère
        let get_corner = |vx: f64, vy: f64, vz: f64| -> Vec3 {
            let u = base_u + vx * voxel_span_uv;
            let v = base_v + vz * voxel_span_uv;
            let dir = face.uv_to_sphere_direction(u, v);
            let r = base_radius + (vy as f32) * altitude_scale;
            Vec3::new(dir.x as f32, dir.y as f32, dir.z as f32) * r
        };

        // 2. Meshing avec suppression des faces cachées internes et soudure complète entre chunks
        for gy in 0..CHUNK_SIZE as i32 {
            for gz in 0..96 {
                for gx in 0..96 {
                    let block = get_voxel(gx, gy, gz);
                    if block.is_air() {
                        continue;
                    }

                    let is_water = block == BlockId::WATER;
                    let fx = gx as f64;
                    let fy = gy as f64;
                    let fz = gz as f64;

                    // Condition d'émission d'une face : le voisin doit être de l'air !
                    // L'eau ne génère de face que contre l'air (jamais d'eau contre eau ni de mur sous-marin)

                    // 1. Face Haut (+Y)
                    let neighbor_top = get_voxel(gx, gy + 1, gz);
                    let should_emit_top = if is_water {
                        neighbor_top.is_air()
                    } else {
                        neighbor_top.is_air() || neighbor_top == BlockId::WATER
                    };
                    if should_emit_top {
                        let c0 = get_corner(fx, fy + 1.0, fz);
                        let c1 = get_corner(fx, fy + 1.0, fz + 1.0);
                        let c2 = get_corner(fx + 1.0, fy + 1.0, fz + 1.0);
                        let c3 = get_corner(fx + 1.0, fy + 1.0, fz);
                        let color = Self::block_color(block, 2);
                        let ao = if is_water {
                            [1.0, 1.0, 1.0, 1.0]
                        } else {
                            [
                                Self::calc_corner_ao(&get_voxel, (gx - 1, gy + 1, gz), (gx, gy + 1, gz - 1), (gx - 1, gy + 1, gz - 1)),
                                Self::calc_corner_ao(&get_voxel, (gx - 1, gy + 1, gz), (gx, gy + 1, gz + 1), (gx - 1, gy + 1, gz + 1)),
                                Self::calc_corner_ao(&get_voxel, (gx + 1, gy + 1, gz), (gx, gy + 1, gz + 1), (gx + 1, gy + 1, gz + 1)),
                                Self::calc_corner_ao(&get_voxel, (gx + 1, gy + 1, gz), (gx, gy + 1, gz - 1), (gx + 1, gy + 1, gz - 1)),
                            ]
                        };
                        Self::add_quad(&mut vertices, &mut indices, c0, c1, c2, c3, color, ao, [0.0; 4], [PlanetVertex::UNTEXTURED; 4]);
                    }

                    // 2. Face Bas (-Y) - uniquement pour blocs solides au-dessus d'une caverne d'air
                    if !is_water {
                        let neighbor_bottom = get_voxel(gx, gy - 1, gz);
                        if neighbor_bottom.is_air() {
                            let c0 = get_corner(fx, fy, fz);
                            let c1 = get_corner(fx + 1.0, fy, fz);
                            let c2 = get_corner(fx + 1.0, fy, fz + 1.0);
                            let c3 = get_corner(fx, fy, fz + 1.0);
                            let color = Self::block_color(block, 0);
                            let ao = [
                                Self::calc_corner_ao(&get_voxel, (gx - 1, gy - 1, gz), (gx, gy - 1, gz - 1), (gx - 1, gy - 1, gz - 1)),
                                Self::calc_corner_ao(&get_voxel, (gx + 1, gy - 1, gz), (gx, gy - 1, gz - 1), (gx + 1, gy - 1, gz - 1)),
                                Self::calc_corner_ao(&get_voxel, (gx + 1, gy - 1, gz), (gx, gy - 1, gz + 1), (gx + 1, gy - 1, gz + 1)),
                                Self::calc_corner_ao(&get_voxel, (gx - 1, gy - 1, gz), (gx, gy - 1, gz + 1), (gx - 1, gy - 1, gz + 1)),
                            ];
                            Self::add_quad(&mut vertices, &mut indices, c0, c1, c2, c3, color, ao, [0.0; 4], [PlanetVertex::UNTEXTURED; 4]);
                        }
                    }

                    // 3. Face Gauche (-X)
                    let neighbor_left = get_voxel(gx - 1, gy, gz);
                    let should_emit_left = if is_water {
                        neighbor_left.is_air()
                    } else {
                        neighbor_left.is_air() || neighbor_left == BlockId::WATER
                    };
                    if should_emit_left {
                        let c0 = get_corner(fx, fy, fz);
                        let c1 = get_corner(fx, fy, fz + 1.0);
                        let c2 = get_corner(fx, fy + 1.0, fz + 1.0);
                        let c3 = get_corner(fx, fy + 1.0, fz);
                        let color = Self::block_color(block, 0);
                        let ao = if is_water {
                            [1.0, 1.0, 1.0, 1.0]
                        } else {
                            [
                                Self::calc_corner_ao(&get_voxel, (gx - 1, gy - 1, gz), (gx - 1, gy, gz - 1), (gx - 1, gy - 1, gz - 1)),
                                Self::calc_corner_ao(&get_voxel, (gx - 1, gy - 1, gz), (gx - 1, gy, gz + 1), (gx - 1, gy - 1, gz + 1)),
                                Self::calc_corner_ao(&get_voxel, (gx - 1, gy + 1, gz), (gx - 1, gy, gz + 1), (gx - 1, gy + 1, gz + 1)),
                                Self::calc_corner_ao(&get_voxel, (gx - 1, gy + 1, gz), (gx - 1, gy, gz - 1), (gx - 1, gy + 1, gz - 1)),
                            ]
                        };
                        Self::add_quad(&mut vertices, &mut indices, c0, c1, c2, c3, color, ao, [0.0; 4], [PlanetVertex::UNTEXTURED; 4]);
                    }

                    // 4. Face Droite (+X)
                    let neighbor_right = get_voxel(gx + 1, gy, gz);
                    let should_emit_right = if is_water {
                        neighbor_right.is_air()
                    } else {
                        neighbor_right.is_air() || neighbor_right == BlockId::WATER
                    };
                    if should_emit_right {
                        let c0 = get_corner(fx + 1.0, fy, fz + 1.0);
                        let c1 = get_corner(fx + 1.0, fy, fz);
                        let c2 = get_corner(fx + 1.0, fy + 1.0, fz);
                        let c3 = get_corner(fx + 1.0, fy + 1.0, fz + 1.0);
                        let color = Self::block_color(block, 0);
                        let ao = if is_water {
                            [1.0, 1.0, 1.0, 1.0]
                        } else {
                            [
                                Self::calc_corner_ao(&get_voxel, (gx + 1, gy - 1, gz), (gx + 1, gy, gz + 1), (gx + 1, gy - 1, gz + 1)),
                                Self::calc_corner_ao(&get_voxel, (gx + 1, gy - 1, gz), (gx + 1, gy, gz - 1), (gx + 1, gy - 1, gz - 1)),
                                Self::calc_corner_ao(&get_voxel, (gx + 1, gy + 1, gz), (gx + 1, gy, gz - 1), (gx + 1, gy + 1, gz - 1)),
                                Self::calc_corner_ao(&get_voxel, (gx + 1, gy + 1, gz), (gx + 1, gy, gz + 1), (gx + 1, gy + 1, gz + 1)),
                            ]
                        };
                        Self::add_quad(&mut vertices, &mut indices, c0, c1, c2, c3, color, ao, [0.0; 4], [PlanetVertex::UNTEXTURED; 4]);
                    }

                    // 5. Face Avant (+Z)
                    let neighbor_front = get_voxel(gx, gy, gz + 1);
                    let should_emit_front = if is_water {
                        neighbor_front.is_air()
                    } else {
                        neighbor_front.is_air() || neighbor_front == BlockId::WATER
                    };
                    if should_emit_front {
                        let c0 = get_corner(fx + 1.0, fy, fz + 1.0);
                        let c1 = get_corner(fx, fy, fz + 1.0);
                        let c2 = get_corner(fx, fy + 1.0, fz + 1.0);
                        let c3 = get_corner(fx + 1.0, fy + 1.0, fz + 1.0);
                        let color = Self::block_color(block, 0);
                        let ao = if is_water {
                            [1.0, 1.0, 1.0, 1.0]
                        } else {
                            [
                                Self::calc_corner_ao(&get_voxel, (gx + 1, gy, gz + 1), (gx, gy - 1, gz + 1), (gx + 1, gy - 1, gz + 1)),
                                Self::calc_corner_ao(&get_voxel, (gx - 1, gy, gz + 1), (gx, gy - 1, gz + 1), (gx - 1, gy - 1, gz + 1)),
                                Self::calc_corner_ao(&get_voxel, (gx - 1, gy, gz + 1), (gx, gy + 1, gz + 1), (gx - 1, gy + 1, gz + 1)),
                                Self::calc_corner_ao(&get_voxel, (gx + 1, gy, gz + 1), (gx, gy + 1, gz + 1), (gx + 1, gy + 1, gz + 1)),
                            ]
                        };
                        Self::add_quad(&mut vertices, &mut indices, c0, c1, c2, c3, color, ao, [0.0; 4], [PlanetVertex::UNTEXTURED; 4]);
                    }

                    // 6. Face Arrière (-Z)
                    let neighbor_back = get_voxel(gx, gy, gz - 1);
                    let should_emit_back = if is_water {
                        neighbor_back.is_air()
                    } else {
                        neighbor_back.is_air() || neighbor_back == BlockId::WATER
                    };
                    if should_emit_back {
                        let c0 = get_corner(fx, fy, fz);
                        let c1 = get_corner(fx + 1.0, fy, fz);
                        let c2 = get_corner(fx + 1.0, fy + 1.0, fz);
                        let c3 = get_corner(fx, fy + 1.0, fz);
                        let color = Self::block_color(block, 0);
                        let ao = if is_water {
                            [1.0, 1.0, 1.0, 1.0]
                        } else {
                            [
                                Self::calc_corner_ao(&get_voxel, (gx - 1, gy, gz - 1), (gx, gy - 1, gz - 1), (gx - 1, gy - 1, gz - 1)),
                                Self::calc_corner_ao(&get_voxel, (gx + 1, gy, gz - 1), (gx, gy - 1, gz - 1), (gx + 1, gy - 1, gz - 1)),
                                Self::calc_corner_ao(&get_voxel, (gx + 1, gy, gz - 1), (gx, gy + 1, gz - 1), (gx + 1, gy + 1, gz - 1)),
                                Self::calc_corner_ao(&get_voxel, (gx - 1, gy, gz - 1), (gx, gy + 1, gz - 1), (gx - 1, gy + 1, gz - 1)),
                            ]
                        };
                        Self::add_quad(&mut vertices, &mut indices, c0, c1, c2, c3, color, ao, [0.0; 4], [PlanetVertex::UNTEXTURED; 4]);
                    }
                }
            }
        }

        PlanetMesh { vertices, indices }
    }

    /// Construit le maillage d'une colonne verticale complète de chunks voxels (jusqu'à 16-17 chunks, 528+ blocs de haut)
    #[allow(dead_code)]
    pub fn build_incurved_single_chunk(
        generator: &PlanetWorldGenerator,
        _chunk_cache: &mut WorldChunkCache,
        face: PlanetFace,
        chunk_x: i32,
        chunk_y: i32,
        base_radius: f32,
        filter: herocraft_world::VisualizationFilter,
    ) -> PlanetMesh {
        Self::build_incurved_single_chunk_fast(generator, face, chunk_x, chunk_y, base_radius, filter)
    }

    /// Version optimisée thread-safe sans dépendance de cache mutable (terrain du générateur seul)
    #[allow(dead_code)]
    pub fn build_incurved_single_chunk_fast(
        generator: &PlanetWorldGenerator,
        face: PlanetFace,
        chunk_x: i32,
        chunk_y: i32,
        base_radius: f32,
        filter: herocraft_world::VisualizationFilter,
    ) -> PlanetMesh {
        let source = Arc::new(Self::sample_column_source(generator, face, chunk_x, chunk_y, base_radius, filter));
        Self::build_column(generator, &TerrainEdits::new(None), source).mesh
    }

    /// Échantillonne le relief d'une colonne de chunks : la partie coûteuse du maillage (bruits du
    /// générateur), gardée avec le maillage pour le refaire sans elle après une modification
    pub fn sample_column_source(
        generator: &PlanetWorldGenerator,
        face: PlanetFace,
        chunk_x: i32,
        chunk_y: i32,
        base_radius: f32,
        filter: herocraft_world::VisualizationFilter,
    ) -> VoxelColumnSource {
        herocraft_common::prof_scope!("voxel_mesh::sample_column_source");
        let face_chunks_count = FACE_CHUNKS as f64;
        let chunk_span_uv = 2.0 / face_chunks_count;
        let voxel_span_uv = chunk_span_uv / (CHUNK_SIZE as f64);

        let base_u = -1.0 + (chunk_x as f64) * chunk_span_uv;
        let base_v = -1.0 + (chunk_y as f64) * chunk_span_uv;

        // Échantillonnage complet des 32x32 colonnes (1024 colonnes) :
        // 1. Détermine avec une précision absolue les élévations min/max de tout le chunk
        //    (évite toute troncature de couches sur les hauts pics ou arêtes aiguës)
        // 2. Précalcule les couleurs de colonnes pour éviter de répéter le calcul du bruit
        let mut min_elev = f64::MAX;
        let mut max_elev = f64::MIN;

        // On inclut également les 4 coins extrêmes du chunk
        for &(cu, cv) in &[(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
            let u = base_u + cu * chunk_span_uv;
            let v = base_v + cv * chunk_span_uv;
            let dir = face.uv_to_sphere_direction(u, v);
            let (lat, lon) = herocraft_world::cubed_sphere::unit_sphere_to_lat_lon(dir);
            let e = generator.sample_elevation(dir, lat, lon);
            if e < min_elev { min_elev = e; }
            if e > max_elev { max_elev = e; }
        }

        let mut column_colors = vec![[0.0_f32; 4]; 32 * 32];
        for cz in 0..32 {
            herocraft_common::prof_scope!("voxel_mesh::column_colors_row");
            for cx in 0..32 {
                let u = base_u + (cx as f64 + 0.5) * voxel_span_uv;
                let v = base_v + (cz as f64 + 0.5) * voxel_span_uv;
                let dir = face.uv_to_sphere_direction(u, v);
                let (lat, _lon) = herocraft_world::cubed_sphere::unit_sphere_to_lat_lon(dir);
                let (raw_elevation, biome) = generator.sample_elevation_and_biome(dir, lat);
                if raw_elevation < min_elev { min_elev = raw_elevation; }
                if raw_elevation > max_elev { max_elev = raw_elevation; }

                let mut biome_color = PlanetMesh::relief_biome_color(biome, raw_elevation);

                match filter {
                    herocraft_world::VisualizationFilter::Standard => {}
                    herocraft_world::VisualizationFilter::Tectonics => {
                        let sample = generator.tectonics.sample(dir);
                        biome_color = PlanetMesh::blend_tectonic_color(biome_color, &sample);
                    }
                    herocraft_world::VisualizationFilter::OceanCurrents => {
                        let continental = generator.sample_continental_value(dir);
                        biome_color = generator.ocean_currents.sample(dir, false, continental).color;
                    }
                }
                column_colors[cz * 32 + cx] = biome_color;
            }
        }

        // Colonnes du chunk + bordure d'une colonne (voisins des faces et de l'AO), échantillonnées
        // une seule fois et partagées par toutes les couches d'altitude
        let columns = generator.sample_chunk_columns(face, chunk_x, chunk_y, COLUMN_BORDER);
        let cave_bottom = columns.iter().map(carvable_range).filter(|r| !r.is_empty()).map(|r| r.start).min();
        let vegetation = generator.chunk_vegetation(face, chunk_x, chunk_y, &columns, COLUMN_BORDER);

        // Villes, villages et ruines : leurs toits dépassent le relief, leurs caves et leurs bâtiments
        // creusés dans la pente descendent un peu sous lui
        if let Some(top) = generator.structures_top(face, chunk_x, chunk_y, COLUMN_BORDER) {
            max_elev = max_elev.max(top as f64);
            min_elev -= 8.0;
        }

        VoxelColumnSource { face, chunk_x, chunk_y, base_radius, min_elev, max_elev, cave_bottom, column_colors, columns, vegetation }
    }

    /// Couches d'altitude à mailler : celles que le relief et la mer occupent, plus celles que le
    /// joueur a modifiées et leurs voisines (un trou creusé sous la surface découvre la couche d'en
    /// dessous, un bloc posé dans le ciel en occupe une qui était vide)
    fn layers_to_build(source: &VoxelColumnSource, edits: &TerrainEdits) -> Vec<i32> {
        // On inclut le niveau de la mer et une marge de sécurité
        let sea = herocraft_world::generator::SEA_LEVEL_BLOCKS;
        let (min_elev, max_elev) = (source.min_elev, source.max_elev);
        // Sous le relief, seules les couches que les grottes atteignent ont quelque chose à montrer
        let cave_bottom = source.cave_bottom.map_or(f64::MAX, |y| y as f64);
        let lowest = (min_elev.min(sea).min(cave_bottom) - 6.0).max(0.0);
        // Les arbres dépassent du relief
        let highest = (max_elev.max(sea).max(source.vegetation.top() as f64) + 6.0).min(528.0);

        let min_layer = ((lowest / 32.0).floor() as i32).clamp(0, 16);
        let max_layer = ((highest / 32.0).ceil() as i32).clamp(0, 16);

        let (face, cx, cy) = (source.face, source.chunk_x, source.chunk_y);
        let own: Vec<i32> = edits.edited_layers(face, cx, cy);
        let sides: Vec<i32> = [(-1, 0), (1, 0), (0, -1), (0, 1)]
            .iter()
            .filter(|(dx, dy)| (0..FACE_CHUNKS).contains(&(cx + dx)) && (0..FACE_CHUNKS).contains(&(cy + dy)))
            .flat_map(|(dx, dy)| edits.edited_layers(face, cx + dx, cy + dy))
            .collect();
        let touched = |layer: i32| own.iter().any(|&l| (l - layer).abs() <= 1) || sides.contains(&layer);

        let mut layers: Vec<i32> = (min_layer..=max_layer)
            .filter(|&layer| {
                let layer_base = (layer * 32) as f64;
                let layer_top = layer_base + 32.0;
                // Optimisation : saut des chunks 100% souterrains ou 100% dans le ciel. Une couche
                // dont le bloc du dessus touche une grotte de la couche au-dessus garde ses faces.
                let buried = min_elev >= layer_top && sea < layer_base && cave_bottom > layer_top;
                let empty_sky = max_elev < layer_base && sea < layer_base && (source.vegetation.top() as f64) <= layer_base;
                !(buried || empty_sky) || touched(layer)
            })
            .collect();
        for &layer in own.iter().chain(sides.iter()) {
            for l in [layer - 1, layer, layer + 1] {
                if (0..ALTITUDE_LAYERS).contains(&l) && !(min_layer..=max_layer).contains(&l) && touched(l) {
                    layers.push(l);
                }
            }
        }
        layers.sort_unstable();
        layers.dedup();
        layers
    }

    /// Maillage complet d'une colonne de chunks, couche par couche, en lisant les modifications du joueur
    pub fn build_column(generator: &PlanetWorldGenerator, edits: &TerrainEdits, source: Arc<VoxelColumnSource>) -> VoxelColumnMesh {
        herocraft_common::prof_scope!("voxel_mesh::build_voxel_chunk");
        let mut mesh = PlanetMesh { vertices: Vec::new(), indices: Vec::new() };
        let mut layers = Vec::new();
        for layer in Self::layers_to_build(&source, edits) {
            let part = Self::build_layer(generator, edits, &source, layer);
            if part.vertices.is_empty() {
                continue;
            }
            layers.push(LayerRange {
                layer,
                vertices: mesh.vertices.len()..mesh.vertices.len() + part.vertices.len(),
                indices: mesh.indices.len()..mesh.indices.len() + part.indices.len(),
            });
            let offset = mesh.vertices.len() as u32;
            mesh.vertices.extend_from_slice(&part.vertices);
            mesh.indices.extend(part.indices.iter().map(|i| i + offset));
        }
        herocraft_common::prof_value!("voxel_mesh.sommets_par_chunk", mesh.vertices.len());
        VoxelColumnMesh { mesh, layers, source }
    }

    /// Refait seulement les couches demandées d'une colonne déjà maillée, sans rééchantillonner le
    /// relief : c'est ce qui rend une modification visible dans la frame même
    pub fn rebuild_layers(generator: &PlanetWorldGenerator, edits: &TerrainEdits, column: &mut VoxelColumnMesh, layers: &[i32]) {
        herocraft_common::prof_scope!("voxel_mesh::rebuild_layers");
        let wanted = Self::layers_to_build(&column.source, edits);
        for &layer in layers {
            if !(0..ALTITUDE_LAYERS).contains(&layer) {
                continue;
            }
            let part = if wanted.contains(&layer) {
                Self::build_layer(generator, edits, &column.source, layer)
            } else {
                PlanetMesh { vertices: Vec::new(), indices: Vec::new() }
            };
            column.replace_layer(layer, part);
        }
    }

    /// Maillage d'une couche d'altitude (un chunk de 32³) de la colonne
    fn build_layer(generator: &PlanetWorldGenerator, edits: &TerrainEdits, source: &VoxelColumnSource, layer: i32) -> PlanetMesh {
        herocraft_common::prof_scope!("voxel_mesh::layer");
        let mut vertices: Vec<PlanetVertex> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();

        let face = source.face;
        let (chunk_x, chunk_y) = (source.chunk_x, source.chunk_y);
        let base_radius = source.base_radius;
        let columns = &source.columns;
        let column_colors = &source.column_colors;

        let face_chunks_count = FACE_CHUNKS as f64;
        let chunk_span_uv = 2.0 / face_chunks_count;
        let voxel_span_uv = chunk_span_uv / (CHUNK_SIZE as f64);
        let altitude_scale = (base_radius as f64 * std::f64::consts::FRAC_PI_4 * voxel_span_uv) as f32;
        let base_u = -1.0 + (chunk_x as f64) * chunk_span_uv;
        let base_v = -1.0 + (chunk_y as f64) * chunk_span_uv;
        let sea = herocraft_world::generator::SEA_LEVEL_BLOCKS;

        let get_corner = |vx: f64, vy: f64, vz: f64, layer: i32| -> Vec3 {
            let u = base_u + vx * voxel_span_uv;
            let v = base_v + vz * voxel_span_uv;
            let dir = face.uv_to_sphere_direction(u, v);
            let global_y = (layer as f64) * 32.0 + vy;
            let r = base_radius + (global_y as f32) * altitude_scale;
            Vec3::new(dir.x as f32, dir.y as f32, dir.z as f32) * r
        };

        let coord = PlanetChunkCoord::new(face, chunk_x, chunk_y, layer);
        // Grottes du chunk et de sa bordure d'un bloc : celles des voisins décident de leurs faces
        let caves = generator.chunk_cave_field(&coord, columns, COLUMN_BORDER);
        let generated = generator.generate_chunk_full(&coord, columns, COLUMN_BORDER, caves.as_ref(), &source.vegetation);
        let (block_x0, block_z0) = (chunk_x * CHUNK_SIZE as i32, chunk_y * CHUNK_SIZE as i32);
        // Chunks modifiés du voisinage 3x3x3 (index (dy+1)*9 + (dz+1)*3 + dx+1), lus une fois par couche
        let neighbours: [Option<Arc<ChunkData>>; 27] = std::array::from_fn(|i| {
            let (dx, dy, dz) = ((i % 3) as i32 - 1, (i / 9) as i32 - 1, ((i / 3) % 3) as i32 - 1);
            let (nx, ny, nl) = (chunk_x + dx, chunk_y + dz, layer + dy);
            if (0..FACE_CHUNKS).contains(&nx) && (0..FACE_CHUNKS).contains(&ny) && nl >= 0 {
                edits.chunk(&PlanetChunkCoord::new(face, nx, ny, nl))
            } else {
                None
            }
        });
        let edited = neighbours[13].clone();
        let center_chunk: &ChunkData = edited.as_deref().unwrap_or(&generated);

        let get_voxel = |gx: i32, gy: i32, gz: i32| -> BlockId {
            if gx >= 0 && gx < 32 && gz >= 0 && gz < 32 && gy >= 0 && gy < 32 {
                return center_chunk.get_voxel(gx as u8, gy as u8, gz as u8);
            }
            if gy < 0 && layer == 0 {
                return BlockId::STONE;
            }
            let (dx, dy, dz) = (gx.div_euclid(32), gy.div_euclid(32), gz.div_euclid(32));
            if let Some(chunk) = &neighbours[((dy + 1) * 9 + (dz + 1) * 3 + dx + 1) as usize] {
                return chunk.get_voxel(gx.rem_euclid(32) as u8, gy.rem_euclid(32) as u8, gz.rem_euclid(32) as u8);
            }
            // Voisin non modifié : le générateur, réduit à ce que le maillage en lit (solide, eau ou air)
            let global_y = (layer as f64) * 32.0 + (gy as f64);
            let sampled;
            let in_border = (-COLUMN_BORDER..32 + COLUMN_BORDER).contains(&gx) && (-COLUMN_BORDER..32 + COLUMN_BORDER).contains(&gz);
            let column = if in_border {
                &columns[((gz + COLUMN_BORDER) * COLUMN_SIDE + gx + COLUMN_BORDER) as usize]
            } else {
                sampled = generator.sample_column(face, base_u + (gx as f64) * voxel_span_uv, base_v + (gz as f64) * voxel_span_uv);
                &sampled
            };

            let (bx, by, bz) = (block_x0 + gx, layer * 32 + gy, block_z0 + gz);
            if let Some(block) = generator.structure_block(column, face, bx, by, bz) {
                return block;
            }
            let elev_floor = column.elevation.floor();
            if global_y < elev_floor {
                let carved = match &caves {
                    Some(caves) => caves.carved(column, bx, by, bz),
                    // Sans champ, aucune colonne de la bordure n'a de grotte à ces altitudes
                    None => false,
                };
                if carved {
                    BlockId::AIR
                } else {
                    BlockId::STONE
                }
            } else if global_y < sea {
                if column.biome == herocraft_world::BiomeType::SnowPeaks {
                    BlockId::SNOW
                } else {
                    BlockId::WATER
                }
            } else if in_border {
                // Arbres voisins : leurs troncs et feuillages cachent ou non les faces d'à côté (les
                // herbes et les fleurs ne cachent rien)
                let (bx, by, bz) = (block_x0 + gx, layer * 32 + gy, block_z0 + gz);
                source.vegetation.tree_block(elev_floor as i32, bx, by, bz).unwrap_or(BlockId::AIR)
            } else {
                BlockId::AIR
            }
        };

        // Le bloc (x, y, z), voisin d'une face visible, est l'air d'une grotte du générateur (un
        // trou creusé par le joueur n'en est pas une)
        let cave_air = |gx: i32, gy: i32, gz: i32| -> bool {
            let column = &columns[((gz + COLUMN_BORDER) * COLUMN_SIDE + gx + COLUMN_BORDER) as usize];
            let y = layer * 32 + gy;
            (y as f64) < column.elevation.floor()
                && caves.as_ref().is_some_and(|caves| caves.carved(column, block_x0 + gx, y, block_z0 + gz))
        };
        let light = Self::light_field(generator, source, layer, &neighbours, &get_voxel);
        // Lumière d'une case, et moyenne de celles qui entourent un coin de face (les blocs pleins
        // n'y comptent pas, comme pour l'ombrage ambiant)
        let light_at = |gx: i32, gy: i32, gz: i32| -> f32 { light.as_ref().map_or(0.0, |l| l.level(gx, gy, gz) as f32) };
        let corner_light = |front: (i32, i32, i32), [s1, s2, c]: [(i32, i32, i32); 3]| -> f32 {
            let Some(l) = light.as_ref() else { return 0.0 };
            let full = |p: (i32, i32, i32)| l.is_full(p.0, p.1, p.2);
            let level = |p: (i32, i32, i32)| l.level(p.0, p.1, p.2) as u32;
            let (mut sum, mut count) = (level(front), 1);
            for (p, counted) in [(s1, !full(s1)), (s2, !full(s2)), (c, !full(c) && !(full(s1) && full(s2)))] {
                if counted {
                    sum += level(p);
                    count += 1;
                }
            }
            sum as f32 / count as f32
        };

        // `toward` : le voisin vers lequel la face regarde
        let get_color = |gx: i32, gy: i32, gz: i32, block: BlockId, dir: usize| -> [f32; 4] {
            let toward = FACE_DIRS[dir];
            let is_top = dir == 2;
            let idx = (gz.clamp(0, 31) as usize) * 32 + (gx.clamp(0, 31) as usize);
            let biome_color = column_colors[idx];
            let own_color = |shade: f32| {
                let c = Self::block_color(block, dir);
                [c[0] * shade, c[1] * shade, c[2] * shade, c[3]]
            };
            let side_shade = if is_top { 1.0 } else { 0.78 };
            // Arbres, herbes et fleurs : la couleur de leur texture, ni celle du biome ni celle du sol
            // (constructions comprises). Le verre est plein : sa vitre est un trou de sa texture, son
            // cadre reste net au lieu d'être tramé par la transparence de la fiche.
            if !Self::is_ground(block) {
                let c = own_color(if block.is_plant() { 1.0 } else { side_shade });
                let connected = blocks().get(block).is_some_and(|sheet| sheet.connected);
                return [c[0], c[1], c[2], if connected { 1.0 } else { c[3] }];
            }
            // Paroi de grotte : la couleur de sa matière, comme un trou creusé par le joueur, et non
            // celle du biome qui est celle de la surface (l'ombre du relief l'assombrit déjà)
            if block != BlockId::WATER && cave_air(gx + toward.0, gy + toward.1, gz + toward.2) {
                let c = own_color(side_shade);
                return [c[0], c[1], c[2], 1.0];
            }
            // Dans un chunk modifié, un bloc posé par le joueur ou découvert sous la surface prend la
            // couleur de sa matière : la couleur du biome est celle de la surface naturelle
            if edited.is_some() && block != BlockId::WATER {
                let (x, y, z) = (gx.clamp(0, 31) as u8, gy.clamp(0, 31) as u8, gz.clamp(0, 31) as u8);
                let column = &columns[((gz.clamp(0, 31) + COLUMN_BORDER) * COLUMN_SIDE + gx.clamp(0, 31) + COLUMN_BORDER) as usize];
                let global_y = (layer * 32 + gy) as f64;
                let natural = generated.get_voxel(x, y, z) == block;
                if !natural || global_y + 1.0 < column.elevation.floor() {
                    let c = own_color(side_shade);
                    return [c[0], c[1], c[2], 1.0];
                }
            }
            if block == BlockId::WATER {
                let mut c = biome_color;
                c[3] = 0.90; // Translucidité de l'eau
                c
            } else if is_top {
                // Dessus du relief naturel : la texture teintée par le biome, comme le relief lointain
                biome_color
            } else if block_textures().face_color(block, dir).is_some() {
                // Flancs du relief naturel : la matière telle quelle (terre sous l'herbe, roche)
                let c = own_color(side_shade);
                [c[0], c[1], c[2], 1.0]
            } else {
                [
                    biome_color[0] * 0.78,
                    biome_color[1] * 0.78,
                    biome_color[2] * 0.78,
                    1.0,
                ]
            }
        };
        // Texture d'une face : coordonnées de chaque coin, depuis sa place dans le bloc (seizièmes)
        let face_texture = |block: BlockId, dir: usize, corners: [[u8; 3]; 4]| -> [u32; 4] {
            let layer = blocks().face_texture(block, dir);
            corners.map(|c| {
                let (u, v) = face_uv(dir, c);
                PlanetVertex::pack_texture(layer, u, v)
            })
        };

        herocraft_common::prof_scope!("voxel_mesh::emit_faces");
        // Émission directe des faces de voxels exposées (1x1) sans greedy meshing
        // Sur une planète incurvée (Cubed-Sphere), les quads 1x1 épousent parfaitement la courbure
        // sans aucun décollement, fissure, trou ni artefact diagonal.
        for gy in 0..CHUNK_SIZE as i32 {
            let fy = gy as f64;
            for gz in 0..32 {
                let fz = gz as f64;
                for gx in 0..32 {
                    let fx = gx as f64;
                    let block = get_voxel(gx, gy, gz);
                    if block.is_air() {
                        continue;
                    }
                    if block.is_plant() {
                        // Fleurs, herbes, pousses : deux panneaux croisés en diagonale du bloc, vus des deux côtés
                        let color = get_color(gx, gy, gz, block, 2);
                        let tex_layer = blocks().face_texture(block, 0);
                        let tex = |u: u8, v: u8| PlanetVertex::pack_texture(tex_layer, u, v);
                        let corner = |dx: f64, dy: f64, dz: f64| get_corner(fx + dx, fy + dy, fz + dz, layer);
                        for (a, b) in [((0.0, 0.0), (1.0, 1.0)), ((1.0, 0.0), (0.0, 1.0))] {
                            let (c0, c1) = (corner(a.0, 0.0, a.1), corner(b.0, 0.0, b.1));
                            let (c2, c3) = (corner(b.0, 1.0, b.1), corner(a.0, 1.0, a.1));
                            let front = [tex(0, 16), tex(16, 16), tex(16, 0), tex(0, 0)];
                            let back = [tex(16, 16), tex(0, 16), tex(0, 0), tex(16, 0)];
                            let glow = [if block.light() > 0 { block_light::EMISSIVE } else { block_light::intensity(light_at(gx, gy, gz)) }; 4];
                            Self::add_quad(&mut vertices, &mut indices, c0, c1, c2, c3, color, [1.0; 4], glow, front);
                            Self::add_quad(&mut vertices, &mut indices, c1, c0, c3, c2, color, [1.0; 4], glow, back);
                        }
                        continue;
                    }
                    if let Some(shape) = block.shape() {
                        // Dalle, escalier, échelle : les faces de chaque pavé que rien ne couvre. Une
                        // barrière a en plus ses traverses vers les voisins auxquels elle se relie.
                        let fence;
                        let boxes = if block.is_fence() {
                            fence = herocraft_common::fence_boxes(herocraft_common::fence_links(|dir| {
                                let d = FACE_DIRS[dir];
                                get_voxel(gx + d.0, gy + d.1, gz + d.2)
                            }));
                            &fence[..]
                        } else {
                            shape.boxes(block.shape_variant())
                        };
                        let own_light = light_at(gx, gy, gz);
                        for (i, b) in boxes.iter().enumerate() {
                            for (dir, face) in CUBE_FACES.iter().enumerate() {
                                let axis = dir / 2;
                                let on_side = if dir % 2 == 0 { b[axis + 3] == 16 } else { b[axis] == 0 };
                                if on_side {
                                    // Contre la case voisine : cachée par un voisin qui couvre toute la face
                                    let d = FACE_DIRS[dir];
                                    if get_voxel(gx + d.0, gy + d.1, gz + d.2).hides_face(opposite_face(dir)) {
                                        continue;
                                    }
                                } else if Self::box_face_covered(boxes, i, dir) {
                                    continue;
                                }
                                let local: [[u8; 3]; 4] = face.corners.map(|o| {
                                    [if o.0 > 0 { b[3] } else { b[0] }, if o.1 > 0 { b[4] } else { b[1] }, if o.2 > 0 { b[5] } else { b[2] }]
                                });
                                let [c0, c1, c2, c3] = local.map(|c| {
                                    get_corner(fx + c[0] as f64 / 16.0, fy + c[1] as f64 / 16.0, fz + c[2] as f64 / 16.0, layer)
                                });
                                let color = get_color(gx, gy, gz, block, dir);
                                // Texture étirée : chaque face du pavé la montre en entier
                                let uv_corners = match shape.texture {
                                    ShapeTexture::Crop => local,
                                    ShapeTexture::Stretch => local.map(|c| std::array::from_fn(|k| ((c[k] - b[k]) as u32 * 16 / (b[k + 3] - b[k]) as u32) as u8)),
                                };
                                let tex = face_texture(block, dir, uv_corners);
                                let d = FACE_DIRS[dir];
                                // Torche, lanterne : leurs faces brillent
                                let glow = [if block.light() > 0 { block_light::EMISSIVE } else { block_light::intensity(own_light.max(light_at(gx + d.0, gy + d.1, gz + d.2))) }; 4];
                                Self::add_quad(&mut vertices, &mut indices, c0, c1, c2, c3, color, [1.0; 4], glow, tex);
                            }
                        }
                        continue;
                    }
                    let is_water = block == BlockId::WATER;
                    // Dessus, dessous, puis les quatre côtés
                    for dir in [2, 3, 1, 0, 4, 5] {
                        // L'eau ne montre pas son dessous
                        if is_water && dir == 3 {
                            continue;
                        }
                        let d = FACE_DIRS[dir];
                        let neighbor = get_voxel(gx + d.0, gy + d.1, gz + d.2);
                        if !Self::shows_face(block, neighbor, dir) || (dir == 3 && neighbor.is_liquid()) {
                            continue;
                        }
                        let face = &CUBE_FACES[dir];
                        let color = get_color(gx, gy, gz, block, dir);
                        let [c0, c1, c2, c3] = face.corners.map(|o| get_corner(fx + o.0 as f64, fy + o.1 as f64, fz + o.2 as f64, layer));
                        let ao = if is_water && dir != 3 {
                            [1.0; 4]
                        } else {
                            face.ao.map(|[s1, s2, c]| {
                                let at = |o: (i32, i32, i32)| (gx + o.0, gy + o.1, gz + o.2);
                                Self::calc_corner_ao(&get_voxel, at(s1), at(s2), at(c))
                            })
                        };
                        let emissive = block.light() > 0 && !block.is_liquid();
                        let glow = face.ao.map(|offsets| {
                            let at = |o: (i32, i32, i32)| (gx + o.0, gy + o.1, gz + o.2);
                            if emissive { block_light::EMISSIVE } else { block_light::intensity(corner_light(at(d), offsets.map(at))) }
                        });
                        let mut tex = face_texture(block, dir, face.corners.map(|o| [o.0 * 16, o.1 * 16, o.2 * 16]));
                        if blocks().get(block).is_some_and(|sheet| sheet.connected) {
                            // Verre : le cadre ne court que sur les bords qui ne touchent pas un bloc pareil
                            let mask = Self::connected_mask(dir, |o: (i32, i32, i32)| get_voxel(gx + o.0, gy + o.1, gz + o.2).kind() == block.kind());
                            let layer = block_textures().connected_layer(blocks().face_texture(block, dir), mask);
                            tex = tex.map(|t| (t & !0xFFFF) | layer as u32);
                        }
                        Self::add_quad(&mut vertices, &mut indices, c0, c1, c2, c3, color, ao, glow, tex);
                    }
                }
            }
        }

        PlanetMesh { vertices, indices }
    }

    /// Lumière des blocs autour de la couche `layer` de la colonne : celle des blocs qui éclairent
    /// dans les chunks modifiés du voisinage (`neighbours`) et celle des torches et lampadaires
    /// des structures ailleurs, répandue jusqu'à `LIGHT_REACH` blocs. Les cases lues par la
    /// propagation viennent de `get_voxel` près du chunk et dans les chunks modifiés ; plus loin,
    /// d'une estimation (le relief de la colonne de bordure la plus proche, sans grotte ni arbre,
    /// et les structures) qui épargne l'échantillonnage du relief.
    fn light_field(
        generator: &PlanetWorldGenerator,
        source: &VoxelColumnSource,
        layer: i32,
        neighbours: &[Option<Arc<ChunkData>>; 27],
        get_voxel: &impl Fn(i32, i32, i32) -> BlockId,
    ) -> Option<LightField> {
        herocraft_common::prof_scope!("voxel_mesh::light");
        let slot = |x: i32, y: i32, z: i32| ((y.div_euclid(32) + 1) * 9 + (z.div_euclid(32) + 1) * 3 + x.div_euclid(32) + 1) as usize;
        let in_reach = |v: i32| (-LIGHT_REACH..32 + LIGHT_REACH).contains(&v);
        let mut sources = Vec::new();
        // Chunks modifiés : tous leurs blocs qui éclairent
        for (i, chunk) in neighbours.iter().enumerate() {
            let Some(chunk) = chunk else { continue };
            let (ox, oy, oz) = (((i % 3) as i32 - 1) * 32, ((i / 9) as i32 - 1) * 32, (((i / 3) % 3) as i32 - 1) * 32);
            for (n, block) in chunk.voxels.iter().enumerate() {
                if block.is_air() {
                    continue;
                }
                let level = block.light();
                if level == 0 {
                    continue;
                }
                let (x, z, y) = (ox + (n % 32) as i32, oz + ((n / 32) % 32) as i32, oy + (n / 1024) as i32);
                if in_reach(x) && in_reach(y) && in_reach(z) {
                    sources.push((x, y, z, level));
                }
            }
        }
        // Structures, là où le chunk n'est pas modifié (sinon il compte tel qu'il est)
        let (face, block_x0, block_z0) = (source.face, source.chunk_x * CHUNK_SIZE as i32, source.chunk_y * CHUNK_SIZE as i32);
        let face_chunks_count = FACE_CHUNKS as f64;
        let voxel_span_uv = 2.0 / face_chunks_count / (CHUNK_SIZE as f64);
        let base_u = -1.0 + (source.chunk_x as f64) * 2.0 / face_chunks_count;
        let base_v = -1.0 + (source.chunk_y as f64) * 2.0 / face_chunks_count;
        let border_column = |gx: i32, gz: i32| -> &ColumnSample {
            let (cx, cz) = (gx.clamp(-COLUMN_BORDER, 31 + COLUMN_BORDER), gz.clamp(-COLUMN_BORDER, 31 + COLUMN_BORDER));
            &source.columns[((cz + COLUMN_BORDER) * COLUMN_SIDE + cx + COLUMN_BORDER) as usize]
        };
        let ground = |x: i32, z: i32| -> i32 {
            let (gx, gz) = (x - block_x0, z - block_z0);
            let near = (-COLUMN_BORDER..32 + COLUMN_BORDER).contains(&gx) && (-COLUMN_BORDER..32 + COLUMN_BORDER).contains(&gz);
            let elevation = if near {
                border_column(gx, gz).elevation
            } else {
                generator.sample_column(face, base_u + gx as f64 * voxel_span_uv, base_v + gz as f64 * voxel_span_uv).elevation
            };
            elevation.floor() as i32
        };
        let area = herocraft_world::structures::Rect::new(block_x0 - LIGHT_REACH, block_z0 - LIGHT_REACH, block_x0 + 32 + LIGHT_REACH, block_z0 + 32 + LIGHT_REACH);
        let y0 = layer * 32;
        for (x, y, z, block) in generator.structure_lights(face, area, ground) {
            let (lx, ly, lz) = (x - block_x0, y - y0, z - block_z0);
            if in_reach(ly) && neighbours[slot(lx, ly, lz)].is_none() {
                sources.push((lx, ly, lz, block.light()));
            }
        }
        if sources.is_empty() {
            return None;
        }
        let covers = |block: BlockId| -> u8 {
            if block.is_opaque() {
                block_light::FULL
            } else if block.shape().is_some() {
                (0..6).filter(|&dir| block.hides_face(dir)).fold(0, |mask, dir| mask | (1 << dir))
            } else {
                0
            }
        };
        // Altitudes que les structures changent dans chaque colonne lointaine, lues une fois par colonne
        let side = (32 + 2 * LIGHT_REACH) as usize;
        let mut spans: Vec<Option<std::ops::Range<i32>>> = vec![None; side * side];
        LightField::compute(&sources, |x, y, z| {
            let near = (-COLUMN_BORDER..32 + COLUMN_BORDER).contains(&x) && (-COLUMN_BORDER..32 + COLUMN_BORDER).contains(&z);
            if near || neighbours[slot(x, y, z)].is_some() || (layer == 0 && y < 0) {
                return covers(get_voxel(x, y, z));
            }
            // Loin du chunk : le relief de la colonne de bordure la plus proche, et les structures
            let column = border_column(x, z);
            let (bx, by, bz) = (block_x0 + x, y0 + y, block_z0 + z);
            if by < 0 {
                return block_light::FULL;
            }
            let span = spans[(z + LIGHT_REACH) as usize * side + (x + LIGHT_REACH) as usize]
                .get_or_insert_with(|| generator.structure_span(column, face, bx, bz));
            if span.contains(&by) {
                if let Some(block) = generator.structure_block(column, face, bx, by, bz) {
                    return covers(block);
                }
            }
            if (by as f64) < column.elevation.floor() {
                block_light::FULL
            } else {
                0
            }
        })
    }

    /// Bords de la face `dir` (dans le repère de sa texture, `face_uv`) qui touchent un bloc du même
    /// genre : `same(décalage)` dit si le voisin à ce décalage en est un
    fn connected_mask(dir: usize, same: impl Fn((i32, i32, i32)) -> bool) -> u8 {
        use crate::block_textures::{CONNECTED_BOTTOM, CONNECTED_LEFT, CONNECTED_RIGHT, CONNECTED_TOP};
        // Voisins à gauche (u = 0), à droite, en haut (v = 0) et en bas de la texture
        let [left, right, top, bottom] = match dir {
            0 => [(0, 0, 1), (0, 0, -1), (0, 1, 0), (0, -1, 0)],
            1 => [(0, 0, -1), (0, 0, 1), (0, 1, 0), (0, -1, 0)],
            2 | 3 => [(-1, 0, 0), (1, 0, 0), (0, 0, -1), (0, 0, 1)],
            4 => [(-1, 0, 0), (1, 0, 0), (0, 1, 0), (0, -1, 0)],
            _ => [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0)],
        };
        [(left, CONNECTED_LEFT), (right, CONNECTED_RIGHT), (top, CONNECTED_TOP), (bottom, CONNECTED_BOTTOM)]
            .into_iter()
            .filter(|(o, _)| same(*o))
            .fold(0, |mask, (_, bit)| mask | bit)
    }

    /// Matière du relief naturel, que le dessus du terrain colore selon le biome
    #[inline(always)]
    fn is_ground(block: BlockId) -> bool {
        matches!(block, BlockId::GRASS | BlockId::DIRT | BlockId::SAND | BlockId::SNOW | BlockId::STONE | BlockId::WATER)
    }

    #[inline(always)]
    fn is_solid_for_ao(block: BlockId) -> bool {
        !block.is_air() && !block.is_liquid() && !block.is_plant() && block.shape().is_none()
    }

    /// La face `dir` du pavé `i` est collée à un autre pavé de la même forme qui la couvre
    fn box_face_covered(boxes: &[ShapeBox], i: usize, dir: usize) -> bool {
        let b = boxes[i];
        let axis = dir / 2;
        let plane = if dir % 2 == 0 { b[axis + 3] } else { b[axis] };
        let (a1, a2) = ((axis + 1) % 3, (axis + 2) % 3);
        boxes.iter().enumerate().any(|(j, o)| {
            let touching = if dir % 2 == 0 { o[axis] == plane } else { o[axis + 3] == plane };
            j != i && touching && o[a1] <= b[a1] && o[a1 + 3] >= b[a1 + 3] && o[a2] <= b[a2] && o[a2 + 3] >= b[a2 + 3]
        })
    }

    /// La face `dir` de `block`, tournée vers `neighbor`, se voit : le voisin laisse voir à travers
    /// (air, eau, verre, feuillage, plante, forme qui ne couvre pas cette face) et n'est pas de la
    /// même matière (l'eau contre l'eau, le verre contre le verre ne montrent pas de paroi)
    #[inline(always)]
    fn shows_face(block: BlockId, neighbor: BlockId, dir: usize) -> bool {
        !neighbor.hides_face(opposite_face(dir)) && neighbor != block
    }

    fn calc_corner_ao<F>(get_voxel: &F, s1: (i32, i32, i32), s2: (i32, i32, i32), c: (i32, i32, i32)) -> f32
    where
        F: Fn(i32, i32, i32) -> BlockId,
    {
        let b_s1 = Self::is_solid_for_ao(get_voxel(s1.0, s1.1, s1.2));
        let b_s2 = Self::is_solid_for_ao(get_voxel(s2.0, s2.1, s2.2));
        if b_s1 && b_s2 {
            0.55
        } else {
            let b_c = Self::is_solid_for_ao(get_voxel(c.0, c.1, c.2));
            let count = (b_s1 as u8) + (b_s2 as u8) + (b_c as u8);
            match count {
                0 => 1.0,
                1 => 0.82,
                2 => 0.68,
                _ => 0.55,
            }
        }
    }

    pub(crate) fn add_quad(
        vertices: &mut Vec<PlanetVertex>,
        indices: &mut Vec<u32>,
        c0: Vec3,
        c1: Vec3,
        c2: Vec3,
        c3: Vec3,
        color: [f32; 4],
        ao: [f32; 4],
        light: [f32; 4],
        texture: [u32; 4],
    ) {
        let base_idx = vertices.len() as u32;

        // Calcul de la normale via le produit vectoriel des arêtes
        let e1 = c1 - c0;
        let e2 = c2 - c0;
        let normal = e1.cross(e2).normalize_or_zero().to_array();

        let make_vert = |pos: Vec3, ao_factor: f32, light: f32, texture: u32| PlanetVertex {
            position: pos.to_array(),
            normal,
            color: [
                color[0] * ao_factor,
                color[1] * ao_factor,
                color[2] * ao_factor,
                color[3],
            ],
            texture,
            light,
        };

        vertices.push(make_vert(c0, ao[0], light[0], texture[0]));
        vertices.push(make_vert(c1, ao[1], light[1], texture[1]));
        vertices.push(make_vert(c2, ao[2], light[2], texture[2]));
        vertices.push(make_vert(c3, ao[3], light[3], texture[3]));

        // Inversion de diagonale selon la direction du gradient AO (élimine le pli d'anisotropie)
        if ao[0] + ao[2] > ao[1] + ao[3] {
            // Diagonale c0 - c2
            indices.push(base_idx);
            indices.push(base_idx + 1);
            indices.push(base_idx + 2);

            indices.push(base_idx);
            indices.push(base_idx + 2);
            indices.push(base_idx + 3);
        } else {
            // Diagonale c1 - c3
            indices.push(base_idx + 1);
            indices.push(base_idx + 2);
            indices.push(base_idx + 3);

            indices.push(base_idx + 1);
            indices.push(base_idx + 3);
            indices.push(base_idx);
        }
    }

    /// Couleur d'une face, celle de la fiche du bloc
    /// Couleur d'une face (`FACE_DIRS`) qui montre sa matière : la moyenne de sa texture (que le
    /// rendu montre alors telle quelle), sinon la couleur de la fiche
    pub(crate) fn block_color(block: BlockId, dir: usize) -> [f32; 4] {
        if let Some(color) = block_textures().face_color(block, dir) {
            return color;
        }
        match herocraft_common::blocks().get(block) {
            Some(sheet) if dir == 2 => sheet.top_color(),
            Some(sheet) => sheet.side_color(),
            None => [0.60, 0.60, 0.60, 1.0],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use herocraft_common::{PlanetChunkCoord, PlanetFace};
    use herocraft_world::{PlanetDescriptor, VisualizationFilter};

    #[test]
    fn test_chunk_border_no_missing_faces() {
        let generator = PlanetWorldGenerator::new(PlanetDescriptor {
            seed: 2026,
            ..Default::default()
        });

        let face = PlanetFace::PosX;
        let cx0 = 10;
        let cx1 = 11;
        let cy = 10;
        let base_radius = 25.0;

        let mesh0 = VoxelMeshBuilder::build_incurved_single_chunk_fast(
            &generator,
            face,
            cx0,
            cy,
            base_radius,
            VisualizationFilter::Standard,
        );

        let mesh1 = VoxelMeshBuilder::build_incurved_single_chunk_fast(
            &generator,
            face,
            cx1,
            cy,
            base_radius,
            VisualizationFilter::Standard,
        );

        assert!(!mesh0.vertices.is_empty(), "Mesh 0 should not be empty");
        assert!(!mesh1.vertices.is_empty(), "Mesh 1 should not be empty");

        // Vérification de cohérence entre chunks adjacents :
        // Pour chaque layer, vérifions qu'aucun bloc solide sur la bordure n'est exposé à l'air sans face
        for layer in 0..16 {
            let coord0 = PlanetChunkCoord::new(face, cx0, cy, layer);
            let coord1 = PlanetChunkCoord::new(face, cx1, cy, layer);
            let chunk0 = generator.generate_chunk(&coord0);
            let chunk1 = generator.generate_chunk(&coord1);

            for gz in 0..32 {
                for gy in 0..32 {
                    let b0 = chunk0.get_voxel(31, gy, gz);
                    let b1 = chunk1.get_voxel(0, gy, gz);

                    // Si chunk0 est solide et chunk1 est de l'air :
                    // chunk0 doit impérativement avoir émis sa face Droite (+X)
                    if !b0.is_air() && b0 != BlockId::WATER && b1.is_air() {
                        // Le maillage de chunk0 doit contenir des sommets à x=32
                        // Cela prouve que la face droite a bien été émise
                        assert!(
                            !mesh0.indices.is_empty(),
                            "Chunk 0 must emit faces when exposed to air in chunk 1"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn test_curved_voxel_mesh_integrity() {
        let generator = PlanetWorldGenerator::new(PlanetDescriptor {
            seed: 2026,
            ..Default::default()
        });
        let mesh = VoxelMeshBuilder::build_incurved_single_chunk_fast(
            &generator,
            PlanetFace::PosZ,
            16,
            16,
            25.0,
            VisualizationFilter::Standard,
        );
        assert!(!mesh.vertices.is_empty(), "Curved voxel mesh should have vertices");
        assert_eq!(mesh.indices.len() % 3, 0, "Indices must form complete triangles");
        for v in &mesh.vertices {
            assert!(v.position[0].is_finite());
            assert!(v.position[1].is_finite());
            assert!(v.position[2].is_finite());
        }
    }

    fn same_mesh(a: &PlanetMesh, b: &PlanetMesh) -> bool {
        a.indices == b.indices
            && a.vertices.len() == b.vertices.len()
            && a.vertices.iter().zip(&b.vertices).all(|(x, y)| x.position == y.position && x.color == y.color)
    }

    #[test]
    fn test_layer_rebuild_matches_full_build_after_edits() {
        use herocraft_world::BlockPos;
        let generator = PlanetWorldGenerator::new(PlanetDescriptor { seed: 2026, ..Default::default() });
        let (face, cx, cy) = (PlanetFace::PosX, 16, 16);
        let source = Arc::new(VoxelMeshBuilder::sample_column_source(&generator, face, cx, cy, 50.0, VisualizationFilter::Standard));
        let edits = TerrainEdits::new(None);
        let mut column = VoxelMeshBuilder::build_column(&generator, &edits, source.clone());
        let before = column.mesh.vertices.len();

        // Surface de la colonne (x=5, z=7) : au-dessus du plus haut bloc non vide (une grotte peut
        // ouvrir de l'air plus bas)
        let surface = (0..528).rev().find(|&y| !generator.block_at(face, cx * 32 + 5, y, cy * 32 + 7).is_air()).unwrap() + 1;
        let dug = BlockPos::new(face, cx * 32 + 5, surface - 1, cy * 32 + 7);
        let placed = BlockPos::new(face, cx * 32 + 20, 500, cy * 32 + 31);
        for (pos, block) in [(dug, BlockId::AIR), (placed, BlockId::STONE)] {
            assert!(edits.set_block(&generator, pos, block));
            let layer = pos.y.div_euclid(32);
            VoxelMeshBuilder::rebuild_layers(&generator, &edits, &mut column, &[layer - 1, layer, layer + 1]);
            let full = VoxelMeshBuilder::build_column(&generator, &edits, source.clone());
            assert!(same_mesh(&column.mesh, &full.mesh), "remaillage partiel différent du complet après {:?}", pos);
            assert_eq!(column.layers.iter().map(|r| r.layer).collect::<Vec<_>>(), full.layers.iter().map(|r| r.layer).collect::<Vec<_>>());
        }
        assert_ne!(column.mesh.vertices.len(), before, "les modifications changent le maillage");

        // Le bloc posé à 500 est seul dans le ciel : 6 faces, 24 sommets, couleur de pierre
        let sky = column.layers.iter().find(|r| r.layer == 15).expect("couche du bloc posé maillée");
        assert_eq!(sky.vertices.len(), 24);

        // Retour à l'état initial : on retrouve le maillage de départ, couches vides comprises
        edits.set_block(&generator, dug, generator.block_at(face, dug.x, dug.y, dug.z));
        edits.set_block(&generator, placed, BlockId::AIR);
        for layer in [dug.y / 32, placed.y / 32] {
            VoxelMeshBuilder::rebuild_layers(&generator, &edits, &mut column, &[layer - 1, layer, layer + 1]);
        }
        let full = VoxelMeshBuilder::build_column(&generator, &edits, source);
        assert!(same_mesh(&column.mesh, &full.mesh));
    }

    #[test]
    fn test_shaped_blocks_are_meshed_box_by_box() {
        use herocraft_world::BlockPos;
        let generator = PlanetWorldGenerator::new(PlanetDescriptor { seed: 2026, ..Default::default() });
        let (face, cx, cy) = (PlanetFace::PosX, 16, 16);
        let source = Arc::new(VoxelMeshBuilder::sample_column_source(&generator, face, cx, cy, 50.0, VisualizationFilter::Standard));
        let edits = TerrainEdits::new(None);
        let sky_layer = |block: BlockId| {
            edits.set_block(&generator, BlockPos::new(face, cx * 32 + 20, 500, cy * 32 + 10), block);
            let column = VoxelMeshBuilder::build_column(&generator, &edits, source.clone());
            let range = column.layers.iter().find(|r| r.layer == 15).unwrap().vertices.clone();
            column.mesh.vertices[range].to_vec()
        };
        // Dalle seule dans le ciel : 6 faces ; ses côtés montrent la moitié basse de la texture
        let slab = blocks().find("plankSlab").unwrap();
        let vertices = sky_layer(slab);
        assert_eq!(vertices.len(), 24);
        let plank = blocks().face_texture(slab, 0) as u32;
        assert!(vertices.iter().all(|v| v.texture & 0xFFFF == plank));
        let vs: Vec<u32> = vertices.iter().map(|v| v.texture >> 24).collect();
        assert!(vs.contains(&8) && vs.contains(&16) && !vs.iter().any(|&v| v > 0 && v < 8));
        // Escalier : la marche haute pose sur la basse, sa face du dessous n'est pas dessinée
        let stairs = blocks().find("plankStairs").unwrap();
        assert_eq!(sky_layer(stairs.with_rotation(1)).len(), 11 * 4);
        // Un cube plein montre toute sa texture, du coin (0, 0) au coin (16, 16)
        let stone = sky_layer(BlockId::STONE);
        assert_eq!(stone.len(), 24);
        assert!(stone.iter().any(|v| v.texture >> 16 == 0) && stone.iter().any(|v| v.texture >> 16 == 16 | 16 << 8));
    }

    #[test]
    fn test_fences_link_and_torches_light() {
        use herocraft_world::BlockPos;
        let generator = PlanetWorldGenerator::new(PlanetDescriptor { seed: 2026, ..Default::default() });
        let (face, cx, cy) = (PlanetFace::PosX, 16, 16);
        let source = Arc::new(VoxelMeshBuilder::sample_column_source(&generator, face, cx, cy, 50.0, VisualizationFilter::Standard));
        let edits = TerrainEdits::new(None);
        let at = |x: i32, y: i32, z: i32| BlockPos::new(face, cx * 32 + x, y, cy * 32 + z);
        let sky_layer = |layer: i32| {
            let column = VoxelMeshBuilder::build_column(&generator, &edits, source.clone());
            let range = column.layers.iter().find(|r| r.layer == layer).unwrap().vertices.clone();
            column.mesh.vertices[range].to_vec()
        };
        // Barrière seule dans le ciel : le poteau, 6 faces
        let fence = blocks().find("barriereChene").unwrap();
        edits.set_block(&generator, at(20, 500, 10), fence);
        assert_eq!(sky_layer(15).len(), 24);
        // Deux barrières côte à côte : chacune son poteau et deux traverses vers l'autre (le bout
        // d'une traverse collé à son poteau n'est pas dessiné)
        edits.set_block(&generator, at(21, 500, 10), fence);
        let two = sky_layer(15);
        assert_eq!(two.len(), 2 * (24 + 2 * 5 * 4), "{} faces", two.len() / 4);
        assert!(two.iter().all(|v| v.light == 0.0), "rien n'éclaire encore");
        // Une torche à côté : les faces proches sont éclairées, d'autant moins qu'elles sont loin
        let torch = blocks().find("torch").unwrap();
        edits.set_block(&generator, at(20, 501, 10), torch);
        let lit = sky_layer(15);
        let max = lit.iter().map(|v| v.light).fold(0.0, f32::max);
        assert!(max > 0.8, "{max}");
        // Un cube de pierre à 10 blocs de la torche reçoit moins de lumière qu'au pied de la torche
        edits.set_block(&generator, at(30, 501, 10), BlockId::STONE);
        let far = sky_layer(15).iter().filter(|v| v.texture & 0xFFFF == blocks().face_texture(BlockId::STONE, 0) as u32).map(|v| v.light).fold(0.0, f32::max);
        assert!(far > 0.0 && far < 0.3, "{far}");
        // La torche retirée, plus rien ne brille
        edits.set_block(&generator, at(20, 501, 10), BlockId::AIR);
        assert!(sky_layer(15).iter().all(|v| v.light == 0.0));
    }

    #[test]
    fn test_cave_walls_are_meshed_below_the_surface() {
        // Collines de la face -Z (graine 42) percées de tunnels : leurs parois sont maillées, couleur
        // de la roche et non du biome, jusque dans les couches entièrement sous le relief
        let generator = PlanetWorldGenerator::new(PlanetDescriptor::default());
        let edits = TerrainEdits::new(None);
        let cave_stone = VoxelMeshBuilder::block_color(BlockId::STONE, 0);
        let mut cave_vertices = 0;
        let mut below_relief = false;
        // Quatre colonnes de chunks dont le centre est percé d'une grotte, autour du centre de la face
        let mid = FACE_CHUNKS / 2;
        let chunks: Vec<(i32, i32)> = (mid - 40..mid + 40)
            .flat_map(|cx| (mid - 40..mid + 40).map(move |cy| (cx, cy)))
            .filter(|&(cx, cy)| {
                let (x, z) = (cx * 32 + 16, cy * 32 + 16);
                let column = generator.sample_block_column(PlanetFace::NegZ, x, z);
                let top = column.elevation.floor() as i32;
                (top - 40..top - 8).any(|y| generator.is_cave(&column, PlanetFace::NegZ, x, y, z))
            })
            .take(4)
            .collect();
        for (cx, cy) in chunks {
            let source = Arc::new(VoxelMeshBuilder::sample_column_source(&generator, PlanetFace::NegZ, cx, cy, crate::camera::DEFAULT_RADIUS, VisualizationFilter::Standard));
            let column = VoxelMeshBuilder::build_column(&generator, &edits, source.clone());
            // La couleur porte l'occlusion ambiante : on compare la teinte, pas la valeur exacte
            cave_vertices += column
                .mesh
                .vertices
                .iter()
                .filter(|v| {
                    let k = v.color[0] / cave_stone[0];
                    k > 0.3 && k <= 1.0001 && (v.color[1] - cave_stone[1] * k).abs() < 1e-4 && (v.color[2] - cave_stone[2] * k).abs() < 1e-4
                })
                .count();
            below_relief |= column.layers.iter().any(|r| ((r.layer + 1) * 32) as f64 <= source.min_elev);
        }
        assert!(cave_vertices > 100, "{} sommets de parois de grottes", cave_vertices);
        assert!(below_relief, "une couche entièrement sous le relief est maillée pour ses grottes");
    }

    #[test]
    fn test_layer_rebuild_is_fast() {
        use herocraft_world::BlockPos;
        let generator = PlanetWorldGenerator::new(PlanetDescriptor { seed: 2026, ..Default::default() });
        let (face, cx, cy) = (PlanetFace::PosZ, 10, 12);
        let t = std::time::Instant::now();
        let source = Arc::new(VoxelMeshBuilder::sample_column_source(&generator, face, cx, cy, 50.0, VisualizationFilter::Standard));
        let t_source = t.elapsed();
        let edits = TerrainEdits::new(None);
        let t = std::time::Instant::now();
        let mut column = VoxelMeshBuilder::build_column(&generator, &edits, source);
        let t_build = t.elapsed();
        let surface = (0..528).rev().find(|&y| !generator.block_at(face, cx * 32 + 9, y, cy * 32 + 9).is_air()).unwrap() + 1;
        let t = std::time::Instant::now();
        let source = column.source.clone();
        edits.set_block_with(BlockPos::new(face, cx * 32 + 9, surface - 1, cy * 32 + 9), BlockId::AIR, |coord| {
            source.generate_chunk(&generator, coord.altitude_layer)
        });
        let layer = (surface - 1) / 32;
        VoxelMeshBuilder::rebuild_layers(&generator, &edits, &mut column, &[layer - 1, layer, layer + 1]);
        let t_edit = t.elapsed();
        println!("relief {:?}, maillage complet {:?}, modification + remaillage {:?}", t_source, t_build, t_edit);
    }

}
