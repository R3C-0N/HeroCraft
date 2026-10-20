use glam::Vec3;
use herocraft_common::PlanetFace;

/// Sommet de boîte englobante pour le raymarching SDF analytique des nuages.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CloudVertex {
    pub position: [f32; 3],     // Position 3D du sommet dans le monde
    pub _pad0: f32,
    pub frame_u: [f32; 3],      // Tangente locale U (East)
    pub _pad1: f32,
    pub frame_v: [f32; 3],      // Tangente locale V (North)
    pub _pad2: f32,
    pub frame_w: [f32; 3],      // Normale locale W (Up/Radiale)
    pub _pad3: f32,
    pub cloud_center: [f32; 3], // Centre du nuage dans l'espace planétaire
    pub _pad4: f32,
    pub box1_bounds: [f32; 4],  // [cx1, cy1, hx1, hy1]
    pub box2_bounds: [f32; 4],  // [cx2, cy2, hx2, hy2]
    pub config: [f32; 4],       // [num_boxes, half_depth, sector_id, 0.0]
}

impl CloudVertex {
    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<CloudVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3, // position
                },
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3, // frame_u
                },
                wgpu::VertexAttribute {
                    offset: 32,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x3, // frame_v
                },
                wgpu::VertexAttribute {
                    offset: 48,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x3, // frame_w
                },
                wgpu::VertexAttribute {
                    offset: 64,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32x3, // cloud_center
                },
                wgpu::VertexAttribute {
                    offset: 80,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32x4, // box1_bounds
                },
                wgpu::VertexAttribute {
                    offset: 96,
                    shader_location: 6,
                    format: wgpu::VertexFormat::Float32x4, // box2_bounds
                },
                wgpu::VertexAttribute {
                    offset: 112,
                    shader_location: 7,
                    format: wgpu::VertexFormat::Float32x4, // config
                },
            ],
        }
    }
}

/// Maillage de boîtes englobantes pour les nuages 3D.
#[derive(Default, Debug, Clone)]
pub struct CloudMesh {
    pub vertices: Vec<CloudVertex>,
    pub indices: Vec<u32>,
}

/// Générateur de nuages 3D physiques réels (Approche Boîtes Englobantes + SDF Analytique).
/// Élimine complètement les problèmes de triangulation et de découpe polygonale :
/// chaque amas de nuage est représenté par une boîte englobante 3D exacte,
/// le fragment shader calculant la forme arrondie marshmallow parfaite via SDF.
pub struct CloudMeshGenerator;

impl CloudMeshGenerator {
    /// Rayon inférieur de la couche nuageuse
    pub const CLOUD_ALTITUDE_BOT: f32 = 8.2;
    /// Rayon supérieur de la couche nuageuse
    pub const CLOUD_ALTITUDE_TOP: f32 = 9.4;

    /// Demi-épaisseur du nuage (1.2 / 2.0 = 0.60)
    pub const CLOUD_HALF_DEPTH: f32 = 0.60;

    /// Génère le maillage complet des boîtes englobantes des nuages autour de la planète.
    pub fn generate_cloud_mesh(base_radius: f32, seed: u64) -> CloudMesh {
        let r_mid = base_radius + (Self::CLOUD_ALTITUDE_BOT + Self::CLOUD_ALTITUDE_TOP) * 0.5;
        let mut mesh = CloudMesh::default();

        for &face in &PlanetFace::ALL {
            Self::generate_face_clouds(&mut mesh, face, r_mid, seed);
        }

        mesh
    }

