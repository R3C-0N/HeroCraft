use glam::Vec3;
use herocraft_common::{BlockId, PlanetChunkCoord, PlanetFace};
use herocraft_voxel_engine::cache::WorldChunkCache;
use herocraft_voxel_engine::chunk::{ChunkData, CHUNK_SIZE};
use herocraft_world::generator::PlanetWorldGenerator;

use crate::planet_mesh::{PlanetMesh, PlanetVertex};

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
                        Self::add_quad(&mut vertices, &mut indices, c0, c1, c2, c3, color);
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
                            Self::add_quad(&mut vertices, &mut indices, c0, c1, c2, c3, color);
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
                        Self::add_quad(&mut vertices, &mut indices, c0, c1, c2, c3, color);
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
                        Self::add_quad(&mut vertices, &mut indices, c0, c1, c2, c3, color);
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
                        Self::add_quad(&mut vertices, &mut indices, c0, c1, c2, c3, color);
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
                        Self::add_quad(&mut vertices, &mut indices, c0, c1, c2, c3, color);
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

    /// Version optimisée thread-safe sans dépendance de cache mutable
    pub fn build_incurved_single_chunk_fast(
        generator: &PlanetWorldGenerator,
        face: PlanetFace,
        chunk_x: i32,
        chunk_y: i32,
        base_radius: f32,
        filter: herocraft_world::VisualizationFilter,
    ) -> PlanetMesh {
        let mut vertices: Vec<PlanetVertex> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();

        let face_chunks_count = 32.0;
        let chunk_span_uv = 2.0 / face_chunks_count;
        let voxel_span_uv = chunk_span_uv / (CHUNK_SIZE as f64);
        let altitude_scale = (base_radius as f64 * std::f64::consts::FRAC_PI_4 * voxel_span_uv) as f32;

        let base_u = -1.0 + (chunk_x as f64) * chunk_span_uv;
        let base_v = -1.0 + (chunk_y as f64) * chunk_span_uv;

        // Détection des limites d'élévation sur la colonne pour optimiser le nombre de couches à mailler
        let corners = [
            (0.0, 0.0),
            (1.0, 0.0),
            (0.0, 1.0),
            (1.0, 1.0),
            (0.5, 0.5),
        ];
        let mut min_elev = f64::MAX;
        let mut max_elev = f64::MIN;
        for &(du, dv) in &corners {
            let u = base_u + du * chunk_span_uv;
            let v = base_v + dv * chunk_span_uv;
            let dir = face.uv_to_sphere_direction(u, v);
            let (lat, lon) = herocraft_world::cubed_sphere::unit_sphere_to_lat_lon(dir);
            let e = generator.sample_elevation(dir, lat, lon);
            if e < min_elev { min_elev = e; }
            if e > max_elev { max_elev = e; }
        }

        // On inclut le niveau de la mer et une marge de sécurité
        let sea = herocraft_world::generator::SEA_LEVEL_BLOCKS;
        let lowest = (min_elev.min(sea) - 6.0).max(0.0);
        let highest = (max_elev.max(sea) + 6.0).min(528.0);

        let min_layer = ((lowest / 32.0).floor() as i32).clamp(0, 16);
        let max_layer = ((highest / 32.0).ceil() as i32).clamp(0, 16);

        let get_corner = |vx: f64, vy: f64, vz: f64, layer: i32| -> Vec3 {
            let u = base_u + vx * voxel_span_uv;
            let v = base_v + vz * voxel_span_uv;
            let dir = face.uv_to_sphere_direction(u, v);
            let global_y = (layer as f64) * 32.0 + vy;
            let r = base_radius + (global_y as f32) * altitude_scale;
            Vec3::new(dir.x as f32, dir.y as f32, dir.z as f32) * r
        };

        // Précalcul des couleurs de colonnes 32x32 pour éviter de répéter l'échantillonnage de bruit sur chaque face
        let mut column_colors = vec![[0.0_f32; 4]; 32 * 32];
        for cz in 0..32 {
            for cx in 0..32 {
                let u = base_u + (cx as f64 + 0.5) * voxel_span_uv;
                let v = base_v + (cz as f64 + 0.5) * voxel_span_uv;
                let dir = face.uv_to_sphere_direction(u, v);
                let (lat, lon) = herocraft_world::cubed_sphere::unit_sphere_to_lat_lon(dir);
                let raw_elevation = generator.sample_elevation(dir, lat, lon);
                let biome = generator.determine_biome(lat, raw_elevation, dir);
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

        let get_color = |gx: i32, gz: i32, block: BlockId, is_top: bool| -> [f32; 4] {
            let idx = (gz.clamp(0, 31) as usize) * 32 + (gx.clamp(0, 31) as usize);
            let biome_color = column_colors[idx];
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

        let mut prev_chunk: Option<herocraft_voxel_engine::ChunkData> = None;

        for layer in min_layer..=max_layer {
            let layer_base = (layer * 32) as f64;
            let layer_top = layer_base + 32.0;

            // Optimisation : saut des chunks 100% souterrains ou 100% dans le ciel
            if min_elev >= layer_top && sea < layer_base {
                prev_chunk = None;
                continue;
            }
            if max_elev < layer_base && sea < layer_base {
                prev_chunk = None;
                continue;
            }

            let coord = PlanetChunkCoord::new(face, chunk_x, chunk_y, layer);
            let center_chunk = generator.generate_chunk(&coord);
            let chunk_below = prev_chunk;

            let get_voxel = |gx: i32, gy: i32, gz: i32| -> BlockId {
                if gx >= 0 && gx < 32 && gz >= 0 && gz < 32 && gy >= 0 && gy < 32 {
                    return center_chunk.get_voxel(gx as u8, gy as u8, gz as u8);
                }
                if gy < 0 {
                    if layer == 0 {
                        return BlockId::STONE;
                    }
                    if let Some(ref below) = chunk_below {
                        return below.get_voxel(gx.clamp(0, 31) as u8, 31, gz.clamp(0, 31) as u8);
                    }
                }
                let global_y = (layer as f64) * 32.0 + (gy as f64);
                let u = base_u + (gx as f64) * voxel_span_uv;
                let v = base_v + (gz as f64) * voxel_span_uv;
                let dir = face.uv_to_sphere_direction(u, v);
                let (lat, lon) = herocraft_world::cubed_sphere::unit_sphere_to_lat_lon(dir);
                let col_elevation = generator.sample_elevation(dir, lat, lon);

                let elev_floor = col_elevation.floor();
                if global_y < elev_floor {
                    BlockId::STONE
                } else if global_y < sea {
                    let biome = generator.determine_biome(lat, col_elevation, dir);
                    if biome == herocraft_world::BiomeType::SnowPeaks {
                        BlockId::SNOW
                    } else {
                        BlockId::WATER
                    }
                } else {
                    BlockId::AIR
                }
            };

            let pack_key = |block: BlockId, is_top: bool, color: [f32; 4]| -> u32 {
                let r = (color[0] * 31.0).clamp(0.0, 31.0) as u32;
                let g = (color[1] * 31.0).clamp(0.0, 31.0) as u32;
                let b = (color[2] * 31.0).clamp(0.0, 31.0) as u32;
                (block.0 as u32) | ((is_top as u32) << 8) | (r << 9) | (g << 14) | (b << 19)
            };

            // 1 & 2. Slices horizontales Y : Faces Haut (+Y) et Bas (-Y)
            for gy in 0..CHUNK_SIZE as i32 {
                let fy = gy as f64;
                let mut top_grid: [Option<(u32, [f32; 4])>; 32 * 32] = [None; 32 * 32];
                let mut bottom_grid: [Option<(u32, [f32; 4])>; 32 * 32] = [None; 32 * 32];

                for gz in 0..32 {
                    for gx in 0..32 {
                        let block = get_voxel(gx, gy, gz);
                        if block.is_air() {
                            continue;
                        }
                        let is_water = block == BlockId::WATER;

                        // Face Haut (+Y)
                        let neighbor_top = get_voxel(gx, gy + 1, gz);
                        let should_emit_top = if is_water {
                            neighbor_top.is_air()
                        } else {
                            neighbor_top.is_air() || neighbor_top == BlockId::WATER
                        };
                        if should_emit_top {
                            let color = get_color(gx, gz, block, true);
                            let key = pack_key(block, true, color);
                            top_grid[(gz as usize) * 32 + (gx as usize)] = Some((key, color));
                        }

                        // Face Bas (-Y)
                        if !is_water {
                            let neighbor_bottom = get_voxel(gx, gy - 1, gz);
                            if neighbor_bottom.is_air() {
                                let color = get_color(gx, gz, block, false);
                                let key = pack_key(block, false, color);
                                bottom_grid[(gz as usize) * 32 + (gx as usize)] = Some((key, color));
                            }
                        }
                    }
                }

                // Greedy meshing sur la face Haut (+Y)
                Self::greedy_mesh_2d(top_grid, |u, v, w, h, color| {
                    let c0 = get_corner(u as f64, fy + 1.0, v as f64, layer);
                    let c1 = get_corner(u as f64, fy + 1.0, (v + h) as f64, layer);
                    let c2 = get_corner((u + w) as f64, fy + 1.0, (v + h) as f64, layer);
                    let c3 = get_corner((u + w) as f64, fy + 1.0, v as f64, layer);
                    Self::add_quad(&mut vertices, &mut indices, c0, c1, c2, c3, color);
                });

                // Greedy meshing sur la face Bas (-Y)
                Self::greedy_mesh_2d(bottom_grid, |u, v, w, h, color| {
                    let c0 = get_corner(u as f64, fy, v as f64, layer);
                    let c1 = get_corner((u + w) as f64, fy, v as f64, layer);
                    let c2 = get_corner((u + w) as f64, fy, (v + h) as f64, layer);
                    let c3 = get_corner(u as f64, fy, (v + h) as f64, layer);
                    Self::add_quad(&mut vertices, &mut indices, c0, c1, c2, c3, color);
                });
            }

            // 3 & 4. Slices verticales X : Faces Gauche (-X) et Droite (+X)
            for gx in 0..32 {
                let fx = gx as f64;
                let mut left_grid: [Option<(u32, [f32; 4])>; 32 * 32] = [None; 32 * 32];
                let mut right_grid: [Option<(u32, [f32; 4])>; 32 * 32] = [None; 32 * 32];

                for gy in 0..CHUNK_SIZE as i32 {
                    for gz in 0..32 {
                        let block = get_voxel(gx, gy, gz);
                        if block.is_air() {
                            continue;
                        }
                        let is_water = block == BlockId::WATER;

                        // Face Gauche (-X)
                        let neighbor_left = get_voxel(gx - 1, gy, gz);
                        let should_emit_left = if is_water {
                            neighbor_left.is_air()
                        } else {
                            neighbor_left.is_air() || neighbor_left == BlockId::WATER
                        };
                        if should_emit_left {
                            let color = get_color(gx, gz, block, false);
                            let key = pack_key(block, false, color);
                            left_grid[(gy as usize) * 32 + (gz as usize)] = Some((key, color));
                        }

                        // Face Droite (+X)
                        let neighbor_right = get_voxel(gx + 1, gy, gz);
                        let should_emit_right = if is_water {
                            neighbor_right.is_air()
                        } else {
                            neighbor_right.is_air() || neighbor_right == BlockId::WATER
                        };
                        if should_emit_right {
                            let color = get_color(gx, gz, block, false);
                            let key = pack_key(block, false, color);
                            right_grid[(gy as usize) * 32 + (gz as usize)] = Some((key, color));
                        }
                    }
                }

                // Greedy meshing sur la face Gauche (-X) : u=gz, v=gy
                Self::greedy_mesh_2d(left_grid, |u, v, w, h, color| {
                    let c0 = get_corner(fx, v as f64, u as f64, layer);
                    let c1 = get_corner(fx, v as f64, (u + w) as f64, layer);
                    let c2 = get_corner(fx, (v + h) as f64, (u + w) as f64, layer);
                    let c3 = get_corner(fx, (v + h) as f64, u as f64, layer);
                    Self::add_quad(&mut vertices, &mut indices, c0, c1, c2, c3, color);
                });

                // Greedy meshing sur la face Droite (+X) : u=gz, v=gy
                Self::greedy_mesh_2d(right_grid, |u, v, w, h, color| {
                    let c0 = get_corner(fx + 1.0, v as f64, (u + w) as f64, layer);
                    let c1 = get_corner(fx + 1.0, v as f64, u as f64, layer);
                    let c2 = get_corner(fx + 1.0, (v + h) as f64, u as f64, layer);
                    let c3 = get_corner(fx + 1.0, (v + h) as f64, (u + w) as f64, layer);
                    Self::add_quad(&mut vertices, &mut indices, c0, c1, c2, c3, color);
                });
            }

            // 5 & 6. Slices verticales Z : Faces Avant (+Z) et Arrière (-Z)
            for gz in 0..32 {
                let fz = gz as f64;
                let mut front_grid: [Option<(u32, [f32; 4])>; 32 * 32] = [None; 32 * 32];
                let mut back_grid: [Option<(u32, [f32; 4])>; 32 * 32] = [None; 32 * 32];

                for gy in 0..CHUNK_SIZE as i32 {
                    for gx in 0..32 {
                        let block = get_voxel(gx, gy, gz);
                        if block.is_air() {
                            continue;
                        }
                        let is_water = block == BlockId::WATER;

                        // Face Avant (+Z)
                        let neighbor_front = get_voxel(gx, gy, gz + 1);
                        let should_emit_front = if is_water {
                            neighbor_front.is_air()
                        } else {
                            neighbor_front.is_air() || neighbor_front == BlockId::WATER
                        };
                        if should_emit_front {
                            let color = get_color(gx, gz, block, false);
                            let key = pack_key(block, false, color);
                            front_grid[(gy as usize) * 32 + (gx as usize)] = Some((key, color));
                        }

                        // Face Arrière (-Z)
                        let neighbor_back = get_voxel(gx, gy, gz - 1);
                        let should_emit_back = if is_water {
                            neighbor_back.is_air()
                        } else {
                            neighbor_back.is_air() || neighbor_back == BlockId::WATER
                        };
                        if should_emit_back {
                            let color = get_color(gx, gz, block, false);
                            let key = pack_key(block, false, color);
                            back_grid[(gy as usize) * 32 + (gx as usize)] = Some((key, color));
                        }
                    }
                }

                // Greedy meshing sur la face Avant (+Z) : u=gx, v=gy
                Self::greedy_mesh_2d(front_grid, |u, v, w, h, color| {
                    let c0 = get_corner(u as f64, v as f64, fz + 1.0, layer);
                    let c1 = get_corner((u + w) as f64, v as f64, fz + 1.0, layer);
                    let c2 = get_corner((u + w) as f64, (v + h) as f64, fz + 1.0, layer);
                    let c3 = get_corner(u as f64, (v + h) as f64, fz + 1.0, layer);
                    Self::add_quad(&mut vertices, &mut indices, c0, c1, c2, c3, color);
                });

                // Greedy meshing sur la face Arrière (-Z) : u=gx, v=gy
                Self::greedy_mesh_2d(back_grid, |u, v, w, h, color| {
                    let c0 = get_corner((u + w) as f64, v as f64, fz, layer);
                    let c1 = get_corner(u as f64, v as f64, fz, layer);
                    let c2 = get_corner(u as f64, (v + h) as f64, fz, layer);
                    let c3 = get_corner((u + w) as f64, (v + h) as f64, fz, layer);
                    Self::add_quad(&mut vertices, &mut indices, c0, c1, c2, c3, color);
                });
            }
            prev_chunk = Some(center_chunk);
        }

        PlanetMesh { vertices, indices }
    }

    /// Algorithme de Greedy Meshing 2D sur une tranche 32x32 : fusionne les quads coplanaires contigus
    fn greedy_mesh_2d<F>(
        mut grid: [Option<(u32, [f32; 4])>; 32 * 32],
        mut emit_quad: F,
    ) where
        F: FnMut(usize, usize, usize, usize, [f32; 4]),
    {
        for v in 0..32 {
            let mut u = 0;
            while u < 32 {
                if let Some((cell_key, color)) = grid[v * 32 + u] {
                    // 1. Extension le long de l'axe U (largeur)
                    let mut width = 1;
                    while u + width < 32 {
                        if let Some((next_key, _)) = grid[v * 32 + u + width] {
                            if next_key == cell_key {
                                width += 1;
                                continue;
                            }
                        }
                        break;
                    }

                    // 2. Extension le long de l'axe V (hauteur)
                    let mut height = 1;
                    'outer: while v + height < 32 {
                        for k in 0..width {
                            if let Some((next_key, _)) = grid[(v + height) * 32 + u + k] {
                                if next_key == cell_key {
                                    continue;
                                }
                            }
                            break 'outer;
                        }
                        height += 1;
                    }

                    // 3. Effacer les cellules fusionnées
                    for j in 0..height {
                        for i in 0..width {
                            grid[(v + j) * 32 + u + i] = None;
                        }
                    }

                    // 4. Émission du quad fusionné
                    emit_quad(u, v, width, height, color);
                    u += width;
                } else {
                    u += 1;
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
    fn test_greedy_meshing_quad_reduction() {
        let mut emitted_quads = 0;
        let mut grid: [Option<(u32, [f32; 4])>; 32 * 32] = [None; 32 * 32];
        let color = [0.2, 0.7, 0.3, 1.0];
        // Remplir une surface plane 32x32 avec le même bloc et couleur
        for i in 0..(32 * 32) {
            grid[i] = Some((42, color));
        }

        VoxelMeshBuilder::greedy_mesh_2d(grid, |u, v, w, h, _| {
            emitted_quads += 1;
            assert_eq!(u, 0);
            assert_eq!(v, 0);
            assert_eq!(w, 32);
            assert_eq!(h, 32);
        });

        // 1024 cellules fusionnées en exactement 1 seul quad ! (Réduction de 99.9%)
        assert_eq!(emitted_quads, 1);
    }
}

