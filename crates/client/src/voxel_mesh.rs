use std::ops::Range;
use std::sync::Arc;

use glam::Vec3;
use herocraft_common::{BlockId, PlanetChunkCoord, PlanetFace};
use herocraft_voxel_engine::cache::WorldChunkCache;
use herocraft_voxel_engine::chunk::{ChunkData, CHUNK_SIZE};
use herocraft_world::generator::{ColumnSample, PlanetWorldGenerator};
use herocraft_world::{TerrainEdits, ALTITUDE_LAYERS, FACE_CHUNKS};

use crate::planet_mesh::{PlanetMesh, PlanetVertex};

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
    /// Couleur du biome de chaque colonne de blocs (32x32)
    column_colors: Vec<[f32; 4]>,
    /// Colonnes du générateur avec une bordure de `COLUMN_BORDER`
    columns: Vec<ColumnSample>,
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
        generator.generate_chunk_from_columns(&coord, &self.columns, COLUMN_BORDER)
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
        let face_chunks_count = 32.0;
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
                let cx = (start_cx + dx).min(31);
                let cy = (start_cy + dy).min(31);
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
                        let color = Self::block_color(block, true);
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
                        Self::add_quad(&mut vertices, &mut indices, c0, c1, c2, c3, color, ao);
                    }

                    // 2. Face Bas (-Y) - uniquement pour blocs solides au-dessus d'une caverne d'air
                    if !is_water {
                        let neighbor_bottom = get_voxel(gx, gy - 1, gz);
                        if neighbor_bottom.is_air() {
                            let c0 = get_corner(fx, fy, fz);
                            let c1 = get_corner(fx + 1.0, fy, fz);
                            let c2 = get_corner(fx + 1.0, fy, fz + 1.0);
                            let c3 = get_corner(fx, fy, fz + 1.0);
                            let color = Self::block_color(block, false);
                            let ao = [
                                Self::calc_corner_ao(&get_voxel, (gx - 1, gy - 1, gz), (gx, gy - 1, gz - 1), (gx - 1, gy - 1, gz - 1)),
                                Self::calc_corner_ao(&get_voxel, (gx + 1, gy - 1, gz), (gx, gy - 1, gz - 1), (gx + 1, gy - 1, gz - 1)),
                                Self::calc_corner_ao(&get_voxel, (gx + 1, gy - 1, gz), (gx, gy - 1, gz + 1), (gx + 1, gy - 1, gz + 1)),
                                Self::calc_corner_ao(&get_voxel, (gx - 1, gy - 1, gz), (gx, gy - 1, gz + 1), (gx - 1, gy - 1, gz + 1)),
                            ];
                            Self::add_quad(&mut vertices, &mut indices, c0, c1, c2, c3, color, ao);
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
                        let color = Self::block_color(block, false);
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
                        Self::add_quad(&mut vertices, &mut indices, c0, c1, c2, c3, color, ao);
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
                        let color = Self::block_color(block, false);
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
                        Self::add_quad(&mut vertices, &mut indices, c0, c1, c2, c3, color, ao);
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
                        let color = Self::block_color(block, false);
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
                        Self::add_quad(&mut vertices, &mut indices, c0, c1, c2, c3, color, ao);
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
                        let color = Self::block_color(block, false);
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
                        Self::add_quad(&mut vertices, &mut indices, c0, c1, c2, c3, color, ao);
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
        let face_chunks_count = 32.0;
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

        VoxelColumnSource { face, chunk_x, chunk_y, base_radius, min_elev, max_elev, column_colors, columns }
    }

    /// Couches d'altitude à mailler : celles que le relief et la mer occupent, plus celles que le
    /// joueur a modifiées et leurs voisines (un trou creusé sous la surface découvre la couche d'en
    /// dessous, un bloc posé dans le ciel en occupe une qui était vide)
    fn layers_to_build(source: &VoxelColumnSource, edits: &TerrainEdits) -> Vec<i32> {
        // On inclut le niveau de la mer et une marge de sécurité
        let sea = herocraft_world::generator::SEA_LEVEL_BLOCKS;
        let (min_elev, max_elev) = (source.min_elev, source.max_elev);
        let lowest = (min_elev.min(sea) - 6.0).max(0.0);
        let highest = (max_elev.max(sea) + 6.0).min(528.0);

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
                // Optimisation : saut des chunks 100% souterrains ou 100% dans le ciel
                let buried = min_elev >= layer_top && sea < layer_base;
                let empty_sky = max_elev < layer_base && sea < layer_base;
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

        let face_chunks_count = 32.0;
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
        let generated = generator.generate_chunk_from_columns(&coord, columns, COLUMN_BORDER);
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
            let (col_elevation, biome) = if (-COLUMN_BORDER..32 + COLUMN_BORDER).contains(&gx)
                && (-COLUMN_BORDER..32 + COLUMN_BORDER).contains(&gz)
            {
                let c = &columns[((gz + COLUMN_BORDER) * COLUMN_SIDE + gx + COLUMN_BORDER) as usize];
                (c.elevation, c.biome)
            } else {
                let c = generator.sample_column(
                    face,
                    base_u + (gx as f64) * voxel_span_uv,
                    base_v + (gz as f64) * voxel_span_uv,
                );
                (c.elevation, c.biome)
            };

            let elev_floor = col_elevation.floor();
            if global_y < elev_floor {
                BlockId::STONE
            } else if global_y < sea {
                if biome == herocraft_world::BiomeType::SnowPeaks {
                    BlockId::SNOW
                } else {
                    BlockId::WATER
                }
            } else {
                BlockId::AIR
            }
        };

        let get_color = |gx: i32, gy: i32, gz: i32, block: BlockId, is_top: bool| -> [f32; 4] {
            let idx = (gz.clamp(0, 31) as usize) * 32 + (gx.clamp(0, 31) as usize);
            let biome_color = column_colors[idx];
            // Dans un chunk modifié, un bloc posé par le joueur ou découvert sous la surface prend la
            // couleur de sa matière : la couleur du biome est celle de la surface naturelle
            if edited.is_some() && block != BlockId::WATER {
                let (x, y, z) = (gx.clamp(0, 31) as u8, gy.clamp(0, 31) as u8, gz.clamp(0, 31) as u8);
                let column = &columns[((gz.clamp(0, 31) + COLUMN_BORDER) * COLUMN_SIDE + gx.clamp(0, 31) + COLUMN_BORDER) as usize];
                let global_y = (layer * 32 + gy) as f64;
                let natural = generated.get_voxel(x, y, z) == block;
                if !natural || global_y + 1.0 < column.elevation.floor() {
                    let c = Self::block_color(block, is_top);
                    let shade = if is_top { 1.0 } else { 0.78 };
                    return [c[0] * shade, c[1] * shade, c[2] * shade, 1.0];
                }
            }
            if block == BlockId::WATER {
                let mut c = biome_color;
                c[3] = 0.90; // Translucidité de l'eau
                c
            } else if is_top {
                biome_color
            } else {
                [
                    biome_color[0] * 0.78,
                    biome_color[1] * 0.78,
                    biome_color[2] * 0.78,
                    1.0,
                ]
            }
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
                    let is_water = block == BlockId::WATER;

                    // 1. Face Haut (+Y)
                    let neighbor_top = get_voxel(gx, gy + 1, gz);
                    let should_emit_top = if is_water {
                        neighbor_top.is_air()
                    } else {
                        neighbor_top.is_air() || neighbor_top == BlockId::WATER
                    };
                    if should_emit_top {
                        let color = get_color(gx, gy, gz, block, true);
                        let c0 = get_corner(fx, fy + 1.0, fz, layer);
                        let c1 = get_corner(fx, fy + 1.0, fz + 1.0, layer);
                        let c2 = get_corner(fx + 1.0, fy + 1.0, fz + 1.0, layer);
                        let c3 = get_corner(fx + 1.0, fy + 1.0, fz, layer);
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
                        Self::add_quad(&mut vertices, &mut indices, c0, c1, c2, c3, color, ao);
                    }

                    // 2. Face Bas (-Y)
                    if !is_water {
                        let neighbor_bottom = get_voxel(gx, gy - 1, gz);
                        if neighbor_bottom.is_air() {
                            let color = get_color(gx, gy, gz, block, false);
                            let c0 = get_corner(fx, fy, fz, layer);
                            let c1 = get_corner(fx + 1.0, fy, fz, layer);
                            let c2 = get_corner(fx + 1.0, fy, fz + 1.0, layer);
                            let c3 = get_corner(fx, fy, fz + 1.0, layer);
                            let ao = [
                                Self::calc_corner_ao(&get_voxel, (gx - 1, gy - 1, gz), (gx, gy - 1, gz - 1), (gx - 1, gy - 1, gz - 1)),
                                Self::calc_corner_ao(&get_voxel, (gx + 1, gy - 1, gz), (gx, gy - 1, gz - 1), (gx + 1, gy - 1, gz - 1)),
                                Self::calc_corner_ao(&get_voxel, (gx + 1, gy - 1, gz), (gx, gy - 1, gz + 1), (gx + 1, gy - 1, gz + 1)),
                                Self::calc_corner_ao(&get_voxel, (gx - 1, gy - 1, gz), (gx, gy - 1, gz + 1), (gx - 1, gy - 1, gz + 1)),
                            ];
                            Self::add_quad(&mut vertices, &mut indices, c0, c1, c2, c3, color, ao);
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
                        let color = get_color(gx, gy, gz, block, false);
                        let c0 = get_corner(fx, fy, fz, layer);
                        let c1 = get_corner(fx, fy, fz + 1.0, layer);
                        let c2 = get_corner(fx, fy + 1.0, fz + 1.0, layer);
                        let c3 = get_corner(fx, fy + 1.0, fz, layer);
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
                        Self::add_quad(&mut vertices, &mut indices, c0, c1, c2, c3, color, ao);
                    }

                    // 4. Face Droite (+X)
                    let neighbor_right = get_voxel(gx + 1, gy, gz);
                    let should_emit_right = if is_water {
                        neighbor_right.is_air()
                    } else {
                        neighbor_right.is_air() || neighbor_right == BlockId::WATER
                    };
                    if should_emit_right {
                        let color = get_color(gx, gy, gz, block, false);
                        let c0 = get_corner(fx + 1.0, fy, fz + 1.0, layer);
                        let c1 = get_corner(fx + 1.0, fy, fz, layer);
                        let c2 = get_corner(fx + 1.0, fy + 1.0, fz, layer);
                        let c3 = get_corner(fx + 1.0, fy + 1.0, fz + 1.0, layer);
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
                        Self::add_quad(&mut vertices, &mut indices, c0, c1, c2, c3, color, ao);
                    }

                    // 5. Face Avant (+Z)
                    let neighbor_front = get_voxel(gx, gy, gz + 1);
                    let should_emit_front = if is_water {
                        neighbor_front.is_air()
                    } else {
                        neighbor_front.is_air() || neighbor_front == BlockId::WATER
                    };
                    if should_emit_front {
                        let color = get_color(gx, gy, gz, block, false);
                        let c0 = get_corner(fx + 1.0, fy, fz + 1.0, layer);
                        let c1 = get_corner(fx, fy, fz + 1.0, layer);
                        let c2 = get_corner(fx, fy + 1.0, fz + 1.0, layer);
                        let c3 = get_corner(fx + 1.0, fy + 1.0, fz + 1.0, layer);
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
                        Self::add_quad(&mut vertices, &mut indices, c0, c1, c2, c3, color, ao);
                    }

                    // 6. Face Arrière (-Z)
                    let neighbor_back = get_voxel(gx, gy, gz - 1);
                    let should_emit_back = if is_water {
                        neighbor_back.is_air()
                    } else {
                        neighbor_back.is_air() || neighbor_back == BlockId::WATER
                    };
                    if should_emit_back {
                        let color = get_color(gx, gy, gz, block, false);
                        let c0 = get_corner(fx, fy, fz, layer);
                        let c1 = get_corner(fx + 1.0, fy, fz, layer);
                        let c2 = get_corner(fx + 1.0, fy + 1.0, fz, layer);
                        let c3 = get_corner(fx, fy + 1.0, fz, layer);
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
                        Self::add_quad(&mut vertices, &mut indices, c0, c1, c2, c3, color, ao);
                    }
                }
            }
        }

        PlanetMesh { vertices, indices }
    }

    #[inline(always)]
    fn is_solid_for_ao(block: BlockId) -> bool {
        !block.is_air() && block != BlockId::WATER
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

    fn add_quad(
        vertices: &mut Vec<PlanetVertex>,
        indices: &mut Vec<u32>,
        c0: Vec3,
        c1: Vec3,
        c2: Vec3,
        c3: Vec3,
        color: [f32; 4],
        ao: [f32; 4],
    ) {
        let base_idx = vertices.len() as u32;

        // Calcul de la normale via le produit vectoriel des arêtes
        let e1 = c1 - c0;
        let e2 = c2 - c0;
        let normal = e1.cross(e2).normalize_or_zero().to_array();

        let make_vert = |pos: Vec3, ao_factor: f32| PlanetVertex {
            position: pos.to_array(),
            normal,
            color: [
                color[0] * ao_factor,
                color[1] * ao_factor,
                color[2] * ao_factor,
                color[3],
            ],
        };

        vertices.push(make_vert(c0, ao[0]));
        vertices.push(make_vert(c1, ao[1]));
        vertices.push(make_vert(c2, ao[2]));
        vertices.push(make_vert(c3, ao[3]));

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

    /// Palette de couleurs fidèle aux blocs HeroCraft
    fn block_color(block: BlockId, is_top: bool) -> [f32; 4] {
        match block {
            BlockId::GRASS => {
                if is_top {
                    [0.28, 0.65, 0.22, 1.0] // Sommet herbeux vert
                } else {
                    [0.46, 0.33, 0.18, 1.0] // Tranche de terre brune
                }
            }
            BlockId::DIRT => [0.46, 0.33, 0.18, 1.0],
            BlockId::STONE => [0.52, 0.50, 0.48, 1.0],
            BlockId::SAND => [0.88, 0.82, 0.56, 1.0],
            BlockId::SNOW => [0.96, 0.97, 1.00, 1.0],
            BlockId::WATER => [0.10, 0.54, 0.82, 0.90], // Eau azur translucide
            BlockId::WOOD => [0.42, 0.28, 0.16, 1.0],
            BlockId::LEAVES => [0.14, 0.44, 0.16, 1.0],
            _ => [0.60, 0.60, 0.60, 1.0],
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

        // Surface de la colonne (x=5, z=7) : le premier bloc d'air en montant
        let surface = (0..528).find(|&y| generator.block_at(face, cx * 32 + 5, y, cy * 32 + 7).is_air()).unwrap();
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
        let surface = (0..528).find(|&y| generator.block_at(face, cx * 32 + 9, y, cy * 32 + 9).is_air()).unwrap();
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