    fn generate_face_clouds(
        mesh: &mut CloudMesh,
        face: PlanetFace,
        r_mid: f32,
        seed: u64,
    ) {
        let face_id = face as u64;

        // Grille de 4x4 secteurs par face (16 secteurs par face)
        for sy in 0..4 {
            for sx in 0..4 {
                let sector_hash = Self::hash_sector(seed, face_id, sx as u64, sy as u64);
                // ~60% de chances d'avoir une formation nuageuse dans ce secteur
                if sector_hash % 100 >= 60 {
                    continue;
                }

                // Coordonnées de base au centre du secteur dans [-0.85, 0.85]
                let jitter_u = (((sector_hash >> 4) % 7) as f64 - 3.0) * 0.03;
                let jitter_v = (((sector_hash >> 8) % 7) as f64 - 3.0) * 0.03;

                let u_c = ((sx as f64 + 0.5) / 2.0 - 1.0) * 0.80 + jitter_u;
                let v_c = ((sy as f64 + 0.5) / 2.0 - 1.0) * 0.80 + jitter_v;

                // Vecteur radial W (Up)
                let dir_w = face.uv_to_sphere_direction(u_c, v_c);
                let w = Vec3::new(dir_w.x as f32, dir_w.y as f32, dir_w.z as f32).normalize();

                // Vecteurs tangents U (East) et V (North)
                let u = if w.y.abs() > 0.96 {
                    Vec3::new(1.0, 0.0, 0.0).cross(w).normalize()
                } else {
                    Vec3::new(0.0, 1.0, 0.0).cross(w).normalize()
                };
                let v = w.cross(u).normalize();

                // Centre du nuage dans le monde
                let cloud_center = w * r_mid;

                // Choix de la morphologie de nuage (style voxel marshmallow de l'image cible)
                let shape_type = (sector_hash >> 16) % 4;
                let (box1_bounds, box2_bounds, num_boxes, min_x, max_x, min_y, max_y) = match shape_type {
                    // Type 0 : Grand nuage en « L » (2 boîtes orthogonales, comme à droite dans l'image 2)
                    0 => {
                        let b1 = [0.0_f32, 0.7_f32, 2.4_f32, 0.95_f32];  // cx1, cy1, hx1, hy1
                        let b2 = [1.45_f32, -0.65_f32, 0.95_f32, 1.65_f32]; // cx2, cy2, hx2, hy2
                        let min_x = (-2.4_f32).min(1.45 - 0.95) - 0.35;
                        let max_x = (2.4_f32).max(1.45 + 0.95) + 0.35;
                        let min_y = (0.7 - 0.95_f32).min(-0.65 - 1.65) - 0.35;
                        let max_y = (0.7 + 0.95_f32).max(-0.65 + 1.65) + 0.35;
                        (b1, b2, 2.0_f32, min_x, max_x, min_y, max_y)
                    }
                    // Type 1 : Nuage en marche d'escalier décalée (2 lobes, comme au centre dans l'image 2)
                    1 => {
                        let b1 = [-0.75_f32, 0.45_f32, 2.0_f32, 0.90_f32];
                        let b2 = [0.85_f32, -0.45_f32, 2.1_f32, 0.90_f32];
                        let min_x = (-0.75 - 2.0_f32).min(0.85 - 2.1) - 0.35;
                        let max_x = (-0.75 + 2.0_f32).max(0.85 + 2.1) + 0.35;
                        let min_y = (0.45 - 0.90_f32).min(-0.45 - 0.90) - 0.35;
                        let max_y = (0.45 + 0.90_f32).max(-0.45 + 0.90) + 0.35;
                        (b1, b2, 2.0_f32, min_x, max_x, min_y, max_y)
                    }
                    // Type 2 : Grand coussin oblong (1 boîte allongée, comme à gauche dans l'image 2)
                    2 => {
                        let b1 = [0.0_f32, 0.0_f32, 2.6_f32, 1.25_f32];
                        let b2 = [0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32];
                        let min_x = -2.6 - 0.35;
                        let max_x = 2.6 + 0.35;
                        let min_y = -1.25 - 0.35;
                        let max_y = 1.25 + 0.35;
                        (b1, b2, 1.0_f32, min_x, max_x, min_y, max_y)
                    }
                    // Type 3 : Coussin compact (1 boîte plus courte)
                    _ => {
                        let b1 = [0.0_f32, 0.0_f32, 1.6_f32, 1.0_f32];
                        let b2 = [0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32];
                        let min_x = -1.6 - 0.35;
                        let max_x = 1.6 + 0.35;
                        let min_y = -1.0 - 0.35;
                        let max_y = 1.0 + 0.35;
                        (b1, b2, 1.0_f32, min_x, max_x, min_y, max_y)
                    }
                };

                let half_depth = Self::CLOUD_HALF_DEPTH;
                let min_z = -half_depth - 0.20;
                let max_z = half_depth + 0.20;

                let config = [num_boxes, half_depth, (sx * 4 + sy) as f32, 0.0];

                // Génération des 8 sommets de la boîte englobante
                let base_idx = mesh.vertices.len() as u32;

                for &iz in &[min_z, max_z] {
                    for &iy in &[min_y, max_y] {
                        for &ix in &[min_x, max_x] {
                            let pos = cloud_center + u * ix + v * iy + w * iz;
                            mesh.vertices.push(CloudVertex {
                                position: pos.to_array(),
                                _pad0: 0.0,
                                frame_u: u.to_array(),
                                _pad1: 0.0,
                                frame_v: v.to_array(),
                                _pad2: 0.0,
                                frame_w: w.to_array(),
                                _pad3: 0.0,
                                cloud_center: cloud_center.to_array(),
                                _pad4: 0.0,
                                box1_bounds,
                                box2_bounds,
                                config,
                            });
                        }
                    }
                }

                // 12 triangles (36 indices) avec enroulement CCW externe
                // Indexation locale :
                // 0: (x0, y0, z0), 1: (x1, y0, z0), 2: (x0, y1, z0), 3: (x1, y1, z0)
                // 4: (x0, y0, z1), 5: (x1, y0, z1), 6: (x0, y1, z1), 7: (x1, y1, z1)
                #[rustfmt::skip]
                let box_indices = [
                    // Face Dessous (-Z)
                    0, 1, 3,  0, 3, 2,
                    // Face Dessus (+Z)
                    4, 6, 7,  4, 7, 5,
                    // Face Sud (-Y)
                    0, 4, 5,  0, 5, 1,
                    // Face Nord (+Y)
                    2, 3, 7,  2, 7, 6,
                    // Face Ouest (-X)
                    0, 2, 6,  0, 6, 4,
                    // Face Est (+X)
                    1, 5, 7,  1, 7, 3,
                ];

                for &idx in &box_indices {
                    mesh.indices.push(base_idx + idx);
                }
            }
        }
    }

