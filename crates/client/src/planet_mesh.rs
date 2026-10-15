use glam::{DVec3, Vec3};
use herocraft_common::PlanetFace;
use herocraft_world::{
    cubed_sphere::unit_sphere_to_lat_lon,
    generator::{BiomeType, PlanetWorldGenerator, SEA_LEVEL_BLOCKS},
};

/// Sommet pour le rendu de la planète
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PlanetVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 4],
}

impl PlanetVertex {
    pub fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<PlanetVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                // Position : 3 x Float32 = 12 octets
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                // Normale : 3 x Float32 = 12 octets
                wgpu::VertexAttribute {
                    offset: 12,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
                // Couleur RGBA : 4 x Float32 = 16 octets
                wgpu::VertexAttribute {
                    offset: 24,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

/// Maillage indexé prêt pour WGPU
#[derive(Clone)]
pub struct PlanetMesh {
    pub vertices: Vec<PlanetVertex>,
    pub indices: Vec<u32>,
}

impl PlanetMesh {
    /// Applique un facteur d'opacité/fondu alpha à tous les sommets du maillage
    #[allow(dead_code)]
    pub fn set_alpha(&mut self, alpha: f32) {
        for v in &mut self.vertices {
            v.color[3] = alpha;
        }
    }
    /// Construit le maillage complet de la planète Cubed-Sphere avec relief et biomes
    #[allow(dead_code)]
    pub fn generate_planet(
        generator: &PlanetWorldGenerator,
        base_radius: f32,
        grid_resolution: usize,
        filter: herocraft_world::VisualizationFilter,
    ) -> Self {
        let n = grid_resolution.max(8);
        let mut vertices: Vec<PlanetVertex> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();

        for &face in &PlanetFace::ALL {
            let base_vertex_index = vertices.len() as u32;

            // 1. Génération de la grille de sommets pour cette face
            let mut face_positions = Vec::with_capacity((n + 1) * (n + 1));
            let mut face_colors = Vec::with_capacity((n + 1) * (n + 1));
            let mut face_normals = Vec::with_capacity((n + 1) * (n + 1));
            let mut is_water_flags = Vec::with_capacity((n + 1) * (n + 1));

            for j in 0..=n {
                let v = -1.0 + 2.0 * (j as f64) / (n as f64);
                for i in 0..=n {
                    let u = -1.0 + 2.0 * (i as f64) / (n as f64);

                    let sphere_dir: DVec3 = face.uv_to_sphere_direction(u, v);
                    let (lat, lon) = unit_sphere_to_lat_lon(sphere_dir);
                    let raw_elevation = generator.sample_elevation(sphere_dir, lat, lon);
                    let biome = generator.determine_biome(lat, raw_elevation, sphere_dir);
                    let is_water = biome.is_water();

                    let color = match filter {
                        herocraft_world::VisualizationFilter::Standard => Self::relief_biome_color(biome, raw_elevation),
                        herocraft_world::VisualizationFilter::Tectonics => {
                            let base_color = Self::relief_biome_color(biome, raw_elevation);
                            let sample = generator.tectonics.sample(sphere_dir);
                            Self::blend_tectonic_color(base_color, &sample)
                        }
                        herocraft_world::VisualizationFilter::OceanCurrents => {
                            let continental = generator.sample_continental_value(sphere_dir);
                            generator.ocean_currents.sample(sphere_dir, is_water, continental).color
                        }
                    };

                    // Pour l'eau (océans et mers), la surface liquide est parfaitement sphérique au niveau de la mer
                    let is_water = biome.is_water();
                    let altitude_scale = (base_radius as f64 * std::f64::consts::FRAC_PI_4 * (2.0 / 1024.0)) as f32;
                    let elevation_clamped = if is_water {
                        SEA_LEVEL_BLOCKS as f32 * altitude_scale
                    } else {
                        (raw_elevation as f32) * altitude_scale
                    };

                    let radius = base_radius + elevation_clamped;
                    let dir_f32 = Vec3::new(sphere_dir.x as f32, sphere_dir.y as f32, sphere_dir.z as f32);
                    let pos = dir_f32 * radius;

                    face_positions.push(pos);
                    face_colors.push(color);
                    face_normals.push(dir_f32);
                    is_water_flags.push(is_water);
                }
            }

            // 2. Calcul des normales par dérivation pour le relief montagneux et les plaines
            for j in 0..=n {
                for i in 0..=n {
                    let idx = j * (n + 1) + i;

                    let pos = face_positions[idx];
                    let norm = face_normals[idx];
                    let is_water = is_water_flags[idx];

                    // Si c'est de l'eau, la normale est la normale sphérique pure pour des reflets lisses
                    let final_normal = if is_water {
                        norm
                    } else {
                        let dx = if i + 1 <= n {
                            face_positions[j * (n + 1) + (i + 1)] - pos
                        } else {
                            pos - face_positions[j * (n + 1) + (i - 1)]
                        };

                        let dy = if j + 1 <= n {
                            face_positions[(j + 1) * (n + 1) + i] - pos
                        } else {
                            pos - face_positions[(j - 1) * (n + 1) + i]
                        };

                        let computed = dx.cross(dy).normalize_or_zero();
                        if computed.dot(norm) > 0.0 {
                            computed
                        } else {
                            -computed
                        }
                    };

                    vertices.push(PlanetVertex {
                        position: pos.to_array(),
                        normal: final_normal.to_array(),
                        color: face_colors[idx],
                    });
                }
            }

            // 3. Triangulation des quads de la face
            for j in 0..n {
                for i in 0..n {
                    let row1 = base_vertex_index + (j * (n + 1) + i) as u32;
                    let row2 = base_vertex_index + ((j + 1) * (n + 1) + i) as u32;

                    let v0 = row1;
                    let v1 = row1 + 1;
                    let v2 = row2 + 1;
                    let v3 = row2;

                    // Premier triangle CCW (v0, v2, v1)
                    indices.push(v0);
                    indices.push(v2);
                    indices.push(v1);

                    // Deuxième triangle CCW (v0, v3, v2)
                    indices.push(v0);
                    indices.push(v3);
                    indices.push(v2);
                }
            }
        }

        PlanetMesh { vertices, indices }
    }

    /// Palette de couleurs enrichie avec gradation altimétrique et bathymétrique
    /// Fait ressortir les fosses océaniques (bleu nuit abyssal très contrasté) et les massifs/super-chaînes de montagnes (ardoise sombre -> granite -> cimes éclatantes)
    pub fn relief_biome_color(biome: BiomeType, raw_elevation: f64) -> [f32; 4] {
        // Enneigement strict au-dessus de 300 blocs pour n'importe quel relief continental
        if !biome.is_water() && raw_elevation >= 300.0 {
            let t = ((raw_elevation - 300.0) / 50.0).clamp(0.0, 1.0) as f32;
            let c_snowline = [0.88, 0.93, 0.99, 1.0]; // Ligne des neiges / séracs bleutés à 300
            let c_summit = [1.00, 1.00, 1.00, 1.0];   // Sommet éclatant de pureté à 350
            return [
                c_snowline[0] * (1.0 - t) + c_summit[0] * t,
                c_snowline[1] * (1.0 - t) + c_summit[1] * t,
                c_snowline[2] * (1.0 - t) + c_summit[2] * t,
                1.0,
            ];
        }

        if biome.is_water() {
            // Dégradé bathymétrique sous-marin :
            // raw_elevation va de ~15.0 (fosses océaniques et subductions profondes) à 100.0 (niveau de la mer)
            let depth = (SEA_LEVEL_BLOCKS - raw_elevation).max(0.0);
            let t = (depth / 80.0).clamp(0.0, 1.0) as f32;

            // 3 paliers d'interpolation :
            // t = 0.0 : Lagons et mers côtières peu profondes (azur / cyan lumineux)
            // t = 0.40 : Bassins océaniques moyens (bleu roi profond)
            // t = 1.0 : Fosses abyssales et zones de subduction (bleu nuit / noir abyssal très sombre et contrasté)
            let c_shallow = [0.12, 0.65, 0.88, 1.0];
            let c_mid = [0.04, 0.22, 0.62, 1.0];
            let c_trench = [0.006, 0.025, 0.16, 1.0];

            if t < 0.40 {
                let k = t / 0.40;
                [
                    c_shallow[0] * (1.0 - k) + c_mid[0] * k,
                    c_shallow[1] * (1.0 - k) + c_mid[1] * k,
                    c_shallow[2] * (1.0 - k) + c_mid[2] * k,
                    1.0,
                ]
            } else {
                let k = (t - 0.40) / 0.60;
                [
                    c_mid[0] * (1.0 - k) + c_trench[0] * k,
                    c_mid[1] * (1.0 - k) + c_trench[1] * k,
                    c_mid[2] * (1.0 - k) + c_trench[2] * k,
                    1.0,
                ]
            }
        } else {
            match biome {
                BiomeType::Beach => [0.88, 0.82, 0.56, 1.0],
                BiomeType::Desert => [0.90, 0.72, 0.38, 1.0],
                BiomeType::Forest => {
                    // Piémont forestier : fonce légèrement avec l'altitude en approchant des montagnes
                    if raw_elevation > 140.0 {
                        let k = ((raw_elevation - 140.0) / 30.0).clamp(0.0, 1.0) as f32;
                        let base = [0.14, 0.44, 0.16, 1.0];
                        let rock = [0.25, 0.35, 0.22, 1.0];
                        [
                            base[0] * (1.0 - k) + rock[0] * k,
                            base[1] * (1.0 - k) + rock[1] * k,
                            base[2] * (1.0 - k) + rock[2] * k,
                            1.0,
                        ]
                    } else {
                        [0.14, 0.44, 0.16, 1.0]
                    }
                }
                BiomeType::Plains => {
                    // Piémont des plaines : transition douce vers la roche sombre
                    if raw_elevation > 140.0 {
                        let k = ((raw_elevation - 140.0) / 30.0).clamp(0.0, 1.0) as f32;
                        let base = [0.28, 0.64, 0.22, 1.0];
                        let rock = [0.35, 0.42, 0.26, 1.0];
                        [
                            base[0] * (1.0 - k) + rock[0] * k,
                            base[1] * (1.0 - k) + rock[1] * k,
                            base[2] * (1.0 - k) + rock[2] * k,
                            1.0,
                        ]
                    } else {
                        [0.28, 0.64, 0.22, 1.0]
                    }
                }
                BiomeType::Mountains => {
                    // Massifs montagneux rocailleux (170.0 -> 300.0) :
                    // - Piémont et alpages (170 -> 215) : transition d'alpages montagnards vers la roche
                    // - Versants rocailleux et crêtes (215 -> 300) : roches minérales rugueuses (ardoise, basalte, granite)
                    let t = ((raw_elevation - 170.0) / 130.0).clamp(0.0, 1.0) as f32;
                    if t < 0.35 {
                        let k = t / 0.35;
                        let c_alpage = [0.26, 0.48, 0.22, 1.0]; // Vert alpage montagnard
                        let c_rock_base = [0.36, 0.34, 0.33, 1.0]; // Roche ardoise sombre
                        [
                            c_alpage[0] * (1.0 - k) + c_rock_base[0] * k,
                            c_alpage[1] * (1.0 - k) + c_rock_base[1] * k,
                            c_alpage[2] * (1.0 - k) + c_rock_base[2] * k,
                            1.0,
                        ]
                    } else {
                        let k = (t - 0.35) / 0.65;
                        let c_rock_base = [0.36, 0.34, 0.33, 1.0]; // Ardoise / basalte sombre
                        let c_granite = [0.60, 0.58, 0.56, 1.0];   // Granite minéral clair
                        [
                            c_rock_base[0] * (1.0 - k) + c_granite[0] * k,
                            c_rock_base[1] * (1.0 - k) + c_granite[1] * k,
                            c_rock_base[2] * (1.0 - k) + c_granite[2] * k,
                            1.0,
                        ]
                    }
                }
                BiomeType::SnowPeaks => {
                    // Calotte polaire affleurante au niveau de la mer (~101) vs Hauts sommets
                    if raw_elevation < SEA_LEVEL_BLOCKS + 5.0 {
                        [0.92, 0.96, 0.99, 1.0] // Banquise polaire immaculée
                    } else {
                        // Enneigement au-dessus de 300.0 jusqu'aux sommets (300.0 -> 350.0)
                        let t = ((raw_elevation - 300.0) / 50.0).clamp(0.0, 1.0) as f32;
                        let c_snowline = [0.88, 0.93, 0.99, 1.0]; // Ligne des neiges / séracs bleutés
                        let c_summit = [1.00, 1.00, 1.00, 1.0];   // Sommet éclatant de pureté
                        [
                            c_snowline[0] * (1.0 - t) + c_summit[0] * t,
                            c_snowline[1] * (1.0 - t) + c_summit[1] * t,
                            c_snowline[2] * (1.0 - t) + c_summit[2] * t,
                            1.0,
                        ]
                    }
                }
                _ => Self::biome_color(biome),
            }
        }
    }

    /// Palette de couleurs de base selon le biome (utilisée comme repli)
    pub fn biome_color(biome: BiomeType) -> [f32; 4] {
        match biome {
            // Zones maritimes (Océans et Mers)
            BiomeType::DeepOcean => [0.03, 0.10, 0.38, 1.0], // Bleu abyssal profond
            BiomeType::Ocean => [0.07, 0.28, 0.68, 1.0],     // Bleu océan intense
            BiomeType::Sea => [0.10, 0.54, 0.82, 1.0],       // Bleu lagon / azur mer intérieure
            // Zones côtières et terrestres
            BiomeType::Beach => [0.88, 0.82, 0.56, 1.0],      // Sable doré des plages
            BiomeType::Plains => [0.28, 0.64, 0.22, 1.0],     // Plaines et prairies verdoyantes
            BiomeType::Forest => [0.14, 0.44, 0.16, 1.0],     // Forêts et forêts boréales
            BiomeType::Desert => [0.90, 0.72, 0.38, 1.0],     // Dunes et déserts chauds
            BiomeType::Mountains => [0.52, 0.50, 0.48, 1.0],  // Roches grises des massifs
            BiomeType::SnowPeaks => [0.96, 0.97, 1.00, 1.0],  // Sommets enneigés et calottes polaires
        }
    }

    /// Calcule la couleur avec transparence pour le filtre tectonique au-dessus du relief naturel
    pub fn blend_tectonic_color(base_color: [f32; 4], sample: &herocraft_world::TectonicSample) -> [f32; 4] {
        // Sur les lignes de failles (< 0.12), l'accent est marqué (80%),
        // au cœur des plaques, transparence douce (35%) pour laisser voir les continents, mers et biomes
        let alpha = if sample.dist_to_boundary < 0.12 {
            0.80_f32
        } else {
            0.35_f32
        };
        [
            base_color[0] * (1.0 - alpha) + sample.debug_color[0] * alpha,
            base_color[1] * (1.0 - alpha) + sample.debug_color[1] * alpha,
            base_color[2] * (1.0 - alpha) + sample.debug_color[2] * alpha,
            1.0,
        ]
    }

    /// Construit le maillage polygonal d'un patch LOD pour un nœud du QuadTree
    pub fn generate_lod_patch(
        generator: &PlanetWorldGenerator,
        key: &herocraft_world::QuadNodeKey,
        base_radius: f32,
        grid_resolution: usize,
        filter: herocraft_world::VisualizationFilter,
    ) -> Self {
        let n = grid_resolution.max(2);
        let mut vertices: Vec<PlanetVertex> = Vec::with_capacity((n + 1) * (n + 1));
        let mut indices: Vec<u32> = Vec::with_capacity(n * n * 6);

        let (u_min, u_max, v_min, v_max) = key.uv_bounds();

        let mut patch_positions = Vec::with_capacity((n + 1) * (n + 1));
        let mut patch_colors = Vec::with_capacity((n + 1) * (n + 1));
        let mut patch_normals = Vec::with_capacity((n + 1) * (n + 1));
        let mut is_water_flags = Vec::with_capacity((n + 1) * (n + 1));

        let altitude_scale = (base_radius as f64 * std::f64::consts::FRAC_PI_4 * (2.0 / 1024.0)) as f32;

        for j in 0..=n {
            let v = v_min + (v_max - v_min) * (j as f64) / (n as f64);
            for i in 0..=n {
                let u = u_min + (u_max - u_min) * (i as f64) / (n as f64);

                let sphere_dir: DVec3 = key.face.uv_to_sphere_direction(u, v);
                let (lat, lon) = unit_sphere_to_lat_lon(sphere_dir);
                let raw_elevation = generator.sample_elevation(sphere_dir, lat, lon);
                let biome = generator.determine_biome(lat, raw_elevation, sphere_dir);
                let is_water = biome.is_water();

                let color = match filter {
                    herocraft_world::VisualizationFilter::Standard => Self::relief_biome_color(biome, raw_elevation),
                    herocraft_world::VisualizationFilter::Tectonics => {
                        let base_color = Self::relief_biome_color(biome, raw_elevation);
                        let sample = generator.tectonics.sample(sphere_dir);
                        Self::blend_tectonic_color(base_color, &sample)
                    }
                    herocraft_world::VisualizationFilter::OceanCurrents => {
                        let continental = generator.sample_continental_value(sphere_dir);
                        generator.ocean_currents.sample(sphere_dir, is_water, continental).color
                    }
                };

                let final_elevation = if is_water {
                    SEA_LEVEL_BLOCKS
                } else if key.level >= 4 {
                    // Pré-voxélisation progressive (Terracing en escalier de 1 bloc) :
                    // On quantifie l'altitude vers des paliers de 1 bloc avec un profil de marche net
                    let base = raw_elevation.floor();
                    let frac = raw_elevation - base;
                    let step = if frac < 0.5 {
                        0.5 * (frac * 2.0).powi(3)
                    } else {
                        1.0 - 0.5 * ((1.0 - frac) * 2.0).powi(3)
                    };
                    base + step
                } else {
                    raw_elevation
                };

                let elevation_clamped = (final_elevation as f32) * altitude_scale;

                let radius = base_radius + elevation_clamped;
                let dir_f32 = Vec3::new(sphere_dir.x as f32, sphere_dir.y as f32, sphere_dir.z as f32);
                let pos = dir_f32 * radius;

                patch_positions.push(pos);
                patch_colors.push(color);
                patch_normals.push(dir_f32);
                is_water_flags.push(is_water);
            }
        }

        // Calcul des normales par dérivation
        for j in 0..=n {
            for i in 0..=n {
                let idx = j * (n + 1) + i;
                let pos = patch_positions[idx];
                let norm = patch_normals[idx];
                let is_water = is_water_flags[idx];

                let final_normal = if is_water {
                    norm
                } else {
                    let dx = if i + 1 <= n {
                        patch_positions[j * (n + 1) + (i + 1)] - pos
                    } else {
                        pos - patch_positions[j * (n + 1) + (i - 1)]
                    };

                    let dy = if j + 1 <= n {
                        patch_positions[(j + 1) * (n + 1) + i] - pos
                    } else {
                        pos - patch_positions[(j - 1) * (n + 1) + i]
                    };

                    let computed = dx.cross(dy).normalize_or_zero();
                    if computed.dot(norm) > 0.0 {
                        computed
                    } else {
                        -computed
                    }
                };

                vertices.push(PlanetVertex {
                    position: pos.to_array(),
                    normal: final_normal.to_array(),
                    color: patch_colors[idx],
                });
            }
        }

        // Triangulation
        for j in 0..n {
            for i in 0..n {
                let row1 = (j * (n + 1) + i) as u32;
                let row2 = ((j + 1) * (n + 1) + i) as u32;

                let v0 = row1;
                let v1 = row1 + 1;
                let v2 = row2 + 1;
                let v3 = row2;

                // Premier triangle CCW (v0, v2, v1)
                indices.push(v0);
                indices.push(v2);
                indices.push(v1);

                // Deuxième triangle CCW (v0, v3, v2)
                indices.push(v0);
                indices.push(v3);
                indices.push(v2);
            }
        }

        PlanetMesh { vertices, indices }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relief_biome_color_trench_vs_shallow_water() {
        let shallow = PlanetMesh::relief_biome_color(BiomeType::Sea, 98.0);
        let mid_ocean = PlanetMesh::relief_biome_color(BiomeType::Ocean, 65.0);
        let trench = PlanetMesh::relief_biome_color(BiomeType::DeepOcean, 20.0);

        // La fosse abyssale doit être nettement plus sombre que le lagon côtier
        let brightness_shallow = shallow[0] * 0.299 + shallow[1] * 0.587 + shallow[2] * 0.114;
        let brightness_trench = trench[0] * 0.299 + trench[1] * 0.587 + trench[2] * 0.114;
        assert!(
            brightness_trench < brightness_shallow * 0.35,
            "La fosse abyssale ({:?}) doit être profondément plus sombre que l'eau côtière ({:?})",
            trench,
            shallow
        );

        // Décroissance continue de clarté
        assert!(shallow[1] > mid_ocean[1]);
        assert!(mid_ocean[2] > trench[2]);
    }

    #[test]
    fn test_relief_biome_color_mountains_progression() {
        let foothills = PlanetMesh::relief_biome_color(BiomeType::Mountains, 185.0);
        let high_ridge = PlanetMesh::relief_biome_color(BiomeType::Mountains, 265.0);
        let snow_summit = PlanetMesh::relief_biome_color(BiomeType::SnowPeaks, 345.0);

        // La base de la montagne est sombre (ardoise), la crête est en granite clair, le sommet est blanc éclatant
        assert!(foothills[0] < high_ridge[0]);
        assert!(high_ridge[0] < snow_summit[0]);
        assert!(snow_summit[0] > 0.95 && snow_summit[1] > 0.95 && snow_summit[2] > 0.95);
    }

    #[test]
    fn test_snowline_at_300() {
        let just_below = PlanetMesh::relief_biome_color(BiomeType::Mountains, 298.0);
        let at_snow = PlanetMesh::relief_biome_color(BiomeType::Mountains, 305.0);

        // Sous 300 : granite / crête rocheuse (< 0.70 de composante rouge/vert)
        assert!(just_below[0] < 0.70);
        // Au-dessus de 300 : enneigement automatique (> 0.85 de clarté éclatante)
        assert!(at_snow[0] > 0.85 && at_snow[1] > 0.85 && at_snow[2] > 0.85);
    }
}
