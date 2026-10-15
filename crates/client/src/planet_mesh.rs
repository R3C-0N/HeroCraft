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
pub struct PlanetMesh {
    pub vertices: Vec<PlanetVertex>,
    pub indices: Vec<u32>,
}

impl PlanetMesh {
    /// Construit le maillage complet de la planète Cubed-Sphere avec relief et biomes
    pub fn generate_planet(generator: &PlanetWorldGenerator, base_radius: f32, grid_resolution: usize) -> Self {
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

                    let color = Self::biome_color(biome);

                    // Pour l'eau (océans et mers), la surface liquide est parfaitement sphérique au niveau de la mer
                    let is_water = biome.is_water();
                    let elevation_clamped = if is_water {
                        SEA_LEVEL_BLOCKS as f32 * 0.22
                    } else {
                        (raw_elevation as f32) * 0.22
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

                    // Premier triangle (v0, v1, v2)
                    indices.push(v0);
                    indices.push(v1);
                    indices.push(v2);

                    // Deuxième triangle (v0, v2, v3)
                    indices.push(v0);
                    indices.push(v2);
                    indices.push(v3);
                }
            }
        }

        PlanetMesh { vertices, indices }
    }

    /// Palette de couleurs selon le biome
    fn biome_color(biome: BiomeType) -> [f32; 4] {
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
}