    fn hash_sector(seed: u64, face: u64, sx: u64, sy: u64) -> u64 {
        let mut h = seed ^ (face.wrapping_mul(0x9E3779B97F4A7C15));
        h = h.wrapping_add(sx.wrapping_mul(0xBF58476D1CE4E5B9));
        h = h ^ (h >> 30);
        h = h.wrapping_add(sy.wrapping_mul(0x94D049BB133111EB));
        h = h ^ (h >> 27);
        h = h.wrapping_mul(0x45D9F3B);
        h ^ (h >> 31)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cloud_mesh_bounded_clusters_and_reasonable_sizes() {
        let mesh = CloudMeshGenerator::generate_cloud_mesh(25.0, 2026);

        // Vérifie que le maillage est généré avec des sommets et indices valides
        assert!(!mesh.vertices.is_empty(), "Le maillage de nuages ne doit pas être vide");
        assert!(!mesh.indices.is_empty(), "Les indices de nuages ne doivent pas être vides");
        assert_eq!(mesh.indices.len() % 3, 0, "Les indices doivent former des triangles complets");

        // Vérifie les bornes d'altitude : tous les sommets doivent être compris entre r_bot et r_top (+ tolérance marge)
        let r_bot = 25.0 + CloudMeshGenerator::CLOUD_ALTITUDE_BOT - 0.60;
        let r_top = 25.0 + CloudMeshGenerator::CLOUD_ALTITUDE_TOP + 0.60;

        for v in &mesh.vertices {
            let pos = Vec3::from(v.position);
            let r = pos.length();
            assert!(
                r >= r_bot && r <= r_top,
                "Position de sommet hors de la couche nuageuse : r = {r}, attendu [{r_bot}, {r_top}]"
            );
        }

        // Le nombre de sommets doit correspondre exactement à des boîtes de 8 sommets
        assert_eq!(mesh.vertices.len() % 8, 0);
        assert_eq!(mesh.indices.len() % 36, 0);
    }

    #[test]
    fn test_cloud_vertex_layout_and_frames() {
        let mesh = CloudMeshGenerator::generate_cloud_mesh(25.0, 2026);

        for v in &mesh.vertices {
            let u = Vec3::from(v.frame_u);
            let v_dir = Vec3::from(v.frame_v);
            let w = Vec3::from(v.frame_w);

            // Vérifie l'orthonormalité du repère local
            assert!((u.length() - 1.0).abs() < 1e-4, "u doit être unitaire");
            assert!((v_dir.length() - 1.0).abs() < 1e-4, "v doit être unitaire");
            assert!((w.length() - 1.0).abs() < 1e-4, "w doit être unitaire");
            assert!(u.dot(v_dir).abs() < 1e-4, "u et v doivent être orthogonaux");
            assert!(u.dot(w).abs() < 1e-4, "u et w doivent être orthogonaux");
            assert!(v_dir.dot(w).abs() < 1e-4, "v et w doivent être orthogonaux");

            // Vérifie la validité des boîtes
            assert!(v.config[0] >= 1.0 && v.config[0] <= 2.0);
            assert_eq!(v.config[1], CloudMeshGenerator::CLOUD_HALF_DEPTH);
            assert!(v.box1_bounds[2] > 0.0 && v.box1_bounds[3] > 0.0);
        }
    }
}
