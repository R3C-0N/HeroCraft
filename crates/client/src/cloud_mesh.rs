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
    /// Rayon supérieur de la couche nuageuse (épaisseur de 1.2 bloc)
    pub const CLOUD_ALTITUDE_TOP: f32 = 9.4;

    /// Demi-épaisseur du nuage (1.2 / 2.0 = 0.60)
    pub const CLOUD_HALF_DEPTH: f32 = 0.60;

    /// Nombre de secteurs par côté de face (au plus un nuage par secteur)
    pub const SECTORS_PER_FACE: usize = 5;
    /// Taille d'un bloc de nuage (les rectangles sont des multiples entiers de ce bloc)
    pub const BLOCK: f32 = 1.6;
    /// Étendue maximale d'un nuage en blocs (grand côté, petit côté)
    pub const MAX_SPAN: (i32, i32) = (4, 3);

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
        let sectors = Self::SECTORS_PER_FACE;

        // Grille de 5x5 secteurs par face : chaque secteur accueille au plus un nuage
        for sy in 0..sectors {
            for sx in 0..sectors {
                let sector_hash = Self::hash_sector(seed, face_id, sx as u64, sy as u64);
                // ~72% de chances d'avoir une formation nuageuse dans ce secteur
                if sector_hash % 100 >= 72 {
                    continue;
                }
                let mut rng = sector_hash;

                // Centre du secteur dans [-0.9, 0.9] avec une légère gigue (reste loin des voisins)
                let jitter_u = ((Self::next_rand(&mut rng) % 7) as f64 - 3.0) * 0.004;
                let jitter_v = ((Self::next_rand(&mut rng) % 7) as f64 - 3.0) * 0.004;
                let u_c = ((sx as f64 + 0.5) / sectors as f64 * 2.0 - 1.0) * 0.90 + jitter_u;
                let v_c = ((sy as f64 + 0.5) / sectors as f64 * 2.0 - 1.0) * 0.90 + jitter_v;

                // Vecteur radial W (Up)
                let dir_w = face.uv_to_sphere_direction(u_c, v_c);
                let w = Vec3::new(dir_w.x as f32, dir_w.y as f32, dir_w.z as f32).normalize();

                // Vecteurs tangents U (East) et V (North), tournés d'un angle aléatoire autour de W
                let u0 = if w.y.abs() > 0.96 {
                    Vec3::new(1.0, 0.0, 0.0).cross(w).normalize()
                } else {
                    Vec3::new(0.0, 1.0, 0.0).cross(w).normalize()
                };
                let v0 = w.cross(u0).normalize();
                let theta = (Self::next_rand(&mut rng) % 360) as f32 * std::f32::consts::PI / 180.0;
                let (s_t, c_t) = theta.sin_cos();
                let u = (u0 * c_t + v0 * s_t).normalize();
                let v = w.cross(u).normalize();

                // Centre du nuage dans le monde
                let cloud_center = w * r_mid;

                // Morphologie aléatoire sur grille de blocs de 1.6 : union de deux rectangles
                // qui partagent au moins un bloc (L, T, croix, Z, dalle simple)
                let (box1_bounds, box2_bounds, num_boxes) = Self::random_shape(&mut rng);
                let pad_xy = 0.65_f32;
                let mut min_x = box1_bounds[0] - box1_bounds[2];
                let mut max_x = box1_bounds[0] + box1_bounds[2];
                let mut min_y = box1_bounds[1] - box1_bounds[3];
                let mut max_y = box1_bounds[1] + box1_bounds[3];
                if num_boxes > 1.5 {
                    min_x = min_x.min(box2_bounds[0] - box2_bounds[2]);
                    max_x = max_x.max(box2_bounds[0] + box2_bounds[2]);
                    min_y = min_y.min(box2_bounds[1] - box2_bounds[3]);
                    max_y = max_y.max(box2_bounds[1] + box2_bounds[3]);
                }
                let (min_x, max_x, min_y, max_y) = (min_x - pad_xy, max_x + pad_xy, min_y - pad_xy, max_y + pad_xy);
                let half_depth = Self::CLOUD_HALF_DEPTH;
                let pad_z = 0.50_f32;
                let min_z = -half_depth - pad_z;
                let max_z = half_depth + pad_z;

                let config = [num_boxes, half_depth, (sx * sectors + sy) as f32, 0.0];

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

                // 12 triangles (36 indices) avec enroulement CCW externe rigoureusement vérifié
                // Indexation locale :
                // 0: (x0, y0, z0), 1: (x1, y0, z0), 2: (x0, y1, z0), 3: (x1, y1, z0)
                // 4: (x0, y0, z1), 5: (x1, y0, z1), 6: (x0, y1, z1), 7: (x1, y1, z1)
                #[rustfmt::skip]
                let box_indices = [
                    // Face Dessous (-Z, normale vers le bas)
                    0, 2, 3,  0, 3, 1,
                    // Face Dessus (+Z, normale vers le haut)
                    4, 5, 7,  4, 7, 6,
                    // Face Sud (-Y, normale vers le Sud)
                    0, 1, 5,  0, 5, 4,
                    // Face Nord (+Y, normale vers le Nord)
                    2, 6, 7,  2, 7, 3,
                    // Face Ouest (-X, normale vers l'Ouest)
                    0, 4, 6,  0, 6, 2,
                    // Face Est (+X, normale vers l'Est)
                    1, 3, 7,  1, 7, 5,
                ];

                for &idx in &box_indices {
                    mesh.indices.push(base_idx + idx);
                }
            }
        }
    }

    /// Générateur pseudo-aléatoire SplitMix64 (déterministe par secteur)
    fn next_rand(state: &mut u64) -> u64 {
        *state = state.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = *state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }

    /// Tire une forme : un rectangle W1xH1 blocs, plus éventuellement un second rectangle W2xH2
    /// qui recouvre au moins un bloc du premier. Retourne les boîtes centrées [cx, cy, hx, hy].
    fn random_shape(rng: &mut u64) -> ([f32; 4], [f32; 4], f32) {
        let pick = |rng: &mut u64, lo: i32, hi: i32| lo + (Self::next_rand(rng) % (hi - lo + 1) as u64) as i32;
        let fits = |w: i32, h: i32| {
            let (a, b) = Self::MAX_SPAN;
            (w <= a && h <= b) || (w <= b && h <= a)
        };
        let to_box = |x0: i32, y0: i32, w: i32, h: i32, cx: f32, cy: f32| {
            let hb = Self::BLOCK * 0.5;
            [
                (x0 as f32 + w as f32 * 0.5) * Self::BLOCK - cx,
                (y0 as f32 + h as f32 * 0.5) * Self::BLOCK - cy,
                w as f32 * hb,
                h as f32 * hb,
            ]
        };

        let w1 = pick(rng, 1, 4);
        let h1 = pick(rng, 1, 2);

        // ~25% de dalles simples, sinon tentative d'union de deux rectangles
        if Self::next_rand(rng) % 4 != 0 {
            for _ in 0..8 {
                let w2 = pick(rng, 1, 2);
                let h2 = pick(rng, 2, 3);
                let ox = pick(rng, -(w2 - 1), w1 - 1);
                let oy = pick(rng, -(h2 - 1), h1 - 1);
                let (x0, x1) = (ox.min(0), (ox + w2).max(w1));
                let (y0, y1) = (oy.min(0), (oy + h2).max(h1));
                let contained = ox >= 0 && oy >= 0 && ox + w2 <= w1 && oy + h2 <= h1;
                if contained || !fits(x1 - x0, y1 - y0) {
                    continue;
                }
                let cx = (x0 + x1) as f32 * 0.5 * Self::BLOCK;
                let cy = (y0 + y1) as f32 * 0.5 * Self::BLOCK;
                return (to_box(0, 0, w1, h1, cx, cy), to_box(ox, oy, w2, h2, cx, cy), 2.0);
            }
        }

        let (w1, h1) = if fits(w1, h1) { (w1, h1) } else { (3, h1) };
        let cx = w1 as f32 * 0.5 * Self::BLOCK;
        let cy = h1 as f32 * 0.5 * Self::BLOCK;
        (to_box(0, 0, w1, h1, cx, cy), [0.0; 4], 1.0)
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

        // Vérifie les bornes d'altitude : tous les sommets doivent être compris dans la couche nuageuse (avec marge de boîte tangente)
        let r_bot = 25.0 + CloudMeshGenerator::CLOUD_ALTITUDE_BOT - 1.0;
        let r_top = 25.0 + CloudMeshGenerator::CLOUD_ALTITUDE_TOP + 1.0;

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

    #[test]
    fn test_cloud_shapes_varied_bounded_and_spaced() {
        let mesh = CloudMeshGenerator::generate_cloud_mesh(25.0, 2026);
        let clouds: Vec<&CloudVertex> = mesh.vertices.iter().step_by(8).collect();
        assert!(clouds.len() >= 6 * 12, "Le ciel doit compter plus de nuages (obtenu {})", clouds.len());

        let mut shapes = std::collections::HashSet::new();
        let mut radii = Vec::new();
        for c in &clouds {
            let (b1, b2, n) = (c.box1_bounds, c.box2_bounds, c.config[0]);
            let mut boxes = vec![b1];
            if n > 1.5 {
                boxes.push(b2);
                // Les deux rectangles se recouvrent (forme connexe, pas de raccord à fleur)
                assert!((b1[0] - b2[0]).abs() < b1[2] + b2[2] - 0.1 && (b1[1] - b2[1]).abs() < b1[3] + b2[3] - 0.1);
            }
            let r = boxes
                .iter()
                .map(|b| ((b[0].abs() + b[2]).powi(2) + (b[1].abs() + b[3]).powi(2)).sqrt())
                .fold(0.0_f32, f32::max);
            assert!(r <= 4.05, "Nuage trop grand : rayon {r}");
            radii.push(r);
            shapes.insert(boxes.iter().map(|b| format!("{:.1},{:.1}", b[2], b[3])).collect::<Vec<_>>().join("|"));
        }
        assert!(shapes.len() >= 10, "Les formes doivent varier (obtenu {} formes)", shapes.len());

        // Aucun nuage ne chevauche son voisin
        for i in 0..clouds.len() {
            for j in i + 1..clouds.len() {
                let d = (Vec3::from(clouds[i].cloud_center) - Vec3::from(clouds[j].cloud_center)).length();
                assert!(d > radii[i] + radii[j], "Nuages {i} et {j} se chevauchent (d = {d})");
            }
        }
    }
}
